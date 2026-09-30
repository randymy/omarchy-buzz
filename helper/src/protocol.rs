use serde::{Deserialize, Serialize};
pub const LIMIT: usize = 65536;
// Status frames can contain 100 held channel rows and up to 200 thread replies;
// `maximum_projected_snapshot_fits_ipc_frame` measures the worst case.
// Incoming commands retain the smaller LIMIT; only projected output uses this.
pub const RESPONSE_LIMIT: usize = 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "roomId")]
    pub room_id: Option<String>,
    #[serde(rename = "rootId")]
    pub root_id: Option<String>,
    pub text: Option<String>,
    pub mentions: Option<Vec<String>>,
    pub generation: Option<u64>,
    #[serde(rename = "instanceId")]
    pub instance_id: Option<String>,
    /// `open_dm` only: the other participants' keys, never the viewer's.
    pub participants: Option<Vec<String>>,
    /// `set_relay` only: the relay address to use. The helper canonicalizes it.
    pub url: Option<String>,
    /// `claim_invite` only: the text the user pasted (link or bare code).
    pub input: Option<String>,
    /// `accept_invite` only: the code the helper parsed and published.
    pub code: Option<String>,
    /// `accept_invite` only: the join policy version shown, or `null` when the
    /// relay has none. Presence is checked on the raw frame.
    #[serde(rename = "policyVersion")]
    pub policy_version: Option<String>,
}
fn canonical_key(value: &str) -> bool {
    nostr::PublicKey::from_hex(value).is_ok_and(|key| key.to_hex() == value)
}
pub fn request(bytes: &[u8]) -> Result<Request, &'static str> {
    if bytes.len() > LIMIT {
        return Err("oversized_request");
    }
    let r: Request = serde_json::from_slice(bytes).map_err(|_| "invalid_request")?;
    let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "invalid_request")?;
    let room_present = raw.get("roomId").is_some();
    let root_present = raw.get("rootId").is_some();
    if r.version != 1 {
        return Err("incompatible_protocol");
    }
    if r.id.is_empty()
        || r.id.len() > 128
        || !r
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("invalid_request");
    }
    if !matches!(
        r.kind.as_str(),
        "get_snapshot"
            | "retry_connection"
            | "subscribe"
            | "fetch_recent"
            | "fetch_older"
            | "fetch_thread"
            | "close_thread"
            | "fetch_recipients"
            | "send_message"
            | "open_dm"
            | "set_relay"
            | "create_identity"
            | "claim_invite"
            | "accept_invite"
            | "open_rooms"
            | "join_room"
            | "leave_room"
    ) {
        return Err("unsupported_request");
    }
    if matches!(
        r.kind.as_str(),
        "fetch_recent"
            | "fetch_older"
            | "fetch_thread"
            | "fetch_recipients"
            | "send_message"
            | "join_room"
            | "leave_room"
    ) {
        let room = r.room_id.as_deref().ok_or("invalid_request")?;
        let parsed = uuid::Uuid::parse_str(room).map_err(|_| "invalid_request")?;
        if parsed.to_string() != room {
            return Err("invalid_request");
        }
    } else if room_present {
        return Err("invalid_request");
    }
    if r.kind == "fetch_thread" || (r.kind == "send_message" && root_present) {
        let root = r.root_id.as_deref().ok_or("invalid_request")?;
        if root.len() != 64
            || !root
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid_request");
        }
    } else if root_present {
        return Err("invalid_request");
    }
    // Publishing requests carry a correlation UUID and the scope they were made in.
    if matches!(r.kind.as_str(), "send_message" | "open_dm") {
        let id = uuid::Uuid::parse_str(&r.id).map_err(|_| "invalid_request")?;
        if id.to_string() != r.id {
            return Err("invalid_request");
        }
        if !r.generation.is_some_and(|g| (1..=2147483647).contains(&g)) {
            return Err("invalid_request");
        }
        let instance = r.instance_id.as_deref().ok_or("invalid_request")?;
        if instance.is_empty()
            || instance.len() > 128
            || !instance
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err("invalid_request");
        }
    } else if r.generation.is_some() || r.instance_id.is_some() {
        return Err("invalid_request");
    }
    if r.kind == "send_message" {
        let text = r.text.as_deref().ok_or("invalid_request")?;
        if text.trim().is_empty() || text.len() > 4096 || text.contains('\0') {
            return Err("invalid_request");
        }
        let mentions = r.mentions.as_ref().ok_or("invalid_request")?;
        if mentions.len() > 20 {
            return Err("invalid_request");
        }
        let mut seen = std::collections::BTreeSet::new();
        for value in mentions {
            if !canonical_key(value) || !seen.insert(value) {
                return Err("invalid_request");
            }
        }
    } else if r.text.is_some() || r.mentions.is_some() {
        return Err("invalid_request");
    }
    if r.kind == "open_dm" {
        // Pinned `build_dm_open` and relay `handle_dm_open`: 1-8 other participants.
        let participants = r.participants.as_ref().ok_or("invalid_request")?;
        if participants.is_empty() || participants.len() > 8 {
            return Err("invalid_request");
        }
        let mut seen = std::collections::BTreeSet::new();
        for value in participants {
            if !canonical_key(value) || !seen.insert(value) {
                return Err("invalid_request");
            }
        }
    } else if r.participants.is_some() {
        return Err("invalid_request");
    }
    if r.kind == "set_relay" {
        // Shape only; `ipc` refuses a non-canonical relay with a category.
        let url = r.url.as_deref().ok_or("invalid_request")?;
        if url.is_empty() || url.len() > 2048 || url.chars().any(char::is_control) {
            return Err("invalid_request");
        }
    } else if r.url.is_some() {
        return Err("invalid_request");
    }
    if r.kind == "claim_invite" {
        // Shape only; the helper parses the invite and answers with a category.
        let input = r.input.as_deref().ok_or("invalid_request")?;
        if input.trim().is_empty() || input.len() > 4096 || input.contains('\0') {
            return Err("invalid_request");
        }
    } else if raw.get("input").is_some() {
        return Err("invalid_request");
    }
    if r.kind == "accept_invite" {
        let code = r.code.as_deref().ok_or("invalid_request")?;
        if !crate::join::valid_code(code) {
            return Err("invalid_request");
        }
        // Required key: a version string, or null when no policy was shown.
        match raw.get("policyVersion") {
            Some(serde_json::Value::Null) => {}
            Some(serde_json::Value::String(v)) if crate::join::valid_version(v) => {}
            _ => return Err("invalid_request"),
        }
    } else if raw.get("code").is_some() || raw.get("policyVersion").is_some() {
        return Err("invalid_request");
    }
    // Room actions are correlated like other publishing requests.
    if matches!(r.kind.as_str(), "join_room" | "leave_room") {
        let id = uuid::Uuid::parse_str(&r.id).map_err(|_| "invalid_request")?;
        if id.to_string() != r.id {
            return Err("invalid_request");
        }
    }
    Ok(r)
}
#[derive(Clone)]
pub struct SendIntent {
    pub request_id: String,
    pub room: String,
    pub root_id: Option<String>,
    pub text: String,
    pub mentions: Vec<String>,
    pub generation: u64,
}
/// A request to open (or reopen) the DM with exactly these other participants.
#[derive(Clone)]
pub struct DmOpenIntent {
    pub request_id: String,
    pub participants: Vec<String>,
    pub generation: u64,
}
pub enum Command {
    Retry,
    FetchRecent(String),
    /// One older page for the selected room, continuing from its held cursor.
    FetchOlder(String),
    FetchThread(String, String),
    CloseThread,
    FetchRecipients(String),
    // Trusted fixture path; external IPC always uses the checked reply boundary.
    #[allow(dead_code)]
    Send(SendIntent),
    SendChecked(
        SendIntent,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    OpenDm(
        DmOpenIntent,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// Setup assist, honoured only while not authenticated. The reply is
    /// `None` once the configuration is saved and published.
    SetRelay(String, tokio::sync::oneshot::Sender<Option<&'static str>>),
    CreateIdentity(tokio::sync::oneshot::Sender<Option<&'static str>>),
    /// `community_join`: parse an invite and read the relay's join policy.
    /// Honoured while authenticated or disconnected with keys loaded.
    ClaimInvite(String, tokio::sync::oneshot::Sender<Option<&'static str>>),
    /// Accept the published policy (if any) and claim the published code.
    AcceptInvite(
        String,
        Option<String>,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// Refresh the open rooms this identity has not joined.
    FetchOpenRooms,
    /// Publish kind 9021 (join) or 9022 (leave) for a room: request id, room.
    RoomAction(
        crate::join::Action,
        String,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub request_id: Option<String>,
    pub room_id: Option<String>,
    pub event_id: Option<String>,
    pub state: String,
    pub category: Option<String>,
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            request_id: None,
            room_id: None,
            event_id: None,
            state: "idle".into(),
            category: None,
        }
    }
}
/// Outcome of the one `open_dm` a helper tracks. `channel_id` and `created`
/// come only from the relay's positive `OK` for the exact event.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmOpen {
    /// `idle`, `sending`, `acknowledged`, `rejected` or `unknown`.
    pub state: String,
    pub request_id: Option<String>,
    pub channel_id: Option<String>,
    pub created: Option<bool>,
    /// `dm_open_rejected`, `dm_open_unknown`, or `dm_open_response_unknown`
    /// (accepted, but the answer named no channel).
    pub category: Option<String>,
}
impl Default for DmOpen {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            request_id: None,
            channel_id: None,
            created: None,
            category: None,
        }
    }
}
/// A relay's join policy as shown to the user before acceptance.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPolicy {
    /// Terms, then privacy text; controls other than newline and tab removed.
    pub text: String,
    pub version: String,
    /// The relay requires a minimum-age attestation with acceptance.
    pub age_required: bool,
    /// The text was longer than the panel shows (`join::POLICY_TEXT`).
    pub truncated: bool,
}
/// The relay's positive claim answer, validated field by field.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimResult {
    /// `joined` or `already_member`.
    pub status: String,
    pub community_id: String,
    pub host: String,
    pub role: String,
}
/// Onboarding step two: the invite being redeemed (`community_join`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinSetup {
    /// `idle`, `checking`, `policy`, `claiming`, `joined` or `failed`.
    pub state: String,
    /// The parsed code, present only in `policy` (awaiting `accept_invite`).
    pub invite_code: Option<String>,
    /// The policy to accept, present only in `policy`; `null` there means none.
    pub join_policy: Option<JoinPolicy>,
    /// Present only in `joined`.
    pub claim: Option<ClaimResult>,
    /// Present only in `failed`.
    pub category: Option<String>,
}
impl Default for JoinSetup {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            invite_code: None,
            join_policy: None,
            claim: None,
            category: None,
        }
    }
}
/// An open stream room this identity has not joined (relay-signed kind 39000).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OpenRoom {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Always `stream`: only open stream rooms are listed.
    pub kind: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct OpenRooms {
    /// `unavailable`, `loading` or `snapshot`.
    pub state: String,
    /// At most `catalog::OPEN_ROOMS`, by name.
    pub rooms: Vec<OpenRoom>,
    pub category: Option<String>,
}
impl OpenRooms {
    pub fn unavailable(category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            rooms: Vec::new(),
            category: category.map(str::to_owned),
        }
    }
}
/// The one join or leave the helper tracks; outcomes only from the relay's
/// exact-ID `OK`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAction {
    /// `idle`, `sending`, `acknowledged`, `rejected` or `unknown`.
    pub state: String,
    /// `join` or `leave`; `null` when idle.
    pub action: Option<String>,
    pub request_id: Option<String>,
    pub room_id: Option<String>,
    /// `join_rejected`, `leave_rejected` or `relay_unavailable` (unknown).
    pub category: Option<String>,
}
impl Default for RoomAction {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            action: None,
            request_id: None,
            room_id: None,
            category: None,
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Recipient {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientsView {
    pub state: String,
    pub room_id: Option<String>,
    pub entries: Vec<Recipient>,
    pub agents: Vec<crate::agents::AgentHint>,
    pub partial: bool,
    pub category: Option<String>,
}
impl RecipientsView {
    pub fn unavailable(room: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            entries: Vec::new(),
            agents: Vec::new(),
            partial: true,
            category: category.map(str::to_owned),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct HistoryRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<crate::history::Reactions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<crate::history::ThreadSummary>,
    pub id: String,
    pub author: String,
    pub time: u64,
    pub text: String,
    pub edited: bool,
    pub truncated: bool,
    pub unavailable: bool,
}
/// A thread reply: a history row plus its place in the reply tree. Kept apart
/// from `HistoryRow` so channel history frames are unchanged.
#[derive(Clone, Serialize)]
pub struct ThreadRow {
    #[serde(flatten)]
    pub row: HistoryRow,
    /// 1..=64; 1 is a direct reply to the root.
    pub depth: u8,
    /// Lowercase event id: the root at depth 1, else an earlier row.
    pub parent: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub state: String,
    pub room_id: Option<String>,
    /// Oldest first: older pages, then the head page; at most `history::HELD_ROWS`.
    pub rows: Vec<HistoryRow>,
    /// True when older rows exist that are not shown.
    pub has_more: Option<bool>,
    /// `history_completeness_unknown` for a snapshot, or `history_older_unheld`
    /// when older rows exist but the held cap is reached; otherwise an error
    /// category with no rows.
    pub category: Option<String>,
    /// Present only while another older page can be requested with `fetch_older`.
    pub next_cursor: Option<crate::history::Cursor>,
    /// `idle`, `loading` or `unavailable`: the latest older-page request. A
    /// failed older page never clears the rows already shown.
    pub older_state: String,
    /// True only while the live subscription for this room is primed. Live
    /// events only trigger refetches; rows always come from verified pages.
    pub live: bool,
}
impl History {
    pub fn unavailable(room: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            rows: Vec::new(),
            has_more: None,
            category: category.map(str::to_owned),
            next_cursor: None,
            older_state: "idle".into(),
            live: false,
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub state: String,
    pub room_id: Option<String>,
    pub root_id: Option<String>,
    /// Oldest first, at most 200 (`thread::ROWS`).
    pub rows: Vec<ThreadRow>,
    /// True only when the last page read was full: more replies likely exist
    /// and are not shown. Heuristic; the legacy thread path signs no bounds.
    pub has_more: Option<bool>,
    /// Snapshot categories (none is signed exhaustion):
    /// - `thread_completeness_unknown`: the read ended on a short page, the
    ///   legacy stop heuristic; access filtering can also shorten a page.
    /// - `thread_more_unshown`: 200 replies read and the last page was full;
    ///   more replies likely exist and are not shown (`has_more`).
    /// - `thread_replies_hidden`: some replies were omitted because their parent
    ///   is deleted, withheld or not loaded; `has_more` still reports the cap.
    ///
    /// Otherwise `thread_unavailable`, `thread_timeout`, `thread_invalid` or
    /// `thread_access_denied` with no rows.
    pub category: Option<String>,
}
impl Thread {
    pub fn unavailable(room: Option<String>, root: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            root_id: root,
            rows: Vec::new(),
            has_more: None,
            category: category.map(str::to_owned),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `"stream"` or `"dm"`; always serialized so the panel validates exactly.
    pub kind: String,
    /// DM participant keys including self; always serialized, empty for streams.
    pub participants: Vec<String>,
    /// Viewer-hidden DM (NIP-DV). The panel decides whether to list it.
    pub hidden: bool,
}
#[derive(Clone, Serialize)]
pub struct Catalog {
    pub state: String,
    pub rooms: Vec<Room>,
    pub category: Option<String>,
}
impl Catalog {
    pub fn unavailable(category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            rooms: Vec::new(),
            category: category.map(str::to_owned),
        }
    }
    pub fn loading() -> Self {
        Self {
            state: "loading".into(),
            ..Self::unavailable(None)
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connection: String,
    pub identity: Option<String>,
    pub relay: Option<String>,
    pub generation: u64,
    pub category: Option<String>,
    pub catalog: Catalog,
    pub history: History,
    pub thread: Thread,
    pub delivery: Delivery,
    pub dm_open: DmOpen,
    pub recipients: RecipientsView,
    pub activity: Vec<crate::activity::Summary>,
    pub setup: JoinSetup,
    pub open_rooms: OpenRooms,
    pub room_action: RoomAction,
}
impl Status {
    pub fn new(c: &crate::config::Config) -> Self {
        Self {
            connection: "unconfigured".into(),
            identity: c.identity.clone(),
            relay: c.relay.clone(),
            generation: 1,
            category: None,
            catalog: Catalog::unavailable(None),
            history: History::unavailable(None, None),
            thread: Thread::unavailable(None, None, None),
            delivery: Delivery::default(),
            dm_open: DmOpen::default(),
            recipients: RecipientsView::unavailable(None, None),
            activity: Vec::new(),
            setup: JoinSetup::default(),
            open_rooms: OpenRooms::unavailable(None),
            room_action: RoomAction::default(),
        }
    }
}
pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"generation":s.generation,"capabilities":["connection_status","room_catalog","room_history","message_send","thread_send","room_recipients","history_auto_refresh","room_activity","agent_profiles","thread_replies","thread_summaries","dm_open","older_history","live_updates","setup_assist","community_join"],"backendRevision":crate::compatibility::BUZZ_REVISION,"status":s})
}
pub async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
) -> Result<Option<Vec<u8>>, &'static str> {
    read_line_buffered(r, &mut Vec::new()).await
}
pub async fn read_response_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
) -> Result<Option<Vec<u8>>, &'static str> {
    read_line_bounded(r, &mut Vec::new(), RESPONSE_LIMIT).await
}
// Retain partial input across select! cancellation; a future-local buffer
// would discard a split request whenever a status update wins the selection.
pub async fn read_line_buffered<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    line: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>, &'static str> {
    read_line_bounded(r, line, LIMIT).await
}
async fn read_line_bounded<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    line: &mut Vec<u8>,
    limit: usize,
) -> Result<Option<Vec<u8>>, &'static str> {
    use tokio::io::AsyncBufReadExt;
    loop {
        let b = r.fill_buf().await.map_err(|_| "io_unavailable")?;
        if b.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("incomplete_frame")
            };
        }
        let n = b
            .iter()
            .position(|x| *x == b'\n')
            .map(|p| p + 1)
            .unwrap_or(b.len());
        if line.len() + n > limit {
            return Err("oversized_request");
        }
        let end = b[n - 1] == b'\n';
        line.extend_from_slice(&b[..n]);
        r.consume(n);
        if end {
            line.pop();
            return Ok(Some(std::mem::take(line)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_authority() {
        assert!(request(br#"{"version":1,"id":"a","type":"send_message"}"#).is_err());
        assert!(request(
            br#"{"version":1,"id":"a","type":"get_snapshot","private_key":"sentinel"}"#
        )
        .is_err());
        assert!(request(&vec![b'a'; LIMIT + 1]).is_err());
    }
    #[tokio::test]
    async fn oversized_frame() {
        let data = vec![b'a'; LIMIT + 1];
        let mut r = tokio::io::BufReader::new(data.as_slice());
        assert!(read_line(&mut r).await.is_err());
    }
    #[tokio::test]
    async fn response_frame_has_separate_bounded_budget() {
        let data = vec![b'a'; LIMIT + 1];
        let mut framed = data.clone();
        framed.push(b'\n');
        let mut reader = tokio::io::BufReader::new(framed.as_slice());
        assert_eq!(read_response_line(&mut reader).await.unwrap(), Some(data));
        let oversized = vec![b'a'; RESPONSE_LIMIT + 1];
        let mut reader = tokio::io::BufReader::new(oversized.as_slice());
        assert!(read_response_line(&mut reader).await.is_err());
    }
}
#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn status_does_not_claim_sync_or_authority() {
        let status = Status::new(&crate::config::Config::default());
        let v = envelope("hello", None, "fixture-instance", &status);
        assert_eq!(v["status"]["connection"], "unconfigured");
        assert_eq!(
            v["capabilities"],
            serde_json::json!([
                "connection_status",
                "room_catalog",
                "room_history",
                "message_send",
                "thread_send",
                "room_recipients",
                "history_auto_refresh",
                "room_activity",
                "agent_profiles",
                "thread_replies",
                "thread_summaries",
                "dm_open",
                "older_history",
                "live_updates",
                "setup_assist",
                "community_join"
            ])
        );
        assert_eq!(v["status"]["catalog"]["state"], "unavailable");
        assert_eq!(v["status"]["catalog"]["rooms"], serde_json::json!([]));
        assert_eq!(v["status"]["dmOpen"]["state"], "idle");
        assert!(v["status"]["dmOpen"]["channelId"].is_null());
        assert!(v.get("privateKey").is_none());
    }
    #[test]
    fn versions_and_identifiers_are_fenced() {
        assert!(request(br#"{"version":2,"id":"a","type":"subscribe"}"#).is_err());
        assert!(request(br#"{"version":1,"id":"../a","type":"subscribe"}"#).is_err());
        assert!(request(br#"{"version":1,"id":"a","type":"subscribe"}"#).is_ok());
    }
    #[test]
    fn history_requests_require_canonical_room_scope() {
        let room = "00000000-0000-4000-8000-000000000001";
        let valid =
            serde_json::json!({"version":1,"id":"history-1","type":"fetch_recent","roomId":room});
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for value in [
            serde_json::json!({"version":1,"id":"a","type":"fetch_recent"}),
            serde_json::json!({"version":1,"id":"a","type":"fetch_recent","roomId":"../room"}),
            serde_json::json!({"version":1,"id":"a","type":"get_snapshot","roomId":room}),
        ] {
            assert!(request(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn older_history_request_carries_only_the_room() {
        let room = "aaaaaaaa-0000-4000-8000-00000000000b";
        let valid = serde_json::json!({"version":1,"id":"ui-7","type":"fetch_older","roomId":room});
        let parsed = request(&serde_json::to_vec(&valid).unwrap()).unwrap();
        assert_eq!(
            (parsed.kind.as_str(), parsed.room_id.as_deref()),
            ("fetch_older", Some(room))
        );
        // The cursor stays in the helper: the panel cannot choose a position.
        for (field, value) in [
            ("roomId", serde_json::json!("../room")),
            ("roomId", serde_json::json!(room.to_uppercase())),
            ("rootId", serde_json::json!("a".repeat(64))),
            ("text", serde_json::json!("x")),
            ("until", serde_json::json!(1)),
            ("before_id", serde_json::json!("a".repeat(64))),
            (
                "nextCursor",
                serde_json::json!({"createdAt":1,"id":"a".repeat(64)}),
            ),
        ] {
            let mut bad = valid.clone();
            bad[field] = value;
            assert!(
                request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("roomId");
        assert!(request(&serde_json::to_vec(&missing).unwrap()).is_err());
    }
    #[test]
    fn thread_requests_require_exact_scope_and_no_authority_fields() {
        let room = "00000000-0000-4000-8000-000000000001";
        let root = "a".repeat(64);
        let valid = serde_json::json!({"version":1,"id":"thread-1","type":"fetch_thread","roomId":room,"rootId":root});
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("rootId", serde_json::json!("A".repeat(64))),
            ("rootId", serde_json::json!("a".repeat(63))),
            ("rootId", serde_json::json!("g".repeat(64))),
            ("roomId", serde_json::json!("../room")),
            ("text", serde_json::json!("unauthorized")),
        ] {
            let mut bad = valid.clone();
            bad[field] = value;
            assert!(
                request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("rootId");
        assert!(request(&serde_json::to_vec(&missing).unwrap()).is_err());
        for kind in [
            "fetch_recent",
            "fetch_recipients",
            "send_message",
            "close_thread",
            "get_snapshot",
        ] {
            let mut bad = valid.clone();
            bad["type"] = serde_json::json!(kind);
            assert!(
                request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{kind}"
            );
        }
        assert!(request(br#"{"version":1,"id":"close-1","type":"close_thread"}"#).is_ok());
        assert!(request(
            &serde_json::to_vec(
                &serde_json::json!({"version":1,"id":"close-1","type":"close_thread","roomId":room})
            )
            .unwrap()
        )
        .is_err());
        assert!(
            request(br#"{"version":1,"id":"close-1","type":"close_thread","rootId":null}"#)
                .is_err()
        );
    }
    #[test]
    fn sender_contract_rejects_unscoped_or_oversized_intents() {
        let valid = serde_json::json!({
            "version":1,"id":"00000000-0000-4000-8000-000000000001","type":"send_message",
            "roomId":"00000000-0000-4000-8000-000000000002", "text":"hello", "mentions":[],
            "instanceId":"test-instance", "generation":1
        });
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        let mut reply = valid.clone();
        reply["rootId"] = serde_json::json!("a".repeat(64));
        assert!(request(&serde_json::to_vec(&reply).unwrap()).is_ok());
        for root in ["A".repeat(64), "a".repeat(63), "g".repeat(64)] {
            reply["rootId"] = serde_json::json!(root);
            assert!(request(&serde_json::to_vec(&reply).unwrap()).is_err());
        }
        reply["rootId"] = serde_json::Value::Null;
        assert!(request(&serde_json::to_vec(&reply).unwrap()).is_err());
        for field in ["roomId", "text", "mentions", "instanceId", "generation"] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("id", serde_json::json!("ui-1")),
            ("text", serde_json::json!(" ")),
            ("text", serde_json::json!("é".repeat(2049))),
            ("generation", serde_json::json!(0)),
            ("instanceId", serde_json::json!("../instance")),
            ("mentions", serde_json::json!(["@codex"])),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "invalid {field}"
            );
        }
        let mut signing = valid.clone();
        signing["kind"] = serde_json::json!(9);
        assert!(request(&serde_json::to_vec(&signing).unwrap()).is_err());
        let mut read = valid;
        read["type"] = serde_json::json!("fetch_recent");
        assert!(request(&serde_json::to_vec(&read).unwrap()).is_err());
    }
    #[test]
    fn open_dm_contract_requires_scoped_bounded_distinct_keys() {
        let key = |c: char| {
            nostr::Keys::parse(&c.to_string().repeat(64))
                .unwrap()
                .public_key()
                .to_hex()
        };
        let valid = serde_json::json!({
            "version":1,"id":"00000000-0000-4000-8000-000000000001","type":"open_dm",
            "participants":[key('1')],"instanceId":"test-instance","generation":1
        });
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        let mut eight = valid.clone();
        eight["participants"] = serde_json::json!("12345678".chars().map(key).collect::<Vec<_>>());
        assert!(request(&serde_json::to_vec(&eight).unwrap()).is_ok());
        for field in ["participants", "instanceId", "generation"] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("id", serde_json::json!("ui-1")),
            ("participants", serde_json::json!([])),
            (
                "participants",
                serde_json::json!("123456789".chars().map(key).collect::<Vec<_>>()),
            ),
            ("participants", serde_json::json!([key('1'), key('1')])),
            ("participants", serde_json::json!([key('1').to_uppercase()])),
            ("participants", serde_json::json!(["npub1invalid"])),
            ("participants", serde_json::json!([&key('1')[..63]])),
            (
                "roomId",
                serde_json::json!("00000000-0000-4000-8000-000000000002"),
            ),
            ("rootId", serde_json::json!("a".repeat(64))),
            ("text", serde_json::json!("hello")),
            ("mentions", serde_json::json!([])),
            ("kind", serde_json::json!(41010)),
            ("generation", serde_json::json!(0)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "invalid {field}"
            );
        }
        // Participants are refused on every other request.
        let mut send = serde_json::json!({
            "version":1,"id":"00000000-0000-4000-8000-000000000001","type":"send_message",
            "roomId":"00000000-0000-4000-8000-000000000002", "text":"hello", "mentions":[],
            "instanceId":"test-instance", "generation":1
        });
        send["participants"] = serde_json::json!([key('1')]);
        assert!(request(&serde_json::to_vec(&send).unwrap()).is_err());
        assert!(request(
            &serde_json::to_vec(&serde_json::json!({"version":1,"id":"a","type":"subscribe","participants":[key('1')]}))
                .unwrap()
        )
        .is_err());
    }
    #[test]
    fn setup_requests_carry_only_their_own_fields() {
        let relay = serde_json::json!({"version":1,"id":"ui-3","type":"set_relay","url":"wss://example.com"});
        let parsed = request(&serde_json::to_vec(&relay).unwrap()).unwrap();
        assert_eq!(parsed.url.as_deref(), Some("wss://example.com"));
        // Shape is checked here; canonical scope is refused by ipc with a category.
        let mut unchecked = relay.clone();
        unchecked["url"] = serde_json::json!("http://example.com");
        assert!(request(&serde_json::to_vec(&unchecked).unwrap()).is_ok());
        for (field, value) in [
            ("url", serde_json::json!("")),
            ("url", serde_json::json!("wss://a\n.example")),
            (
                "url",
                serde_json::json!(format!("wss://{}", "a".repeat(2048))),
            ),
            ("url", serde_json::json!(7)),
            ("url", serde_json::Value::Null),
            (
                "roomId",
                serde_json::json!("00000000-0000-4000-8000-000000000001"),
            ),
            ("text", serde_json::json!("x")),
            ("identity", serde_json::json!("a".repeat(64))),
            ("privateKey", serde_json::json!("a".repeat(64))),
            ("generation", serde_json::json!(1)),
        ] {
            let mut bad = relay.clone();
            bad[field] = value;
            assert!(
                request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut missing = relay.clone();
        missing.as_object_mut().unwrap().remove("url");
        assert!(request(&serde_json::to_vec(&missing).unwrap()).is_err());
        let create = serde_json::json!({"version":1,"id":"ui-4","type":"create_identity"});
        assert!(request(&serde_json::to_vec(&create).unwrap()).is_ok());
        for (field, value) in [
            ("url", serde_json::json!("wss://example.com")),
            ("secret", serde_json::json!("a".repeat(64))),
            ("identity", serde_json::json!("a".repeat(64))),
            ("participants", serde_json::json!([])),
        ] {
            let mut bad = create.clone();
            bad[field] = value;
            assert!(
                request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{field}"
            );
        }
        // No other request may carry a relay address.
        assert!(request(
            br#"{"version":1,"id":"a","type":"retry_connection","url":"wss://example.com"}"#
        )
        .is_err());
    }
    #[test]
    fn join_requests_carry_only_their_own_fields() {
        let parse = |v: &serde_json::Value| request(&serde_json::to_vec(v).unwrap());
        let code = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        let room = "00000000-0000-4000-8000-000000000001";
        let id = "00000000-0000-4000-8000-000000000002";
        let claim = serde_json::json!({"version":1,"id":"ui-1","type":"claim_invite","input":"https://relay.example/invite/x"});
        assert_eq!(
            parse(&claim).unwrap().input.as_deref(),
            Some("https://relay.example/invite/x")
        );
        let accept = serde_json::json!({"version":1,"id":"ui-2","type":"accept_invite","code":code,"policyVersion":"v1"});
        let parsed = parse(&accept).unwrap();
        assert_eq!(
            (parsed.code.as_deref(), parsed.policy_version.as_deref()),
            (Some(code), Some("v1"))
        );
        let mut none = accept.clone();
        none["policyVersion"] = serde_json::Value::Null;
        assert!(parse(&none).unwrap().policy_version.is_none());
        let open = serde_json::json!({"version":1,"id":"ui-3","type":"open_rooms"});
        assert!(parse(&open).is_ok());
        let join = serde_json::json!({"version":1,"id":id,"type":"join_room","roomId":room});
        assert_eq!(parse(&join).unwrap().room_id.as_deref(), Some(room));
        let mut leave = join.clone();
        leave["type"] = serde_json::json!("leave_room");
        assert!(parse(&leave).is_ok());
        for (base, field, value) in [
            (&claim, "input", serde_json::json!("")),
            (&claim, "input", serde_json::json!("  ")),
            (&claim, "input", serde_json::json!("a\u{0}b")),
            (&claim, "input", serde_json::json!("x".repeat(4097))),
            (&claim, "input", serde_json::Value::Null),
            (&claim, "code", serde_json::json!(code)),
            (&claim, "url", serde_json::json!("wss://relay.example")),
            (&claim, "privateKey", serde_json::json!("a".repeat(64))),
            (&accept, "code", serde_json::json!("has space")),
            (&accept, "code", serde_json::json!("")),
            (&accept, "policyVersion", serde_json::json!("")),
            (&accept, "policyVersion", serde_json::json!(1)),
            (&accept, "input", serde_json::json!("x")),
            (&accept, "roomId", serde_json::json!(room)),
            (&open, "roomId", serde_json::json!(room)),
            (&open, "input", serde_json::json!("x")),
            (&join, "id", serde_json::json!("ui-4")),
            (&join, "roomId", serde_json::json!("../room")),
            (&join, "code", serde_json::json!(code)),
            (&join, "generation", serde_json::json!(1)),
            (&set_relay_frame(), "input", serde_json::json!("x")),
        ] {
            let mut bad = base.clone();
            bad[field] = value;
            assert!(parse(&bad).is_err(), "{bad}");
        }
        for (base, field) in [
            (&claim, "input"),
            (&accept, "code"),
            (&accept, "policyVersion"),
            (&join, "roomId"),
        ] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse(&missing).is_err(), "{missing}");
        }
    }
    fn set_relay_frame() -> serde_json::Value {
        serde_json::json!({"version":1,"id":"ui-5","type":"set_relay","url":"wss://relay.example"})
    }
    #[test]
    fn maximum_projected_snapshot_fits_ipc_frame() {
        let mut status = Status::new(&crate::config::Config::default());
        status.relay = Some("x".repeat(2048));
        status.identity = Some("a".repeat(64));
        // Quotes/backslashes expand on JSON encoding. Projection replaces controls.
        status.catalog.rooms = (0..20)
            .map(|_| Room {
                id: "00000000-0000-4000-8000-000000000001".into(),
                name: "\\".repeat(128),
                description: "\\".repeat(256),
                kind: "stream".into(),
                participants: (0..9).map(|_| "e".repeat(64)).collect(),
                hidden: true,
            })
            .collect();
        status.history.rows = (0..crate::history::HELD_ROWS)
            .map(|_| HistoryRow {
                reactions: None,
                thread: None,
                id: "a".repeat(64),
                author: "b".repeat(64),
                time: u64::MAX,
                text: "\\".repeat(768),
                edited: true,
                truncated: true,
                unavailable: false,
            })
            .collect();
        status.history.has_more = Some(true);
        status.history.category = Some("history_completeness_unknown".into());
        status.history.next_cursor = Some(crate::history::Cursor {
            created_at: u64::MAX,
            id: "f".repeat(64),
        });
        status.history.older_state = "unavailable".into();
        status.thread.room_id = Some("00000000-0000-4000-8000-000000000001".into());
        status.thread.root_id = Some("a".repeat(64));
        for row in &mut status.history.rows {
            row.thread = Some(crate::history::ThreadSummary {
                replies: 1_000_000,
                last_reply_at: Some(u64::MAX),
                participants: vec!["c".repeat(64); 10],
            });
        }
        status.thread.rows = (0..crate::thread::ROWS)
            .map(|_| ThreadRow {
                row: status.history.rows[0].clone(),
                depth: 64,
                parent: "e".repeat(64),
            })
            .collect();
        status.thread.has_more = Some(true);
        status.thread.category = Some("thread_replies_hidden".into());
        status.recipients.entries = (0..20)
            .map(|_| Recipient {
                key: "c".repeat(64),
                name: "\\".repeat(64),
            })
            .collect();
        status.activity = (0..20)
            .map(|_| crate::activity::Summary {
                room_id: "00000000-0000-4000-8000-000000000001".into(),
                epoch: u64::MAX,
                observed: 1_000_000_000,
            })
            .collect();
        status.recipients.agents = (0..10)
            .map(|_| crate::agents::AgentHint {
                key: "c".repeat(64),
                name: "\\".repeat(64),
                profile_event_id: "d".repeat(64),
                execution_state: "unknown",
            })
            .collect();
        status.delivery = Delivery {
            request_id: Some("00000000-0000-4000-8000-000000000001".into()),
            room_id: Some("00000000-0000-4000-8000-000000000002".into()),
            event_id: Some("a".repeat(64)),
            state: "acknowledged".into(),
            category: None,
        };
        status.dm_open = DmOpen {
            state: "acknowledged".into(),
            request_id: Some("00000000-0000-4000-8000-000000000003".into()),
            channel_id: Some("00000000-0000-4000-8000-000000000004".into()),
            created: Some(true),
            category: Some("dm_open_response_unknown".into()),
        };
        status.setup = JoinSetup {
            state: "policy".into(),
            invite_code: Some("a".repeat(crate::join::CODE_BYTES)),
            join_policy: Some(JoinPolicy {
                text: "\\".repeat(crate::join::POLICY_TEXT),
                version: "\\".repeat(128),
                age_required: true,
                truncated: true,
            }),
            claim: None,
            category: None,
        };
        status.open_rooms = OpenRooms {
            state: "snapshot".into(),
            rooms: (0..crate::catalog::OPEN_ROOMS)
                .map(|_| OpenRoom {
                    id: "00000000-0000-4000-8000-000000000001".into(),
                    name: "\\".repeat(128),
                    description: "\\".repeat(256),
                    kind: "stream".into(),
                })
                .collect(),
            category: None,
        };
        status.room_action = RoomAction {
            state: "rejected".into(),
            action: Some("leave".into()),
            request_id: Some("00000000-0000-4000-8000-000000000005".into()),
            room_id: Some("00000000-0000-4000-8000-000000000006".into()),
            category: Some("leave_rejected".into()),
        };
        let encoded = serde_json::to_vec(&envelope(
            "status",
            Some(&"a".repeat(128)),
            &"i".repeat(128),
            &status,
        ))
        .unwrap();
        eprintln!(
            "maximum projected status frame: {} bytes",
            encoded.len() + 1
        );
        assert!(
            encoded.len() + 1 <= RESPONSE_LIMIT,
            "{} byte frame",
            encoded.len()
        );
    }
    #[tokio::test]
    async fn requires_complete_frames() {
        let mut r = tokio::io::BufReader::new(&b"{}"[..]);
        assert_eq!(read_line(&mut r).await.unwrap_err(), "incomplete_frame");
        let mut r = tokio::io::BufReader::new(&b"{}\n{}\n"[..]);
        assert_eq!(read_line(&mut r).await.unwrap().unwrap(), b"{}");
        assert_eq!(read_line(&mut r).await.unwrap().unwrap(), b"{}");
        assert!(read_line(&mut r).await.unwrap().is_none());
    }
}
#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn partial_request_survives_status_update() {
        let (mut sender, receiver) = tokio::io::duplex(256);
        let mut receiver = tokio::io::BufReader::new(receiver);
        let mut partial = Vec::new();
        sender.write_all(b"{\"version\":1,").await.unwrap();
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(20),
            read_line_buffered(&mut receiver, &mut partial)
        )
        .await
        .is_err());
        sender
            .write_all(b"\"id\":\"a\",\"type\":\"subscribe\"}\n")
            .await
            .unwrap();
        let frame = read_line_buffered(&mut receiver, &mut partial)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request(&frame).unwrap().kind, "subscribe");
        assert!(partial.is_empty());
    }
}
