use crate::{
    config,
    protocol::{Command, History, Status, Thread},
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
// Keep related views in one transaction: IPC may observe every publication.
fn publish_status(tx: &watch::Sender<Status>, change: impl FnOnce(&mut Status)) {
    tx.send_modify(|status| {
        change(status);
        // `live` describes a shown snapshot; it can never outlive one.
        if status.history.state != "snapshot" {
            status.history.live = false;
        }
        if status.thread.root_id.is_some()
            && (status.history.state != "snapshot"
                || status.thread.room_id != status.history.room_id
                || !status.thread.root_id.as_ref().is_some_and(|root| {
                    status
                        .history
                        .rows
                        .iter()
                        .any(|row| &row.id == root && !row.unavailable)
                })
                || !status
                    .thread
                    .room_id
                    .as_ref()
                    .is_some_and(|room| status.catalog.rooms.iter().any(|entry| &entry.id == room)))
        {
            status.thread = Thread::unavailable(None, None, None);
        }
        #[cfg(test)]
        assert!(
            status.activity.iter().all(|entry| status
                .catalog
                .rooms
                .iter()
                .any(|room| room.id == entry.room_id)),
            "published activity references an inaccessible room"
        );
    });
}
fn update(tx: &watch::Sender<Status>, state: &str, category: Option<&str>) {
    publish_status(tx, |s| {
        s.connection = state.into();
        s.category = category.map(str::to_owned);
        if state != "authenticated" {
            s.catalog = crate::protocol::Catalog::unavailable(None);
            s.history = History::unavailable(None, None);
            s.thread = Thread::unavailable(None, None, None);
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
pub(crate) async fn connect_identity(
    relay: &str,
    keys: &nostr::Keys,
) -> Result<NostrWsConnection, &'static str> {
    connect_attested(relay, keys, None).await
}
/// `connect_identity`, optionally presenting a NIP-OA `auth` tag in the AUTH
/// event (an agent key admitted through its owner).
pub(crate) async fn connect_attested(
    relay: &str,
    keys: &nostr::Keys,
    auth_tag: Option<&nostr::Tag>,
) -> Result<NostrWsConnection, &'static str> {
    match timeout(
        Duration::from_secs(45),
        NostrWsConnection::connect_authenticated(relay, keys, auth_tag),
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
            publish_status(tx, |s| {
                if s.relay != c.relay || s.identity != c.identity {
                    s.relay = c.relay.clone();
                    s.identity = c.identity.clone();
                    s.generation = s.generation.saturating_add(1);
                    s.delivery = crate::protocol::Delivery::default();
                    s.dm_open = crate::protocol::DmOpen::default();
                    s.connection = "unconfigured".into();
                    s.category = None;
                    s.catalog = crate::protocol::Catalog::unavailable(None);
                    s.history = History::unavailable(None, None);
                    s.thread = Thread::unavailable(None, None, None);
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
    // Cadence of the joined-room check after each completed discovery.
    catalog: Duration,
    // Selected-room head poll without a primed live subscription.
    head: Duration,
    // Head poll, and the helper's own open-thread refresh, while the live
    // subscription is primed. Polling stays the safety net for missed events.
    live_poll: Duration,
}
const FRESHNESS: FreshnessPolicy = FreshnessPolicy {
    interval: Duration::from_secs(20),
    response: Duration::from_secs(5),
    catalog: Duration::from_secs(30),
    head: Duration::from_secs(5),
    live_poll: Duration::from_secs(30),
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
        Command::OpenDm(_, reply) => {
            let _ = reply.send(Some("dm_open_unavailable"));
        }
        Command::Send(intent) => publish_status(tx, |s| {
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
fn thread_category(error: &str) -> &'static str {
    match error {
        "thread_timeout" | "query_timeout" => "thread_timeout",
        "query_access_denied" => "thread_access_denied",
        e if e.starts_with("thread_")
            || matches!(
                e,
                "query_oversized"
                    | "query_invalid_response"
                    | "query_invalid_signature"
                    | "query_invalid_scope"
                    | "query_redirect_rejected"
            ) =>
        {
            "thread_invalid"
        }
        _ => "thread_unavailable",
    }
}
fn thread_allowed(
    status: &Status,
    selected_history: Option<&str>,
    room: &str,
    root: &str,
    fresh: bool,
    pinned: bool,
) -> bool {
    fresh
        && pinned
        && selected_history == Some(room)
        && status.catalog.rooms.iter().any(|r| r.id == room)
        && status.history.state == "snapshot"
        && status.history.room_id.as_deref() == Some(room)
        && status
            .history
            .rows
            .iter()
            .any(|r| r.id == root && !r.unavailable)
}
fn thread_result_allowed(
    status: &Status,
    selected_history: Option<&str>,
    room: &str,
    root: &str,
    fresh: bool,
    pinned: bool,
    ticket: u64,
    current_ticket: u64,
    generation: u64,
) -> bool {
    ticket == current_ticket
        && generation == status.generation
        && thread_allowed(status, selected_history, room, root, fresh, pinned)
        && status.thread.room_id.as_deref() == Some(room)
        && status.thread.root_id.as_deref() == Some(root)
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
    // A DM open cannot outlive its connection: its answer would arrive on this socket.
    let mut opener = crate::dm_open::Opener::default();
    let mut live = crate::live::Live::default();
    let result = observe_inner(
        conn,
        keys,
        relay,
        relay_pin,
        tx,
        retry,
        backoff,
        policy,
        sender,
        &mut opener,
        &mut live,
    )
    .await;
    // Every exit (Retry, shutdown, failure) closes the live subscription first,
    // under a short deadline; a dead socket is dropped by the caller anyway.
    if let Some(close) = live.close() {
        let _ = timeout(Duration::from_secs(1), conn.send_raw(&close)).await;
    }
    if let Some(delivery) = sender.unknown() {
        publish_status(tx, |s| s.delivery = delivery);
    }
    if let Some(view) = opener.unknown() {
        publish_status(tx, |s| s.dm_open = view);
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
    opener: &mut crate::dm_open::Opener,
    live: &mut crate::live::Live,
) -> ConnectionExit {
    let mut pending: Option<String> = None;
    let mut due = tokio::time::Instant::now();
    let mut catalog_due = tokio::time::Instant::now();
    let mut fresh = false;
    let mut jobs: tokio::task::JoinSet<Result<crate::catalog::Catalog, &'static str>> =
        tokio::task::JoinSet::new();
    let mut history_jobs = tokio::task::JoinSet::new();
    let mut history_ticket = 0_u64;
    let mut thread_jobs = tokio::task::JoinSet::new();
    let mut thread_ticket = 0_u64;
    let mut selected_history: Option<String> = None;
    let mut history_due = tokio::time::Instant::now();
    let mut activity = crate::activity::Tracker::default();
    let mut activity_jobs = tokio::task::JoinSet::new();
    let mut activity_due = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut activity_cursor = 0_usize;
    let mut recipient_jobs = tokio::task::JoinSet::new();
    let mut recipient_ticket = 0_u64;
    // Older pages held for the selected room. Every path that resets that
    // room's history (`drop_older!`) discards them and any in-flight older read.
    let mut held = crate::history::Held::default();
    let mut older_jobs = tokio::task::JoinSet::new();
    let mut older_ticket = 0_u64;
    macro_rules! drop_older {
        () => {{
            older_jobs.abort_all();
            older_jobs = tokio::task::JoinSet::new();
            older_ticket = older_ticket.wrapping_add(1);
            held = crate::history::Held::default();
        }};
    }
    // Live triggers (see `live.rs`) only schedule these refetches through the
    // verified page paths; `last_*` is when the latest read of each started.
    let mut head_refetch: Option<tokio::time::Instant> = None;
    let mut thread_refetch: Option<tokio::time::Instant> = None;
    let mut last_head: Option<tokio::time::Instant> = None;
    let mut last_thread: Option<tokio::time::Instant> = None;
    macro_rules! send_frame {
        ($frame:expr) => {{
            match timeout(policy.response, conn.send_raw(&$frame)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return ConnectionExit::Failure(category(&e)),
                Err(_) => return ConnectionExit::Failure("relay_timeout"),
            }
        }};
    }
    macro_rules! close_live {
        () => {{
            thread_refetch = None;
            if let Some(close) = live.close() {
                publish_status(tx, |s| s.history.live = false);
                send_frame!(close);
            }
        }};
    }
    macro_rules! project {
        () => {
            held.project().map(|mut view| {
                view.live = live.primed_for(view.room_id.as_deref());
                view
            })
        };
    }
    macro_rules! head_interval {
        ($room:expr) => {
            if live.primed_for(Some(($room).as_str())) {
                policy.live_poll
            } else {
                policy.head
            }
        };
    }
    macro_rules! spawn_head {
        ($room:expr) => {{
            let room: String = $room;
            history_ticket = history_ticket.wrapping_add(1);
            head_refetch = None;
            last_head = Some(tokio::time::Instant::now());
            let ticket = history_ticket;
            let generation = tx.borrow().generation;
            let relay = relay.to_owned();
            let keys = keys.clone();
            let pin = relay_pin.unwrap();
            let id = uuid::Uuid::parse_str(&room).expect("selected canonical room");
            history_jobs.spawn(async move {
                let result = match timeout(
                    Duration::from_secs(15),
                    crate::history::fetch(&relay, &keys, pin, id),
                )
                .await
                {
                    Ok(r) => r,
                    Err(_) => Err("history_timeout"),
                };
                (ticket, generation, room, result)
            });
        }};
    }
    loop {
        // Only the selected room of a fresh session may hold the live
        // subscription; every path that drops the selection closes it here.
        if live.room().is_some() && (!fresh || selected_history.as_deref() != live.room()) {
            close_live!();
        }
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
                if let Some(delivery)=sender.unknown() {publish_status(tx, |s|s.delivery=delivery);}
            },
            _=tokio::time::sleep_until(opener.deadline()), if opener.is_pending()=> {
                if let Some(view)=opener.unknown() {publish_status(tx, |s|s.dm_open=view);}
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
                    if let Some(root)=intent.root_id.as_deref() {
                        let status=tx.borrow();
                        if !thread_allowed(&status,selected_history.as_deref(),&intent.room,root,fresh,relay_pin.is_some())
                            || status.thread.state!="snapshot"
                            || status.thread.room_id.as_deref()!=Some(intent.room.as_str())
                            || status.thread.root_id.as_deref()!=Some(root) {
                            if let Some(reply)=reply {let _=reply.send(Some("send_access_denied"));}
                            continue;
                        }
                    }
                    let (delivery,event)=sender.prepare(intent,relay,keys,&tx.borrow(),fresh,relay_pin.is_some());
                    if reply.is_some() && delivery.state=="failed" {
                        if let Some(reply)=reply {let _=reply.send(Some(delivery.category.as_deref().and_then(send_category).unwrap_or("send_unavailable")));}
                        continue;
                    }
                    publish_status(tx, |s|s.delivery=delivery);
                    if reply.is_some_and(|reply|reply.send(None).is_err()) && event.is_some() {
                        // Reservation completed after the caller stopped waiting. No EVENT
                        // is sent, and the durable association remains conservatively unknown.
                        if let Some(delivery)=sender.unknown() {publish_status(tx, |s|s.delivery=delivery);}
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
                Some(Command::OpenDm(intent,reply))=> {
                    if reply.is_closed() {continue;}
                    let prepared={let status=tx.borrow();opener.prepare(&intent,keys,&status,fresh,relay_pin.is_some())};
                    let (view,event)=match prepared {
                        // An identical replay keeps the pending receipt.
                        Ok(None)=>{let _=reply.send(None);continue;},
                        Ok(Some(prepared))=>prepared,
                        Err(category)=>{let _=reply.send(Some(category));continue;},
                    };
                    if reply.send(None).is_err() {
                        // Nothing was written: the caller left before publication.
                        opener.abandon();
                        continue;
                    }
                    publish_status(tx, |s|s.dm_open=view);
                    let frame=serde_json::json!(["EVENT",event]);
                    // A dropped/timed-out write may already have reached the relay.
                    match timeout(policy.response,conn.send_raw(&frame)).await {
                        Ok(Ok(()))=>{},
                        Ok(Err(e))=>return ConnectionExit::Failure(category(&e)),
                        Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                    }
                },
                Some(Command::FetchRecipients(room))=> {
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    let status=tx.borrow();
                    let allowed=fresh && relay_pin.is_some() && status.catalog.rooms.iter().any(|r|r.id==room);
                    let generation=status.generation;drop(status);
                    let parsed=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room);
                    if !allowed || parsed.is_none() {
                        publish_status(tx, |s|s.recipients=crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_access_denied")));
                        continue;
                    }
                    publish_status(tx, |s|s.recipients=crate::protocol::RecipientsView {state:"loading".into(),..crate::protocol::RecipientsView::unavailable(Some(room.clone()),None)});
                    let ticket=recipient_ticket;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=parsed.unwrap();
                    recipient_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::recipients::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("recipients_timeout")};
                        (ticket,generation,room,result)
                    });
                },
                Some(Command::CloseThread)=> {
                    thread_refetch=None;
                    thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);
                    publish_status(tx,|s|s.thread=Thread::unavailable(None,None,None));
                },
                Some(Command::FetchThread(room,root))=> {
                    thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);
                    let status=tx.borrow();
                    let allowed=thread_allowed(&status,selected_history.as_deref(),&room,&root,fresh,relay_pin.is_some());
                    drop(status);
                    let parsed=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room);
                    let valid_root=root.len()==64 && root.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b));
                    if !allowed || parsed.is_none() || !valid_root {
                        publish_status(tx,|s|s.thread=Thread::unavailable(None,None,Some("thread_access_denied")));
                        continue;
                    }
                    publish_status(tx,|s|s.thread=Thread{state:"loading".into(),..Thread::unavailable(Some(room.clone()),Some(root.clone()),None)});
                    let ticket=thread_ticket;let generation=tx.borrow().generation;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=parsed.unwrap();
                    last_thread=Some(tokio::time::Instant::now());
                    thread_jobs.spawn(async move {
                        // Up to four sequential pages (each query is capped at 10 s); abort semantics are unchanged.
                        let result=match timeout(Duration::from_secs(20),crate::thread::fetch(&relay,&keys,pin,id,&root)).await {Ok(r)=>r,Err(_)=>Err("thread_timeout")};
                        (ticket,generation,room,root,result)
                    });
                },
                Some(Command::FetchRecent(room))=> {
                    // Re-selecting (the same or another room) re-issues the live
                    // subscription after the new head page, as on reconnect.
                    close_live!();
                    live.select(&room);
                    thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);
                    publish_status(tx,|s|s.thread=Thread::unavailable(None,None,None));
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new();
                    history_ticket=history_ticket.wrapping_add(1); drop_older!();
                    selected_history=None;
                    let allowed=fresh && relay_pin.is_some() && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                    let parsed=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room);
                    if !allowed || parsed.is_none() {
                        publish_status(tx, |s|s.history=History::unavailable(Some(room),Some("history_access_denied")));
                        continue;
                    }
                    selected_history=Some(room.clone());
                    history_due=tokio::time::Instant::now()+policy.head;
                    head_refetch=None;last_head=Some(tokio::time::Instant::now());
                    publish_status(tx, |s|s.history=History {state:"loading".into(),..History::unavailable(Some(room.clone()),None)});
                    let ticket=history_ticket; let generation=tx.borrow().generation; let relay=relay.to_owned(); let keys=keys.clone(); let pin=relay_pin.unwrap(); let id=parsed.unwrap();
                    history_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::history::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("history_timeout")};
                        (ticket,generation,room,result)
                    });
                },
                Some(Command::FetchOlder(room))=> {
                    // One older page at a time, only for the selected room's current
                    // snapshot and only from the signed cursor the helper holds.
                    let status=tx.borrow();
                    let allowed=fresh && relay_pin.is_some() && selected_history.as_deref()==Some(room.as_str())
                        && status.catalog.state=="partial" && status.catalog.rooms.iter().any(|r|r.id==room)
                        && status.history.state=="snapshot" && status.history.room_id.as_deref()==Some(room.as_str());
                    let generation=status.generation;drop(status);
                    let Some(cursor)=held.continuation().cloned().filter(|_|allowed && older_jobs.is_empty()) else {continue;};
                    held.older_state="loading";
                    if let Some(view)=project!() {publish_status(tx,|s|s.history=view);}
                    let ticket=older_ticket;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=uuid::Uuid::parse_str(&room).expect("selected canonical room");
                    older_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::history::fetch_older(&relay,&keys,pin,id,&cursor)).await {Ok(r)=>r,Err(_)=>Err("history_timeout")};
                        (ticket,generation,room,cursor,result)
                    });
                }
            },
            _=tokio::time::sleep_until(history_due), if selected_history.is_some() && fresh && history_jobs.is_empty()=> {
                let room=selected_history.as_ref().unwrap().clone();
                let allowed=relay_pin.is_some() && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                history_due=tokio::time::Instant::now()+head_interval!(&room);
                if allowed {spawn_head!(room);}
            },
            // A verified live event for the selected room: refetch its head page.
            _=tokio::time::sleep_until(head_refetch.unwrap_or(history_due)), if head_refetch.is_some() && selected_history.is_some() && fresh && history_jobs.is_empty()=> {
                head_refetch=None;
                let room=selected_history.as_ref().unwrap().clone();
                let allowed=relay_pin.is_some() && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                if allowed {
                    history_due=tokio::time::Instant::now()+head_interval!(&room);
                    spawn_head!(room);
                }
            },
            // Helper-initiated open-thread refresh: after a live event with an `e`
            // tag, and every `live_poll` while live. Same scope checks as a
            // `fetch_thread` request; the shown snapshot stays until the result.
            _=tokio::time::sleep_until(thread_refetch.unwrap_or(history_due)), if thread_refetch.is_some() && fresh && thread_jobs.is_empty()=> {
                thread_refetch=None;
                let scope={
                    let status=tx.borrow();
                    match (status.thread.room_id.as_deref(),status.thread.root_id.as_deref()) {
                        (Some(room),Some(root)) if status.thread.state=="snapshot" && thread_allowed(&status,selected_history.as_deref(),room,root,fresh,relay_pin.is_some())=>Some((room.to_owned(),root.to_owned(),status.generation)),
                        _=>None,
                    }
                };
                let Some((room,root,generation))=scope else {continue;};
                let Some(id)=uuid::Uuid::parse_str(&room).ok().filter(|id|id.to_string()==room) else {continue;};
                thread_ticket=thread_ticket.wrapping_add(1);last_thread=Some(tokio::time::Instant::now());
                let ticket=thread_ticket;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();
                thread_jobs.spawn(async move {
                    let result=match timeout(Duration::from_secs(20),crate::thread::fetch(&relay,&keys,pin,id,&root)).await {Ok(r)=>r,Err(_)=>Err("thread_timeout")};
                    (ticket,generation,room,root,result)
                });
            },
            result=recipient_jobs.join_next(), if !recipient_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    publish_status(tx, |s|s.recipients=crate::protocol::RecipientsView::unavailable(s.recipients.room_id.clone(),Some("recipients_unavailable")));
                }
                if let Some(Ok((ticket,generation,room,result)))=result {
                    let status=tx.borrow();
                    let allowed=ticket==recipient_ticket && generation==status.generation && fresh && relay_pin.is_some() && status.catalog.rooms.iter().any(|r|r.id==room);drop(status);
                    if allowed {
                        let denied=result.as_ref().is_err_and(|error|recipients_category(error)=="recipients_access_denied");
                        let mut revoked_delivery=None;
                        if denied {
                            if tx.borrow().history.room_id.as_deref()==Some(room.as_str()) {
                                history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                                selected_history=None;
                            }
                            revoked_delivery=sender.revoke_room(&room);
                            activity.forget(&room);
                        }
                        publish_status(tx, |s| {
                            if denied {
                                s.catalog.rooms.retain(|r|r.id!=room);
                                s.activity=activity.summaries();
                                if s.history.room_id.as_deref()==Some(room.as_str()) {s.history=History::unavailable(Some(room.clone()),Some("history_access_denied"));}
                            }
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                            s.recipients=match result {
                        Ok(r) if r.room==room=>crate::protocol::RecipientsView {state:"snapshot".into(),room_id:Some(r.room),partial:r.partial,category:None,agents:r.agents,entries:r.entries.into_iter().map(|r|crate::protocol::Recipient {key:r.key,name:r.name}).collect()},
                        Ok(_)=>crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_invalid")),
                        Err(error)=>crate::protocol::RecipientsView::unavailable(Some(room),Some(recipients_category(error))),
                    };});}
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
                        let denied=matches!(&result,Err("query_access_denied"));
                        let mut revoked_delivery=None;
                        match result {
                            Ok(h) if h.room==room=>activity.observe(&room,&h.rows,&keys.public_key().to_hex(),nostr::Timestamp::now().as_secs()),
                            Err("query_access_denied")=>{
                                activity.forget(&room);
                                if selected_history.as_deref()==Some(room.as_str()) {
                                    selected_history=None;
                                    history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                                }
                                if tx.borrow().recipients.room_id.as_deref()==Some(room.as_str()) {
                                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                                }
                                revoked_delivery=sender.revoke_room(&room);
                            },
                            _=>activity.forget(&room),
                        }
                        publish_status(tx, |s| {
                            if denied {
                                s.catalog.rooms.retain(|r|r.id!=room);
                                if s.history.room_id.as_deref()==Some(room.as_str()) {s.history=History::unavailable(Some(room.clone()),Some("history_access_denied"));}
                                if s.recipients.room_id.as_deref()==Some(room.as_str()) {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room.clone()),Some("recipients_access_denied"));}
                            }
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                            s.activity=activity.summaries();
                        });
                    }
                } else {
                    // A task panic cannot leave a stale activity claim visible.
                    activity=crate::activity::Tracker::default();publish_status(tx, |s|s.activity.clear());
                }
            },
            result=thread_jobs.join_next(), if !thread_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);
                    let scope={
                        let status=tx.borrow();
                        match (status.thread.room_id.as_deref(),status.thread.root_id.as_deref()) {
                            (Some(room),Some(root)) if thread_allowed(&status,selected_history.as_deref(),room,root,fresh,relay_pin.is_some()) => Some((room.to_owned(),root.to_owned())),
                            _=>None,
                        }
                    };
                    publish_status(tx,|s|s.thread=match scope {
                        Some((room,root))=>Thread::unavailable(Some(room),Some(root),Some("thread_unavailable")),
                        None=>Thread::unavailable(None,None,None),
                    });
                }
                if let Some(Ok((ticket,generation,room,root,result)))=result {
                    let status=tx.borrow();
                    let allowed=thread_result_allowed(&status,selected_history.as_deref(),&room,&root,fresh,relay_pin.is_some(),ticket,thread_ticket,generation);
                    drop(status);
                    if allowed {
                        if result.as_ref().is_err_and(|error|thread_category(error)=="thread_access_denied") {
                            selected_history=None;
                            history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                            let revoked_delivery=sender.revoke_room(&room);
                            publish_status(tx,|s| {
                                s.catalog.rooms.retain(|entry|entry.id!=room);
                                s.history=History::unavailable(Some(room),Some("history_access_denied"));
                                s.thread=Thread::unavailable(None,None,Some("thread_access_denied"));
                                if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                            });
                            continue;
                        }
                        publish_status(tx,|s|s.thread=match result {
                            Ok(thread) if thread.room==room && thread.root==root && thread.rows.len()<=crate::thread::ROWS=>Thread {
                                state:"snapshot".into(),room_id:Some(room.clone()),root_id:Some(root.clone()),
                                has_more:Some(thread.has_more),category:Some(thread.category.into()),
                                rows:thread.rows.into_iter().map(|r|crate::protocol::ThreadRow{depth:r.depth,parent:r.parent,row:crate::protocol::HistoryRow{ reactions:None,thread:None,id:r.id,author:r.author_pubkey,time:r.timestamp,text:r.text,edited:r.edited,truncated:r.truncated,unavailable:r.unavailable}}).collect(),
                            },
                            Ok(_)=>Thread::unavailable(Some(room.clone()),Some(root.clone()),Some("thread_invalid")),
                            Err(error)=> {
                                // Query/reducer errors are static categories, never relay payloads or credentials.
                                eprintln!("omarchy-buzz: thread read failed: {error}");
                                Thread::unavailable(Some(room.clone()),Some(root.clone()),Some(thread_category(error)))
                            },
                        });
                        if live.primed_for(Some(room.as_str())) && tx.borrow().thread.state=="snapshot" && thread_refetch.is_none() {
                            thread_refetch=Some(tokio::time::Instant::now()+policy.live_poll);
                        }
                    }
                }
            },
            result=older_jobs.join_next(), if !older_jobs.is_empty()=>match result {
                Some(Ok((ticket,generation,room,cursor,result)))=> {
                    let status=tx.borrow();
                    let allowed=ticket==older_ticket && generation==status.generation && fresh && selected_history.as_deref()==Some(room.as_str())
                        && status.catalog.state=="partial" && status.catalog.rooms.iter().any(|r|r.id==room)
                        && status.history.state=="snapshot" && status.history.room_id.as_deref()==Some(room.as_str());
                    drop(status);
                    if !allowed {continue;}
                    if result.as_ref().is_err_and(|error|history_category(error)=="history_access_denied") {
                        // As for a denied head read: the room is revoked everywhere.
                        selected_history=None;
                        history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                        if tx.borrow().recipients.room_id.as_deref()==Some(room.as_str()) {
                            recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                        }
                        activity.forget(&room);
                        let revoked_delivery=sender.revoke_room(&room);
                        publish_status(tx,|s| {
                            s.catalog.rooms.retain(|r|r.id!=room);
                            s.activity=activity.summaries();
                            s.history=History::unavailable(Some(room.clone()),Some("history_access_denied"));
                            if s.recipients.room_id.as_deref()==Some(room.as_str()) {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room.clone()),Some("recipients_access_denied"));}
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                        });
                        continue;
                    }
                    // A stale page (the cursor moved meanwhile) is discarded quietly.
                    // Any other failure keeps the rows shown and reports the attempt.
                    let outcome=result.and_then(|page|held.older(&cursor,page));
                    held.older_state=match outcome {
                        Ok(())|Err("history_stale_cursor")=>"idle",
                        Err(error)=> {eprintln!("omarchy-buzz: older history read failed: {error}");"unavailable"},
                    };
                    if let Some(view)=project!() {publish_status(tx,|s|s.history=view);}
                },
                Some(Err(error)) if !error.is_cancelled()=> {
                    held.older_state="unavailable";
                    if let Some(view)=project!() {publish_status(tx,|s|s.history=view);}
                },
                _=>{},
            },
            result=history_jobs.join_next(), if !history_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1); drop_older!();
                    publish_status(tx, |s|s.history=History::unavailable(s.history.room_id.clone(),Some("history_unavailable")));
                }
                if let Some(Ok((ticket,generation,room,result)))=result {
                    let allowed=ticket==history_ticket && generation==tx.borrow().generation && fresh && selected_history.as_deref()==Some(room.as_str()) && tx.borrow().catalog.state=="partial" && tx.borrow().catalog.rooms.iter().any(|r|r.id==room);
                    if allowed {
                        history_due=tokio::time::Instant::now()+head_interval!(&room);
                        let fetched=matches!(&result,Ok(h) if h.room==room);
                        let denied=result.as_ref().is_err_and(|error|history_category(error)=="history_access_denied");
                        let mut revoked_delivery=None;
                        if denied {
                            selected_history=None;
                            if tx.borrow().recipients.room_id.as_deref()==Some(room.as_str()) {
                                recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                            }
                            revoked_delivery=sender.revoke_room(&room);
                        }
                        match &result {
                            Ok(h) if h.room==room=>activity.observe(&room,&h.rows,&keys.public_key().to_hex(),nostr::Timestamp::now().as_secs()),
                            _=>activity.forget(&room),
                        }
                        // A new head is reconciled with held older pages; any error drops them.
                        let projected=match result {
                            Ok(h) if h.room==room=> {held.head(h);project!().expect("head just held")},
                            Ok(_)=> {drop_older!();History::unavailable(Some(room.clone()),Some("history_invalid"))},
                            Err(error)=> {drop_older!();History::unavailable(Some(room.clone()),Some(history_category(error)))},
                        };
                        publish_status(tx, |s| {
                            if denied {
                                s.catalog.rooms.retain(|r|r.id!=room);
                                if s.recipients.room_id.as_deref()==Some(room.as_str()) {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room.clone()),Some("recipients_access_denied"));}
                            }
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                            s.activity=activity.summaries();
                            s.history=projected;
                        });
                        // The live subscription is (re)armed only after a verified
                        // head page for the selected room in a fresh session.
                        if fetched && fresh && live.room().is_none() && live.can_arm(tokio::time::Instant::now()) {
                            let request=live.arm(&room,nostr::Timestamp::now().as_secs());
                            send_frame!(request);
                        }
                    }
                }
            },
            result=jobs.join_next(), if !jobs.is_empty()=> {
                let cancelled=matches!(&result,Some(Err(error)) if error.is_cancelled());
                match result {
                    Some(Ok(Ok(catalog))) if fresh=> {
                        *relay_pin=Some(catalog.signer);
                        let next_catalog=crate::protocol::Catalog {
                            state:catalog.state.into(),category:Some(catalog.category.into()),
                            rooms:catalog.rooms.into_iter().map(|r|crate::protocol::Room {id:r.id,name:r.name,description:r.description,kind:r.kind.into(),participants:r.participants,hidden:r.hidden}).collect(),
                        };
                        // A room that left the joined set loses its views, jobs and pending
                        // delivery as an access denial would; remaining rooms keep theirs.
                        let (removed,lost_recipients,lost_thread)={
                            let status=tx.borrow();
                            let removed:Vec<String>=status.catalog.rooms.iter().filter(|r|!next_catalog.rooms.iter().any(|n|n.id==r.id)).map(|r|r.id.clone()).collect();
                            let lost_recipients=status.recipients.room_id.clone().filter(|room|removed.contains(room));
                            let lost_thread=status.thread.room_id.as_ref().is_some_and(|room|removed.contains(room));
                            (removed,lost_recipients,lost_thread)
                        };
                        activity.retain(&next_catalog.rooms);
                        let removed_selection=selected_history.as_ref().filter(|room|!next_catalog.rooms.iter().any(|r|r.id==**room)).cloned();
                        if removed_selection.is_some() {
                            selected_history=None;
                            history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                        }
                        if lost_recipients.is_some() {recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);}
                        if lost_thread {thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);}
                        // The single in-flight activity read does not record its room.
                        if !removed.is_empty() {activity_jobs.abort_all();activity_jobs=tokio::task::JoinSet::new();}
                        let mut revoked_delivery=None;
                        for room in &removed {
                            if let Some(delivery)=sender.revoke_room(room) {revoked_delivery=Some(delivery);}
                        }
                        // Publish the catalog and all dependent views under one watch lock.
                        publish_status(tx, |s| {
                            s.catalog=next_catalog;
                            s.activity=activity.summaries();
                            if let Some(room)=removed_selection {s.history=History::unavailable(Some(room),Some("history_access_denied"));}
                            else if let Some(room)=s.history.room_id.clone().filter(|room|removed.contains(room)) {s.history=History::unavailable(Some(room),Some("history_access_denied"));}
                            if let Some(room)=lost_recipients {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_access_denied"));}
                            if lost_thread {s.thread=Thread::unavailable(None,None,Some("thread_access_denied"));}
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                        });
                    },
                    failed @ (Some(Ok(Err(_)))|Some(Err(_))) if fresh && !cancelled=>{
                        let category=match failed {Some(Ok(Err(error)))=>catalog_category(error),_=>"room_catalog_unavailable"};
                        // A failed check proves nothing about any room: clear every dependent view.
                        selected_history=None;
                        history_jobs.abort_all();history_jobs=tokio::task::JoinSet::new();history_ticket=history_ticket.wrapping_add(1); drop_older!();
                        recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                        thread_jobs.abort_all();thread_jobs=tokio::task::JoinSet::new();thread_ticket=thread_ticket.wrapping_add(1);
                        activity_jobs.abort_all();activity_jobs=tokio::task::JoinSet::new();
                        activity=crate::activity::Tracker::default();
                        publish_status(tx, |s|{
                            s.activity.clear();s.catalog=crate::protocol::Catalog::unavailable(Some(category));s.history=History::unavailable(None,None);
                            s.recipients=crate::protocol::RecipientsView::unavailable(None,None);s.thread=Thread::unavailable(None,None,None);
                        });
                    },
                    _=>{},
                }
                if !cancelled {catalog_due=tokio::time::Instant::now()+policy.catalog;}
            },
            _=tokio::time::sleep_until(catalog_due), if fresh && jobs.is_empty()=> {
                // Re-checking a published catalog keeps every view and job until the
                // result is known; only a first check shows loading.
                let background=relay_pin.is_some() && matches!(tx.borrow().catalog.state.as_str(),"partial"|"ready");
                if !background {
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1); drop_older!();
                    activity_jobs.abort_all();activity_jobs=tokio::task::JoinSet::new();
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    publish_status(tx, |s| {s.activity.clear();s.catalog=crate::protocol::Catalog::loading();s.history=History::unavailable(None,None);s.recipients=crate::protocol::RecipientsView::unavailable(None,None);});
                }
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
                Ok(RelayMessage::Eose { subscription_id }) if live.is(&subscription_id)=> {
                    if live.eose(&subscription_id) {
                        let primed=live.primed_room().map(str::to_owned);
                        publish_status(tx,|s|s.history.live=s.history.state=="snapshot" && s.history.room_id==primed);
                        history_due=tokio::time::Instant::now()+policy.live_poll;
                        let open=tx.borrow().thread.state=="snapshot" && tx.borrow().thread.room_id==primed;
                        if open && thread_refetch.is_none() {thread_refetch=Some(tokio::time::Instant::now()+policy.live_poll);}
                    }
                },
                // The relay ended the live subscription: poll, re-arm after backoff.
                Ok(RelayMessage::Closed { subscription_id, .. }) if live.is(&subscription_id)=> {
                    live.closed_by_relay(tokio::time::Instant::now());
                    thread_refetch=None;
                    publish_status(tx,|s|s.history.live=false);
                    history_due=history_due.min(tokio::time::Instant::now()+policy.head);
                },
                Ok(RelayMessage::Event { subscription_id, event }) if live.is(&subscription_id)=> {
                    let now=tokio::time::Instant::now();
                    let verdict={
                        let status=tx.borrow();
                        live.event(&subscription_id,&event,*relay_pin,|id|status.history.rows.iter().any(|r|r.id==id),now)
                    };
                    match verdict {
                        crate::live::Frame::Trigger { thread }=> {
                            let gap=live.gap(now);
                            let at=|last:Option<tokio::time::Instant>|(now+crate::live::DEBOUNCE).max(last.map_or(now,|l|l+gap));
                            if head_refetch.is_none() {head_refetch=Some(at(last_head));}
                            let open={let status=tx.borrow();status.thread.root_id.is_some() && status.thread.room_id.as_deref()==live.room()};
                            if thread && open {
                                let next=at(last_thread);
                                thread_refetch=Some(thread_refetch.map_or(next,|t|t.min(next)));
                            }
                        },
                        crate::live::Frame::Flood(close)=> {
                            // A count only; frame contents are never logged.
                            eprintln!("omarchy-buzz: live subscription closed after repeated unverifiable frames");
                            thread_refetch=None;
                            publish_status(tx,|s|s.history.live=false);
                            send_frame!(close);
                            history_due=history_due.min(now+policy.head);
                        },
                        crate::live::Frame::Ignored|crate::live::Frame::Other=>{},
                    }
                },
                Ok(RelayMessage::Auth { challenge })=> {
                    if challenge.len()>1024 { return ConnectionExit::Failure("relay_protocol_error"); }
                    // Close the live subscription before `authenticate`: while it
                    // waits for the challenge and OK, the pinned client buffers every
                    // unrelated frame in an unbounded queue. With the subscription
                    // closed only frames already in flight can land there. It is
                    // re-armed after `fresh` and a new head page (history result arm).
                    close_live!();
                    head_refetch=None;
                    if let Some(delivery)=sender.unknown() {publish_status(tx, |s|s.delivery=delivery);}
                    if let Some(view)=opener.unknown() {publish_status(tx, |s|s.dm_open=view);}
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    fresh=false;
                    jobs.abort_all();
                    history_jobs.abort_all(); history_jobs=tokio::task::JoinSet::new(); history_ticket=history_ticket.wrapping_add(1); drop_older!();
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
                Ok(RelayMessage::Ok(ok))=> {
                    if let Some(delivery)=sender.acknowledge(&ok.event_id,ok.accepted) {publish_status(tx, |s|s.delivery=delivery);}
                    else if let Some(view)=opener.acknowledge(&ok.event_id,ok.accepted,&ok.message) {
                        // Re-check joined rooms now so the opened DM is listed; a check
                        // already in flight may predate it, so it is replaced.
                        if view.state=="acknowledged" && fresh {jobs.abort_all();catalog_due=tokio::time::Instant::now();}
                        publish_status(tx, |s|s.dm_open=view);
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
        publish_status(&tx, |s| {
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
mod thread_policy_tests {
    use super::*;
    #[test]
    fn thread_fetch_requires_current_signed_history_root_and_membership() {
        let room = "00000000-0000-4000-8000-000000000001";
        let root = "a".repeat(64);
        let mut status = Status::new(&config::Config::default());
        status.catalog.rooms.push(crate::protocol::Room {
            id: room.into(),
            name: "room".into(),
            description: String::new(),
            kind: "stream".into(),
            participants: Vec::new(),
            hidden: false,
        });
        status.history = History {
            state: "snapshot".into(),
            room_id: Some(room.into()),
            rows: vec![crate::protocol::HistoryRow {
                reactions: None,
                thread: None,
                id: root.clone(),
                author: "b".repeat(64),
                time: 1,
                text: "signed".into(),
                edited: false,
                truncated: false,
                unavailable: false,
            }],
            has_more: Some(false),
            category: Some("history_completeness_unknown".into()),
            next_cursor: None,
            older_state: "idle".into(),
            live: false,
        };
        let allowed = |s: &Status, selected: Option<&str>, fresh, pinned| {
            thread_allowed(s, selected, room, &root, fresh, pinned)
        };
        assert!(allowed(&status, Some(room), true, true));
        assert!(!allowed(&status, Some(room), false, true));
        assert!(!allowed(&status, Some(room), true, false));
        assert!(!allowed(&status, None, true, true));
        assert!(!thread_allowed(
            &status,
            Some(room),
            room,
            &"c".repeat(64),
            true,
            true
        ));
        status.history.rows[0].unavailable = true;
        assert!(!allowed(&status, Some(room), true, true));
        status.history.rows[0].unavailable = false;
        status.thread = Thread {
            state: "loading".into(),
            room_id: Some(room.into()),
            root_id: Some(root.clone()),
            rows: Vec::new(),
            has_more: None,
            category: None,
        };
        assert!(thread_result_allowed(
            &status,
            Some(room),
            room,
            &root,
            true,
            true,
            3,
            3,
            status.generation
        ));
        assert!(!thread_result_allowed(
            &status,
            Some(room),
            room,
            &root,
            true,
            true,
            2,
            3,
            status.generation
        ));
        assert!(!thread_result_allowed(
            &status,
            Some(room),
            room,
            &root,
            true,
            true,
            3,
            3,
            status.generation + 1
        ));
        status.thread.root_id = Some("c".repeat(64));
        assert!(!thread_result_allowed(
            &status,
            Some(room),
            room,
            &root,
            true,
            true,
            3,
            3,
            status.generation
        ));
        status.history.rows.clear();
        assert!(!allowed(&status, Some(room), true, true));
        status.history.state = "loading".into();
        assert!(!allowed(&status, Some(room), true, true));
        status.catalog.rooms.clear();
        assert!(!allowed(&status, Some(room), true, true));
    }
    #[test]
    fn publishing_history_loss_clears_thread_atomically() {
        let room = "00000000-0000-4000-8000-000000000001";
        let root = "a".repeat(64);
        let mut status = Status::new(&config::Config::default());
        status.catalog.rooms.push(crate::protocol::Room {
            id: room.into(),
            name: "room".into(),
            description: String::new(),
            kind: "stream".into(),
            participants: Vec::new(),
            hidden: false,
        });
        status.history = History {
            state: "snapshot".into(),
            room_id: Some(room.into()),
            rows: vec![crate::protocol::HistoryRow {
                reactions: None,
                thread: None,
                id: root.clone(),
                author: "b".repeat(64),
                time: 1,
                text: "signed".into(),
                edited: false,
                truncated: false,
                unavailable: false,
            }],
            has_more: Some(false),
            category: Some("history_completeness_unknown".into()),
            next_cursor: None,
            older_state: "idle".into(),
            live: false,
        };
        status.thread = Thread {
            state: "snapshot".into(),
            room_id: Some(room.into()),
            root_id: Some(root.clone()),
            rows: Vec::new(),
            has_more: Some(false),
            category: Some("thread_completeness_unknown".into()),
        };
        let (tx, rx) = watch::channel(status);
        publish_status(&tx, |s| {
            s.thread = Thread::unavailable(
                Some(room.into()),
                Some(root.clone()),
                Some("thread_timeout"),
            )
        });
        assert_eq!(
            rx.borrow().thread.category.as_deref(),
            Some("thread_timeout")
        );
        assert_eq!(rx.borrow().thread.root_id.as_deref(), Some(root.as_str()));
        publish_status(&tx, |s| s.history.rows.clear());
        assert_eq!(rx.borrow().thread.state, "unavailable");
        assert!(rx.borrow().thread.root_id.is_none());
    }
}

#[cfg(test)]
#[path = "auth_send_tests.rs"]
mod send_tests;

#[cfg(test)]
#[path = "auth_send_integration_tests.rs"]
mod send_integration_tests;

#[cfg(test)]
#[path = "auth_activity_tests.rs"]
mod activity_integration_tests;

#[cfg(test)]
#[path = "auth_live_tests.rs"]
mod live_integration_tests;
