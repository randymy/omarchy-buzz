//! "Update your status" (`user_status`): a NIP-38 kind 30315 event on Buzz's
//! single `d:general` coordinate, built exactly as pinned
//! `buzz_sdk::build_user_status` (`builders.rs:1917-1925`) plus the NIP-40
//! `["expiration", "<unix seconds>"]` tag Buzz Desktop adds client side
//! (`desktop/src/shared/api/relayClientSession.ts:388-394`). Clearing publishes
//! the empty replacement (no `emoji`, no `expiration`), as Desktop does.
//!
//! The relay neither enforces nor purges expiry (docs/PRESENCE_MAP.md §1):
//! every reader evaluates it (`recipients::status`). The helper signs, tracks
//! one publication at a time and reports an outcome only from the relay's
//! exact-ID `OK`, like `dm_open`.
use crate::protocol::{Status, StatusIntent, UserStatusView};
use nostr::{Event, Keys, Tag, Timestamp};
use serde::Serialize;
use tokio::time::{Duration, Instant};

pub const KIND: u16 = 30315;
/// Status text, after trimming. Desktop has no cap (PRESENCE_MAP §2); a line
/// in the panel has no use for more.
pub const TEXT_BYTES: usize = 200;
/// One hour to one week; the panel offers 1 h, 4 h, 24 h and 1 week.
pub const HOURS: std::ops::RangeInclusive<u32> = 1..=168;
pub const DEFAULT_HOURS: u32 = 24;
/// Same bound as a kind-9 send: no `OK` by then means the outcome is unknown.
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// At most one publication per this interval (`status_rate_limited`).
pub const GAP: Duration = Duration::from_secs(5);
/// Every category a status request or the `user_status` view may carry.
#[cfg(test)]
pub const CATEGORIES: [&str; 4] = [
    "status_invalid",
    "status_rate_limited",
    "status_rejected",
    "relay_unavailable",
];

/// A verified, sanitized status. `expires_at` is unix seconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub text: String,
    pub emoji: Option<String>,
    pub expires_at: Option<u64>,
}

/// Unicode bidi controls: they must not reorder the surrounding name or key.
pub(crate) fn bidi(ch: char) -> bool {
    matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}
/// Joiners, selectors, skin tones, keycap and tag characters: valid inside an
/// emoji sequence, never at its start.
fn modifier(ch: char) -> bool {
    matches!(ch, '\u{200d}' | '\u{fe00}'..='\u{fe0f}' | '\u{20e3}' | '\u{1f3fb}'..='\u{1f3ff}' | '\u{e0020}'..='\u{e007f}' | '\u{0300}'..='\u{036f}')
}
fn forbidden(ch: char) -> bool {
    ch.is_ascii()
        || ch.is_control()
        || ch.is_whitespace()
        || bidi(ch)
        || matches!(ch, '\u{200b}' | '\u{200c}' | '\u{2060}'..='\u{2064}' | '\u{feff}' | '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..)
}
/// A `:shortcode:` token (resolved by Buzz against custom emoji) or a short
/// native glyph sequence: 1-8 scalar values, no ASCII, controls, whitespace,
/// bidi or invisible characters, not starting with a joiner or modifier.
pub fn valid_emoji(value: &str) -> bool {
    if let Some(inner) = value.strip_prefix(':').and_then(|v| v.strip_suffix(':')) {
        return (1..=32).contains(&inner.len())
            && inner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b));
    }
    let count = value.chars().count();
    (1..=8).contains(&count)
        && !value.chars().next().is_some_and(modifier)
        && !value.chars().any(forbidden)
}

/// A checked `set_status`: trimmed text, optional emoji and hours.
#[derive(Debug, PartialEq)]
pub struct Wanted {
    pub text: String,
    pub emoji: Option<String>,
    pub hours: u32,
}
/// `status_invalid` unless the text (trimmed) fits, carries no control or
/// bidi character, the emoji is valid, something is set and the hours are in
/// range. Clearing is `clear_status`, never an empty `set_status`.
pub fn check(text: &str, emoji: Option<&str>, hours: Option<u32>) -> Result<Wanted, &'static str> {
    let text = text.trim();
    if text.len() > TEXT_BYTES || text.chars().any(|c| c.is_control() || bidi(c)) {
        return Err("status_invalid");
    }
    let emoji = match emoji.map(str::trim) {
        Some(e) if valid_emoji(e) => Some(e.to_owned()),
        Some(_) => return Err("status_invalid"),
        None => None,
    };
    if text.is_empty() && emoji.is_none() {
        return Err("status_invalid");
    }
    let hours = hours.unwrap_or(DEFAULT_HOURS);
    if !HOURS.contains(&hours) {
        return Err("status_invalid");
    }
    Ok(Wanted {
        text: text.to_owned(),
        emoji,
        hours,
    })
}

/// Pinned `build_user_status` (trimmed content, `["d","general"]`, `["emoji",…]`
/// when non-blank) plus `["expiration", "<secs>"]` when given.
pub fn build_event(
    text: &str,
    emoji: Option<&str>,
    expires_at: Option<u64>,
    keys: &Keys,
    created_at: Option<Timestamp>,
) -> Result<Event, &'static str> {
    let mut builder = buzz_sdk::build_user_status(text, emoji).map_err(|_| "status_invalid")?;
    if let Some(at) = expires_at {
        builder = builder.tag(
            Tag::parse(["expiration", at.to_string().as_str()]).map_err(|_| "status_invalid")?,
        );
    }
    if let Some(timestamp) = created_at {
        builder = builder.custom_created_at(timestamp);
    }
    builder.sign_with_keys(keys).map_err(|_| "status_invalid")
}

/// The last publication admitted, for the rate limit.
#[derive(Default)]
pub struct Gate {
    last: Option<Instant>,
}
impl Gate {
    pub const fn new() -> Self {
        Self { last: None }
    }
    /// The earliest instant `admit` accepts again, or none when it would now.
    pub fn ready_at(&self, gap: Duration) -> Option<Instant> {
        self.last.map(|last| last + gap)
    }
    /// Admits (and records) a publication unless one was admitted within `gap`.
    pub fn admit(&mut self, now: Instant, gap: Duration) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < gap)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}
/// Process-wide, so a reconnect does not reset the limit.
pub static GATE: std::sync::Mutex<Gate> = std::sync::Mutex::new(Gate::new());

struct Pending {
    event: Event,
    deadline: Instant,
}
/// The one status publication a connection tracks; its `OK` arrives on it.
#[derive(Default)]
pub struct Publisher {
    pending: Option<Pending>,
}
/// The relay's answer to the pending publication.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Accepted: the status now shown (none after a clear or an expired one).
    Accepted(Option<UserStatus>),
    Rejected,
}
impl Publisher {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn deadline(&self) -> Instant {
        self.pending
            .as_ref()
            .map(|p| p.deadline)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))
    }
    /// Checks and signs a request. Refusals are categories; nothing is signed.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        intent: &StatusIntent,
        keys: &Keys,
        status: &Status,
        ready: bool,
        gate: &mut Gate,
        gap: Duration,
        now: Instant,
        unix_now: u64,
    ) -> Result<(UserStatusView, Event), &'static str> {
        if self.pending.is_some() {
            return Err("status_rate_limited");
        }
        if !ready || status.connection != "authenticated" {
            return Err("relay_unavailable");
        }
        let (text, emoji, expires_at) = match &intent.set {
            Some(set) => {
                let wanted = check(&set.text, set.emoji.as_deref(), set.hours)?;
                let expires = unix_now.saturating_add(u64::from(wanted.hours) * 3600);
                (wanted.text, wanted.emoji, Some(expires))
            }
            None => (String::new(), None, None),
        };
        if !gate.admit(now, gap) {
            return Err("status_rate_limited");
        }
        let event = build_event(&text, emoji.as_deref(), expires_at, keys, None)?;
        self.pending = Some(Pending {
            event: event.clone(),
            deadline: now + TIMEOUT,
        });
        Ok((
            UserStatusView {
                state: "sending".into(),
                mine: status.user_status.mine.clone(),
                category: None,
            },
            event,
        ))
    }
    /// Forgets a prepared publication whose caller left before its EVENT was written.
    pub fn abandon(&mut self) {
        self.pending = None;
    }
    /// Resolves the pending publication from an `OK` for its exact event ID.
    /// An accepted event passes through the same verification as any other.
    pub fn acknowledge(
        &mut self,
        event_id: &str,
        accepted: bool,
        unix_now: u64,
    ) -> Option<Outcome> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.event.id.to_hex() == event_id)
        {
            return None;
        }
        let pending = self.pending.take()?;
        if !accepted {
            return Some(Outcome::Rejected);
        }
        let own = pending.event.pubkey.to_hex();
        let mine = crate::recipients::status(
            std::slice::from_ref(&own),
            std::slice::from_ref(&pending.event),
            unix_now,
        )
        .ok()
        .and_then(|mut found| found.remove(&own));
        Some(Outcome::Accepted(mine))
    }
    /// Timeout, disconnect or re-authentication: the relay may have stored it.
    pub fn unknown(&mut self) -> bool {
        self.pending.take().is_some()
    }
}

/// The view after a failed or unknown publication: the shown status stays.
pub fn failed(status: &Status, category: &str) -> UserStatusView {
    UserStatusView {
        state: "failed".into(),
        mine: status.user_status.mine.clone(),
        category: Some(category.to_owned()),
    }
}

#[cfg(test)]
#[path = "user_status_tests.rs"]
mod tests;
