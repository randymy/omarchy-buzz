//! Presence (`presence`): Buzz's own kind 20001 heartbeat (online / away /
//! offline), built with pinned `buzz_sdk::build_presence_update`
//! (`builders.rs:1898-1910`: content is the state, plus `["status", state]`)
//! and sent over the WebSocket only (ephemeral kinds are refused over HTTP,
//! docs/PRESENCE_MAP.md §5).
//!
//! Publishing: the panel sends the person's preference (`auto`/`away`/
//! `offline`, Desktop's `PresencePreference`) and an idle hint; the helper
//! derives the state, publishes it once per change and re-publishes it every
//! `HEARTBEAT` while online or away (the relay keeps it 180 s,
//! `buzz-pubsub/src/presence.rs:16`). At most one publication is in flight and
//! at most one is admitted per `GAP`; a change inside the gap waits for it.
//!
//! Reading: a signed `POST /query` for kind 20001 with `authors` is answered by
//! the relay's `synthesize_presence` (`api/bridge.rs:2539-2631`) with events
//! signed by the relay's own key (NIP-11 `self`) carrying `["p", subject]`.
//! `verify` is the only routine that turns such events into states.
use crate::protocol::PresenceView;
use nostr::{Event, Keys, PublicKey, Timestamp};
use std::collections::BTreeMap;
use tokio::time::{Duration, Instant};

pub const KIND: u16 = 20001;
/// Desktop's `PRESENCE_HEARTBEAT_INTERVAL_MS` (`presence.ts:51`).
pub const HEARTBEAT: Duration = Duration::from_secs(60);
/// At most one publication per this interval.
pub const GAP: Duration = Duration::from_secs(5);
/// No `OK` by then: the outcome is unknown (`relay_unavailable`).
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// The relay's 180 s TTL plus a skew margin: an older (or further future)
/// snapshot event says nothing current, so it reads as offline.
pub const FRESH_SECS: u64 = 240;
/// Subjects per query (the relay intercepts any author list; the helper keeps
/// reads as small as its other batched reads).
pub const AUTHORS: usize = 20;
/// Subjects per refresh: the shown roster (≤ 20) and DM partners.
pub const SUBJECTS: usize = 60;
/// Legacy `{"status": …}` content is accepted up to this size (`event.rs:838-847`).
pub const LEGACY_BYTES: usize = 128;
/// Every category the `presence` view may carry.
#[cfg(test)]
pub const CATEGORIES: [&str; 2] = ["presence_rejected", "relay_unavailable"];

/// The person's preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Away,
    Offline,
}
impl Mode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "away" => Some(Self::Away),
            "offline" => Some(Self::Offline),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Away => "away",
            Self::Offline => "offline",
        }
    }
}
/// The published state: `offline` and `away` are pinned; `auto` follows the
/// idle hint (Desktop's `resolveAutomaticPresenceStatus`, `presence.ts:62-70`).
pub fn derive(mode: Mode, active: bool) -> &'static str {
    match (mode, active) {
        (Mode::Offline, _) => "offline",
        (Mode::Away, _) => "away",
        (Mode::Auto, true) => "online",
        (Mode::Auto, false) => "away",
    }
}

/// Pinned `build_presence_update`, signed with this identity.
pub fn build_event(
    state: &str,
    keys: &Keys,
    created_at: Option<Timestamp>,
) -> Result<Event, &'static str> {
    let mut builder = buzz_sdk::build_presence_update(state).map_err(|_| "presence_invalid")?;
    if let Some(timestamp) = created_at {
        builder = builder.custom_created_at(timestamp);
    }
    builder.sign_with_keys(keys).map_err(|_| "presence_invalid")
}

/// Process-wide, so a reconnect does not reset the limit.
pub static GATE: std::sync::Mutex<crate::user_status::Gate> =
    std::sync::Mutex::new(crate::user_status::Gate::new());

struct Pending {
    event: Event,
    state: &'static str,
    deadline: Instant,
}
/// The heartbeat a connection keeps; its `OK`s arrive on that connection.
#[derive(Default)]
pub struct Publisher {
    /// The panel's latest `(mode, active)`; none until the first
    /// `set_presence` and again after the last panel left or shutdown.
    wanted: Option<(Mode, bool)>,
    /// The state last written on this connection and when.
    sent: Option<&'static str>,
    sent_at: Option<Instant>,
    pending: Option<Pending>,
    published: Option<&'static str>,
    published_at: Option<u64>,
    category: Option<&'static str>,
}
impl Publisher {
    /// Records the panel's preference and idle hint. False when nothing
    /// changed (a no-op: nothing is scheduled).
    pub fn set(&mut self, mode: Mode, active: bool) -> bool {
        if self.wanted == Some((mode, active)) {
            return false;
        }
        self.wanted = Some((mode, active));
        true
    }
    pub fn configured(&self) -> bool {
        self.wanted.is_some()
    }
    /// The last panel left (or the helper stops): publish `offline` once if
    /// anything else was published, then nothing until the next `set`.
    pub fn detach(&mut self) {
        self.wanted = None;
    }
    /// The state this connection should show the relay now.
    fn target(&self) -> Option<&'static str> {
        match self.wanted {
            Some((mode, active)) => Some(derive(mode, active)),
            None => self.sent.filter(|s| *s != "offline").map(|_| "offline"),
        }
    }
    /// When the next publication is due: at once for a change, every
    /// `heartbeat` while online or away, never while one is in flight, and
    /// never before the rate gate (`ready`) admits it.
    pub fn due(
        &self,
        now: Instant,
        ready: Option<Instant>,
        heartbeat: Duration,
    ) -> Option<Instant> {
        if self.pending.is_some() {
            return None;
        }
        let target = self.target()?;
        let at = if Some(target) != self.sent {
            now
        } else if target != "offline" {
            self.sent_at.map_or(now, |at| at + heartbeat)
        } else {
            return None;
        };
        Some(ready.map_or(at, |ready| at.max(ready)))
    }
    /// Signs the due state and tracks it. The caller has admitted it through
    /// the gate and writes the EVENT.
    pub fn prepare(&mut self, keys: &Keys, now: Instant) -> Result<Event, &'static str> {
        let state = self.target().ok_or("presence_invalid")?;
        let event = build_event(state, keys, None)?;
        self.sent = Some(state);
        self.sent_at = Some(now);
        self.pending = Some(Pending {
            event: event.clone(),
            state,
            deadline: now + TIMEOUT,
        });
        Ok(event)
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    #[cfg(test)]
    pub fn pending_id(&self) -> Option<String> {
        self.pending.as_ref().map(|p| p.event.id.to_hex())
    }
    pub fn deadline(&self) -> Instant {
        self.pending
            .as_ref()
            .map(|p| p.deadline)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))
    }
    /// Resolves the pending publication from an `OK` for its exact event id.
    /// A refusal never changes the connection; it is a category only.
    pub fn acknowledge(&mut self, event_id: &str, accepted: bool, unix_now: u64) -> bool {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.event.id.to_hex() == event_id)
        {
            return false;
        }
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if accepted {
            self.published = Some(pending.state);
            self.published_at = Some(unix_now);
            self.category = None;
        } else {
            self.category = Some("presence_rejected");
        }
        true
    }
    /// No `OK` in time (or the session re-authenticates): the relay may have
    /// stored it. The next heartbeat tries again.
    pub fn unknown(&mut self) -> bool {
        if self.pending.take().is_some() {
            self.category = Some("relay_unavailable");
            return true;
        }
        false
    }
    /// A re-authenticated session starts over: the state is written again.
    pub fn reauthenticated(&mut self) {
        self.unknown();
        self.sent = None;
        self.sent_at = None;
    }
    /// The state shown beside this identity's own name: what the relay accepted.
    pub fn published(&self) -> Option<&'static str> {
        self.published
    }
    pub fn view(&self, authenticated: bool) -> PresenceView {
        if !authenticated {
            return PresenceView::unavailable();
        }
        let state = if self.category.is_some() {
            "failed"
        } else if self.published.is_some() {
            "ready"
        } else {
            "unavailable"
        };
        PresenceView {
            state: state.into(),
            mode: self.wanted.map(|(mode, _)| mode.as_str().to_owned()),
            published: self.published.map(str::to_owned),
            last_published_at: self.published_at,
            category: self.category.map(str::to_owned),
            peers: Vec::new(),
        }
    }
}

/// `online`, `away` or `offline` from bare content or the legacy
/// `{"status": …}` JSON the relay also accepts (bounded); anything else is none.
pub fn content_state(content: &str) -> Option<&'static str> {
    let value = if content.starts_with('{') {
        if content.len() > LEGACY_BYTES {
            return None;
        }
        let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
        parsed.get("status")?.as_str()?.to_owned()
    } else {
        content.to_owned()
    };
    match value.as_str() {
        "online" => Some("online"),
        "away" => Some("away"),
        "offline" => Some("offline"),
        _ => None,
    }
}
fn canonical_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// States for `subjects` from a presence read, failing closed: any event with a
/// bad signature, another kind or a signer other than the relay's pinned
/// NIP-11 `self` key rejects the whole read. A relay-signed event that names
/// no usable subject (no `p` tag, several, a non-canonical one) or a subject
/// outside `subjects` is ignored: it is not a forgery, and the maintainer's
/// relay was seen returning one subject-less relay-signed event beside the
/// expected ones (the pinned source always tags, so that relay runs another
/// build); ignoring keeps the closed world, since only asked-for subjects are
/// ever shown. A subject the relay returned nothing for is offline (an empty
/// snapshot is authoritative, `bridge.rs:2545-2548`); a stale event
/// (`created_at` more than `FRESH_SECS` from now) is offline; one with
/// unrecognised content leaves its subject unknown (absent from the map). A
/// self-signed peer event is never used here (Desktop trusts a `p` subject
/// only from the relay, `presence.ts:3-6`).
pub fn verify(
    relay: &PublicKey,
    subjects: &[String],
    events: &[Event],
    now: u64,
) -> Result<BTreeMap<String, &'static str>, &'static str> {
    let mut newest: BTreeMap<String, &Event> = BTreeMap::new();
    for event in events {
        if event.verify().is_err() || event.kind.as_u16() != KIND || event.pubkey != *relay {
            return Err("presence_invalid");
        }
        let mut p = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().is_some_and(|v| v == "p"));
        let subject = match (p.next().map(|t| t.as_slice()), p.next()) {
            (Some([_, subject, ..]), None) if canonical_hex(subject) => subject.clone(),
            _ => continue,
        };
        if !subjects.contains(&subject) {
            continue;
        }
        if newest
            .get(&subject)
            .is_none_or(|old| event.created_at > old.created_at)
        {
            newest.insert(subject, event);
        }
    }
    let mut out = BTreeMap::new();
    for subject in subjects {
        let state = match newest.get(subject) {
            None => Some("offline"),
            Some(event) if now.abs_diff(event.created_at.as_secs()) > FRESH_SECS => Some("offline"),
            Some(event) => content_state(&event.content),
        };
        if let Some(state) = state {
            out.insert(subject.clone(), state);
        }
    }
    Ok(out)
}

/// One refresh: `subjects` (canonical hex, at most `SUBJECTS`) in batches of
/// `AUTHORS`, one query after another so only one presence read is in flight.
pub async fn read(
    relay_url: &str,
    keys: &Keys,
    relay: PublicKey,
    subjects: Vec<String>,
) -> Result<BTreeMap<String, &'static str>, &'static str> {
    if subjects.len() > SUBJECTS {
        return Err("presence_invalid");
    }
    let mut out = BTreeMap::new();
    for batch in subjects.chunks(AUTHORS) {
        let authors = batch
            .iter()
            .map(|key| PublicKey::from_hex(key).map_err(|_| "presence_invalid"))
            .collect::<Result<Vec<_>, _>>()?;
        let events = crate::query::query(
            relay_url,
            keys,
            &crate::query::QueryRequest::Presence { authors },
        )
        .await?;
        out.extend(verify(&relay, batch, &events, Timestamp::now().as_secs())?);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "presence_tests.rs"]
mod tests;
