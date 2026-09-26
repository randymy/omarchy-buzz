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

pub async fn run(
    _initial: config::Config,
    tx: watch::Sender<Status>,
    mut retry: mpsc::Receiver<()>,
) {
    loop {
        // Coalesce retry requests already queued for this attempt. Only this
        // sequential task owns key lookups, including any pending prompt.
        while retry.try_recv().is_ok() {}
        let c = match apply_loaded_config(&tx, config::load()) {
            Ok(c) => c,
            Err(()) => {
                if retry.recv().await.is_none() {
                    return;
                }
                continue;
            }
        };
        if c.relay.is_none() || c.identity.is_none() {
            update(&tx, "unconfigured", None);
            if retry.recv().await.is_none() {
                return;
            }
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
                continue;
            }
            Err(_) => {
                update(&tx, "unavailable", Some("identity_unavailable"));
                if retry.recv().await.is_none() {
                    return;
                }
                continue;
            }
        };
        update(&tx, "connecting", None);
        let relay = c.relay.as_deref().unwrap_or_default();
        let result = timeout(
            Duration::from_secs(45),
            NostrWsConnection::connect_authenticated(relay, &keys, None),
        )
        .await;
        let mut conn = match result {
            Ok(Ok(c)) => c,
            other => {
                let e = match &other {
                    Ok(Err(e)) => category(e),
                    _ => "relay_timeout",
                };
                update(&tx, "disconnected", Some(e));
                if retry.recv().await.is_none() {
                    return;
                }
                continue;
            }
        };
        update(&tx, "authenticated", None);
        loop {
            tokio::select! {
                r=retry.recv()=> { if r.is_none() { return; } let _=timeout(Duration::from_secs(3), conn.disconnect()).await; break; },
                msg=conn.next_event(Duration::from_secs(30))=>match msg {
                    Ok(RelayMessage::Auth { challenge })=> {
                        // recv_one stores this challenge for authenticate. Reject the
                        // oversize path too: pending_challenge bypasses the upstream
                        // size guard in wait_for_auth_challenge.
                        if challenge.len() > 1024 {
                            update(&tx,"disconnected",Some("relay_protocol_error"));
                            if retry.recv().await.is_none() { return; }
                            break;
                        }
                        update(&tx,"connecting",None);
                        match timeout(Duration::from_secs(25),conn.authenticate(&keys,None)).await {
                            Ok(Ok(()))=>update(&tx,"authenticated",None),
                            Ok(Err(e))=> { update(&tx,"disconnected",Some(category(&e))); if retry.recv().await.is_none() { return; } break; },
                            Err(_)=> { update(&tx,"disconnected",Some("relay_timeout")); if retry.recv().await.is_none() { return; } break; }
                        }
                    },
                    Ok(_)|Err(WsClientError::Timeout)=>{},
                    Err(e)=> { update(&tx,"disconnected",Some(category(&e))); if retry.recv().await.is_none() { return; } break; }
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
