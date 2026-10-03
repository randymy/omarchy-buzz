//! Joining a community from the panel (`community_join`, onboarding step two).
//!
//! Invites follow the pinned relay's HTTP API
//! (`crates/buzz-relay/src/api/invites.rs`) and Desktop's client
//! (`desktop/src/shared/api/invites.ts`, `inviteHelpers.ts`):
//!
//! 1. `GET /api/join-policy` → `{"policy":{terms_markdown, privacy_markdown,
//!    age_attestation_required, version}}`, `{}` or 404 (no policy).
//! 2. With a policy, `POST /api/invites/accept-policy` `{code, policy_version,
//!    age_confirmed}` → `{"receipt"}`. The relay reads no NIP-98 header here
//!    (a pre-membership exempt route); Desktop sends none either.
//! 3. `POST /api/invites/claim` `{code[, policy_receipt]}`, NIP-98 signed with a
//!    payload hash by the joining key → `{status, community_id, host, role}`.
//!
//! Claiming grants relay membership only, never room membership. The invite
//! may name a relay only to confirm it is the configured one: an invite never
//! switches relays. Open rooms are joined with kind 9021 and left with 9022.
use crate::protocol::{ClaimResult, JoinPolicy, RoomAction, Status};
use nostr::{Event, Keys};
use reqwest::{redirect::Policy, Client, StatusCode};
use std::time::Duration;
use tokio::time::Instant;

/// Invite codes are base64url segments joined by `.` (`invite_token.rs`,
/// `buzz-core` `invite.rs`); anything else is refused before any request.
pub const CODE_BYTES: usize = 1024;
const INPUT_BYTES: usize = 4096;
/// Two policy documents of at most 256 KiB each, JSON-escaped (Desktop allows 4 MiB).
const POLICY_BYTES: usize = 2 * 1024 * 1024;
/// The policy text shown in the panel; longer text is cut and marked.
pub const POLICY_TEXT: usize = 64 * 1024;
const VERSION_CHARS: usize = 128;
const RESPONSE_BYTES: usize = 16 * 1024;
const RECEIPT_BYTES: usize = 2048;
/// Same bound as a kind-9 send: no `OK` by then means the outcome is unknown.
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(15);

/// Fixed categories returned to the panel for invites and room actions.
#[cfg(test)]
pub const CATEGORIES: [&str; 11] = [
    "invite_invalid",
    "invite_relay_mismatch",
    "invite_rejected",
    "invite_rate_limited",
    "policy_required",
    "relay_unavailable",
    "setup_busy",
    "setup_not_allowed",
    "room_not_open",
    "join_rejected",
    "leave_rejected",
];

#[derive(Debug, PartialEq)]
pub struct Invite {
    pub code: String,
    /// The relay the invite names (`ws(s)://host[:port]`), if any.
    pub relay: Option<String>,
}

pub fn valid_code(code: &str) -> bool {
    (1..=CODE_BYTES).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn valid_version(version: &str) -> bool {
    let count = version.chars().count();
    (1..=VERSION_CHARS).contains(&count) && !version.chars().any(char::is_control)
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Desktop's `parseInviteInput` (`inviteHelpers.ts:26-77`): an
/// `http(s)://<relay>/invite/<code>` link (relay `ws(s)://<host>`), a
/// `buzz://join?relay=<ws(s) url>&code=<code>` link, or a bare code with no
/// `://` and no `/`. Credentials or fragments anywhere refuse the input; the
/// code must then be a well-formed invite code.
pub fn parse_invite(input: &str) -> Result<Invite, &'static str> {
    const INVALID: &str = "invite_invalid";
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > INPUT_BYTES {
        return Err(INVALID);
    }
    let invite = match url::Url::parse(trimmed) {
        Ok(url) => {
            if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
                return Err(INVALID);
            }
            match url.scheme() {
                "buzz" => {
                    if url.host_str() != Some("join") {
                        return Err(INVALID);
                    }
                    // `URLSearchParams.get`: the first value of each name.
                    let first = |name: &str| {
                        url.query_pairs()
                            .find(|(k, _)| k == name)
                            .map(|(_, v)| v.into_owned())
                    };
                    let (Some(relay), Some(code)) = (first("relay"), first("code")) else {
                        return Err(INVALID);
                    };
                    if relay.is_empty() || code.is_empty() {
                        return Err(INVALID);
                    }
                    if !relay.starts_with("ws://") && !relay.starts_with("wss://") {
                        return Err(INVALID);
                    }
                    let nested = url::Url::parse(&relay).map_err(|_| INVALID)?;
                    if !nested.username().is_empty()
                        || nested.password().is_some()
                        || nested.fragment().is_some()
                    {
                        return Err(INVALID);
                    }
                    Invite {
                        code,
                        relay: Some(relay),
                    }
                }
                scheme @ ("http" | "https") => {
                    let rest = url.path().strip_prefix("/invite/").ok_or(INVALID)?;
                    let encoded = rest.strip_suffix('/').unwrap_or(rest);
                    if encoded.is_empty() || encoded.contains('/') {
                        return Err(INVALID);
                    }
                    let host = url.host_str().ok_or(INVALID)?;
                    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
                    let ws = if scheme == "https" { "wss" } else { "ws" };
                    Invite {
                        code: percent_decode(encoded).ok_or(INVALID)?,
                        relay: Some(format!("{ws}://{host}{port}")),
                    }
                }
                _ => return Err(INVALID),
            }
        }
        Err(_) => {
            if trimmed.contains("://") || trimmed.contains('/') {
                return Err(INVALID);
            }
            Invite {
                code: trimmed.to_owned(),
                relay: None,
            }
        }
    };
    if !valid_code(&invite.code) {
        return Err(INVALID);
    }
    Ok(invite)
}

/// The invite's relay, if named, must be the configured relay (scheme, host
/// and port after canonicalization). The panel never switches relays.
pub fn check_relay(invite: &Invite, configured: &str) -> Result<(), &'static str> {
    match &invite.relay {
        None => Ok(()),
        Some(relay) if crate::config::canonical_relay(relay).as_deref() == Ok(configured) => Ok(()),
        Some(_) => Err("invite_relay_mismatch"),
    }
}

/// `http(s)://<configured relay host>/<path>`, like `/query`.
pub(crate) fn http_url(relay: &str, path: &str) -> Result<url::Url, &'static str> {
    let canonical = crate::config::canonical_relay(relay).map_err(|_| "relay_unavailable")?;
    let mut url = url::Url::parse(&canonical).map_err(|_| "relay_unavailable")?;
    let scheme = if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    url.set_scheme(scheme).map_err(|_| "relay_unavailable")?;
    url.set_path(path);
    Ok(url)
}

pub(crate) fn client() -> Result<Client, &'static str> {
    Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "relay_unavailable")
}

pub(crate) async fn body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, &'static str> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("relay_unavailable");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "relay_unavailable")? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("relay_unavailable");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Removes controls except newline and tab, then cuts at `POLICY_TEXT` bytes.
fn policy_text(text: &str) -> (String, bool) {
    let mut out = String::new();
    for ch in text.chars() {
        let ch = if ch.is_control() && !matches!(ch, '\n' | '\t') {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > POLICY_TEXT {
            return (out, true);
        }
        out.push(ch);
    }
    (out, false)
}

/// The `GET /api/join-policy` body (`invites.rs` `join_policy`). `{}` and a
/// `null` policy mean none. Unknown keys or wrong types refuse the response.
pub fn parse_policy(bytes: &[u8]) -> Result<Option<JoinPolicy>, &'static str> {
    const INVALID: &str = "relay_unavailable";
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let top = value.as_object().ok_or(INVALID)?;
    if top.keys().any(|k| k != "policy") {
        return Err(INVALID);
    }
    let policy = match top.get("policy") {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(policy) => policy.as_object().ok_or(INVALID)?,
    };
    if policy.keys().any(|k| {
        !matches!(
            k.as_str(),
            "terms_markdown" | "privacy_markdown" | "age_attestation_required" | "version"
        )
    }) {
        return Err(INVALID);
    }
    let document = |name: &str| match policy.get(name) {
        None | Some(serde_json::Value::Null) => Ok(""),
        Some(serde_json::Value::String(text)) => Ok(text.as_str()),
        Some(_) => Err(INVALID),
    };
    let (terms, privacy) = (document("terms_markdown")?, document("privacy_markdown")?);
    let age_required = policy
        .get("age_attestation_required")
        .and_then(serde_json::Value::as_bool)
        .ok_or(INVALID)?;
    let version = policy
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|v| valid_version(v))
        .ok_or(INVALID)?;
    let joined = match (terms.trim().is_empty(), privacy.trim().is_empty()) {
        (false, false) => format!("{terms}\n\n{privacy}"),
        (false, true) => terms.to_owned(),
        (true, false) => privacy.to_owned(),
        (true, true) => String::new(),
    };
    let (text, truncated) = policy_text(&joined);
    Ok(Some(JoinPolicy {
        text,
        version: version.to_owned(),
        age_required,
        truncated,
    }))
}

pub async fn fetch_policy(relay: &str) -> Result<Option<JoinPolicy>, &'static str> {
    let response = client()?
        .get(http_url(relay, "/api/join-policy")?)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        // Relays that predate join policies have none (Desktop treats 404 so).
        StatusCode::NOT_FOUND => Ok(None),
        StatusCode::OK => parse_policy(&body(response, POLICY_BYTES).await?),
        _ => Err("relay_unavailable"),
    }
}

/// The relay's `{"error": …}` text, only to choose a fixed category.
async fn error_text(response: reqwest::Response) -> String {
    body(response, RESPONSE_BYTES)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_default()
}

/// Exchanges acceptance of `version` for a receipt bound to `code`.
pub async fn accept_policy(
    relay: &str,
    code: &str,
    version: &str,
    age_confirmed: bool,
) -> Result<String, &'static str> {
    let payload = serde_json::json!({"code": code, "policy_version": version, "age_confirmed": age_confirmed});
    let response = client()?
        .post(http_url(relay, "/api/invites/accept-policy")?)
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(&payload).map_err(|_| "relay_unavailable")?)
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        StatusCode::OK => {}
        // `join_policy_not_accepted` (changed version) or no policy any more.
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => return Err("policy_required"),
        StatusCode::TOO_MANY_REQUESTS => return Err("invite_rate_limited"),
        _ => return Err("relay_unavailable"),
    }
    let value: serde_json::Value = serde_json::from_slice(&body(response, RESPONSE_BYTES).await?)
        .map_err(|_| "relay_unavailable")?;
    let object = value.as_object().ok_or("relay_unavailable")?;
    let receipt = object
        .get("receipt")
        .and_then(serde_json::Value::as_str)
        .filter(|r| {
            object.len() == 1
                && (1..=RECEIPT_BYTES).contains(&r.len())
                && r.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
        .ok_or("relay_unavailable")?;
    Ok(receipt.to_owned())
}

/// Exactly the relay's claim answer: `status` `joined`/`already_member`, a
/// canonical community UUID, a host name and a role word. Anything else,
/// including extra keys, refuses the response.
pub fn parse_claim(bytes: &[u8]) -> Result<ClaimResult, &'static str> {
    const INVALID: &str = "relay_unavailable";
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let object = value.as_object().ok_or(INVALID)?;
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["community_id", "host", "role", "status"] {
        return Err(INVALID);
    }
    let text = |name: &str| {
        object
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or(INVALID)
    };
    let status = text("status")?;
    let community = text("community_id")?;
    let host = text("host")?;
    let role = text("role")?;
    let canonical = uuid::Uuid::parse_str(community).is_ok_and(|id| id.to_string() == community);
    let host_ok = (1..=255).contains(&host.len())
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b));
    let role_ok =
        (1..=32).contains(&role.len()) && role.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    if !matches!(status, "joined" | "already_member") || !canonical || !host_ok || !role_ok {
        return Err(INVALID);
    }
    Ok(ClaimResult {
        status: status.into(),
        community_id: community.into(),
        host: host.into(),
        role: role.into(),
    })
}

/// Claims `code` for `keys` (NIP-98, payload-bound, `u` = the exact URL).
pub async fn claim(
    relay: &str,
    keys: &Keys,
    code: &str,
    receipt: Option<&str>,
) -> Result<ClaimResult, &'static str> {
    let url = http_url(relay, "/api/invites/claim")?;
    let mut payload = serde_json::json!({ "code": code });
    if let Some(receipt) = receipt {
        payload["policy_receipt"] = serde_json::json!(receipt);
    }
    let bytes = serde_json::to_vec(&payload).map_err(|_| "relay_unavailable")?;
    let auth = crate::query::authorization(keys, &url, &bytes).map_err(|_| "relay_unavailable")?;
    let response = client()?
        .post(url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .header("Content-Type", "application/json")
        .body(bytes)
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        StatusCode::OK => parse_claim(&body(response, RESPONSE_BYTES).await?),
        StatusCode::TOO_MANY_REQUESTS => Err("invite_rate_limited"),
        StatusCode::FORBIDDEN => Err(if error_text(response).await == "join_policy_required" {
            "policy_required"
        } else {
            "invite_rejected"
        }),
        // Malformed body or a refused NIP-98 proof.
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => Err("invite_rejected"),
        _ => Err("relay_unavailable"),
    }
}

/// Step one of a redemption: parse, confirm the relay, read its policy.
pub async fn check_invite(
    relay: &str,
    input: &str,
) -> Result<(String, Option<JoinPolicy>), &'static str> {
    let invite = parse_invite(input)?;
    check_relay(&invite, relay)?;
    let policy = fetch_policy(relay).await?;
    Ok((invite.code, policy))
}

/// Step two: accept the shown policy (if any), then claim. `shown` is the
/// published policy the user accepted; `version` must name it.
pub async fn redeem(
    relay: &str,
    keys: &Keys,
    code: &str,
    shown: Option<&JoinPolicy>,
    version: Option<&str>,
) -> Result<ClaimResult, &'static str> {
    let receipt = match (shown, version) {
        (None, None) => None,
        (Some(policy), Some(version)) if policy.version == version => {
            Some(accept_policy(relay, code, version, policy.age_required).await?)
        }
        _ => return Err("policy_required"),
    };
    claim(relay, keys, code, receipt.as_deref()).await
}

/// The result of one invite request run beside the relay connection.
pub enum InviteStep {
    Checked(Result<(String, Option<JoinPolicy>), &'static str>),
    Redeemed(Result<ClaimResult, &'static str>),
}

/// `ClaimInvite`: the view published before the check, then the outcome.
pub fn checking() -> crate::protocol::JoinSetup {
    crate::protocol::JoinSetup {
        state: "checking".into(),
        ..Default::default()
    }
}
pub fn failed(category: &'static str) -> crate::protocol::JoinSetup {
    crate::protocol::JoinSetup {
        state: "failed".into(),
        category: Some(category.into()),
        ..Default::default()
    }
}
pub fn awaiting(code: String, policy: Option<JoinPolicy>) -> crate::protocol::JoinSetup {
    crate::protocol::JoinSetup {
        state: "policy".into(),
        invite_code: Some(code),
        join_policy: policy,
        ..Default::default()
    }
}
pub fn joined(result: ClaimResult) -> crate::protocol::JoinSetup {
    crate::protocol::JoinSetup {
        state: "joined".into(),
        claim: Some(result),
        ..Default::default()
    }
}
/// The published invite awaiting acceptance, if `code` names it.
pub fn awaiting_invite(status: &Status, code: &str) -> Result<Option<JoinPolicy>, &'static str> {
    if status.setup.state != "policy" || status.setup.invite_code.as_deref() != Some(code) {
        return Err("invite_invalid");
    }
    Ok(status.setup.join_policy.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Join,
    Leave,
}
impl Action {
    pub fn word(self) -> &'static str {
        match self {
            Action::Join => "join",
            Action::Leave => "leave",
        }
    }
    fn rejected(self) -> &'static str {
        match self {
            Action::Join => "join_rejected",
            Action::Leave => "leave_rejected",
        }
    }
}

struct Pending {
    request_id: String,
    room: String,
    /// `join`, `leave`, or a `rooms::Change` word.
    action: &'static str,
    /// The category an `OK false` becomes.
    rejected: &'static str,
    event_id: String,
    deadline: Instant,
}

/// One join (kind 9021) or leave (kind 9022) at a time, resolved only by the
/// relay's `OK` for its exact event ID, like `dm_open::Opener`.
#[derive(Default)]
pub struct RoomActions {
    pending: Option<Pending>,
}
impl RoomActions {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn deadline(&self) -> Instant {
        self.pending
            .as_ref()
            .map(|p| p.deadline)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))
    }
    /// Checks and signs a request. A join needs a room in the published open
    /// rooms snapshot; a leave needs a joined stream room.
    pub fn prepare(
        &mut self,
        action: Action,
        request_id: &str,
        room: &str,
        keys: &Keys,
        status: &Status,
        fresh: bool,
        trusted: bool,
    ) -> Result<(RoomAction, Event), &'static str> {
        if self.pending.is_some() {
            return Err("setup_busy");
        }
        if status.room_action.request_id.as_deref() == Some(request_id) {
            return Err("setup_busy");
        }
        if !fresh || !trusted || status.connection != "authenticated" {
            return Err("relay_unavailable");
        }
        let id = uuid::Uuid::parse_str(room)
            .ok()
            .filter(|id| id.to_string() == room);
        let joined = status.catalog.rooms.iter().find(|r| r.id == room);
        let event = match action {
            Action::Join => {
                let open = status.open_rooms.state == "snapshot"
                    && status.open_rooms.rooms.iter().any(|r| r.id == room);
                let (Some(id), true, None) = (id, open, joined) else {
                    return Err("room_not_open");
                };
                buzz_sdk::build_join(id)
            }
            Action::Leave => {
                let (Some(id), Some(joined)) = (id, joined) else {
                    return Err("leave_rejected");
                };
                if joined.kind != "stream" {
                    return Err("leave_rejected");
                }
                buzz_sdk::build_leave(id)
            }
        }
        .map_err(|_| "relay_unavailable")?
        .sign_with_keys(keys)
        .map_err(|_| "relay_unavailable")?;
        Ok((
            self.start(request_id, room, action.word(), action.rejected(), &event),
            event,
        ))
    }
    fn start(
        &mut self,
        request_id: &str,
        room: &str,
        action: &'static str,
        rejected: &'static str,
        event: &Event,
    ) -> RoomAction {
        self.pending = Some(Pending {
            request_id: request_id.into(),
            room: room.into(),
            action,
            rejected,
            event_id: event.id.to_hex(),
            deadline: Instant::now() + ACTION_TIMEOUT,
        });
        view("sending", self.pending.as_ref().unwrap(), None, None)
    }
    /// Checks and signs a room creation or management change (`rooms`). The
    /// target must be a joined stream room; removal names a member of the
    /// room's verified roster other than this identity (that is a leave).
    /// Whether this identity may do it is the relay's decision, not checked.
    pub fn prepare_change(
        &mut self,
        change: &crate::rooms::Change,
        request_id: &str,
        keys: &Keys,
        status: &Status,
        fresh: bool,
        trusted: bool,
    ) -> Result<(RoomAction, Event), &'static str> {
        use crate::rooms::Change;
        if self.pending.is_some() || status.room_action.request_id.as_deref() == Some(request_id) {
            return Err("setup_busy");
        }
        if !fresh || !trusted || status.connection != "authenticated" {
            return Err("relay_unavailable");
        }
        let new_room = uuid::Uuid::new_v4();
        let room = change.room(new_room);
        let stream = status
            .catalog
            .rooms
            .iter()
            .any(|r| r.id == room.to_string() && r.kind == "stream");
        if !matches!(change, Change::Create { .. }) && !stream {
            return Err("room_invalid");
        }
        let rostered = |key: &nostr::PublicKey| {
            status.room_detail.state == "snapshot"
                && status.room_detail.room_id.as_deref() == Some(room.to_string().as_str())
                && status
                    .room_detail
                    .members
                    .iter()
                    .any(|m| m.key == key.to_hex())
        };
        match change {
            Change::RemoveMember { key, .. } if *key == keys.public_key() || !rostered(key) => {
                return Err("room_invalid")
            }
            Change::AddMember { key, .. }
                if *key == keys.public_key()
                    || (status.room_detail.members.len() < crate::rooms::MEMBERS
                        && rostered(key)) =>
            {
                return Err("room_invalid")
            }
            _ => {}
        }
        let event = change
            .build(room)?
            .sign_with_keys(keys)
            .map_err(|_| "relay_unavailable")?;
        let started = self.start(
            request_id,
            &room.to_string(),
            change.word(),
            change.rejected(),
            &event,
        );
        Ok((started, event))
    }
    /// Forgets a prepared action whose caller left before its EVENT was written.
    pub fn abandon(&mut self) {
        self.pending = None;
    }
    pub fn acknowledge(&mut self, event_id: &str, accepted: bool) -> Option<RoomAction> {
        self.acknowledge_with(event_id, accepted, "")
    }
    /// As `acknowledge`; a refusal carries the relay's own words (`detail`).
    pub fn acknowledge_with(
        &mut self,
        event_id: &str,
        accepted: bool,
        message: &str,
    ) -> Option<RoomAction> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.event_id == event_id)
        {
            return None;
        }
        let pending = self.pending.take()?;
        Some(if accepted {
            view("acknowledged", &pending, None, None)
        } else {
            let detail = refusal(message);
            view("rejected", &pending, Some(pending.rejected), detail)
        })
    }
    /// Timeout, disconnect or re-authentication: the relay may have applied it.
    pub fn unknown(&mut self) -> Option<RoomAction> {
        let pending = self.pending.take()?;
        Some(view("unknown", &pending, Some("relay_unavailable"), None))
    }
}

/// The relay's `OK false` text for the panel: plain, one line, at most
/// `REFUSAL_BYTES`. Relay words are shown, never interpreted.
pub const REFUSAL_BYTES: usize = 200;
pub fn refusal(message: &str) -> Option<String> {
    let text = crate::recipients::sanitize(message, REFUSAL_BYTES);
    (!text.is_empty()).then_some(text)
}

fn view(
    state: &str,
    pending: &Pending,
    category: Option<&str>,
    detail: Option<String>,
) -> RoomAction {
    RoomAction {
        state: state.into(),
        action: Some(pending.action.into()),
        detail,
        request_id: Some(pending.request_id.clone()),
        room_id: Some(pending.room.clone()),
        category: category.map(str::to_owned),
    }
}

#[cfg(test)]
#[path = "join_tests.rs"]
pub(crate) mod tests;
