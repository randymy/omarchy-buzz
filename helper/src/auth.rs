use crate::{config, protocol::Status};
use buzz_ws_client::{NostrWsConnection, RelayMessage, WsClientError};
use tokio::{
    sync::{mpsc, watch},
    time::{timeout, Duration},
};
use zeroize::Zeroizing;
pub const SERVICE: &str = "omarchy-buzz.identity.v1";

pub fn read_keys(c: &config::Config) -> Result<nostr::Keys, &'static str> {
    let e =
        keyring::Entry::new(SERVICE, &config::account(c)?).map_err(|_| "identity_unavailable")?;
    let secret = Zeroizing::new(e.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => "identity_missing",
        _ => "identity_unavailable",
    })?);
    let keys = nostr::Keys::parse(secret.as_str()).map_err(|_| "identity_invalid")?;
    if Some(keys.public_key().to_hex()).as_deref() != c.identity.as_deref() {
        return Err("identity_invalid");
    }
    Ok(keys)
}
fn update(tx: &watch::Sender<Status>, state: &str, category: Option<&str>) {
    tx.send_modify(|s| {
        s.connection = state.into();
        s.category = category.map(str::to_owned);
        if state != "authenticated" {
            s.catalog = crate::protocol::Catalog::unavailable(None);
        }
    });
}
fn category(e: &WsClientError) -> &'static str {
    match e {
        WsClientError::AuthFailed(_) => "auth_rejected",
        WsClientError::Timeout | WsClientError::NoAuthChallenge => "relay_timeout",
        WsClientError::ConnectionClosed | WsClientError::WebSocket(_) => "relay_unavailable",
        _ => "relay_protocol_error",
    }
}
async fn connect_identity(
    relay: &str,
    keys: &nostr::Keys,
) -> Result<NostrWsConnection, &'static str> {
    match timeout(
        Duration::from_secs(45),
        NostrWsConnection::connect_authenticated(relay, keys, None),
    )
    .await
    {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(error)) => Err(category(&error)),
        Err(_) => Err("relay_timeout"),
    }
}

fn apply_loaded_config(
    tx: &watch::Sender<Status>,
    loaded: Result<config::Config, &'static str>,
) -> Result<config::Config, ()> {
    match loaded {
        Ok(c) => {
            tx.send_modify(|s| {
                if s.relay != c.relay || s.identity != c.identity {
                    s.relay = c.relay.clone();
                    s.identity = c.identity.clone();
                    s.generation = s.generation.saturating_add(1);
                    s.connection = "unconfigured".into();
                    s.category = None;
                    s.catalog = crate::protocol::Catalog::unavailable(None);
                }
            });
            Ok(c)
        }
        Err(error) => {
            // Invalid disk configuration never falls back to authenticating
            // with the previously loaded identity/origin.
            let category = if error == "config_unavailable" {
                "config_unavailable"
            } else {
                "invalid_config"
            };
            update(tx, "unavailable", Some(category));
            Err(())
        }
    }
}

fn catalog_category(error: &str) -> &'static str {
    match error {
        "relay_identity_changed" => "relay_identity_changed",
        "discovery_signer_unavailable" => "relay_identity_unavailable",
        "discovery_timeout" | "query_timeout" => "room_catalog_timeout",
        "discovery_invalid_info"
        | "discovery_oversized"
        | "discovery_redirect_rejected"
        | "query_oversized"
        | "query_invalid_response"
        | "query_invalid_signature"
        | "query_invalid_scope"
        | "query_redirect_rejected"
        | "catalog_invalid_signature"
        | "catalog_untrusted_author"
        | "catalog_invalid_shape"
        | "catalog_conflicting_snapshot"
        | "catalog_invalid_membership"
        | "catalog_invalid_scope"
        | "catalog_oversized" => "room_catalog_invalid",
        _ => "room_catalog_unavailable",
    }
}

// NIP-45 COUNT is supported by the pinned relay's handle_count. It is a
// narrow own-profile read, not a presence/process-health assertion.
#[derive(Clone, Copy)]
struct FreshnessPolicy {
    interval: Duration,
    response: Duration,
}
const FRESHNESS: FreshnessPolicy = FreshnessPolicy {
    interval: Duration::from_secs(20),
    response: Duration::from_secs(5),
};
#[derive(Default)]
struct Backoff {
    failures: u8,
}
impl Backoff {
    fn reset(&mut self) {
        self.failures = 0;
    }
    fn delay(&mut self, error: &str) -> Option<Duration> {
        if !matches!(error, "relay_timeout" | "relay_unavailable") || self.failures >= 5 {
            return None;
        }
        let seconds = 1_u64 << self.failures;
        self.failures += 1;
        Some(Duration::from_secs(seconds))
    }
}
enum ConnectionExit {
    Retry,
    Shutdown,
    Failure(&'static str),
}
async fn wait_after_failure(
    error: &str,
    backoff: &mut Backoff,
    retry: &mut mpsc::Receiver<()>,
) -> bool {
    if let Some(delay) = backoff.delay(error) {
        tokio::select! {
            _=tokio::time::sleep(delay)=>true,
            r=retry.recv()=> { if r.is_some() { backoff.reset(); true } else { false } }
        }
    } else {
        if retry.recv().await.is_some() {
            backoff.reset();
            true
        } else {
            false
        }
    }
}
// Independent timer deadlines remain effective even when a relay emits unrelated
// notices/events continuously. Only the exact probe ID refreshes authentication.
async fn observe_connection(
    conn: &mut NostrWsConnection,
    keys: &nostr::Keys,
    relay: &str,
    relay_pin: &mut Option<nostr::PublicKey>,
    tx: &watch::Sender<Status>,
    retry: &mut mpsc::Receiver<()>,
    backoff: &mut Backoff,
    policy: FreshnessPolicy,
) -> ConnectionExit {
    let mut pending: Option<String> = None;
    let mut due = tokio::time::Instant::now();
    let mut catalog_due = tokio::time::Instant::now();
    let mut fresh = false;
    let mut jobs: tokio::task::JoinSet<Result<crate::catalog::Catalog, &'static str>> =
        tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            biased;
            r=retry.recv()=>return if r.is_some() { backoff.reset(); ConnectionExit::Retry } else { ConnectionExit::Shutdown },
            _=tokio::time::sleep_until(due)=> {
                if pending.is_some() { return ConnectionExit::Failure("relay_timeout"); }
                let id=format!("omarchy-buzz-liveness-{}",uuid::Uuid::new_v4());
                let frame=serde_json::json!(["COUNT",id,{"kinds":[0],"authors":[keys.public_key().to_hex()],"limit":1}]);
                match timeout(policy.response,conn.send_raw(&frame)).await {
                    Ok(Ok(()))=> {},
                    Ok(Err(e))=>return ConnectionExit::Failure(category(&e)),
                    Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                }
                pending=Some(id);
                due=tokio::time::Instant::now()+policy.response;
            },
            result=jobs.join_next(), if !jobs.is_empty()=> {
                let cancelled=matches!(&result,Some(Err(error)) if error.is_cancelled());
                match result {
                    Some(Ok(Ok(catalog))) if fresh=> {
                        *relay_pin=Some(catalog.signer);
                        tx.send_modify(|s|s.catalog=crate::protocol::Catalog {
                            state:catalog.state.into(),category:Some(catalog.category.into()),
                            rooms:catalog.rooms.into_iter().map(|r|crate::protocol::Room {id:r.id,name:r.name,description:r.description}).collect(),
                        });
                    },
                    Some(Ok(Err(error))) if fresh=>tx.send_modify(|s|s.catalog=crate::protocol::Catalog::unavailable(Some(catalog_category(error)))),
                    Some(Err(error)) if fresh && !error.is_cancelled()=>tx.send_modify(|s|s.catalog=crate::protocol::Catalog::unavailable(Some("room_catalog_unavailable"))),
                    _=>{},
                }
                if !cancelled {catalog_due=tokio::time::Instant::now()+Duration::from_secs(30);}
            },
            _=tokio::time::sleep_until(catalog_due), if fresh && jobs.is_empty()=> {
                tx.send_modify(|s|s.catalog=crate::protocol::Catalog::loading());
                let relay=relay.to_owned();let keys=keys.clone();let pin=*relay_pin;
                jobs.spawn(async move {
                    match timeout(Duration::from_secs(15),crate::catalog::discover(&relay,&keys,pin)).await {
                        Ok(result)=>result,Err(_)=>Err("discovery_timeout"),
                    }
                });
            },
            msg=conn.next_event(Duration::from_secs(30))=>match msg {
                Ok(RelayMessage::Count { subscription_id, .. }) if pending.as_deref()==Some(subscription_id.as_str())=> {
                    pending=None;
                    due=tokio::time::Instant::now()+policy.interval;
                    backoff.reset();
                    update(tx,"authenticated",None);
                    if !fresh {fresh=true;catalog_due=tokio::time::Instant::now();}
                },
                Ok(RelayMessage::Closed { subscription_id, .. }) if pending.as_deref()==Some(subscription_id.as_str())=>return ConnectionExit::Failure("relay_protocol_error"),
                Ok(RelayMessage::Auth { challenge })=> {
                    if challenge.len()>1024 { return ConnectionExit::Failure("relay_protocol_error"); }
                    fresh=false;
                    jobs.abort_all();
                    update(tx,"connecting",None);
                    match timeout(Duration::from_secs(25),conn.authenticate(keys,None)).await {
                        Ok(Ok(()))=> { pending=None; due=tokio::time::Instant::now(); },
                        Ok(Err(e))=>return ConnectionExit::Failure(category(&e)),
                        Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                    }
                },
                Ok(_)|Err(WsClientError::Timeout)=>{},
                Err(e)=>return ConnectionExit::Failure(category(&e)),
            }
        }
    }
}

pub async fn run(
    _initial: config::Config,
    tx: watch::Sender<Status>,
    mut retry: mpsc::Receiver<()>,
) {
    let mut backoff = Backoff::default();
    let mut pin_origin: Option<String> = None;
    let mut relay_pin: Option<nostr::PublicKey> = None;
    loop {
        // Coalesce retry requests already queued for this attempt. Only this
        // sequential task owns key lookups, including any pending prompt.
        while retry.try_recv().is_ok() {
            backoff.reset();
        }
        let c = match apply_loaded_config(&tx, config::load()) {
            Ok(c) => c,
            Err(()) => {
                if retry.recv().await.is_none() {
                    return;
                }
                backoff.reset();
                continue;
            }
        };
        if pin_origin != c.relay {
            pin_origin = c.relay.clone();
            relay_pin = None;
        }
        if c.relay.is_none() || c.identity.is_none() {
            update(&tx, "unconfigured", None);
            if retry.recv().await.is_none() {
                return;
            }
            backoff.reset();
            continue;
        }
        update(&tx, "connecting", None);
        let cfg = c.clone();
        let mut key_read = tokio::task::spawn_blocking(move || read_keys(&cfg));
        let key_result = match timeout(Duration::from_secs(15), &mut key_read).await {
            Ok(result) => result,
            Err(_) => {
                // A blocking Secret Service prompt cannot be cancelled safely.
                // Keep this one lookup alive instead of spawning more on retry.
                update(&tx, "unavailable", Some("identity_access_pending"));
                key_read.await
            }
        };
        // Setup may have changed while an unlock prompt was pending. Wait for
        // its single operation to finish, then reload before any relay auth.
        if retry.try_recv().is_ok() {
            backoff.reset();
            continue;
        }
        let keys = match key_result {
            Ok(Ok(k)) => k,
            Ok(Err(e)) => {
                update(
                    &tx,
                    if e == "identity_unavailable" {
                        "unavailable"
                    } else {
                        "unconfigured"
                    },
                    Some(e),
                );
                if retry.recv().await.is_none() {
                    return;
                }
                backoff.reset();
                continue;
            }
            Err(_) => {
                update(&tx, "unavailable", Some("identity_unavailable"));
                if retry.recv().await.is_none() {
                    return;
                }
                backoff.reset();
                continue;
            }
        };
        update(&tx, "connecting", None);
        let relay = c.relay.as_deref().unwrap_or_default();
        let mut conn = match connect_identity(relay, &keys).await {
            Ok(connection) => connection,
            Err(error) => {
                update(&tx, "disconnected", Some(error));
                if !wait_after_failure(error, &mut backoff, &mut retry).await {
                    return;
                }
                continue;
            }
        };
        let exit = observe_connection(
            &mut conn,
            &keys,
            relay,
            &mut relay_pin,
            &tx,
            &mut retry,
            &mut backoff,
            FRESHNESS,
        )
        .await;
        // A timed-out socket is dropped before backoff; graceful close is only
        // attempted for deliberate Retry, under its own short deadline.
        match exit {
            ConnectionExit::Retry => {
                let _ = timeout(Duration::from_secs(3), conn.disconnect()).await;
            }
            ConnectionExit::Shutdown => return,
            ConnectionExit::Failure(error) => {
                drop(conn);
                update(&tx, "disconnected", Some(error));
                if !wait_after_failure(error, &mut backoff, &mut retry).await {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    fn fixture(relay: Option<&str>, identity: Option<&str>) -> config::Config {
        config::Config {
            relay: relay.map(str::to_owned),
            identity: identity.map(str::to_owned),
        }
    }
    #[test]
    fn scope_generation_changes_only_with_public_scope() {
        let initial = fixture(None, None);
        let (tx, rx) = watch::channel(Status::new(&initial));
        assert!(apply_loaded_config(&tx, Ok(initial)).is_ok());
        assert_eq!(rx.borrow().generation, 1);
        let c = fixture(Some("wss://example.com/"), None);
        assert!(apply_loaded_config(&tx, Ok(c.clone())).is_ok());
        assert_eq!(rx.borrow().generation, 2);
        assert!(apply_loaded_config(&tx, Ok(c)).is_ok());
        assert_eq!(rx.borrow().generation, 2);
        assert!(apply_loaded_config(
            &tx,
            Ok(fixture(
                Some("wss://example.com/"),
                Some("synthetic-public-key")
            ))
        )
        .is_ok());
        assert_eq!(rx.borrow().generation, 3);
        assert_eq!(
            rx.borrow().identity.as_deref(),
            Some("synthetic-public-key")
        );
    }
    #[test]
    fn malformed_reload_disables_auth_without_inventing_scope() {
        let c = fixture(Some("wss://example.com/"), None);
        let (tx, rx) = watch::channel(Status::new(&c));
        assert!(apply_loaded_config(&tx, Err("invalid_relay")).is_err());
        assert_eq!(rx.borrow().generation, 1);
        assert_eq!(rx.borrow().connection, "unavailable");
        assert_eq!(rx.borrow().category.as_deref(), Some("invalid_config"));
        assert!(apply_loaded_config(&tx, Err("config_unavailable")).is_err());
        assert_eq!(rx.borrow().category.as_deref(), Some("config_unavailable"));
    }
}

#[cfg(test)]
#[path = "auth_wire_tests.rs"]
mod wire_tests;

#[cfg(test)]
#[path = "auth_liveness_tests.rs"]
mod liveness_tests;

#[cfg(test)]
#[path = "auth_catalog_tests.rs"]
mod catalog_integration_tests;
