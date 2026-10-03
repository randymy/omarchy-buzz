use serde::{Deserialize, Serialize};
pub const LIMIT: usize = 65536;
// Status frames can contain 100 held channel rows and up to 200 thread replies;
// `maximum_projected_snapshot_fits_ipc_frame` measures the worst case.
// Incoming commands retain the smaller LIMIT; only projected output uses this.
pub const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
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
    /// `mint_invite` only: how many people may use the invite (1-100).
    #[serde(rename = "maxUses")]
    pub max_uses: Option<u32>,
    /// `mint_invite` only: hours until the invite expires (1-720).
    #[serde(rename = "expiresInHours")]
    pub expires_in_hours: Option<u32>,
    /// `download_attachment`/`thumbnail_attachment` only: the row carrying it.
    #[serde(rename = "eventId")]
    pub event_id: Option<String>,
    /// Attachment requests: the attachment's SHA-256 (lowercase hex).
    pub hash: Option<String>,
    /// `upload_attachment`: the file the user typed; `open_download`: a path
    /// the helper reported. Both are checked again by the helper.
    pub path: Option<String>,
    /// `set_status` only: a native emoji or `:shortcode:` (`user_status::valid_emoji`).
    pub emoji: Option<String>,
    /// `set_presence` only: `auto`, `away` or `offline` (`presence::Mode`).
    pub mode: Option<String>,
    /// `set_presence` only: the panel's idle hint (input within the threshold).
    pub active: Option<bool>,
    /// `switch_community`/`rename_community`/`leave_community`: a configured
    /// community's relay (canonicalized and checked by the helper).
    pub relay: Option<String>,
    /// `rename_community` (the new local label, sanitized by the helper) and
    /// `create_room`/`update_room` (the room name, checked by `rooms`).
    pub name: Option<String>,
    /// `search_people` only: the typed search text, empty for the directory.
    /// The helper normalizes and bounds it (`recipients::people_query`).
    pub query: Option<String>,
    /// `create_room` (optional) and `update_room`: the room description.
    pub about: Option<String>,
    /// `set_room_topic` only.
    pub topic: Option<String>,
    /// `create_room` only: `open` or `private`.
    pub visibility: Option<String>,
    /// `add_room_member`/`remove_room_member` only: the member's key (hex).
    pub key: Option<String>,
}
fn is_hash(value: &str) -> bool {
    crate::attachments::is_hash(value)
}
/// An absolute path without `..`, `.` or empty components, controls or NUL.
pub fn plain_absolute_path(value: &str) -> bool {
    value.len() <= 4096
        && value.starts_with('/')
        && value.len() > 1
        && !value.chars().any(char::is_control)
        && value[1..]
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
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
            | "search_people"
            | "send_message"
            | "edit_message"
            | "delete_message"
            | "add_reaction"
            | "remove_reaction"
            | "open_dm"
            | "set_relay"
            | "create_identity"
            | "claim_invite"
            | "accept_invite"
            | "open_rooms"
            | "join_room"
            | "leave_room"
            | "mint_invite"
            | "download_attachment"
            | "thumbnail_attachment"
            | "open_download"
            | "upload_attachment"
            | "remove_pending_attachment"
            | "set_status"
            | "clear_status"
            | "set_presence"
            | "join_community"
            | "switch_community"
            | "rename_community"
            | "leave_community"
            | "create_room"
            | "update_room"
            | "set_room_topic"
            | "add_room_member"
            | "remove_room_member"
            | "fetch_room_detail"
            | "load_more_rooms"
            | "refresh_rooms"
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
            | "edit_message"
            | "delete_message"
            | "add_reaction"
            | "remove_reaction"
            | "join_room"
            | "leave_room"
            | "upload_attachment"
            | "update_room"
            | "set_room_topic"
            | "add_room_member"
            | "remove_room_member"
            | "fetch_room_detail"
    ) {
        let room = r.room_id.as_deref().ok_or("invalid_request")?;
        let parsed = uuid::Uuid::parse_str(room).map_err(|_| "invalid_request")?;
        if parsed.to_string() != room {
            return Err("invalid_request");
        }
    } else if room_present {
        return Err("invalid_request");
    }
    if r.kind == "fetch_thread"
        || (matches!(r.kind.as_str(), "send_message" | "upload_attachment") && root_present)
    {
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
    // Publishing requests carry a correlation UUID and the scope they were made
    // in; so do community changes that replace or leave the session's relay.
    if matches!(
        r.kind.as_str(),
        "send_message"
            | "edit_message"
            | "delete_message"
            | "add_reaction"
            | "remove_reaction"
            | "open_dm"
            | "join_community"
            | "switch_community"
            | "leave_community"
    ) {
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
        // May be empty when the draft carries attachments; the sender checks.
        let text = r.text.as_deref().ok_or("invalid_request")?;
        if text.len() > 4096 || text.contains('\0') {
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
    } else if r.kind == "edit_message" {
        // Shape only; `sending` checks the target and that the text changes nothing else.
        let text = r.text.as_deref().ok_or("invalid_request")?;
        if text.trim().is_empty() || text.len() > 4096 || text.contains('\0') {
            return Err("invalid_request");
        }
        if r.mentions.is_some() {
            return Err("invalid_request");
        }
    } else if (r.text.is_some() && r.kind != "set_status") || r.mentions.is_some() {
        return Err("invalid_request");
    }
    if matches!(
        r.kind.as_str(),
        "edit_message" | "delete_message" | "add_reaction" | "remove_reaction"
    ) && !r.event_id.as_deref().is_some_and(is_hash)
    {
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
    if r.kind == "search_people" {
        // Shape only; an empty query lists the directory.
        let query = r.query.as_deref().ok_or("invalid_request")?;
        if query.len() > 256 || query.contains('\0') {
            return Err("invalid_request");
        }
    } else if raw.get("query").is_some() {
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
    if matches!(
        r.kind.as_str(),
        "switch_community" | "rename_community" | "leave_community"
    ) {
        // Shape only; the helper answers `community_unknown` for anything else.
        let relay = r.relay.as_deref().ok_or("invalid_request")?;
        if relay.is_empty() || relay.len() > 2048 || relay.chars().any(char::is_control) {
            return Err("invalid_request");
        }
    } else if raw.get("relay").is_some() {
        return Err("invalid_request");
    }
    if matches!(
        r.kind.as_str(),
        "rename_community" | "create_room" | "update_room"
    ) {
        // Shape only; the helper sanitizes and bounds it (`config::label`,
        // `rooms::plain`).
        let name = r.name.as_deref().ok_or("invalid_request")?;
        if name.trim().is_empty() || name.len() > 1024 || name.contains('\0') {
            return Err("invalid_request");
        }
    } else if raw.get("name").is_some() {
        return Err("invalid_request");
    }
    // Room management (`rooms`): shape only; `rooms::plain` bounds the text.
    let shaped = |value: &Option<String>| {
        value
            .as_deref()
            .is_some_and(|v| v.len() <= 2048 && !v.contains('\0'))
    };
    match r.kind.as_str() {
        "create_room" if raw.get("about").is_none() || shaped(&r.about) => {}
        "update_room" if shaped(&r.about) => {}
        "create_room" | "update_room" => return Err("invalid_request"),
        _ if raw.get("about").is_some() => return Err("invalid_request"),
        _ => {}
    }
    if r.kind == "set_room_topic" {
        if !shaped(&r.topic) {
            return Err("invalid_request");
        }
    } else if raw.get("topic").is_some() {
        return Err("invalid_request");
    }
    if r.kind == "create_room" {
        if !matches!(r.visibility.as_deref(), Some("open" | "private")) {
            return Err("invalid_request");
        }
    } else if raw.get("visibility").is_some() {
        return Err("invalid_request");
    }
    if matches!(r.kind.as_str(), "add_room_member" | "remove_room_member") {
        if !r.key.as_deref().is_some_and(canonical_key) {
            return Err("invalid_request");
        }
    } else if raw.get("key").is_some() {
        return Err("invalid_request");
    }
    if r.kind == "claim_invite" || r.kind == "join_community" {
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
    if r.kind == "mint_invite" {
        let uses = r.max_uses.ok_or("invalid_request")?;
        let hours = r.expires_in_hours.ok_or("invalid_request")?;
        if !crate::invites::MAX_USES.contains(&uses) || !crate::invites::HOURS.contains(&hours) {
            return Err("invalid_request");
        }
    } else if r.max_uses.is_some()
        || (raw.get("expiresInHours").is_some() && r.kind != "set_status")
    {
        return Err("invalid_request");
    }
    if r.kind == "set_status" {
        // Shape only; `user_status::check` answers `status_invalid` with the
        // exact bounds. `emoji` and `expiresInHours` are optional, never null.
        let text = r.text.as_deref().ok_or("invalid_request")?;
        if text.len() > 1024 || text.contains('\0') {
            return Err("invalid_request");
        }
        match raw.get("emoji") {
            None => {}
            Some(serde_json::Value::String(e)) if !e.is_empty() && e.len() <= 128 => {}
            _ => return Err("invalid_request"),
        }
        match raw.get("expiresInHours") {
            None => {}
            Some(serde_json::Value::Number(_)) if r.expires_in_hours.is_some() => {}
            _ => return Err("invalid_request"),
        }
    } else if matches!(r.kind.as_str(), "add_reaction" | "remove_reaction") {
        // Shape only; `sending` applies `history::chip_emoji`.
        match raw.get("emoji") {
            Some(serde_json::Value::String(e)) if !e.is_empty() && e.len() <= 128 => {}
            _ => return Err("invalid_request"),
        }
    } else if raw.get("emoji").is_some() {
        return Err("invalid_request");
    }
    if r.kind == "set_presence" {
        // Both required, never null.
        if !matches!(raw.get("mode"), Some(serde_json::Value::String(m)) if crate::presence::Mode::parse(m).is_some())
            || !matches!(raw.get("active"), Some(serde_json::Value::Bool(_)))
        {
            return Err("invalid_request");
        }
    } else if raw.get("mode").is_some() || raw.get("active").is_some() {
        return Err("invalid_request");
    }
    if matches!(
        r.kind.as_str(),
        "download_attachment" | "thumbnail_attachment"
    ) {
        if !r.event_id.as_deref().is_some_and(is_hash) {
            return Err("invalid_request");
        }
    } else if raw.get("eventId").is_some()
        && !matches!(
            r.kind.as_str(),
            "edit_message" | "delete_message" | "add_reaction" | "remove_reaction"
        )
    {
        return Err("invalid_request");
    }
    if matches!(
        r.kind.as_str(),
        "download_attachment" | "thumbnail_attachment" | "remove_pending_attachment"
    ) {
        if !r.hash.as_deref().is_some_and(is_hash) {
            return Err("invalid_request");
        }
    } else if raw.get("hash").is_some() {
        return Err("invalid_request");
    }
    if matches!(r.kind.as_str(), "upload_attachment" | "open_download") {
        if !r.path.as_deref().is_some_and(plain_absolute_path) {
            return Err("invalid_request");
        }
    } else if raw.get("path").is_some() {
        return Err("invalid_request");
    }
    // Room actions and mints are correlated like other publishing requests.
    if matches!(
        r.kind.as_str(),
        "join_room"
            | "leave_room"
            | "mint_invite"
            | "set_status"
            | "clear_status"
            | "set_presence"
            | "rename_community"
            | "create_room"
            | "update_room"
            | "set_room_topic"
            | "add_room_member"
            | "remove_room_member"
    ) {
        let id = uuid::Uuid::parse_str(&r.id).map_err(|_| "invalid_request")?;
        if id.to_string() != r.id {
            return Err("invalid_request");
        }
    }
    Ok(r)
}
/// Requests that publish through the single message-delivery slot.
pub fn publishes_message(kind: &str) -> bool {
    matches!(
        kind,
        "send_message" | "edit_message" | "delete_message" | "add_reaction" | "remove_reaction"
    )
}
/// What a send request publishes. The target is an event id; `SendIntent::text`
/// carries an edit's new text or a reaction's emoji.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Post,
    Edit(String),
    Delete(String),
    React(String),
    Unreact(String),
}
/// The publishing room requests the helper answers by the relay's `OK`
/// (`join::RoomActions`), one at a time.
pub fn publishes_room_change(kind: &str) -> bool {
    matches!(
        kind,
        "join_room"
            | "leave_room"
            | "create_room"
            | "update_room"
            | "set_room_topic"
            | "add_room_member"
            | "remove_room_member"
    )
}
/// The `rooms::Change` a checked `create_room`..`remove_room_member` request
/// names; none when its text is outside `rooms` bounds.
pub fn room_change(r: &Request) -> Option<crate::rooms::Change> {
    use crate::rooms::{plain, Change, ABOUT_BYTES, NAME_BYTES, TOPIC_BYTES};
    let room = || {
        r.room_id
            .as_deref()
            .and_then(|v| uuid::Uuid::parse_str(v).ok())
    };
    let key = || {
        r.key
            .as_deref()
            .and_then(|v| nostr::PublicKey::from_hex(v).ok())
    };
    let name = || plain(r.name.as_deref()?, NAME_BYTES).filter(|n| !n.is_empty());
    let about = || plain(r.about.as_deref().unwrap_or_default(), ABOUT_BYTES);
    Some(match r.kind.as_str() {
        "create_room" => Change::Create {
            name: name()?,
            about: about()?,
            private: r.visibility.as_deref() == Some("private"),
        },
        "update_room" => Change::Details {
            room: room()?,
            name: name()?,
            about: about()?,
        },
        "set_room_topic" => Change::Topic {
            room: room()?,
            topic: plain(r.topic.as_deref()?, TOPIC_BYTES)?,
        },
        "add_room_member" => Change::AddMember {
            room: room()?,
            key: key()?,
        },
        "remove_room_member" => Change::RemoveMember {
            room: room()?,
            key: key()?,
        },
        _ => return None,
    })
}
#[derive(Clone)]
pub struct SendIntent {
    pub action: Action,
    pub request_id: String,
    pub room: String,
    pub root_id: Option<String>,
    pub text: String,
    pub mentions: Vec<String>,
    pub generation: u64,
}
/// `set_status` (with `set`) or `clear_status` (without).
#[derive(Clone)]
pub struct StatusIntent {
    pub set: Option<StatusSet>,
}
#[derive(Clone)]
pub struct StatusSet {
    pub text: String,
    pub emoji: Option<String>,
    /// `None` is the default (`user_status::DEFAULT_HOURS`).
    pub hours: Option<u32>,
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
    /// A people directory (empty) or search read: the request ID, which the
    /// view echoes, and the text, already normalized by the helper.
    SearchPeople(String, String),
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
    /// `room_manage`: create a room or change one (request id).
    RoomChange(
        crate::rooms::Change,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// `room_manage`: read a joined stream room's roster, roles and topic.
    FetchRoomDetail(String),
    /// Read the joined-room catalog again now; `true` also loads its next page.
    RefreshRooms(bool),
    /// Publish kind 9021 (join) or 9022 (leave) for a room: request id, room.
    RoomAction(
        crate::join::Action,
        String,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// `invite_mint`: mint an invite (max uses, hours). Honoured only while
    /// authenticated.
    MintInvite(u32, u32, tokio::sync::oneshot::Sender<Option<&'static str>>),
    /// `attachments`: download a held row's attachment (event id, hash).
    DownloadAttachment(
        String,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// Verify and cache a held row's image for an inline preview.
    ThumbnailAttachment(
        String,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// `xdg-open` a file this helper saved.
    OpenDownload(String, tokio::sync::oneshot::Sender<Option<&'static str>>),
    /// Upload a file for the draft in (room, thread root).
    UploadAttachment(
        String,
        Option<String>,
        String,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// Drop a pending attachment (hash) from every draft.
    RemovePendingAttachment(String, tokio::sync::oneshot::Sender<Option<&'static str>>),
    /// `user_status`: publish or clear this identity's status.
    SetStatus(
        StatusIntent,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// `presence`: the panel's preference and idle hint.
    SetPresence(
        crate::presence::Mode,
        bool,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// The last panel bridge left: publish `offline` once, then stop.
    PresenceDetach,
    /// `communities`: join, switch, rename or leave. The generation is the
    /// scope the request was made in (none for a rename).
    Community(
        crate::communities::Request,
        Option<u64>,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
    /// The helper is stopping: publish `offline` (best effort); the reply
    /// comes once the relay answered or nothing was sent.
    PresenceShutdown(tokio::sync::oneshot::Sender<()>),
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
    /// `join`, `leave`, `create`, `details`, `topic`, `add_member` or
    /// `remove_member`; `null` when idle. For `create`, `room_id` is the new room.
    pub action: Option<String>,
    pub request_id: Option<String>,
    pub room_id: Option<String>,
    /// `<action>_rejected` (`join_rejected`, `leave_rejected`, `create_rejected`,
    /// `edit_rejected`, `member_add_rejected`, `member_remove_rejected`) or
    /// `relay_unavailable` (unknown).
    pub category: Option<String>,
    /// Only with `rejected`: the relay's own refusal text, plain, bounded
    /// (`join::REFUSAL_BYTES`).
    pub detail: Option<String>,
}
impl Default for RoomAction {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            action: None,
            request_id: None,
            room_id: None,
            category: None,
            detail: None,
        }
    }
}
/// One member of a room's verified roster (`rooms::Detail`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RoomMember {
    pub key: String,
    /// A profile name hint; empty when unknown.
    pub name: String,
    pub role: String,
}
/// A joined stream room's relay-signed roster, roles and topic
/// (`room_manage`). Owners and admins come first.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDetailView {
    /// `unavailable`, `loading` or `snapshot`.
    pub state: String,
    pub room_id: Option<String>,
    pub topic: String,
    /// `open` or `private` in a snapshot, else empty.
    pub visibility: String,
    /// This identity's role in a snapshot, else empty.
    pub role: String,
    pub members: Vec<RoomMember>,
    /// Members past `rooms::MEMBERS` are not listed.
    pub truncated: bool,
    /// `room_detail_unavailable`, `room_detail_invalid` or
    /// `room_detail_access_denied`.
    pub category: Option<String>,
}
impl RoomDetailView {
    pub fn unavailable(room: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            topic: String::new(),
            visibility: String::new(),
            role: String::new(),
            members: Vec::new(),
            truncated: false,
            category: category.map(str::to_owned),
        }
    }
}
impl From<crate::rooms::Detail> for RoomDetailView {
    fn from(d: crate::rooms::Detail) -> Self {
        Self {
            state: "snapshot".into(),
            room_id: Some(d.room),
            topic: d.topic,
            visibility: d.visibility,
            role: d.role,
            members: d
                .members
                .into_iter()
                .map(|m| RoomMember {
                    key: m.key,
                    name: m.name,
                    role: m.role,
                })
                .collect(),
            truncated: d.truncated,
            category: None,
        }
    }
}
/// The last invite minted here (`invite_mint`). The code is a credential for
/// joining: shown to the owner only, never logged.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invites {
    /// `idle`, `minting`, `minted` or `failed`.
    pub state: String,
    /// Present only in `minted`: a canonical v2 code.
    pub code: Option<String>,
    /// Unix seconds; present only in `minted`.
    pub expires_at: Option<u64>,
    pub max_uses: Option<u32>,
    /// Always `member` in `minted`: every claimed invite grants it.
    pub role: Option<String>,
    /// Present only in `failed`: `invite_forbidden`, `invite_rejected`,
    /// `invite_rate_limited`, `relay_unavailable` or `setup_busy`.
    pub category: Option<String>,
}
impl Default for Invites {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            code: None,
            expires_at: None,
            max_uses: None,
            role: None,
            category: None,
        }
    }
}
/// The one download the helper tracks (`attachments`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// `idle`, `downloading`, `done` or `failed`.
    pub state: String,
    pub event_id: Option<String>,
    pub hash: Option<String>,
    /// Present only in `done`: the saved file under the Downloads directory.
    pub path: Option<String>,
    /// Bytes received and verified so far.
    pub received: u64,
    /// The declared size.
    pub size: Option<u64>,
    /// Present only in `failed`: `attachment_unknown`, `attachment_forbidden`,
    /// `attachment_mismatch`, `attachment_too_large`, `relay_unavailable`.
    pub category: Option<String>,
}
impl Default for Download {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            event_id: None,
            hash: None,
            path: None,
            received: 0,
            size: None,
            category: None,
        }
    }
}
/// A verified image cached for an inline preview.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Thumbnail {
    pub hash: String,
    pub path: String,
}
/// An uploaded file waiting to be sent with the draft of `scope`
/// (`<roomId>` or `<roomId>:<rootId>`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PendingAttachment {
    pub scope: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub url: String,
    pub hash: String,
    pub dim: Option<String>,
}
/// The one upload the helper tracks.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Upload {
    /// `idle`, `uploading`, `done` or `failed`.
    pub state: String,
    pub scope: Option<String>,
    pub name: Option<String>,
    /// Present only in `failed`: `attachment_invalid`,
    /// `attachment_type_refused`, `attachment_too_large`,
    /// `attachment_forbidden`, `relay_unavailable`.
    pub category: Option<String>,
}
impl Default for Upload {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            scope: None,
            name: None,
            category: None,
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Recipient {
    pub key: String,
    pub name: String,
    /// A verified, unexpired status (`user_status`); always serialized.
    pub status: Option<RecipientStatus>,
    /// `online`, `away` or `offline` from a verified relay snapshot
    /// (`presence`); none when unknown. Always serialized.
    pub presence: Option<String>,
}
/// Another identity's presence (`presence`): a member of the shown roster or a
/// DM partner, from the latest verified relay snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PresencePeer {
    pub key: String,
    pub presence: String,
}
/// This identity's presence and the verified states of those shown.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceView {
    /// `unavailable` (no session, or nothing accepted yet), `ready` or `failed`.
    pub state: String,
    /// The preference the panel last sent on this connection, or none.
    pub mode: Option<String>,
    /// The state the relay last accepted from this identity, or none.
    pub published: Option<String>,
    /// Unix seconds of that acceptance.
    pub last_published_at: Option<u64>,
    /// Present only in `failed`: one of `presence::CATEGORIES`.
    pub category: Option<String>,
    /// At most `presence::SUBJECTS`, sorted by key; unknown keys are absent.
    pub peers: Vec<PresencePeer>,
}
impl PresenceView {
    pub fn unavailable() -> Self {
        Self {
            state: "unavailable".into(),
            mode: None,
            published: None,
            last_published_at: None,
            category: None,
            peers: Vec::new(),
        }
    }
}
/// Another member's status as shown beside their name.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecipientStatus {
    pub text: String,
    pub emoji: Option<String>,
}
impl From<&crate::user_status::UserStatus> for RecipientStatus {
    fn from(status: &crate::user_status::UserStatus) -> Self {
        Self {
            text: status.text.clone(),
            emoji: status.emoji.clone(),
        }
    }
}
/// This identity's status (`user_status`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UserStatusView {
    /// `unavailable` (not read yet in this session), `ready`, `sending` or
    /// `failed`.
    pub state: String,
    /// The verified, unexpired status, or none.
    pub mine: Option<crate::user_status::UserStatus>,
    /// Present only in `failed`: one of `user_status::CATEGORIES`. After a
    /// disconnect the view is `unavailable` and keeps a failure's category.
    pub category: Option<String>,
}
impl UserStatusView {
    pub fn unavailable(category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            mine: None,
            category: category.map(str::to_owned),
        }
    }
}
/// The latest people directory or search read, independent of any room.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleView {
    /// `unavailable` (nothing read, or no session), `loading` or `snapshot`.
    pub state: String,
    /// The `search_people` request this view answers; none before the first.
    pub request_id: Option<String>,
    /// The normalized search text this view answers; empty is the directory.
    pub query: String,
    /// At most `recipients::PEOPLE`, in display order, without this identity.
    pub entries: Vec<crate::recipients::Person>,
    /// Present only when `unavailable` after a failed read.
    pub category: Option<String>,
}
impl PeopleView {
    pub fn unavailable(request: Option<&str>, query: &str, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            request_id: request.map(str::to_owned),
            query: query.into(),
            entries: Vec::new(),
            category: category.map(str::to_owned),
        }
    }
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
    /// Always present: at most `attachments::MAX`.
    pub attachments: Vec<crate::attachments::Attachment>,
    #[serde(rename = "attachmentsUnavailable")]
    pub attachments_unavailable: bool,
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
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub state: String,
    pub rooms: Vec<Room>,
    pub category: Option<String>,
    /// More joined rooms: `none`, `available`, `loading`, `failed` or `limit`
    /// (`catalog::MAX_ROOMS` are held; later rooms are not loadable here).
    pub more: String,
    /// Only with `more` = `failed`: `room_catalog_unavailable` or
    /// `room_catalog_timeout`.
    pub more_category: Option<String>,
}
impl Catalog {
    pub fn unavailable(category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            rooms: Vec::new(),
            category: category.map(str::to_owned),
            more: "none".into(),
            more_category: None,
        }
    }
    pub fn loading() -> Self {
        Self {
            state: "loading".into(),
            ..Self::unavailable(None)
        }
    }
}
/// One configured community as the panel shows it. `name` is the local label;
/// `hint` the relay's own NIP-11 `name` (untrusted, sanitized, display only).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommunityEntry {
    pub relay: String,
    pub name: String,
    pub host: String,
    pub active: bool,
    pub hint: Option<String>,
}
/// The configured communities (`communities`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitiesView {
    /// `ready`, `joining`, `switching`, `renaming`, `leaving` or `failed`.
    pub state: String,
    /// The active relay, also the entry marked `active`.
    pub active: Option<String>,
    /// At most `config::COMMUNITIES`, in the configured order.
    pub entries: Vec<CommunityEntry>,
    /// Present only in `failed`: one of `communities::CATEGORIES`.
    pub category: Option<String>,
    /// First setup saved a community from an invite link; the panel redeems it
    /// once the identity exists.
    pub pending_invite: bool,
    /// `already_absent` after a leave the relay answered with "not a member".
    pub notice: Option<String>,
}
/// Every `status.category` the helper publishes. The panel's `acceptFrame`
/// refuses a frame with any other one, so a new entry needs a panel update.
/// `clock_skew` is a rejected authentication with a clock offset of at least
/// `clock::THRESHOLD` seconds.
pub const CONNECTION_CATEGORIES: [&str; 13] = [
    "identity_access_pending",
    "identity_missing",
    "identity_locked",
    "identity_invalid",
    "identity_unavailable",
    "auth_rejected",
    "clock_skew",
    "relay_timeout",
    "relay_unavailable",
    "relay_resource_limit",
    "relay_protocol_error",
    "config_unavailable",
    "invalid_config",
];
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connection: String,
    pub identity: Option<String>,
    pub relay: Option<String>,
    pub generation: u64,
    /// One of `CONNECTION_CATEGORIES`, or none.
    pub category: Option<String>,
    /// True while an automatic reconnection is scheduled or under way
    /// (`disconnected` or `connecting` after a retried failure); `category`
    /// still names that failure. Never true while `authenticated`.
    pub reconnecting: bool,
    /// Last measured `relay − local` clock offset in seconds (HTTP `Date`),
    /// bounded to ±`clock::BOUND`; none until measured on this relay.
    /// Informational only: nothing is ever adjusted with it.
    pub clock_skew_seconds: Option<i64>,
    pub catalog: Catalog,
    pub history: History,
    pub thread: Thread,
    pub delivery: Delivery,
    pub dm_open: DmOpen,
    pub recipients: RecipientsView,
    pub people: PeopleView,
    pub activity: Vec<crate::activity::Summary>,
    pub setup: JoinSetup,
    pub open_rooms: OpenRooms,
    pub room_action: RoomAction,
    pub room_detail: RoomDetailView,
    pub invites: Invites,
    pub download: Download,
    /// At most `media::THUMBNAILS`, most recent last.
    pub thumbnails: Vec<Thumbnail>,
    /// At most `media::PENDING` per scope and `media::PENDING_TOTAL` in all.
    pub pending_attachments: Vec<PendingAttachment>,
    pub upload: Upload,
    pub user_status: UserStatusView,
    pub presence: PresenceView,
    pub communities: CommunitiesView,
}
impl Status {
    pub fn new(c: &crate::config::Config) -> Self {
        Self {
            connection: "unconfigured".into(),
            identity: c.identity.clone(),
            relay: c.relay.clone(),
            generation: 1,
            category: None,
            reconnecting: false,
            clock_skew_seconds: None,
            catalog: Catalog::unavailable(None),
            history: History::unavailable(None, None),
            thread: Thread::unavailable(None, None, None),
            delivery: Delivery::default(),
            dm_open: DmOpen::default(),
            recipients: RecipientsView::unavailable(None, None),
            people: PeopleView::unavailable(None, "", None),
            activity: Vec::new(),
            setup: JoinSetup::default(),
            open_rooms: OpenRooms::unavailable(None),
            room_action: RoomAction::default(),
            room_detail: RoomDetailView::unavailable(None, None),
            invites: Invites::default(),
            download: Download::default(),
            thumbnails: Vec::new(),
            pending_attachments: Vec::new(),
            upload: Upload::default(),
            user_status: UserStatusView::unavailable(None),
            presence: PresenceView::unavailable(),
            communities: crate::communities::view(c),
        }
    }
}
pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"generation":s.generation,"capabilities":["connection_status","room_catalog","room_history","message_send","thread_send","room_recipients","history_auto_refresh","room_activity","agent_profiles","thread_replies","thread_summaries","dm_open","older_history","live_updates","setup_assist","community_join","invite_mint","attachments","user_status","presence","communities","people_search","message_actions","room_manage"],"backendRevision":crate::compatibility::BUZZ_REVISION,"status":s})
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
    #[test]
    fn mint_requests_are_bounded() {
        let id = "11111111-1111-4111-8111-111111111111";
        let ok = serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":168});
        let r = request(ok.to_string().as_bytes()).unwrap();
        assert_eq!((r.max_uses, r.expires_in_hours), (Some(5), Some(168)));
        for bad in [
            serde_json::json!({"version":1,"id":"ui-1","type":"mint_invite","maxUses":5,"expiresInHours":168}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":0,"expiresInHours":168}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":101,"expiresInHours":168}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":0}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":721}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5.5,"expiresInHours":24}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":-1,"expiresInHours":24}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","expiresInHours":24}),
            serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":24,"role":"admin"}),
            serde_json::json!({"version":1,"id":id,"type":"get_snapshot","maxUses":5}),
        ] {
            assert!(request(bad.to_string().as_bytes()).is_err(), "{bad}");
        }
    }
    #[test]
    fn people_requests_carry_only_a_bounded_query() {
        let parse = |v: &serde_json::Value| request(&serde_json::to_vec(v).unwrap());
        let ok = serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":"ann"});
        assert_eq!(parse(&ok).unwrap().query.as_deref(), Some("ann"));
        let all = serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":""});
        assert!(parse(&all).is_ok());
        for bad in [
            serde_json::json!({"version":1,"id":"ui-3","type":"search_people"}),
            serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":null}),
            serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":"a".repeat(257)}),
            serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":"a\u{0}"}),
            serde_json::json!({"version":1,"id":"ui-3","type":"search_people","query":"a","roomId":"11111111-1111-4111-8111-111111111111"}),
            serde_json::json!({"version":1,"id":"ui-3","type":"get_snapshot","query":"a"}),
        ] {
            assert!(parse(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn presence_requests_carry_only_mode_and_active() {
        let parse = |v: &serde_json::Value| request(&serde_json::to_vec(v).unwrap());
        let id = "55555555-5555-4555-8555-555555555555";
        for mode in ["auto", "away", "offline"] {
            for active in [true, false] {
                let ok = serde_json::json!({"version":1,"id":id,"type":"set_presence","mode":mode,"active":active});
                let parsed = parse(&ok).unwrap();
                assert_eq!(
                    (parsed.mode.as_deref(), parsed.active),
                    (Some(mode), Some(active))
                );
            }
        }
        let base = serde_json::json!({"version":1,"id":id,"type":"set_presence","mode":"auto","active":true});
        for (field, value) in [
            ("id", serde_json::json!("ui-1")),
            ("mode", serde_json::Value::Null),
            ("mode", serde_json::json!("online")),
            ("mode", serde_json::json!("Auto")),
            ("mode", serde_json::json!(1)),
            ("active", serde_json::Value::Null),
            ("active", serde_json::json!(1)),
            ("active", serde_json::json!("true")),
            ("text", serde_json::json!("online")),
            ("emoji", serde_json::json!("🙂")),
            ("kind", serde_json::json!(20001)),
            ("pubkey", serde_json::json!("a".repeat(64))),
            (
                "roomId",
                serde_json::json!("00000000-0000-4000-8000-000000000001"),
            ),
        ] {
            let mut bad = base.clone();
            bad[field] = value;
            assert!(parse(&bad).is_err(), "{bad}");
        }
        for field in ["mode", "active"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse(&missing).is_err(), "{missing}");
        }
        // No other request carries them.
        for extra in [
            serde_json::json!({"mode":"auto"}),
            serde_json::json!({"active":true}),
        ] {
            let mut other = serde_json::json!({"version":1,"id":"a","type":"subscribe"});
            other
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(parse(&other).is_err(), "{other}");
        }
    }
    #[test]
    fn status_requests_carry_only_their_own_fields() {
        let parse = |v: &serde_json::Value| request(&serde_json::to_vec(v).unwrap());
        let id = "33333333-3333-4333-8333-333333333333";
        let set = serde_json::json!({"version":1,"id":id,"type":"set_status","text":"In a meeting","emoji":"🗣️","expiresInHours":4});
        let parsed = parse(&set).unwrap();
        assert_eq!(
            (
                parsed.text.as_deref(),
                parsed.emoji.as_deref(),
                parsed.expires_in_hours
            ),
            (Some("In a meeting"), Some("🗣️"), Some(4))
        );
        // Emoji and hours are optional; the text may be empty (shape only here).
        let minimal = serde_json::json!({"version":1,"id":id,"type":"set_status","text":""});
        let parsed = parse(&minimal).unwrap();
        assert!(parsed.emoji.is_none() && parsed.expires_in_hours.is_none());
        let clear = serde_json::json!({"version":1,"id":id,"type":"clear_status"});
        assert!(parse(&clear).is_ok());
        for (base, field, value) in [
            (&set, "id", serde_json::json!("ui-1")),
            (&set, "text", serde_json::Value::Null),
            (&set, "text", serde_json::json!("a".repeat(1025))),
            (&set, "text", serde_json::json!("a\u{0}b")),
            (&set, "text", serde_json::json!(7)),
            (&set, "emoji", serde_json::Value::Null),
            (&set, "emoji", serde_json::json!("")),
            (&set, "emoji", serde_json::json!("x".repeat(129))),
            (&set, "expiresInHours", serde_json::Value::Null),
            (&set, "expiresInHours", serde_json::json!(-1)),
            (&set, "expiresInHours", serde_json::json!(1.5)),
            (&set, "maxUses", serde_json::json!(1)),
            (
                &set,
                "roomId",
                serde_json::json!("00000000-0000-4000-8000-000000000001"),
            ),
            (&set, "kind", serde_json::json!(30315)),
            (&set, "tags", serde_json::json!([["d", "general"]])),
            (&set, "pubkey", serde_json::json!("a".repeat(64))),
            (&clear, "text", serde_json::json!("x")),
            (&clear, "emoji", serde_json::json!("🙂")),
            (&clear, "expiresInHours", serde_json::json!(24)),
            (&clear, "id", serde_json::json!("ui-2")),
        ] {
            let mut bad = base.clone();
            bad[field] = value;
            assert!(parse(&bad).is_err(), "{bad}");
        }
        let mut missing = set.clone();
        missing.as_object_mut().unwrap().remove("text");
        assert!(parse(&missing).is_err());
        // Other requests never carry an emoji, and a mint still needs its own hours.
        assert!(
            parse(&serde_json::json!({"version":1,"id":"a","type":"subscribe","emoji":"🙂"}))
                .is_err()
        );
        assert!(parse(
            &serde_json::json!({"version":1,"id":"a","type":"subscribe","expiresInHours":24})
        )
        .is_err());
        assert!(parse(&serde_json::json!({"version":1,"id":id,"type":"mint_invite","maxUses":5,"expiresInHours":24,"emoji":"🙂"})).is_err());
    }
    #[test]
    fn attachment_requests_carry_only_their_own_fields() {
        let parse = |v: &serde_json::Value| request(&serde_json::to_vec(v).unwrap());
        let (event, hash) = ("a".repeat(64), "b".repeat(64));
        let room = "00000000-0000-4000-8000-000000000001";
        let download = serde_json::json!({"version":1,"id":"ui-1","type":"download_attachment","eventId":event,"hash":hash});
        let parsed = parse(&download).unwrap();
        assert_eq!(
            (parsed.event_id.as_deref(), parsed.hash.as_deref()),
            (Some(event.as_str()), Some(hash.as_str()))
        );
        let mut thumb = download.clone();
        thumb["type"] = serde_json::json!("thumbnail_attachment");
        assert!(parse(&thumb).is_ok());
        let open = serde_json::json!({"version":1,"id":"ui-2","type":"open_download","path":"/home/u/Downloads/a.pdf"});
        assert_eq!(
            parse(&open).unwrap().path.as_deref(),
            Some("/home/u/Downloads/a.pdf")
        );
        let upload = serde_json::json!({"version":1,"id":"ui-3","type":"upload_attachment","roomId":room,"path":"/home/u/a b.png"});
        assert!(parse(&upload).unwrap().root_id.is_none());
        let mut thread_upload = upload.clone();
        thread_upload["rootId"] = serde_json::json!(event);
        assert_eq!(
            parse(&thread_upload).unwrap().root_id.as_deref(),
            Some(event.as_str())
        );
        let remove = serde_json::json!({"version":1,"id":"ui-4","type":"remove_pending_attachment","hash":hash});
        assert!(parse(&remove).is_ok());
        for (base, field, value) in [
            (&download, "hash", serde_json::json!("B".repeat(64))),
            (&download, "hash", serde_json::json!("b".repeat(63))),
            (&download, "eventId", serde_json::json!("../x")),
            (&download, "path", serde_json::json!("/tmp/x")),
            (
                &download,
                "url",
                serde_json::json!("https://relay.example/media/x"),
            ),
            (&download, "roomId", serde_json::json!(room)),
            (&thumb, "eventId", serde_json::Value::Null),
            (&open, "path", serde_json::json!("relative/a.pdf")),
            (&open, "path", serde_json::json!("/home/u/../etc/passwd")),
            (&open, "path", serde_json::json!("/home/u/./a")),
            (&open, "path", serde_json::json!("/home//u")),
            (&open, "path", serde_json::json!("/home/u/a\nb")),
            (&open, "path", serde_json::json!("/")),
            (
                &open,
                "path",
                serde_json::json!(format!("/{}", "a".repeat(4096))),
            ),
            (&open, "hash", serde_json::json!(hash)),
            (&upload, "roomId", serde_json::json!("../room")),
            (&upload, "rootId", serde_json::json!("A".repeat(64))),
            (&upload, "hash", serde_json::json!(hash)),
            (&upload, "text", serde_json::json!("x")),
            (&remove, "path", serde_json::json!("/tmp/x")),
            (&remove, "eventId", serde_json::json!(event)),
        ] {
            let mut bad = base.clone();
            bad[field] = value;
            assert!(parse(&bad).is_err(), "{bad}");
        }
        for (base, field) in [
            (&download, "eventId"),
            (&download, "hash"),
            (&open, "path"),
            (&upload, "path"),
            (&upload, "roomId"),
            (&remove, "hash"),
        ] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse(&missing).is_err(), "{missing}");
        }
        // Other requests never carry attachment fields.
        for field in ["eventId", "hash", "path"] {
            let mut bad = serde_json::json!({"version":1,"id":"a","type":"subscribe"});
            bad[field] = serde_json::json!("x");
            assert!(parse(&bad).is_err(), "{field}");
        }
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
                "community_join",
                "invite_mint",
                "attachments",
                "user_status",
                "presence",
                "communities",
                "people_search",
                "message_actions",
                "room_manage"
            ])
        );
        assert_eq!(v["status"]["catalog"]["state"], "unavailable");
        assert_eq!(v["status"]["catalog"]["rooms"], serde_json::json!([]));
        assert_eq!(v["status"]["dmOpen"]["state"], "idle");
        assert!(v["status"]["dmOpen"]["channelId"].is_null());
        assert!(v.get("privateKey").is_none());
    }
    #[test]
    fn message_action_requests_are_exactly_shaped() {
        let room = "11111111-1111-4111-8111-111111111111";
        let id = "22222222-2222-4222-8222-222222222222";
        let target = "b".repeat(64);
        let base = |kind: &str| {
            serde_json::json!({"version":1,"id":id,"type":kind,"roomId":room,"eventId":target,
                "generation":1,"instanceId":"i"})
        };
        let with = |mut v: serde_json::Value, key: &str, value: serde_json::Value| {
            v[key] = value;
            serde_json::to_vec(&v).unwrap()
        };
        let ok = |bytes: Vec<u8>| request(&bytes).is_ok();
        assert!(ok(with(base("edit_message"), "text", "new".into())));
        assert!(ok(with(base("add_reaction"), "emoji", "👍".into())));
        assert!(ok(with(base("remove_reaction"), "emoji", "👍".into())));
        assert!(ok(serde_json::to_vec(&base("delete_message")).unwrap()));
        let bad = [
            with(base("edit_message"), "text", "".into()),
            with(base("edit_message"), "text", "  ".into()),
            with(base("edit_message"), "text", "x".repeat(4097).into()),
            with(base("edit_message"), "mentions", serde_json::json!([])),
            serde_json::to_vec(&base("edit_message")).unwrap(),
            serde_json::to_vec(&base("add_reaction")).unwrap(),
            with(base("add_reaction"), "emoji", "".into()),
            with(base("add_reaction"), "emoji", "x".repeat(129).into()),
            with(base("delete_message"), "text", "x".into()),
            with(base("delete_message"), "emoji", "x".into()),
            with(base("delete_message"), "eventId", "short".into()),
            with(base("delete_message"), "rootId", target.clone().into()),
            with(base("delete_message"), "roomId", "../x".into()),
            with(base("delete_message"), "id", "not-a-uuid".into()),
            with(base("delete_message"), "generation", 0.into()),
        ];
        for bytes in bad {
            assert!(
                request(&bytes).is_err(),
                "{}",
                String::from_utf8_lossy(&bytes)
            );
        }
        for missing in ["eventId", "instanceId"] {
            let mut v = base("delete_message");
            v.as_object_mut().unwrap().remove(missing);
            assert!(request(&serde_json::to_vec(&v).unwrap()).is_err());
        }
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
            ("text", serde_json::json!("é".repeat(2049))),
            ("text", serde_json::json!("a\u{0}b")),
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
        // A blank text is shaped correctly; the sender refuses it unless the
        // draft carries attachments.
        let mut blank = valid.clone();
        blank["text"] = serde_json::json!(" ");
        assert!(request(&serde_json::to_vec(&blank).unwrap()).is_ok());
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
    /// The largest attachment a row can carry once JSON-encoded: quotes
    /// double on encoding, controls and separators are replaced.
    fn worst_attachment() -> crate::attachments::Attachment {
        crate::attachments::Attachment {
            name: "\"".repeat(crate::attachments::NAME_CHARS),
            mime: "a".repeat(crate::attachments::MIME_BYTES),
            size: crate::attachments::MAX_SIZE,
            url: "u".repeat(crate::attachments::URL_BYTES),
            hash: "f".repeat(64),
            dim: Some("16384x16384".into()),
            kind: "image",
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
        status.catalog.rooms = (0..crate::catalog::MAX_ROOMS)
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
                attachments: vec![worst_attachment(); crate::attachments::MAX],
                attachments_unavailable: true,
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
        crate::attachments::bound(
            status.history.rows.iter_mut(),
            crate::attachments::FRAME_HISTORY,
        );
        // Replies carry no reactions or summary (`auth` projects them as None).
        status.thread.rows = (0..crate::thread::ROWS)
            .map(|_| ThreadRow {
                row: HistoryRow {
                    reactions: None,
                    thread: None,
                    attachments: vec![worst_attachment(); crate::attachments::MAX],
                    ..status.history.rows[0].clone()
                },
                depth: 64,
                parent: "e".repeat(64),
            })
            .collect();
        crate::attachments::bound(
            status.thread.rows.iter_mut().map(|r| &mut r.row),
            crate::attachments::FRAME_THREAD,
        );
        assert_eq!(
            status
                .history
                .rows
                .iter()
                .chain(status.thread.rows.iter().map(|r| &r.row))
                .map(|r| r.attachments.len())
                .sum::<usize>(),
            crate::attachments::FRAME_HISTORY + crate::attachments::FRAME_THREAD
        );
        status.thread.has_more = Some(true);
        status.thread.category = Some("thread_replies_hidden".into());
        status.recipients.entries = (0..20)
            .map(|_| Recipient {
                key: "c".repeat(64),
                name: "\\".repeat(64),
                status: Some(RecipientStatus {
                    text: "\\".repeat(crate::user_status::TEXT_BYTES),
                    emoji: Some(format!(":{}:", "a".repeat(32))),
                }),
                presence: Some("offline".into()),
            })
            .collect();
        status.activity = (0..20)
            .map(|_| crate::activity::Summary {
                room_id: "00000000-0000-4000-8000-000000000001".into(),
                epoch: u64::MAX,
                observed: 1_000_000_000,
                notice: Some(crate::activity::Notice {
                    seq: u64::MAX,
                    kind: "mention",
                    count: 999,
                    room_name: "\\".repeat(128),
                    sender: "\\".repeat(64),
                    snippet: "\\".repeat(100),
                    event_id: "a".repeat(64),
                    thread_root: Some("b".repeat(64)),
                }),
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
            detail: Some("\\".repeat(crate::join::REFUSAL_BYTES)),
        };
        status.room_detail = RoomDetailView {
            state: "snapshot".into(),
            room_id: Some("00000000-0000-4000-8000-000000000006".into()),
            topic: "\\".repeat(crate::rooms::TOPIC_BYTES),
            visibility: "private".into(),
            role: "owner".into(),
            members: (0..crate::rooms::MEMBERS)
                .map(|_| RoomMember {
                    key: "e".repeat(64),
                    name: "\\".repeat(64),
                    role: "unknown".into(),
                })
                .collect(),
            truncated: true,
            category: None,
        };
        status.download = Download {
            state: "downloading".into(),
            event_id: Some("a".repeat(64)),
            hash: Some("b".repeat(64)),
            path: Some("/".repeat(4096)),
            received: u64::MAX,
            size: Some(u64::MAX),
            category: Some("attachment_storage_unavailable".into()),
        };
        status.thumbnails = (0..crate::media::THUMBNAILS)
            .map(|_| Thumbnail {
                hash: "c".repeat(64),
                path: "\\".repeat(crate::media::THUMB_PATH_BYTES),
            })
            .collect();
        status.pending_attachments = (0..crate::media::PENDING_TOTAL)
            .map(|_| {
                let a = worst_attachment();
                PendingAttachment {
                    scope: format!("{}:{}", "0".repeat(36), "a".repeat(64)),
                    name: a.name,
                    mime: a.mime,
                    size: a.size,
                    url: a.url,
                    hash: a.hash,
                    dim: a.dim,
                }
            })
            .collect();
        status.upload = Upload {
            state: "failed".into(),
            scope: Some(format!("{}:{}", "0".repeat(36), "a".repeat(64))),
            name: Some("\"".repeat(crate::attachments::NAME_CHARS)),
            category: Some("attachment_type_refused".into()),
        };
        status.user_status = UserStatusView {
            state: "failed".into(),
            mine: Some(crate::user_status::UserStatus {
                text: "\\".repeat(crate::user_status::TEXT_BYTES),
                emoji: Some(format!(":{}:", "a".repeat(32))),
                expires_at: Some(u64::MAX),
            }),
            category: Some("status_rate_limited".into()),
        };
        status.presence = PresenceView {
            state: "failed".into(),
            mode: Some("offline".into()),
            published: Some("offline".into()),
            last_published_at: Some(u64::MAX),
            category: Some("presence_rejected".into()),
            peers: (0..crate::presence::SUBJECTS)
                .map(|_| PresencePeer {
                    key: "d".repeat(64),
                    presence: "offline".into(),
                })
                .collect(),
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
