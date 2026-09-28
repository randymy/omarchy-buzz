use crate::{
    config,
    protocol::{Command, History, Status},
};
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
            s.history = History::unavailable(None, None);
            s.activity.clear();
            s.recipients = crate::protocol::RecipientsView::unavailable(None, None);
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
                    s.delivery = crate::protocol::Delivery::default();
                    s.connection = "unconfigured".into();
                    s.category = None;
                    s.catalog = crate::protocol::Catalog::unavailable(None);
                    s.history = History::unavailable(None, None);
                    s.activity.clear();
                    s.recipients = crate::protocol::RecipientsView::unavailable(None, None);
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
fn offline_command(command: Command, tx: &watch::Sender<Status>) -> bool {
    match command {
        Command::Retry => return true,
        Command::SendChecked(_, reply) => {
            let _ = reply.send(Some("send_unavailable"));
        }
        Command::Send(intent) => tx.send_modify(|s| {
            s.delivery = crate::protocol::Delivery {
                request_id: Some(intent.request_id),
                room_id: Some(intent.room),
                event_id: None,
                state: "failed".into(),
                category: Some("send_unavailable".into()),
            }
        }),
        _ => {}
    }
    false
}
async fn next_retry(commands: &mut mpsc::Receiver<Command>, tx: &watch::Sender<Status>) -> bool {
    while let Some(command) = commands.recv().await {
        if offline_command(command, tx) {
            return true;
        }
    }
    false
}
fn history_category(error: &str) -> &'static str {
    match error {
        "history_timeout" | "query_timeout" => "history_timeout",
        "query_access_denied" => "history_access_denied",
        e if e.starts_with("history_")
            || matches!(
                e,
                "query_oversized"
                    | "query_invalid_response"
                    | "query_invalid_signature"
                    | "query_invalid_scope"
                    | "query_redirect_rejected"
            ) =>
        {
            "history_invalid"
        }
        _ => "history_unavailable",
    }
}
fn send_category(category: &str) -> Option<&'static str> {
    match category {
        "send_invalid" => Some("send_invalid"),
        "send_busy" => Some("send_busy"),
        "send_request_reused" => Some("send_request_reused"),
        "send_scope_changed" => Some("send_scope_changed"),
        "send_access_denied" => Some("send_access_denied"),
        "send_ledger_unavailable" => Some("send_ledger_unavailable"),
        "send_unavailable" => Some("send_unavailable"),
        _ => None,
    }
}
fn recipients_category(error: &str) -> &'static str {
    match error {
        "query_timeout" | "recipients_timeout" => "recipients_timeout",
        "query_access_denied" | "recipients_access_denied" => "recipients_access_denied",
        e if e.starts_with("recipients_")
            || matches!(
                e,
                "query_oversized"
                    | "query_invalid_response"
                    | "query_invalid_signature"
                    | "query_invalid_scope"
                    | "query_redirect_rejected"
            ) =>
        {
            "recipients_invalid"
        }
        _ => "recipients_unavailable",
    }
}
async fn wait_after_failure(
    error: &str,
    backoff: &mut Backoff,
    retry: &mut mpsc::Receiver<Command>,
    tx: &watch::Sender<Status>,
) -> bool {
    if let Some(delay) = backoff.delay(error) {
        tokio::select! {
            _=tokio::time::sleep(delay)=>true,
            r=next_retry(retry,tx)=> { if r { backoff.reset(); true } else { false } }
        }
    } else {
        if next_retry(retry, tx).await {
            backoff.reset();
            true
        } else {
            false
        }
    }
}
// Independent timer deadlines remain effective even when a relay emits unrelated
// notices/events continuously. Only the exact probe ID refreshes authentication.
#[cfg(test)]
async fn observe_connection(
    conn: &mut NostrWsConnection,
    keys: &nostr::Keys,
    relay: &str,
    relay_pin: &mut Option<nostr::PublicKey>,
    tx: &watch::Sender<Status>,
    retry: &mut mpsc::Receiver<Command>,
    backoff: &mut Backoff,
    policy: FreshnessPolicy,
) -> ConnectionExit {
    observe_sending(
        conn,
        keys,
        relay,
        relay_pin,
        tx,
        retry,
        backoff,
        policy,
        &mut crate::sending::Sender::new(None),
    )
    .await
}
async fn observe_sending(
    conn: &mut NostrWsConnection,
    keys: &nostr::Keys,
    relay: &str,
    relay_pin: &mut Option<nostr::PublicKey>,
    tx: &watch::Sender<Status>,
    retry: &mut mpsc::Receiver<Command>,
    backoff: &mut Backoff,
    policy: FreshnessPolicy,
    sender: &mut crate::sending::Sender,
) -> ConnectionExit {
    let result = observe_inner(
        conn, keys, relay, relay_pin, tx, retry, backoff, policy, sender,
    )
    .await;
    if let Some(delivery) = sender.unknown() {
        tx.send_modify(|s| s.delivery = delivery);
    }
    result
}
async fn observe_inner(
    conn: &mut NostrWsConnection,
    keys: &nostr::Keys,
    relay: &str,
    relay_pin: &mut Option<nostr::PublicKey>,
    tx: &watch::Sender<Status>,
    retry: &mut mpsc::Receiver<Command>,
    backoff: &mut Backoff,
    policy: FreshnessPolicy,
    sender: &mut crate::sending::Sender,
) -> ConnectionExit {
    let mut pending: Option<String> = None;
    let mut due = tokio::time::Instant::now();
    let mut catalog_due = tokio::time::Instant::now();
    let mut fresh = false;
    let mut jobs: tokio::task::JoinSet<Result<crate::catalog::Catalog, &'static str>> =
        tokio::task::JoinSet::new();
    let mut history_jobs = tokio::task::JoinSet::new();
    let mut history_ticket = 0_u64;
    let mut selected_history: Option<String> = None;
    let mut history_due = tokio::time::Instant::now();
    let mut activity = crate::activity::Tracker::default();
    let mut activity_jobs = tokio::task::JoinSet::new();
    let mut activity_due = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut activity_cursor = 0_usize;
    let mut recipient_jobs = tokio::task::JoinSet::new();
    let mut recipient_ticket = 0_u64;
    loop {
        tokio::select! {
            biased;
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
            _=tokio::time::sleep_until(sender.deadline()), if sender.is_pending()=> {
                if let Some(delivery)=sender.unknown() {tx.send_modify(|s|s.delivery=delivery);}
            },
            command=retry.recv()=>match command {
                Some(Command::Retry)=> {backoff.reset(); update(tx,"connecting",None); return ConnectionExit::Retry;},
                None=>{update(tx,"disconnected",None);return ConnectionExit::Shutdown;},
                Some(command @ (Command::Send(_) | Command::SendChecked(..)))=> {
                    let (intent,reply)=match command {Command::Send(intent)=>(intent,None),Command::SendChecked(intent,reply)=>(intent,Some(reply)),_=>unreachable!()};
                    if reply.as_ref().is_some_and(|reply|reply.is_closed()) {continue;}
                    if let Some(category)=sender.pending_error(&intent) {
                        if let Some(reply)=reply {let _=reply.send(Some(category));}
                        // Keep the active watch receipt intact for every rejected replay.
                        continue;
                    }
                    let (delivery,event)=sender.prepare(intent,relay,keys,&tx.borrow(),fresh,relay_pin.is_some());
                    if reply.is_some() && delivery.state=="failed" {
                        if let Some(reply)=reply {let _=reply.send(Some(delivery.category.as_deref().and_then(send_category).unwrap_or("send_unavailable")));}
                        continue;
                    }
                    tx.send_modify(|s|s.delivery=delivery);
                    if reply.is_some_and(|reply|reply.send(None).is_err()) && event.is_some() {
                        // Reservation completed after the caller stopped waiting. No EVENT
                        // is sent, and the durable association remains conservatively unknown.
                        if let Some(delivery)=sender.unknown() {tx.send_modify(|s|s.delivery=delivery);}
                        continue;
                    }
                    if let Some(event)=event {
                        let frame=serde_json::json!(["EVENT",event]);
                        // A dropped/timed-out write may already have reached the relay.
                        match timeout(policy.response,conn.send_raw(&frame)).await {
                            Ok(Ok(()))=>{},
                            Ok(Err(e))=>return ConnectionExit::Failure(category(&e)),
                            Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                        }
                    }
                },
                Some(Command::FetchRecipients(room))=> {
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    let status=tx.borrow();
                    let allowed=fresh && relay_pin.is_some() && status.catalog.rooms.iter().any(|r|r.id==room);
                    let generation=status.generation;drop(status);
                    let parsed=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room);
                    if !allowed || parsed.is_none() {
                        tx.send_modify(|s|s.recipients=crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_access_denied")));
                        continue;
                    }
                    tx.send_modify(|s|s.recipients=crate::protocol::RecipientsView {state:"loading".into(),..crate::protocol::RecipientsView::unavailable(Some(room.clone()),None)});
                    let ticket=recipient_ticket;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=parsed.unwrap();
                    recipient_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::recipients::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("recipients_timeout")};
                        (ticket,generation,room,result)
                    });
                },
                Some(Command::FetchRecent(room))=> {
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new();
                    history_ticket=history_ticket.wrapping_add(1);
                    selected_history=None;
                    let allowed=fresh && relay_pin.is_some() && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                    let parsed=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room);
                    if !allowed || parsed.is_none() {
                        tx.send_modify(|s|s.history=History::unavailable(Some(room),Some("history_access_denied")));
                        continue;
                    }
                    selected_history=Some(room.clone());
                    history_due=tokio::time::Instant::now()+Duration::from_secs(5);
                    tx.send_modify(|s|s.history=History {state:"loading".into(),..History::unavailable(Some(room.clone()),None)});
                    let ticket=history_ticket; let generation=tx.borrow().generation; let relay=relay.to_owned(); let keys=keys.clone(); let pin=relay_pin.unwrap(); let id=parsed.unwrap();
                    history_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::history::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("history_timeout")};
                        (ticket,generation,room,result)
                    });
                }
            },
            _=tokio::time::sleep_until(history_due), if selected_history.is_some() && fresh && history_jobs.is_empty()=> {
                let room=selected_history.as_ref().unwrap().clone();
                let allowed=relay_pin.is_some() && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                if allowed {
                    history_ticket=history_ticket.wrapping_add(1);
                    let ticket=history_ticket;let generation=tx.borrow().generation;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=uuid::Uuid::parse_str(&room).expect("selected canonical room");
                    history_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::history::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("history_timeout")};
                        (ticket,generation,room,result)
                    });
                }
                history_due=tokio::time::Instant::now()+Duration::from_secs(5);
            },
            result=recipient_jobs.join_next(), if !recipient_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    tx.send_modify(|s|s.recipients=crate::protocol::RecipientsView::unavailable(s.recipients.room_id.clone(),Some("recipients_unavailable")));
                }
                if let Some(Ok((ticket,generation,room,result)))=result {
                    let status=tx.borrow();
                    let allowed=ticket==recipient_ticket && generation==status.generation && fresh && relay_pin.is_some() && status.catalog.rooms.iter().any(|r|r.id==room);drop(status);
                    if allowed {
                        if result.as_ref().is_err_and(|error|recipients_category(error)=="recipients_access_denied") {
                            if tx.borrow().history.room_id.as_deref()==Some(room.as_str()) {
                                history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1);
                                selected_history=None;
                            }
                            if let Some(delivery)=sender.revoke_room(&room) {tx.send_modify(|s|s.delivery=delivery);}
                            activity.forget(&room);
                            tx.send_modify(|s| {
                                s.catalog.rooms.retain(|r|r.id!=room);
                                s.activity.retain(|r|r.room_id!=room);
                                if s.history.room_id.as_deref()==Some(room.as_str()) {s.history=History::unavailable(Some(room.clone()),Some("history_access_denied"));}
                            });
                        }
                        tx.send_modify(|s|s.recipients=match result {
                        Ok(r) if r.room==room=>crate::protocol::RecipientsView {state:"snapshot".into(),room_id:Some(r.room),partial:r.partial,category:None,agents:r.agents,entries:r.entries.into_iter().map(|r|crate::protocol::Recipient {key:r.key,name:r.name}).collect()},
                        Ok(_)=>crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_invalid")),
                        Err(error)=>crate::protocol::RecipientsView::unavailable(Some(room),Some(recipients_category(error))),
                    });}
                }
            },
            _=tokio::time::sleep_until(activity_due), if fresh && activity_jobs.is_empty()=> {
                activity_due=tokio::time::Instant::now()+Duration::from_secs(5);
                let status=tx.borrow();
                let rooms=&status.catalog.rooms;
                let candidate=if status.catalog.state=="partial" && !rooms.is_empty() {
                    let room=rooms[activity_cursor % rooms.len()].id.clone();
                    activity_cursor=activity_cursor.wrapping_add(1);
                    Some(room).filter(|r|selected_history.as_ref()!=Some(r))
                } else {None};
                let generation=status.generation;drop(status);
                if let (Some(room),Some(pin))=(candidate,*relay_pin) {
                    let relay=relay.to_owned();let keys=keys.clone();let id=uuid::Uuid::parse_str(&room).expect("catalog canonical room");
                    activity_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::history::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("history_timeout")};
                        (generation,room,result)
                    });
                }
            },
            result=activity_jobs.join_next(), if !activity_jobs.is_empty()=> {
                activity_due=tokio::time::Instant::now()+Duration::from_secs(5);
                if let Some(Ok((generation,room,result)))=result {
                    let allowed=fresh && generation==tx.borrow().generation && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                    if allowed {
                        match result {
                            Ok(h) if h.room==room=>activity.observe(&room,&h.rows,&keys.public_key().to_hex(),nostr::Timestamp::now().as_secs()),
                            Err("query_access_denied")=>{
                                activity.forget(&room);
                                if selected_history.as_deref()==Some(room.as_str()) {
                                    selected_history=None;
                                    history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1);
                                }
                                if tx.borrow().recipients.room_id.as_deref()==Some(room.as_str()) {
                                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                                }
                                if let Some(delivery)=sender.revoke_room(&room) {tx.send_modify(|s|s.delivery=delivery);}
                                tx.send_modify(|s| {
                                    s.catalog.rooms.retain(|r|r.id!=room);
                                    if s.history.room_id.as_deref()==Some(room.as_str()) {s.history=History::unavailable(Some(room.clone()),Some("history_access_denied"));}
                                    if s.recipients.room_id.as_deref()==Some(room.as_str()) {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room.clone()),Some("recipients_access_denied"));}
                                });
                            },
                            _=>activity.forget(&room),
                        }
                        tx.send_modify(|s|s.activity=activity.summaries());
                    }
                } else {
                    // A task panic cannot leave a stale activity claim visible.
                    activity=crate::activity::Tracker::default();tx.send_modify(|s|s.activity.clear());
                }
            },
            result=history_jobs.join_next(), if !history_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1);
                    tx.send_modify(|s|s.history=History::unavailable(s.history.room_id.clone(),Some("history_unavailable")));
                }
                if let Some(Ok((ticket,generation,room,result)))=result {
                    let allowed=ticket==history_ticket && generation==tx.borrow().generation && fresh && selected_history.as_deref()==Some(room.as_str()) && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                    if allowed {
                        history_due=tokio::time::Instant::now()+Duration::from_secs(5);
                        if result.as_ref().is_err_and(|error|history_category(error)=="history_access_denied") {
                            selected_history=None;
                            if tx.borrow().recipients.room_id.as_deref()==Some(room.as_str()) {
                                recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                            }
                            if let Some(delivery)=sender.revoke_room(&room) {tx.send_modify(|s|s.delivery=delivery);}
                            tx.send_modify(|s| {
                                s.catalog.rooms.retain(|r|r.id!=room);
                                if s.recipients.room_id.as_deref()==Some(room.as_str()) {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room.clone()),Some("recipients_access_denied"));}
                            });
                        }
                        match &result {
                            Ok(h) if h.room==room=>activity.observe(&room,&h.rows,&keys.public_key().to_hex(),nostr::Timestamp::now().as_secs()),
                            _=>activity.forget(&room),
                        }
                        tx.send_modify(|s|s.activity=activity.summaries());
                        tx.send_modify(|s|s.history=match result {
                            Ok(h) if h.room==room=>History {state:"snapshot".into(),room_id:Some(h.room),has_more:Some(h.has_more),category:Some(h.category.into()),rows:h.rows.into_iter().map(|r|crate::protocol::HistoryRow {id:r.id,author:r.author_pubkey,time:r.timestamp,text:r.text,edited:r.edited,truncated:r.truncated,unavailable:r.unavailable}).collect()},
                            Ok(_)=>History::unavailable(Some(room),Some("history_invalid")),
                            Err(error)=>History::unavailable(Some(room),Some(history_category(error))),
                        });
                    }
                }
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
                        activity.retain(&tx.borrow().catalog.rooms);
                        tx.send_modify(|s|s.activity=activity.summaries());
                        if let Some(room)=selected_history.as_ref() {
                            if tx.borrow().catalog.rooms.iter().any(|r|r.id==*room) {
                                history_due=tokio::time::Instant::now();
                            } else {
                                let room=room.clone();selected_history=None;
                                tx.send_modify(|s|s.history=History::unavailable(Some(room),Some("history_access_denied")));
                            }
                        }
                    },
                    Some(Ok(Err(error))) if fresh=>{
                        selected_history=None;
                        activity=crate::activity::Tracker::default();
                        tx.send_modify(|s|s.activity.clear());
                        tx.send_modify(|s|{s.catalog=crate::protocol::Catalog::unavailable(Some(catalog_category(error)));s.history=History::unavailable(None,None);});
                    },
                    Some(Err(error)) if fresh && !error.is_cancelled()=>{
                        selected_history=None;
                        activity=crate::activity::Tracker::default();
                        tx.send_modify(|s|s.activity.clear());
                        tx.send_modify(|s|{s.catalog=crate::protocol::Catalog::unavailable(Some("room_catalog_unavailable"));s.history=History::unavailable(None,None);});
                    },
                    _=>{},
                }
                if !cancelled {catalog_due=tokio::time::Instant::now()+Duration::from_secs(30);}
            },
            _=tokio::time::sleep_until(catalog_due), if fresh && jobs.is_empty()=> {
                history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1);
                activity_jobs.abort_all();activity_jobs=tokio::task::JoinSet::new();
                tx.send_modify(|s|s.activity.clear());
                recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                tx.send_modify(|s| {s.catalog=crate::protocol::Catalog::loading();s.history=History::unavailable(None,None);s.recipients=crate::protocol::RecipientsView::unavailable(None,None);});
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
                    if let Some(delivery)=sender.unknown() {tx.send_modify(|s|s.delivery=delivery);}
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    fresh=false;
                    jobs.abort_all();
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1);
                    selected_history=None;
                    activity_jobs.abort_all();activity_jobs=tokio::task::JoinSet::new();
                    activity=crate::activity::Tracker::default();
                    update(tx,"connecting",None);
                    match timeout(Duration::from_secs(25),conn.authenticate(keys,None)).await {
                        Ok(Ok(()))=> { pending=None; due=tokio::time::Instant::now(); },
                        Ok(Err(e))=>return ConnectionExit::Failure(category(&e)),
                        Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                    }
                },
                Ok(RelayMessage::Ok(ok))=> {if let Some(delivery)=sender.acknowledge(&ok.event_id,ok.accepted) {tx.send_modify(|s|s.delivery=delivery);}},
                Ok(_)|Err(WsClientError::Timeout)=>{},
                Err(e)=>return ConnectionExit::Failure(category(&e)),
            }
        }
    }
}

pub async fn run(
    _initial: config::Config,
    tx: watch::Sender<Status>,
    mut retry: mpsc::Receiver<Command>,
) {
    let ledger = crate::ledger::default_path()
        .ok()
        .and_then(|path| crate::ledger::Ledger::open(path).ok());
    let mut sender = crate::sending::Sender::new(ledger);
    let mut backoff = Backoff::default();
    let mut sending_scope: Option<(Option<String>, Option<String>)> = None;
    let mut pin_origin: Option<String> = None;
    let mut relay_pin: Option<nostr::PublicKey> = None;
    loop {
        // Coalesce retry requests already queued for this attempt. Only this
        // sequential task owns key lookups, including any pending prompt.
        while let Ok(command) = retry.try_recv() {
            if offline_command(command, &tx) {
                backoff.reset();
            }
        }
        let c = match apply_loaded_config(&tx, config::load()) {
            Ok(c) => c,
            Err(()) => {
                if !next_retry(&mut retry, &tx).await {
                    return;
                }
                backoff.reset();
                continue;
            }
        };
        let scope = (c.relay.clone(), c.identity.clone());
        if sending_scope.as_ref() != Some(&scope) {
            sender.clear_scope();
            sending_scope = Some(scope);
        }
        if pin_origin != c.relay {
            pin_origin = c.relay.clone();
            relay_pin = None;
        }
        if c.relay.is_none() || c.identity.is_none() {
            update(&tx, "unconfigured", None);
            if !next_retry(&mut retry, &tx).await {
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
        let mut retry_requested = false;
        while let Ok(command) = retry.try_recv() {
            if offline_command(command, &tx) {
                retry_requested = true;
            }
        }
        if retry_requested {
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
                if !next_retry(&mut retry, &tx).await {
                    return;
                }
                backoff.reset();
                continue;
            }
            Err(_) => {
                update(&tx, "unavailable", Some("identity_unavailable"));
                if !next_retry(&mut retry, &tx).await {
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
                if !wait_after_failure(error, &mut backoff, &mut retry, &tx).await {
                    return;
                }
                continue;
            }
        };
        let exit = observe_sending(
            &mut conn,
            &keys,
            relay,
            &mut relay_pin,
            &tx,
            &mut retry,
            &mut backoff,
            FRESHNESS,
            &mut sender,
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
                if !wait_after_failure(error, &mut backoff, &mut retry, &tx).await {
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
        tx.send_modify(|s| {
            s.delivery = crate::protocol::Delivery {
                request_id: Some("old-request".into()),
                room_id: Some("old-room".into()),
                event_id: Some("old-event".into()),
                state: "acknowledged".into(),
                category: None,
            }
        });
        let c = fixture(Some("wss://example.com/"), None);
        assert!(apply_loaded_config(&tx, Ok(c.clone())).is_ok());
        assert_eq!(rx.borrow().generation, 2);
        assert_eq!(rx.borrow().delivery.state, "idle");
        assert!(rx.borrow().delivery.event_id.is_none());
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

#[cfg(test)]
#[path = "auth_history_tests.rs"]
mod history_integration_tests;

#[cfg(test)]
#[path = "auth_send_tests.rs"]
mod send_tests;

#[cfg(test)]
#[path = "auth_send_integration_tests.rs"]
mod send_integration_tests;

#[cfg(test)]
#[path = "auth_activity_tests.rs"]
mod activity_integration_tests;
