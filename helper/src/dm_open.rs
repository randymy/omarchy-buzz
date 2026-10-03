//! Opening a direct message: one signed kind 41010 command, published outside
//! the kind-9 send ledger.
//!
//! The pinned relay resolves the DM by its participant set and answers the same
//! channel for every repeat (`command_executor.rs:297-429`), so a lost answer is
//! safe to retry with a new request. The helper still never re-signs an intent
//! it has seen, tracks one open at a time, and reports an outcome only from the
//! relay's exact-ID `OK`.
use crate::protocol::{DmOpen, DmOpenIntent, Status};
use nostr::{Event, Keys, PublicKey};
use std::collections::VecDeque;
use tokio::time::{Duration, Instant};

/// Same bound as a kind-9 send: no `OK` by then means the outcome is unknown.
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// The relay's answer is `response:{"channel_id":…,"created":…}`; anything
/// longer is not that answer.
const RESPONSE_LIMIT: usize = 1024;

struct Pending {
    request_id: String,
    participants: Vec<String>,
    event_id: String,
    deadline: Instant,
}

#[derive(Default)]
pub struct Opener {
    pending: Option<Pending>,
    /// Keys the helper itself just served from the relay's people directory or
    /// search, oldest first. Only this connection's reads are remembered.
    served: VecDeque<String>,
}

/// Enough for several full directory pages; older keys fall off.
const SERVED: usize = 200;

impl Opener {
    /// Remembers verified keys from a directory or search read.
    pub fn remember(&mut self, keys: impl IntoIterator<Item = String>) {
        for key in keys {
            self.served.retain(|k| *k != key);
            self.served.push_back(key);
            if self.served.len() > SERVED {
                self.served.pop_front();
            }
        }
    }
    pub fn forget_served(&mut self) {
        self.served.clear();
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn deadline(&self) -> Instant {
        self.pending
            .as_ref()
            .map(|p| p.deadline)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))
    }
    /// Checks and signs an intent. `Ok(None)` is an identical replay of the
    /// pending open: its receipt stays and nothing is signed again.
    pub fn prepare(
        &mut self,
        intent: &DmOpenIntent,
        keys: &Keys,
        status: &Status,
        fresh: bool,
        trusted: bool,
    ) -> Result<Option<(DmOpen, Event)>, &'static str> {
        if let Some(pending) = &self.pending {
            if pending.request_id != intent.request_id {
                return Err("dm_open_busy");
            }
            if pending.participants != intent.participants {
                return Err("dm_open_request_reused");
            }
            return Ok(None);
        }
        // A request ID already reported is never signed again.
        if status.dm_open.request_id.as_deref() == Some(intent.request_id.as_str()) {
            return Err("dm_open_request_reused");
        }
        if !valid(intent) {
            return Err("dm_open_invalid");
        }
        if intent.generation != status.generation {
            return Err("dm_open_scope_changed");
        }
        if !fresh
            || !trusted
            || status.connection != "authenticated"
            || !matches!(status.catalog.state.as_str(), "partial" | "ready")
        {
            return Err("dm_open_unavailable");
        }
        let own = keys.public_key().to_hex();
        if intent.participants.iter().any(|key| *key == own) {
            return Err("dm_open_invalid");
        }
        if !intent
            .participants
            .iter()
            .all(|key| allowed(status, &self.served, key.as_str()))
        {
            return Err("dm_open_access_denied");
        }
        let event = build_event(&intent.participants, keys, None)?;
        let event_id = event.id.to_hex();
        self.pending = Some(Pending {
            request_id: intent.request_id.clone(),
            participants: intent.participants.clone(),
            event_id,
            deadline: Instant::now() + TIMEOUT,
        });
        Ok(Some((
            view(&intent.request_id, "sending", None, None, None),
            event,
        )))
    }
    /// Forgets a prepared open whose caller left before its EVENT was written.
    pub fn abandon(&mut self) {
        self.pending = None;
    }
    /// Resolves the pending open from an `OK` for its exact event ID.
    pub fn acknowledge(&mut self, event_id: &str, accepted: bool, message: &str) -> Option<DmOpen> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.event_id == event_id)
        {
            return None;
        }
        let pending = self.pending.take()?;
        if !accepted {
            return Some(view(
                &pending.request_id,
                "rejected",
                None,
                None,
                Some("dm_open_rejected"),
            ));
        }
        Some(match parse_response(message) {
            Some((channel, created)) => view(
                &pending.request_id,
                "acknowledged",
                Some(channel),
                Some(created),
                None,
            ),
            // Accepted (for example a relay `duplicate:` answer) without a channel.
            None => view(
                &pending.request_id,
                "acknowledged",
                None,
                None,
                Some("dm_open_response_unknown"),
            ),
        })
    }
    /// Timeout, disconnect or re-authentication: the relay may have opened it.
    pub fn unknown(&mut self) -> Option<DmOpen> {
        let pending = self.pending.take()?;
        Some(view(
            &pending.request_id,
            "unknown",
            None,
            None,
            Some("dm_open_unknown"),
        ))
    }
}

/// Keys a DM may be opened with: the verified roster of the room whose members
/// are on screen, a participant of a DM already in the joined catalog, or a key
/// the helper served from the relay's people directory or search (`served`).
/// Everything offered is data the helper verified; the panel is never trusted.
pub fn allowed(status: &Status, served: &VecDeque<String>, key: &str) -> bool {
    let roster = status.recipients.state == "snapshot"
        && status
            .recipients
            .room_id
            .as_ref()
            .is_some_and(|room| status.catalog.rooms.iter().any(|r| &r.id == room))
        && status
            .recipients
            .entries
            .iter()
            .any(|entry| entry.key == key);
    roster
        || served.iter().any(|k| k == key)
        || status
            .catalog
            .rooms
            .iter()
            .any(|room| room.kind == "dm" && room.participants.iter().any(|p| p == key))
}

/// Exactly `buzz_sdk::build_dm_open`: kind 41010, empty content, one `p` tag
/// per other participant in the given order, no `d` or `h` tag.
pub fn build_event(
    participants: &[String],
    keys: &Keys,
    created_at: Option<nostr::Timestamp>,
) -> Result<Event, &'static str> {
    let keys_hex: Vec<&str> = participants.iter().map(String::as_str).collect();
    let mut builder = buzz_sdk::build_dm_open(&keys_hex).map_err(|_| "dm_open_invalid")?;
    if let Some(timestamp) = created_at {
        builder = builder.custom_created_at(timestamp);
    }
    builder.sign_with_keys(keys).map_err(|_| "dm_open_invalid")
}

/// The relay's positive answer: `response:` then `{"channel_id","created"}`
/// (`command_executor.rs:417-428`). Desktop's `parse_command_response` also
/// accepts the bare JSON object; so does this. The channel must be a canonical
/// UUID and `created` a boolean; other fields are ignored as Desktop does.
pub fn parse_response(message: &str) -> Option<(String, bool)> {
    if message.len() > RESPONSE_LIMIT {
        return None;
    }
    let json = message.strip_prefix("response:").unwrap_or(message);
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let channel = value.get("channel_id")?.as_str()?;
    let created = value.get("created")?.as_bool()?;
    let parsed = uuid::Uuid::parse_str(channel).ok()?;
    (parsed.to_string() == channel).then(|| (channel.to_owned(), created))
}

fn valid(intent: &DmOpenIntent) -> bool {
    uuid::Uuid::parse_str(&intent.request_id).is_ok_and(|id| id.to_string() == intent.request_id)
        && !intent.participants.is_empty()
        && intent.participants.len() <= 8
        && intent
            .participants
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == intent.participants.len()
        && intent
            .participants
            .iter()
            .all(|p| PublicKey::from_hex(p).is_ok_and(|key| key.to_hex() == *p))
}

fn view(
    request: &str,
    state: &str,
    channel: Option<String>,
    created: Option<bool>,
    category: Option<&str>,
) -> DmOpen {
    DmOpen {
        state: state.into(),
        request_id: Some(request.to_owned()),
        channel_id: channel,
        created,
        category: category.map(str::to_owned),
    }
}

#[cfg(test)]
#[path = "dm_open_tests.rs"]
mod tests;
