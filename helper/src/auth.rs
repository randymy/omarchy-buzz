use crate::{
    config,
    protocol::{Command, History, Status, Thread},
};
use buzz_ws_client::{ConnectionOptions, NostrWsConnection, RelayMessage, WsClientError};
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
    #[cfg(test)]
    assert!(
        category.is_none_or(|c| crate::protocol::CONNECTION_CATEGORIES.contains(&c)),
        "unlisted connection category {category:?}"
    );
    publish_status(tx, |s| {
        s.connection = state.into();
        s.category = category.map(str::to_owned);
        if state != "authenticated" {
            s.catalog = crate::protocol::Catalog::unavailable(None);
            s.history = History::unavailable(None, None);
            s.thread = Thread::unavailable(None, None, None);
            s.activity.clear();
            s.recipients = crate::protocol::RecipientsView::unavailable(None, None);
            s.open_rooms = crate::protocol::OpenRooms::unavailable(None);
            // Nothing is known about the status without a session; a failure's
            // category stays readable.
            let failed = (s.user_status.state == "failed")
                .then(|| s.user_status.category.clone())
                .flatten();
            s.user_status = crate::protocol::UserStatusView::unavailable(failed.as_deref());
            s.presence = crate::protocol::PresenceView::unavailable();
            for entry in s.recipients.entries.iter_mut() {
                entry.presence = None;
            }
        }
    });
}
fn category(e: &WsClientError) -> &'static str {
    match e {
        WsClientError::AuthFailed(_) => "auth_rejected",
        WsClientError::Timeout | WsClientError::NoAuthChallenge => "relay_timeout",
        WsClientError::ConnectionClosed | WsClientError::WebSocket(_) => "relay_unavailable",
        WsClientError::ResourceLimit => "relay_resource_limit",
        _ => "relay_protocol_error",
    }
}
/// The connection's transport and replay budgets. Every frame the relay
/// sends is capped before it is parsed, and frames buffered while the client
/// waits for `AUTH` or an `OK` are counted and bounded; passing a budget
/// closes the socket with `relay_resource_limit`. Events the helper accepts
/// are at most `live::MAX_EVENT_BYTES` (64 KiB), so a 256 KiB frame leaves
/// room for the relay envelope and anything it may legitimately add.
pub(crate) fn connection_options() -> ConnectionOptions {
    ConnectionOptions {
        connect_timeout: Duration::from_secs(20),
        authentication_timeout: Duration::from_secs(40),
        write_timeout: Duration::from_secs(10),
        close_timeout: Duration::from_secs(5),
        max_frame_bytes: MAX_FRAME_BYTES,
        max_message_bytes: MAX_FRAME_BYTES,
        max_buffered_messages: MAX_BUFFERED_MESSAGES,
        max_buffered_bytes: MAX_BUFFERED_BYTES,
    }
}
pub(crate) const MAX_FRAME_BYTES: usize = 256 * 1024;
pub(crate) const MAX_BUFFERED_MESSAGES: usize = 128;
pub(crate) const MAX_BUFFERED_BYTES: usize = 2 * 1024 * 1024;
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
        NostrWsConnection::connect_authenticated_with_options(
            relay,
            keys,
            auth_tag,
            connection_options(),
        ),
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
                    // A measurement belongs to the relay it was taken against.
                    s.clock_skew_seconds = None;
                    s.catalog = crate::protocol::Catalog::unavailable(None);
                    s.history = History::unavailable(None, None);
                    s.thread = Thread::unavailable(None, None, None);
                    s.activity.clear();
                    s.recipients = crate::protocol::RecipientsView::unavailable(None, None);
                    s.setup = crate::protocol::JoinSetup::default();
                    s.open_rooms = crate::protocol::OpenRooms::unavailable(None);
                    s.room_action = crate::protocol::RoomAction::default();
                    s.download = Default::default();
                    s.thumbnails.clear();
                    s.pending_attachments.clear();
                    s.upload = Default::default();
                    s.user_status = crate::protocol::UserStatusView::unavailable(None);
                    s.presence = crate::protocol::PresenceView::unavailable();
                    s.communities = crate::communities::view(&c);
                }
                // A rename or a removed inactive community changes only the list.
                let listed = crate::communities::view(&c);
                s.communities.active = listed.active;
                s.communities.entries = listed.entries;
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
    // At most one status publication per this interval (`user_status::GAP`).
    status_gap: Duration,
    // At most one presence publication per this interval (`presence::GAP`),
    // and the re-publication interval while online or away.
    presence_gap: Duration,
    presence_heartbeat: Duration,
}
const FRESHNESS: FreshnessPolicy = FreshnessPolicy {
    interval: Duration::from_secs(20),
    response: Duration::from_secs(5),
    catalog: Duration::from_secs(30),
    head: Duration::from_secs(5),
    live_poll: Duration::from_secs(30),
    status_gap: crate::user_status::GAP,
    presence_gap: crate::presence::GAP,
    presence_heartbeat: crate::presence::HEARTBEAT,
};
struct Backoff {
    failures: u8,
    // Set when the relay rejected a re-authentication of a session that had
    // authenticated since start or Retry. Until success, Retry or exhaustion,
    // `auth_rejected` then retries like a network failure. A rejected first
    // authentication never sets it: that means a wrong or revoked identity.
    reauth_rejected: bool,
    // Set while the latest failure was `clock_skew` (a rejected first or
    // re-authentication explained by the clock offset). The clock can be fixed
    // while the helper keeps trying, so it retries like a network failure.
    clock_skew: bool,
    unit: Duration,
}
impl Default for Backoff {
    fn default() -> Self {
        Self {
            failures: 0,
            reauth_rejected: false,
            clock_skew: false,
            unit: Duration::from_secs(1),
        }
    }
}
impl Backoff {
    fn reset(&mut self) {
        self.failures = 0;
        self.reauth_rejected = false;
        self.clock_skew = false;
    }
    fn delay(&mut self, error: &str) -> Option<Duration> {
        let retryable = matches!(
            error,
            "relay_timeout" | "relay_unavailable" | "relay_resource_limit" | "clock_skew"
        ) || (error == "auth_rejected" && self.reauth_rejected);
        if !retryable || self.failures >= 5 {
            self.reauth_rejected = false;
            self.clock_skew = false;
            return None;
        }
        self.clock_skew = error == "clock_skew";
        let factor = 1_u32 << self.failures;
        self.failures += 1;
        Some(self.unit * factor)
    }
    /// The category shown while an automatic retry is in progress.
    fn retrying(&self) -> Option<&'static str> {
        if self.clock_skew {
            Some("clock_skew")
        } else {
            self.reauth_rejected.then_some("auth_rejected")
        }
    }
}
enum ConnectionExit {
    Retry,
    Shutdown,
    Failure(&'static str),
}
/// A command received while not authenticated. `session` is the relay and
/// loaded identity of a connection that failed (disconnected, possibly
/// refused because the identity is not a member yet): invites can be redeemed
/// through HTTP then, and a successful claim reconnects.
pub(crate) async fn offline_command(
    command: Command,
    tx: &watch::Sender<Status>,
    setup: &crate::setup::Setup,
    session: Option<(&str, &nostr::Keys)>,
) -> bool {
    match command {
        Command::Retry => return true,
        Command::SetRelay(url, reply) => {
            let result = crate::setup::set_relay(setup, &url).await;
            return setup_done(result.map(|_| ()), reply, tx, setup);
        }
        Command::CreateIdentity(reply) => {
            let result = crate::setup::create_identity(setup).await;
            return setup_done(result.map(|_| ()), reply, tx, setup);
        }
        Command::ClaimInvite(input, reply) => {
            let Some((relay, _)) = session else {
                let _ = reply.send(Some("setup_not_allowed"));
                return false;
            };
            publish_status(tx, |s| s.setup = crate::join::checking());
            let result = crate::join::check_invite(relay, &input).await;
            invite_checked(result, reply, tx);
        }
        Command::AcceptInvite(code, version, reply) => {
            let Some((relay, keys)) = session else {
                let _ = reply.send(Some("setup_not_allowed"));
                return false;
            };
            let shown = match crate::join::awaiting_invite(&tx.borrow(), &code) {
                Ok(shown) => shown,
                Err(category) => {
                    let _ = reply.send(Some(category));
                    return false;
                }
            };
            publish_status(tx, |s| s.setup = claiming());
            let result =
                crate::join::redeem(relay, keys, &code, shown.as_ref(), version.as_deref()).await;
            // A new member reconnects at once: its identity may now authenticate.
            return invite_redeemed(result, reply, tx);
        }
        Command::Community(request, generation, reply) => {
            if reply.is_closed() {
                return false;
            }
            if generation.is_some_and(|g| g != tx.borrow().generation) {
                let _ = reply.send(Some("join_busy"));
                return false;
            }
            community_started(tx, request.state());
            let result =
                crate::communities::perform(setup, request, session.map(|(_, keys)| keys)).await;
            let claimed = result.as_ref().is_ok_and(|d| d.claimed);
            // A switch, or new membership of this relay, reconnects at once.
            return community_done(result, reply, tx, setup) || claimed;
        }
        Command::RoomAction(_, _, _, reply)
        | Command::MintInvite(_, _, reply)
        | Command::DownloadAttachment(_, _, reply)
        | Command::ThumbnailAttachment(_, _, reply)
        | Command::UploadAttachment(_, _, _, reply)
        | Command::SetStatus(_, reply)
        | Command::SetPresence(_, _, reply) => {
            let _ = reply.send(Some("relay_unavailable"));
        }
        // Nothing can be published without a session.
        Command::PresenceShutdown(reply) => {
            let _ = reply.send(());
        }
        // Drafts and saved files do not need the relay.
        command @ (Command::OpenDownload(..) | Command::RemovePendingAttachment(..)) => {
            local_media_command(command, tx);
        }
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
/// `open_download` and `remove_pending_attachment`: no relay involved.
fn local_media_command(command: Command, tx: &watch::Sender<Status>) {
    match command {
        Command::OpenDownload(path, reply) => {
            let result = crate::media::Dirs::from_env()
                .and_then(|dirs| crate::media::openable(&path, &dirs.downloads))
                .and_then(|path| crate::media::open(&path));
            let _ = reply.send(result.err());
        }
        Command::RemovePendingAttachment(hash, reply) => {
            let known = tx
                .borrow()
                .pending_attachments
                .iter()
                .any(|p| p.hash == hash);
            if known {
                publish_status(tx, |s| s.pending_attachments.retain(|p| p.hash != hash));
            }
            let _ = reply.send((!known).then_some("attachment_unknown"));
        }
        _ => {}
    }
}
/// Thread replies as published: attachments bounded per frame.
fn thread_rows(mut rows: Vec<crate::protocol::ThreadRow>) -> Vec<crate::protocol::ThreadRow> {
    crate::attachments::bound(
        rows.iter_mut().map(|r| &mut r.row),
        crate::attachments::FRAME_THREAD,
    );
    rows
}
/// A verified preview for an image attachment: the cached file when its bytes
/// still hash to the attachment, else a verified download kept in the cache.
async fn thumbnail(
    keys: &nostr::Keys,
    relay: &str,
    attachment: &crate::attachments::Attachment,
    dirs: &crate::media::Dirs,
) -> Result<std::path::PathBuf, &'static str> {
    let (thumbs, cached_attachment) = (dirs.thumbs.clone(), attachment.clone());
    if let Ok(Some(path)) =
        tokio::task::spawn_blocking(move || crate::media::cached(&thumbs, &cached_attachment)).await
    {
        return Ok(path);
    }
    let progress = std::sync::atomic::AtomicU64::new(0);
    let temp = crate::media::fetch(
        keys,
        relay,
        attachment,
        &dirs.staging,
        crate::media::THUMB_SOURCE_BYTES,
        &progress,
    )
    .await?;
    let (thumbs, attachment) = (dirs.thumbs.clone(), attachment.clone());
    tokio::task::spawn_blocking(move || crate::media::keep_thumbnail(&temp, &thumbs, &attachment))
        .await
        .unwrap_or(Err("attachment_storage_unavailable"))
}
fn claiming() -> crate::protocol::JoinSetup {
    crate::protocol::JoinSetup {
        state: "claiming".into(),
        ..Default::default()
    }
}
/// Publishes the checked invite (awaiting acceptance) or its refusal, then answers.
fn invite_checked(
    result: Result<(String, Option<crate::protocol::JoinPolicy>), &'static str>,
    reply: tokio::sync::oneshot::Sender<Option<&'static str>>,
    tx: &watch::Sender<Status>,
) {
    let category = result.as_ref().err().copied();
    publish_status(tx, |s| {
        s.setup = match result {
            Ok((code, policy)) => crate::join::awaiting(code, policy),
            Err(category) => crate::join::failed(category),
        }
    });
    let _ = reply.send(category);
}
/// Publishes the claim outcome, then answers. True when the relay granted
/// (or confirmed) membership.
fn invite_redeemed(
    result: Result<crate::protocol::ClaimResult, &'static str>,
    reply: tokio::sync::oneshot::Sender<Option<&'static str>>,
    tx: &watch::Sender<Status>,
) -> bool {
    let category = result.as_ref().err().copied();
    publish_status(tx, |s| {
        s.setup = match result {
            Ok(claim) => crate::join::joined(claim),
            Err(category) => crate::join::failed(category),
        }
    });
    let _ = reply.send(category);
    category.is_none()
}
fn community_started(tx: &watch::Sender<Status>, state: &str) {
    publish_status(tx, |s| {
        s.communities.state = state.into();
        s.communities.category = None;
        s.communities.notice = None;
        s.communities.pending_invite = false;
    });
}
/// Publishes a finished community request (the saved list, a new generation
/// when the active relay changed, an invite's terms to accept) before
/// answering. True when the active relay changed: the caller reconnects.
fn community_done(
    result: Result<crate::communities::Done, &'static str>,
    reply: tokio::sync::oneshot::Sender<Option<&'static str>>,
    tx: &watch::Sender<Status>,
    setup: &crate::setup::Setup,
) -> bool {
    let done = match result {
        Ok(done) => done,
        Err(category) => {
            publish_status(tx, |s| {
                s.communities.state = "failed".into();
                s.communities.category = Some(category.into());
            });
            let _ = reply.send(Some(category));
            return false;
        }
    };
    let before = tx.borrow().generation;
    let _ = apply_loaded_config(tx, setup.load());
    let renewed = tx.borrow().generation != before;
    publish_status(tx, |s| {
        // An invite minted for another relay is not shown again.
        if renewed {
            s.invites = Default::default();
        }
        s.communities.state = "ready".into();
        s.communities.category = None;
        s.communities.pending_invite = done.pending_invite;
        s.communities.notice = done.notice.map(str::to_owned);
        if let Some((code, policy)) = done.policy {
            s.setup = crate::join::awaiting(code, Some(policy));
        }
    });
    let _ = reply.send(None);
    done.switched
}
/// Publishes a saved setup change before answering, so the status frame that
/// follows the reply already carries the new relay or public identity. A
/// saved change then reconnects like `retry_connection`.
fn setup_done(
    result: Result<(), &'static str>,
    reply: tokio::sync::oneshot::Sender<Option<&'static str>>,
    tx: &watch::Sender<Status>,
    setup: &crate::setup::Setup,
) -> bool {
    if result.is_ok() {
        // An invite minted for another relay or identity is not shown again.
        publish_status(tx, |s| s.invites = Default::default());
        let _ = apply_loaded_config(tx, setup.load());
    }
    let _ = reply.send(result.err());
    result.is_ok()
}
async fn next_retry(
    commands: &mut mpsc::Receiver<Command>,
    tx: &watch::Sender<Status>,
    setup: &crate::setup::Setup,
    session: Option<(&str, &nostr::Keys)>,
) -> bool {
    while let Some(command) = commands.recv().await {
        if offline_command(command, tx, setup, session).await {
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
    setup: &crate::setup::Setup,
    session: Option<(&str, &nostr::Keys)>,
) -> bool {
    let delay = backoff.delay(error);
    // A rejected authentication that is being retried (re-authentication or
    // clock skew) is still an attempt to connect; it becomes `disconnected`
    // only once the budget is spent, and then waits for Retry.
    let state = if delay.is_some() && matches!(error, "auth_rejected" | "clock_skew") {
        "connecting"
    } else {
        "disconnected"
    };
    update(tx, state, Some(error));
    if let Some(delay) = delay {
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            // A received command is handled outside the select, so the backoff
            // timer can never cancel a setup change halfway through.
            let command = tokio::select! {
                _=&mut sleep=>return true,
                command=retry.recv()=>command,
            };
            let Some(command) = command else {
                return false;
            };
            if offline_command(command, tx, setup, session).await {
                backoff.reset();
                return true;
            }
        }
    } else if next_retry(retry, tx, setup, session).await {
        backoff.reset();
        true
    } else {
        false
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
        &crate::setup::Setup::unavailable(),
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
    setup: &crate::setup::Setup,
) -> ConnectionExit {
    // A DM open cannot outlive its connection: its answer would arrive on this socket.
    let mut opener = crate::dm_open::Opener::default();
    let mut live = crate::live::Live::default();
    // A join or leave cannot outlive its connection either: its OK arrives here.
    let mut actions = crate::join::RoomActions::default();
    // A status publication is answered on this connection too.
    let mut statuses = crate::user_status::Publisher::default();
    // So is the presence heartbeat; the panel sends its preference on connect.
    let mut presence = crate::presence::Publisher::default();
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
        &mut actions,
        &mut statuses,
        &mut presence,
        setup,
    )
    .await;
    if statuses.unknown() {
        publish_status(tx, |s| {
            s.user_status = crate::user_status::failed(s, "relay_unavailable")
        });
    }
    if let Some(view) = actions.unknown() {
        publish_status(tx, |s| s.room_action = view);
    }
    // An invite request still running on this connection was aborted with it.
    // A claim may have reached the relay; the user can redeem again (claims are
    // idempotent: `already_member`).
    publish_status(tx, |s| {
        if matches!(s.setup.state.as_str(), "checking" | "claiming") {
            s.setup = crate::join::failed("relay_unavailable");
        }
        // A mint may have reached the relay; an unshown invite is only unused.
        if s.invites.state == "minting" {
            s.invites = crate::invites::failed("relay_unavailable");
        }
        // Transfers run with the connection's identity and end with it.
        if s.download.state == "downloading" {
            s.download.state = "failed".into();
            s.download.category = Some("relay_unavailable".into());
        }
        if s.upload.state == "uploading" {
            s.upload.state = "failed".into();
            s.upload.category = Some("relay_unavailable".into());
        }
        // A community request runs with the connection's key and ends with it;
        // the next configuration load shows what was saved.
        if crate::communities::busy(&s.communities) {
            s.communities.state = "failed".into();
            s.communities.category = Some("relay_unavailable".into());
        }
    });
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
    actions: &mut crate::join::RoomActions,
    statuses: &mut crate::user_status::Publisher,
    presence: &mut crate::presence::Publisher,
    setup: &crate::setup::Setup,
) -> ConnectionExit {
    let mut pending: Option<String> = None;
    // `communities`: one join, switch, rename or leave at a time beside the
    // connection; NIP-11 name hints for listed communities read once.
    let mut community_jobs = tokio::task::JoinSet::new();
    let mut hint_jobs = tokio::task::JoinSet::new();
    if let Ok(c) = setup.load() {
        let relays = crate::communities::unhinted(&c);
        if !relays.is_empty() {
            hint_jobs.spawn(crate::communities::fetch_hints(relays));
        }
    }
    // `presence`: one verified read of the shown roster and DM partners at a
    // time, on the joined-room check cycle (and when the roster or the panel's
    // first preference arrives), never before the panel asked for presence.
    // States older than `presence::FRESH_SECS` are dropped, never shown.
    let mut presence_jobs = tokio::task::JoinSet::new();
    let mut presence_ticket = 0_u64;
    let mut presence_seen: std::collections::BTreeMap<String, &'static str> =
        std::collections::BTreeMap::new();
    let mut presence_read_at: Option<tokio::time::Instant> = None;
    // A shutdown waits (briefly, bounded by the daemon) for its offline OK.
    let mut presence_shutdown: Option<tokio::sync::oneshot::Sender<()>> = None;
    // `user_status`: this identity's own status comes with each roster read
    // (the verified roster includes it; no extra subscription or poll), and is
    // read again after each accepted publication. A failed read is retried a
    // few times, then waits for the next trigger. `status_epoch` counts
    // accepted publications so an older read never replaces a newer status.
    let mut own_jobs = tokio::task::JoinSet::new();
    let mut own_ticket = 0_u64;
    let mut own_due: Option<tokio::time::Instant> = None;
    let mut own_attempts = 0_u32;
    let mut status_epoch = 0_u64;
    // Expiry of the statuses shown beside roster names (the shown roster and
    // this identity only), applied locally on each joined-room check: the
    // relay never expires them.
    let mut status_expiry: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    let own_key = keys.public_key().to_hex();
    // At most one invite request (HTTP) and one open-rooms read at a time.
    let mut invite_jobs = tokio::task::JoinSet::new();
    // At most one mint at a time (`invite_mint`).
    let mut mint_jobs = tokio::task::JoinSet::new();
    let mut open_jobs = tokio::task::JoinSet::new();
    let mut open_ticket = 0_u64;
    // Attachments: one download, one preview (from a bounded queue) and one
    // upload at a time; each ends with this connection.
    let mut download_jobs = tokio::task::JoinSet::new();
    let download_progress = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut progress_due = tokio::time::Instant::now();
    let mut thumb_jobs = tokio::task::JoinSet::new();
    let mut thumb_queue: std::collections::VecDeque<crate::attachments::Attachment> =
        std::collections::VecDeque::new();
    let mut upload_jobs = tokio::task::JoinSet::new();
    // Re-read open rooms once the catalog settles after a join or leave.
    let mut open_after_catalog = false;
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
    macro_rules! spawn_open_rooms {
        () => {{
            open_jobs.abort_all();
            open_jobs = tokio::task::JoinSet::new();
            open_ticket = open_ticket.wrapping_add(1);
            let status = tx.borrow();
            let ready = fresh
                && relay_pin.is_some()
                && matches!(status.catalog.state.as_str(), "partial" | "ready");
            let joined: std::collections::BTreeSet<String> =
                status.catalog.rooms.iter().map(|r| r.id.clone()).collect();
            let generation = status.generation;
            drop(status);
            if ready {
                publish_status(tx, |s| {
                    s.open_rooms = crate::protocol::OpenRooms {
                        state: "loading".into(),
                        ..crate::protocol::OpenRooms::unavailable(None)
                    }
                });
                let ticket = open_ticket;
                let relay = relay.to_owned();
                let keys = keys.clone();
                let pin = relay_pin.unwrap();
                open_jobs.spawn(async move {
                    let result = match timeout(
                        Duration::from_secs(15),
                        crate::catalog::discover_open(&relay, &keys, pin, joined),
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(_) => Err("query_timeout"),
                    };
                    (ticket, generation, result)
                });
            } else {
                publish_status(tx, |s| {
                    s.open_rooms =
                        crate::protocol::OpenRooms::unavailable(Some("relay_unavailable"))
                });
            }
        }};
    }
    macro_rules! pump_thumbs {
        () => {{
            if thumb_jobs.is_empty() {
                if let (Some(attachment), Ok(dirs)) =
                    (thumb_queue.pop_front(), crate::media::Dirs::from_env())
                {
                    let relay = relay.to_owned();
                    let keys = keys.clone();
                    thumb_jobs.spawn(async move {
                        let result = timeout(
                            Duration::from_secs(120),
                            thumbnail(&keys, &relay, &attachment, &dirs),
                        )
                        .await
                        .unwrap_or(Err("relay_unavailable"));
                        (attachment.hash, result)
                    });
                }
            }
        }};
    }
    macro_rules! spawn_own_status {
        () => {{
            own_jobs.abort_all();
            own_jobs = tokio::task::JoinSet::new();
            own_ticket = own_ticket.wrapping_add(1);
            own_due = None;
            let ticket = own_ticket;
            let epoch = status_epoch;
            let relay = relay.to_owned();
            let keys = keys.clone();
            own_jobs.spawn(async move {
                let result = match timeout(
                    Duration::from_secs(15),
                    crate::recipients::own_status(&relay, &keys),
                )
                .await
                {
                    Ok(r) => r,
                    Err(_) => Err("query_timeout"),
                };
                (ticket, epoch, result)
            });
        }};
    }
    macro_rules! project_presence {
        () => {{
            let own = keys.public_key().to_hex();
            publish_status(tx, |s| apply_presence(s, presence, &presence_seen, &own));
        }};
    }
    macro_rules! spawn_presence {
        () => {{
            if presence.configured() && fresh && presence_jobs.is_empty() {
                if let Some(pin) = *relay_pin {
                    let subjects = presence_subjects(&tx.borrow(), &keys.public_key().to_hex());
                    if !subjects.is_empty() {
                        presence_ticket = presence_ticket.wrapping_add(1);
                        let ticket = presence_ticket;
                        let relay = relay.to_owned();
                        let keys = keys.clone();
                        presence_jobs.spawn(async move {
                            let result = match timeout(
                                Duration::from_secs(30),
                                crate::presence::read(&relay, &keys, pin, subjects),
                            )
                            .await
                            {
                                Ok(r) => r,
                                Err(_) => Err("query_timeout"),
                            };
                            (ticket, result)
                        });
                    }
                }
            }
        }};
    }
    macro_rules! answer_shutdown {
        () => {{
            // Nothing in flight and nothing more owed: the shutdown may proceed.
            if presence_shutdown.is_some()
                && !presence.is_pending()
                && presence
                    .due(tokio::time::Instant::now(), None, policy.presence_heartbeat)
                    .is_none()
            {
                if let Some(reply) = presence_shutdown.take() {
                    let _ = reply.send(());
                }
            }
        }};
    }
    loop {
        // Only the selected room of a fresh session may hold the live
        // subscription; every path that drops the selection closes it here.
        if live.room().is_some() && (!fresh || selected_history.as_deref() != live.room()) {
            close_live!();
        }
        let presence_at = {
            let ready = crate::presence::GATE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .ready_at(policy.presence_gap);
            presence.due(
                tokio::time::Instant::now(),
                ready,
                policy.presence_heartbeat,
            )
        };
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
            _=tokio::time::sleep_until(actions.deadline()), if actions.is_pending()=> {
                if let Some(view)=actions.unknown() {publish_status(tx, |s|s.room_action=view);}
            },
            // No OK in time: the relay may have stored it; the shown status stays.
            _=tokio::time::sleep_until(statuses.deadline()), if statuses.is_pending()=> {
                if statuses.unknown() {publish_status(tx, |s|s.user_status=crate::user_status::failed(s,"relay_unavailable"));}
            },
            // No OK in time: a category only; the connection is unaffected.
            _=tokio::time::sleep_until(presence.deadline()), if presence.is_pending()=> {
                if presence.unknown() {project_presence!();}
                answer_shutdown!();
            },
            // A change, or the heartbeat: one signed kind 20001 through the gate.
            _=tokio::time::sleep_until(presence_at.unwrap_or(due)), if presence_at.is_some() && fresh=> {
                let now=tokio::time::Instant::now();
                let admitted=crate::presence::GATE.lock().unwrap_or_else(|e|e.into_inner()).admit(now,policy.presence_gap);
                if !admitted {continue;}
                match presence.prepare(keys,now) {
                    Ok(event)=> {
                        // A dropped/timed-out write may already have reached the relay.
                        send_frame!(serde_json::json!(["EVENT",event]));
                    },
                    Err(category)=> {eprintln!("omarchy-buzz: presence not published: {category}");presence.detach();},
                }
            },
            result=presence_jobs.join_next(), if !presence_jobs.is_empty()=> {
                match result {
                    Some(Ok((ticket,Ok(seen)))) if ticket==presence_ticket && fresh=> {
                        presence_seen=seen;
                        presence_read_at=Some(tokio::time::Instant::now());
                        project_presence!();
                    },
                    // A category only; the shown states age out after `FRESH_SECS`.
                    Some(Ok((ticket,Err(error)))) if ticket==presence_ticket=>eprintln!("omarchy-buzz: presence read failed: {error}"),
                    _=>{},
                }
            },
            command=retry.recv()=>match command {
                Some(Command::Retry)=> {backoff.reset(); update(tx,"connecting",None); return ConnectionExit::Retry;},
                None=>{update(tx,"disconnected",None);return ConnectionExit::Shutdown;},
                // Setup never replaces a working session.
                Some(Command::SetRelay(_,reply) | Command::CreateIdentity(reply))=> {let _=reply.send(Some("setup_not_allowed"));},
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
                Some(Command::ClaimInvite(input,reply))=> {
                    if !invite_jobs.is_empty() {let _=reply.send(Some("setup_busy"));continue;}
                    publish_status(tx,|s|s.setup=crate::join::checking());
                    let relay=relay.to_owned();
                    invite_jobs.spawn(async move {
                        let result=crate::join::check_invite(&relay,&input).await;
                        (crate::join::InviteStep::Checked(result),reply)
                    });
                },
                Some(Command::AcceptInvite(code,version,reply))=> {
                    if !invite_jobs.is_empty() {let _=reply.send(Some("setup_busy"));continue;}
                    let shown=match crate::join::awaiting_invite(&tx.borrow(),&code) {Ok(shown)=>shown,Err(category)=>{let _=reply.send(Some(category));continue;}};
                    publish_status(tx,|s|s.setup=claiming());
                    let relay=relay.to_owned();let keys=keys.clone();
                    invite_jobs.spawn(async move {
                        let result=crate::join::redeem(&relay,&keys,&code,shown.as_ref(),version.as_deref()).await;
                        (crate::join::InviteStep::Redeemed(result),reply)
                    });
                },
                Some(Command::FetchOpenRooms)=> {spawn_open_rooms!();},
                Some(Command::MintInvite(max_uses,hours,reply))=> {
                    if !mint_jobs.is_empty() {let _=reply.send(Some("setup_busy"));continue;}
                    if !fresh {let _=reply.send(Some("relay_unavailable"));continue;}
                    publish_status(tx,|s|s.invites=crate::invites::minting());
                    let relay=relay.to_owned();let keys=keys.clone();
                    mint_jobs.spawn(async move {(crate::invites::mint(&relay,&keys,max_uses,hours).await,reply)});
                },
                Some(Command::DownloadAttachment(event_id,hash,reply))=> {
                    if !download_jobs.is_empty() {let _=reply.send(Some("setup_busy"));continue;}
                    if !fresh {let _=reply.send(Some("relay_unavailable"));continue;}
                    let Some(attachment)=crate::media::held(&tx.borrow(),&event_id,&hash) else {let _=reply.send(Some("attachment_unknown"));continue;};
                    let dirs=match crate::media::Dirs::from_env() {Ok(d)=>d,Err(c)=>{let _=reply.send(Some(c));continue;}};
                    publish_status(tx,|s|s.download=crate::media::downloading(&event_id,&attachment));
                    download_progress.store(0,std::sync::atomic::Ordering::Relaxed);
                    progress_due=tokio::time::Instant::now()+Duration::from_millis(250);
                    let relay=relay.to_owned();let keys=keys.clone();let progress=download_progress.clone();
                    download_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(1800),crate::media::fetch(&keys,&relay,&attachment,&dirs.staging,crate::media::DOWNLOAD_CAP,&progress)).await {
                            Ok(Ok(temp))=>{let (downloads,name)=(dirs.downloads.clone(),attachment.name.clone());
                                tokio::task::spawn_blocking(move||crate::media::save(&temp,&downloads,&name)).await.unwrap_or(Err("attachment_storage_unavailable"))},
                            Ok(Err(c))=>Err(c),
                            Err(_)=>Err("relay_unavailable"),
                        };
                        (event_id,attachment,result)
                    });
                    let _=reply.send(None);
                },
                Some(Command::ThumbnailAttachment(event_id,hash,reply))=> {
                    if !fresh {let _=reply.send(Some("relay_unavailable"));continue;}
                    let Some(attachment)=crate::media::held(&tx.borrow(),&event_id,&hash) else {let _=reply.send(Some("attachment_unknown"));continue;};
                    if attachment.kind!="image" || attachment.size>crate::media::THUMB_SOURCE_BYTES {let _=reply.send(Some("attachment_too_large"));continue;}
                    let known=tx.borrow().thumbnails.iter().any(|t|t.hash==hash) || thumb_queue.iter().any(|a|a.hash==hash);
                    if !known {
                        if thumb_queue.len()>=crate::media::THUMB_QUEUE {let _=reply.send(Some("setup_busy"));continue;}
                        thumb_queue.push_back(attachment);
                        pump_thumbs!();
                    }
                    let _=reply.send(None);
                },
                Some(Command::UploadAttachment(room,root,path,reply))=> {
                    if !upload_jobs.is_empty() {let _=reply.send(Some("setup_busy"));continue;}
                    if !fresh {let _=reply.send(Some("relay_unavailable"));continue;}
                    let scope=crate::media::scope(&room,root.as_deref());
                    let refused={let status=tx.borrow();
                        let in_room=status.catalog.rooms.iter().any(|r|r.id==room);
                        let in_thread=root.as_deref().is_none_or(|root|status.thread.room_id.as_deref()==Some(room.as_str()) && status.thread.root_id.as_deref()==Some(root));
                        let count=status.pending_attachments.iter().filter(|p|p.scope==scope).count();
                        !in_room || !in_thread || count>=crate::media::PENDING || status.pending_attachments.len()>=crate::media::PENDING_TOTAL};
                    if refused {let _=reply.send(Some("attachment_invalid"));continue;}
                    let candidate=match crate::media::check_upload(&path) {Ok(c)=>c,Err(c)=> {
                        publish_status(tx,|s|s.upload=crate::protocol::Upload {state:"failed".into(),scope:Some(scope),name:None,category:Some(c.into())});
                        let _=reply.send(Some(c));continue;}};
                    publish_status(tx,|s|s.upload=crate::protocol::Upload {state:"uploading".into(),scope:Some(scope.clone()),name:Some(candidate.name.clone()),category:None});
                    let relay=relay.to_owned();let keys=keys.clone();
                    upload_jobs.spawn(async move {
                        let result=timeout(Duration::from_secs(900),crate::media::upload(&keys,&relay,candidate)).await.unwrap_or(Err("relay_unavailable"));
                        (scope,result)
                    });
                    let _=reply.send(None);
                },
                Some(command @ (Command::OpenDownload(..) | Command::RemovePendingAttachment(..)))=> {local_media_command(command,tx);},
                Some(Command::RoomAction(action,request_id,room,reply))=> {
                    if reply.is_closed() {continue;}
                    let prepared={let status=tx.borrow();actions.prepare(action,&request_id,&room,keys,&status,fresh,relay_pin.is_some())};
                    let (view,event)=match prepared {Ok(p)=>p,Err(category)=>{let _=reply.send(Some(category));continue;}};
                    if reply.send(None).is_err() {
                        // Nothing was written: the caller left before publication.
                        actions.abandon();
                        continue;
                    }
                    publish_status(tx,|s|s.room_action=view);
                    // A dropped/timed-out write may already have reached the relay.
                    send_frame!(serde_json::json!(["EVENT",event]));
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
                Some(Command::SetStatus(intent,reply))=> {
                    if reply.is_closed() {continue;}
                    let prepared={
                        let status=tx.borrow();
                        let mut gate=crate::user_status::GATE.lock().unwrap_or_else(|e|e.into_inner());
                        statuses.prepare(&intent,keys,&status,fresh && relay_pin.is_some(),&mut gate,policy.status_gap,tokio::time::Instant::now(),nostr::Timestamp::now().as_secs())
                    };
                    let (view,event)=match prepared {Ok(p)=>p,Err(category)=>{let _=reply.send(Some(category));continue;}};
                    if reply.send(None).is_err() {
                        // Nothing was written: the caller left before publication.
                        statuses.abandon();
                        continue;
                    }
                    publish_status(tx,|s|s.user_status=view);
                    // A dropped/timed-out write may already have reached the relay.
                    send_frame!(serde_json::json!(["EVENT",event]));
                },
                Some(Command::SetPresence(mode,active,reply))=> {
                    if reply.is_closed() {continue;}
                    if !fresh {let _=reply.send(Some("relay_unavailable"));continue;}
                    let first=!presence.configured();
                    // An unchanged preference and hint is a no-op; the heartbeat
                    // loop above publishes a changed state when the gate allows.
                    if presence.set(mode,active) {project_presence!();}
                    let _=reply.send(None);
                    if first {spawn_presence!();}
                },
                Some(Command::Community(request,generation,reply))=> {
                    if reply.is_closed() {continue;}
                    if !community_jobs.is_empty() || generation.is_some_and(|g|g!=tx.borrow().generation) {let _=reply.send(Some("join_busy"));continue;}
                    community_started(tx,request.state());
                    let setup=setup.clone();let keys=keys.clone();
                    community_jobs.spawn(async move {(crate::communities::perform(&setup,request,Some(&keys)).await,reply)});
                },
                Some(Command::PresenceDetach)=> {
                    presence.detach();
                    presence_jobs.abort_all();presence_jobs=tokio::task::JoinSet::new();presence_ticket=presence_ticket.wrapping_add(1);
                    project_presence!();
                },
                Some(Command::PresenceShutdown(reply))=> {
                    // Best effort, bounded by the caller: `offline` once, as soon as the
                    // gate allows (the heartbeat arm sends it); answered on its OK, or at
                    // once when nothing is owed. A heartbeat in flight is abandoned.
                    presence.detach();
                    if presence.is_pending() {presence.unknown();}
                    presence_shutdown=Some(reply);
                    if !fresh {if let Some(reply)=presence_shutdown.take() {let _=reply.send(());}}
                    answer_shutdown!();
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
                    let ticket=recipient_ticket;let epoch=status_epoch;let relay=relay.to_owned();let keys=keys.clone();let pin=relay_pin.unwrap();let id=parsed.unwrap();
                    recipient_jobs.spawn(async move {
                        let result=match timeout(Duration::from_secs(15),crate::recipients::fetch(&relay,&keys,pin,id)).await {Ok(r)=>r,Err(_)=>Err("recipients_timeout")};
                        (ticket,generation,room,epoch,result)
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
            result=community_jobs.join_next(), if !community_jobs.is_empty()=> {
                match result {
                    Some(Ok((result,reply)))=> {
                        let claimed=result.as_ref().is_ok_and(|d|d.claimed);
                        if community_done(result,reply,tx,setup) {backoff.reset(); update(tx,"connecting",None); return ConnectionExit::Retry;}
                        if claimed && fresh {
                            // New membership of this relay: re-check joined rooms now.
                            jobs.abort_all();catalog_due=tokio::time::Instant::now();open_after_catalog=true;
                        }
                    },
                    // A panicked request proves nothing; its reply was dropped (unknown).
                    _=>publish_status(tx,|s|{s.communities.state="failed".into();s.communities.category=Some("relay_unavailable".into());}),
                }
            },
            _=hint_jobs.join_next(), if !hint_jobs.is_empty()=> {publish_status(tx,|s|crate::communities::refresh_hints(&mut s.communities));},
            result=invite_jobs.join_next(), if !invite_jobs.is_empty()=> {
                match result {
                    Some(Ok((crate::join::InviteStep::Checked(checked),reply)))=>invite_checked(checked,reply,tx),
                    Some(Ok((crate::join::InviteStep::Redeemed(redeemed),reply)))=> {
                        if invite_redeemed(redeemed,reply,tx) && fresh {
                            // New or confirmed relay membership: re-check joined rooms
                            // now, then the open rooms against them.
                            jobs.abort_all();catalog_due=tokio::time::Instant::now();open_after_catalog=true;
                        }
                    },
                    // A panicked request proves nothing; its reply was dropped (unknown).
                    _=>publish_status(tx,|s|s.setup=crate::join::failed("relay_unavailable")),
                }
            },
            _=tokio::time::sleep_until(progress_due), if !download_jobs.is_empty()=> {
                progress_due=tokio::time::Instant::now()+Duration::from_millis(250);
                let received=download_progress.load(std::sync::atomic::Ordering::Relaxed);
                if tx.borrow().download.received!=received {publish_status(tx,|s|if s.download.state=="downloading" {s.download.received=received;});}
            },
            result=download_jobs.join_next(), if !download_jobs.is_empty()=> {
                publish_status(tx,|s|s.download=match result {
                    Some(Ok((event_id,attachment,Ok(path))))=>crate::protocol::Download {
                        state:"done".into(),path:Some(path.to_string_lossy().into_owned()),received:attachment.size,
                        ..crate::media::downloading(&event_id,&attachment)},
                    Some(Ok((event_id,attachment,Err(category))))=>crate::protocol::Download {
                        state:"failed".into(),received:0,category:Some(category.into()),
                        ..crate::media::downloading(&event_id,&attachment)},
                    _=>crate::protocol::Download {state:"failed".into(),category:Some("relay_unavailable".into()),..s.download.clone()},
                });
            },
            result=thumb_jobs.join_next(), if !thumb_jobs.is_empty()=> {
                match result {
                    Some(Ok((hash,Ok(path)))) if path.to_string_lossy().len()<=crate::media::THUMB_PATH_BYTES=>publish_status(tx,|s| {
                        s.thumbnails.retain(|t|t.hash!=hash);
                        s.thumbnails.push(crate::protocol::Thumbnail {hash,path:path.to_string_lossy().into_owned()});
                        if s.thumbnails.len()>crate::media::THUMBNAILS {s.thumbnails.remove(0);}
                    }),
                    // A category only; no URL, path or bytes are logged.
                    Some(Ok((_,Err(category))))=>eprintln!("omarchy-buzz: preview unavailable: {category}"),
                    _=>{},
                }
                pump_thumbs!();
            },
            result=upload_jobs.join_next(), if !upload_jobs.is_empty()=> {
                match result {
                    Some(Ok((scope,Ok(mut pending))))=> {
                        pending.scope=scope.clone();
                        let name=pending.name.clone();
                        publish_status(tx,|s| {
                            let count=s.pending_attachments.iter().filter(|p|p.scope==scope).count();
                            let fits=count<crate::media::PENDING && s.pending_attachments.len()<crate::media::PENDING_TOTAL;
                            let duplicate=s.pending_attachments.iter().any(|p|p.scope==scope && p.hash==pending.hash);
                            if fits && !duplicate {s.pending_attachments.push(pending);}
                            s.upload=if fits || duplicate {crate::protocol::Upload {state:"done".into(),scope:Some(scope),name:Some(name),category:None}}
                                else {crate::protocol::Upload {state:"failed".into(),scope:Some(scope),name:Some(name),category:Some("attachment_invalid".into())}};
                        });
                    },
                    Some(Ok((scope,Err(category))))=>publish_status(tx,|s|s.upload=crate::protocol::Upload {state:"failed".into(),scope:Some(scope),name:s.upload.name.clone(),category:Some(category.into())}),
                    _=>publish_status(tx,|s|s.upload=crate::protocol::Upload {state:"failed".into(),category:Some("relay_unavailable".into()),..s.upload.clone()}),
                }
            },
            result=mint_jobs.join_next(), if !mint_jobs.is_empty()=> {
                // The code is published to the panel only; nothing here logs it.
                match result {
                    Some(Ok((minted,reply)))=> {
                        let category=minted.as_ref().err().copied();
                        publish_status(tx,|s|s.invites=match minted {Ok(m)=>crate::invites::minted(m),Err(c)=>crate::invites::failed(c)});
                        let _=reply.send(category);
                    },
                    _=>publish_status(tx,|s|s.invites=crate::invites::failed("relay_unavailable")),
                }
            },
            result=open_jobs.join_next(), if !open_jobs.is_empty()=> {
                if let Some(Ok((ticket,generation,result)))=result {
                    let status=tx.borrow();
                    let current=ticket==open_ticket && generation==status.generation && fresh && relay_pin.is_some()
                        && matches!(status.catalog.state.as_str(),"partial"|"ready");
                    let joined:std::collections::BTreeSet<String>=status.catalog.rooms.iter().map(|r|r.id.clone()).collect();
                    drop(status);
                    if current {
                        publish_status(tx,|s|s.open_rooms=match result {
                            // Rooms joined meanwhile are dropped against the current catalog.
                            Ok(rooms)=>crate::protocol::OpenRooms {state:"snapshot".into(),rooms:rooms.into_iter().filter(|r|!joined.contains(&r.id)).collect(),category:None},
                            Err(error)=> {eprintln!("omarchy-buzz: open rooms read failed: {error}");crate::protocol::OpenRooms::unavailable(Some("relay_unavailable"))},
                        });
                    }
                } else if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    publish_status(tx,|s|s.open_rooms=crate::protocol::OpenRooms::unavailable(Some("relay_unavailable")));
                }
            },
            result=recipient_jobs.join_next(), if !recipient_jobs.is_empty()=> {
                if matches!(&result,Some(Err(e)) if !e.is_cancelled()) {
                    recipient_jobs.abort_all();recipient_jobs=tokio::task::JoinSet::new();recipient_ticket=recipient_ticket.wrapping_add(1);
                    publish_status(tx, |s|s.recipients=crate::protocol::RecipientsView::unavailable(s.recipients.room_id.clone(),Some("recipients_unavailable")));
                }
                if let Some(Ok((ticket,generation,room,epoch,result)))=result {
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
                            // The roster includes this identity: its status read is this
                            // identity's too, unless a publication was accepted meanwhile.
                            if let Ok(r)=&result {
                                if r.room==room && r.statuses_known {
                                    status_expiry.retain(|key,_|*key==own_key || r.entries.iter().any(|e|e.key==*key));
                                    for entry in &r.entries {
                                        match entry.status.as_ref().and_then(|st|st.expires_at) {
                                            Some(at)=>{status_expiry.insert(entry.key.clone(),at);},
                                            None=>{status_expiry.remove(&entry.key);},
                                        }
                                    }
                                    if let Some(me)=r.entries.iter().find(|e|e.key==own_key) {
                                        if epoch==status_epoch && s.user_status.state!="sending" {
                                            s.user_status.mine=me.status.clone();
                                            if s.user_status.state=="unavailable" {s.user_status.state="ready".into();s.user_status.category=None;}
                                        }
                                    }
                                }
                            }
                            s.recipients=match result {
                        Ok(r) if r.room==room=>crate::protocol::RecipientsView {state:"snapshot".into(),room_id:Some(r.room),partial:r.partial,category:None,agents:r.agents,entries:r.entries.into_iter().map(|r|crate::protocol::Recipient {status:r.status.as_ref().map(Into::into),presence:None,key:r.key,name:r.name}).collect()},
                        Ok(_)=>crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_invalid")),
                        Err(error)=>crate::protocol::RecipientsView::unavailable(Some(room),Some(recipients_category(error))),
                    };
                            apply_presence(s,presence,&presence_seen,&own_key);
                        });
                        // A newly shown roster: read its members' presence now.
                        spawn_presence!();
                    }
                }
            },
            _=tokio::time::sleep_until(own_due.unwrap_or(due)), if own_due.is_some() && fresh && own_jobs.is_empty()=> {
                spawn_own_status!();
            },
            result=own_jobs.join_next(), if !own_jobs.is_empty()=> {
                match result {
                    Some(Ok((ticket,epoch,Ok(mine)))) if ticket==own_ticket && fresh=> {
                        own_attempts=0;
                        if epoch==status_epoch {
                            publish_status(tx,|s| {
                                if s.user_status.state!="sending" {
                                    // Others see the same status beside this identity's name.
                                    if let Some(entry)=s.recipients.entries.iter_mut().find(|e|e.key==own_key) {entry.status=mine.as_ref().map(Into::into);}
                                    s.user_status.mine=mine;
                                    if s.user_status.state=="unavailable" {s.user_status.state="ready".into();s.user_status.category=None;}
                                }
                            });
                        }
                    },
                    Some(Ok((ticket,_,Err(error)))) if ticket==own_ticket && fresh=> {
                        // A category only; the read is retried a few times.
                        eprintln!("omarchy-buzz: own status read failed: {error}");
                        if own_attempts<3 {own_attempts+=1;own_due=Some(tokio::time::Instant::now()+Duration::from_secs(5*u64::from(own_attempts)));}
                    },
                    _=>{},
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
                                rows:thread_rows(thread.rows.into_iter().map(|r|crate::protocol::ThreadRow{depth:r.depth,parent:r.parent,row:crate::protocol::HistoryRow{ reactions:None,thread:None,id:r.id,author:r.author_pubkey,time:r.timestamp,text:r.text,edited:r.edited,truncated:r.truncated,unavailable:r.unavailable,attachments:r.attachments,attachments_unavailable:r.attachments_unavailable}}).collect()),
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
                        let skew=catalog.clock_skew;
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
                            if skew.is_some() {s.clock_skew_seconds=skew;}
                            s.activity=activity.summaries();
                            if let Some(room)=removed_selection {s.history=History::unavailable(Some(room),Some("history_access_denied"));}
                            else if let Some(room)=s.history.room_id.clone().filter(|room|removed.contains(room)) {s.history=History::unavailable(Some(room),Some("history_access_denied"));}
                            if let Some(room)=lost_recipients {s.recipients=crate::protocol::RecipientsView::unavailable(Some(room),Some("recipients_access_denied"));}
                            if lost_thread {s.thread=Thread::unavailable(None,None,Some("thread_access_denied"));}
                            if let Some(delivery)=revoked_delivery {s.delivery=delivery;}
                            // An open room that is now joined is no longer offered.
                            let joined=&s.catalog.rooms;
                            s.open_rooms.rooms.retain(|r|!joined.iter().any(|j|j.id==r.id));
                        });
                        // Statuses expire on the reader's clock (the relay keeps them).
                        let now_secs=nostr::Timestamp::now().as_secs();
                        let expired:Vec<String>=status_expiry.iter().filter(|(_,at)|**at<=now_secs).map(|(k,_)|k.clone()).collect();
                        for key in &expired {status_expiry.remove(key);}
                        let mine_expired=tx.borrow().user_status.mine.as_ref().is_some_and(|m|m.expires_at.is_some_and(|at|at<=now_secs));
                        if mine_expired || !expired.is_empty() {
                            publish_status(tx,|s| {
                                if mine_expired {s.user_status.mine=None;}
                                for entry in s.recipients.entries.iter_mut() {
                                    if expired.contains(&entry.key) || (mine_expired && entry.key==own_key) {entry.status=None;}
                                }
                            });
                        }
                        let listed=tx.borrow().open_rooms.state!="unavailable";
                        if open_after_catalog || (listed && !removed.is_empty()) {open_after_catalog=false;spawn_open_rooms!();}
                        // Presence follows the joined-room check: states too old to
                        // mean anything are dropped, then the shown keys are read again.
                        if presence_read_at.is_some_and(|at|at.elapsed()>Duration::from_secs(crate::presence::FRESH_SECS)) {
                            presence_seen.clear();presence_read_at=None;project_presence!();
                        }
                        spawn_presence!();
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
                        presence_jobs.abort_all();presence_jobs=tokio::task::JoinSet::new();presence_ticket=presence_ticket.wrapping_add(1);
                        presence_seen.clear();presence_read_at=None;
                        project_presence!();
                        publish_status(tx, |s|{
                            s.activity.clear();s.catalog=crate::protocol::Catalog::unavailable(Some(category));s.history=History::unavailable(None,None);
                            s.recipients=crate::protocol::RecipientsView::unavailable(None,None);s.thread=Thread::unavailable(None,None,None);
                            s.open_rooms=crate::protocol::OpenRooms::unavailable(None);
                        });
                        open_jobs.abort_all();open_jobs=tokio::task::JoinSet::new();open_ticket=open_ticket.wrapping_add(1);
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
                    if !fresh {fresh=true;catalog_due=tokio::time::Instant::now();project_presence!();}
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
                    if let Some(view)=actions.unknown() {publish_status(tx, |s|s.room_action=view);}
                    if statuses.unknown() {publish_status(tx, |s|s.user_status=crate::user_status::failed(s,"relay_unavailable"));}
                    own_jobs.abort_all();own_jobs=tokio::task::JoinSet::new();own_ticket=own_ticket.wrapping_add(1);own_due=None;
                    // The heartbeat starts over once the session is fresh again.
                    presence.reauthenticated();
                    answer_shutdown!();
                    presence_jobs.abort_all();presence_jobs=tokio::task::JoinSet::new();presence_ticket=presence_ticket.wrapping_add(1);
                    presence_seen.clear();presence_read_at=None;
                    open_jobs.abort_all();open_jobs=tokio::task::JoinSet::new();open_ticket=open_ticket.wrapping_add(1);
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
                        Ok(Err(e))=> {
                            let error=category(&e);
                            // This session authenticated once; a rejection now is retried
                            // with backoff (observed transiently on a live relay).
                            if error=="auth_rejected" {backoff.reauth_rejected=true;}
                            return ConnectionExit::Failure(error);
                        },
                        Err(_)=>return ConnectionExit::Failure("relay_timeout"),
                    }
                },
                Ok(RelayMessage::Ok(ok))=> {
                    if let Some(delivery)=sender.acknowledge(&ok.event_id,ok.accepted) {
                        // Accepted: the attachments it carried leave the draft.
                        let sent=if delivery.state=="acknowledged" {sender.take_sent_media()} else {None};
                        publish_status(tx, |s| {
                            s.delivery=delivery;
                            if let Some((scope,hashes))=sent {s.pending_attachments.retain(|p|!(p.scope==scope && hashes.contains(&p.hash)));}
                        });
                    }
                    else if let Some(view)=actions.acknowledge(&ok.event_id,ok.accepted) {
                        // Re-check joined rooms now; open rooms follow that result.
                        if view.state=="acknowledged" && fresh {jobs.abort_all();catalog_due=tokio::time::Instant::now();open_after_catalog=true;}
                        publish_status(tx, |s|s.room_action=view);
                    }
                    else if presence.acknowledge(&ok.event_id,ok.accepted,nostr::Timestamp::now().as_secs()) {
                        project_presence!();
                        answer_shutdown!();
                    }
                    else if let Some(outcome)=statuses.acknowledge(&ok.event_id,ok.accepted,nostr::Timestamp::now().as_secs()) {
                        match outcome {
                            crate::user_status::Outcome::Accepted(mine)=> {
                                // Any read started before this answer is stale now.
                                status_epoch=status_epoch.wrapping_add(1);
                                own_jobs.abort_all();own_jobs=tokio::task::JoinSet::new();own_ticket=own_ticket.wrapping_add(1);
                                match mine.as_ref().and_then(|m|m.expires_at) {
                                    Some(at)=>{status_expiry.insert(own_key.clone(),at);},
                                    None=>{status_expiry.remove(&own_key);},
                                }
                                publish_status(tx,|s| {
                                    if let Some(entry)=s.recipients.entries.iter_mut().find(|e|e.key==own_key) {entry.status=mine.as_ref().map(Into::into);}
                                    s.user_status=crate::protocol::UserStatusView {state:"ready".into(),mine,category:None};
                                });
                                // Confirm with a read of what the relay now holds.
                                own_attempts=0;
                                own_due=Some(tokio::time::Instant::now()+Duration::from_secs(1));
                            },
                            crate::user_status::Outcome::Rejected=>publish_status(tx,|s|s.user_status=crate::user_status::failed(s,"status_rejected")),
                        }
                    }
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

/// The keys whose presence is shown: the selected room's verified roster and
/// the partners of listed DMs, without this identity, at most
/// `presence::SUBJECTS` (sorted).
fn presence_subjects(s: &Status, own: &str) -> Vec<String> {
    let mut keys = std::collections::BTreeSet::new();
    if s.recipients.state == "snapshot" {
        keys.extend(s.recipients.entries.iter().map(|e| e.key.clone()));
    }
    for room in &s.catalog.rooms {
        if room.kind == "dm" && !room.hidden {
            keys.extend(room.participants.iter().cloned());
        }
    }
    keys.remove(own);
    keys.into_iter().take(crate::presence::SUBJECTS).collect()
}
/// Publishes this identity's presence view, the verified states of others
/// (`peers`) and each roster entry's state. This identity's own entry shows
/// what the relay accepted from it. Nothing is shown without a session.
fn apply_presence(
    s: &mut Status,
    presence: &crate::presence::Publisher,
    seen: &std::collections::BTreeMap<String, &'static str>,
    own: &str,
) {
    let authenticated = s.connection == "authenticated";
    let mut view = presence.view(authenticated);
    if authenticated {
        view.peers = seen
            .iter()
            .take(crate::presence::SUBJECTS)
            .map(|(key, state)| crate::protocol::PresencePeer {
                key: key.clone(),
                presence: (*state).to_owned(),
            })
            .collect();
    }
    for entry in s.recipients.entries.iter_mut() {
        entry.presence = if !authenticated {
            None
        } else if entry.key == own {
            presence.published()
        } else {
            seen.get(&entry.key).copied()
        }
        .map(str::to_owned);
    }
    s.presence = view;
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
    let setup = crate::setup::Setup::system();
    loop {
        // Coalesce retry requests already queued for this attempt. Only this
        // sequential task owns key lookups, including any pending prompt.
        while let Ok(command) = retry.try_recv() {
            if offline_command(command, &tx, &setup, None).await {
                backoff.reset();
            }
        }
        let c = match apply_loaded_config(&tx, config::load()) {
            Ok(c) => c,
            Err(()) => {
                if !next_retry(&mut retry, &tx, &setup, None).await {
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
            if !next_retry(&mut retry, &tx, &setup, None).await {
                return;
            }
            backoff.reset();
            continue;
        }
        update(&tx, "connecting", backoff.retrying());
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
            if offline_command(command, &tx, &setup, None).await {
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
                if !next_retry(&mut retry, &tx, &setup, None).await {
                    return;
                }
                backoff.reset();
                continue;
            }
            Err(_) => {
                update(&tx, "unavailable", Some("identity_unavailable"));
                if !next_retry(&mut retry, &tx, &setup, None).await {
                    return;
                }
                backoff.reset();
                continue;
            }
        };
        let relay = c.relay.as_deref().unwrap_or_default();
        if !connect_and_observe(
            relay,
            &keys,
            &mut relay_pin,
            &tx,
            &mut retry,
            &mut backoff,
            FRESHNESS,
            &mut sender,
            &setup,
        )
        .await
        {
            return;
        }
    }
}
/// A rejected authentication (first or re-authentication) measures the clock
/// offset once, with one `HEAD` for the relay's `Date` header, and publishes it.
/// It reads `clock_skew` when the last measurement is at least
/// `clock::THRESHOLD` seconds either way. Other failures pass unchanged.
async fn explain_rejection(
    error: &'static str,
    relay: &str,
    tx: &watch::Sender<Status>,
) -> &'static str {
    if error != "auth_rejected" {
        return error;
    }
    let measured = timeout(Duration::from_secs(5), crate::clock::probe(relay))
        .await
        .ok()
        .flatten();
    if measured.is_some() {
        publish_status(tx, |s| s.clock_skew_seconds = measured);
    }
    if crate::clock::explains_rejection(tx.borrow().clock_skew_seconds) {
        "clock_skew"
    } else {
        "auth_rejected"
    }
}
/// One connection: authenticate, observe until it ends, then wait out any
/// backoff. Returns false on shutdown.
async fn connect_and_observe(
    relay: &str,
    keys: &nostr::Keys,
    relay_pin: &mut Option<nostr::PublicKey>,
    tx: &watch::Sender<Status>,
    retry: &mut mpsc::Receiver<Command>,
    backoff: &mut Backoff,
    policy: FreshnessPolicy,
    sender: &mut crate::sending::Sender,
    setup: &crate::setup::Setup,
) -> bool {
    update(tx, "connecting", backoff.retrying());
    let mut conn = match connect_identity(relay, keys).await {
        Ok(connection) => connection,
        Err(error) => {
            let error = explain_rejection(error, relay, tx).await;
            return wait_after_failure(error, backoff, retry, tx, setup, Some((relay, keys))).await;
        }
    };
    let exit = observe_sending(
        &mut conn, keys, relay, relay_pin, tx, retry, backoff, policy, sender, setup,
    )
    .await;
    // A timed-out socket is dropped before backoff; graceful close is only
    // attempted for deliberate Retry, under its own short deadline.
    match exit {
        ConnectionExit::Retry => {
            let _ = timeout(Duration::from_secs(3), conn.disconnect()).await;
            true
        }
        ConnectionExit::Shutdown => false,
        ConnectionExit::Failure(error) => {
            drop(conn);
            let error = explain_rejection(error, relay, tx).await;
            wait_after_failure(error, backoff, retry, tx, setup, Some((relay, keys))).await
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
            communities: Vec::new(),
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
                attachments: vec![],
                attachments_unavailable: false,
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
                attachments: vec![],
                attachments_unavailable: false,
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

#[cfg(test)]
#[path = "auth_reauth_tests.rs"]
mod reauth_tests;
