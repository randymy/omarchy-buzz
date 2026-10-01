//! Verified roster keys with optional self-asserted plaintext profile names.
//! Names are display hints, never ownership, classification, or authority.
use crate::query::{query, QueryRequest};
use crate::user_status::UserStatus;
use nostr::{Event, Keys, PublicKey, Timestamp};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const LIMIT: usize = 20;
#[derive(Clone, Debug)]
pub struct Recipient {
    pub key: String,
    pub name: String,
    /// Verified, unexpired `user_status`; none when absent or not read.
    pub status: Option<UserStatus>,
}
#[derive(Clone, Debug)]
pub struct Recipients {
    pub room: String,
    pub partial: bool,
    pub entries: Vec<Recipient>,
    pub agents: Vec<crate::agents::AgentHint>,
    /// The status read succeeded: an entry without a status has none.
    pub statuses_known: bool,
}

pub fn roster(
    room: Uuid,
    member: PublicKey,
    signer: PublicKey,
    events: &[Event],
    now: u64,
) -> Result<Recipients, &'static str> {
    if events.len() != 1 {
        return Err("recipients_invalid_roster");
    }
    let event = &events[0];
    event.verify().map_err(|_| "recipients_invalid_roster")?;
    if event.pubkey != signer
        || event.kind.as_u16() != 39002
        || !event.content.is_empty()
        || event.created_at.as_secs() > now.saturating_add(60)
    {
        return Err("recipients_invalid_roster");
    }
    let scope = room.to_string();
    let mut d = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "d"));
    if d.next()
        .and_then(|t| t.as_slice().get(1))
        .map(String::as_str)
        != Some(scope.as_str())
        || d.next().is_some()
    {
        return Err("recipients_invalid_roster");
    }
    let mut keys = BTreeSet::new();
    for tag in event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "p"))
    {
        let value = tag.as_slice().get(1).ok_or("recipients_invalid_roster")?;
        let public = PublicKey::from_hex(value).map_err(|_| "recipients_invalid_roster")?;
        if public.to_hex() != *value || !keys.insert(value.clone()) {
            return Err("recipients_invalid_roster");
        }
    }
    if !keys.contains(&member.to_hex()) {
        return Err("recipients_access_denied");
    }
    // A bounded snapshot has no completeness/freshness guarantee beyond this read.
    // Validate the whole roster before taking the bounded UI subset.
    Ok(Recipients {
        room: scope,
        partial: true,
        entries: keys
            .into_iter()
            .take(LIMIT)
            .map(|key| Recipient {
                key,
                name: String::new(),
                status: None,
            })
            .collect(),
        agents: Vec::new(),
        statuses_known: false,
    })
}
/// Sanitized display name from one kind 0 event; empty when absent or malformed.
pub(crate) fn name(event: &Event) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.content) else {
        return String::new();
    };
    let Some(object) = value.as_object() else {
        return String::new();
    };
    let chosen = object
        .get("display_name")
        .filter(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
        .or_else(|| object.get("name"));
    let Some(value) = chosen.and_then(|v| v.as_str()) else {
        return String::new();
    };
    sanitize(value, 64)
}
/// An untrusted self-asserted label, at most `limit` bytes: controls and bidi
/// formatting become spaces (they must not reorder an adjacent identity key);
/// emoji ZWJ sequences are preserved.
pub(crate) fn sanitize(value: &str, limit: usize) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let ch = if ch.is_control() || crate::user_status::bidi(ch) {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > limit {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}
/// Verified `user_status` (kind 30315, `d:general`) per author, for authors
/// with a current status. The single verification routine for status events
/// (docs/PRESENCE_MAP.md §6): any event with a bad signature, another kind, an
/// author outside `roster`, a future time or not exactly one `d` tag equal to
/// `general` rejects the whole read. Per author the newest by `created_at`
/// (the lower id on a tie) is the state: it counts as no status when expired (`expiration` at or
/// before `now`; the relay never enforces it), when its `emoji` or
/// `expiration` tag is malformed or repeated, or when it is empty after the
/// text is sanitized like a name.
pub fn status(
    roster: &[String],
    events: &[Event],
    now: u64,
) -> Result<BTreeMap<String, UserStatus>, &'static str> {
    let general = |e: &Event| {
        let mut d = e
            .tags
            .iter()
            .filter(|t| t.as_slice().first().is_some_and(|v| v == "d"));
        let first = d.next().map(|t| t.as_slice());
        first.is_some_and(|t| t.len() == 2 && t[1] == "general") && d.next().is_none()
    };
    if events.len() > 200
        || events.iter().any(|e| {
            e.verify().is_err()
                || e.kind.as_u16() != crate::user_status::KIND
                || e.created_at.as_secs() > now.saturating_add(60)
                || !roster.iter().any(|key| *key == e.pubkey.to_hex())
                || !general(e)
        })
    {
        return Err("status_invalid");
    }
    let mut latest: BTreeMap<String, &Event> = BTreeMap::new();
    for event in events {
        let key = event.pubkey.to_hex();
        // NIP-01 replaceable order, as Desktop's `statusVersionIsAtLeast`:
        // the newer `created_at`, then the lower id on a tie.
        if latest.get(&key).is_none_or(|old| {
            event.created_at > old.created_at
                || (event.created_at == old.created_at && event.id < old.id)
        }) {
            latest.insert(key, event);
        }
    }
    let values = |e: &Event, name: &str| -> Vec<String> {
        e.tags
            .iter()
            .filter(|t| t.as_slice().first().is_some_and(|v| v == name))
            .map(|t| t.as_slice().get(1).cloned().unwrap_or_default())
            .collect()
    };
    let mut out = BTreeMap::new();
    for (key, event) in latest {
        let expires_at = match values(event, "expiration").as_slice() {
            [] => None,
            [value]
                if (1..=20).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit()) =>
            {
                match value.parse::<u64>() {
                    Ok(at) if at > now => Some(at),
                    _ => continue,
                }
            }
            _ => continue,
        };
        let emoji = match values(event, "emoji").as_slice() {
            [] => None,
            [value] if crate::user_status::valid_emoji(value) => Some(value.clone()),
            _ => continue,
        };
        let text = sanitize(&event.content, crate::user_status::TEXT_BYTES);
        if text.is_empty() && emoji.is_none() {
            continue;
        }
        out.insert(
            key,
            UserStatus {
                text,
                emoji,
                expires_at,
            },
        );
    }
    Ok(out)
}
/// Bad signature/scope/time rejects the entire profile projection, preserving
/// the trusted roster keys with empty names. Missing or malformed per-author
/// names also remain empty. No profile field is an agent/human classification.
pub fn profiles(recipients: &mut Recipients, events: &[Event], now: u64) {
    for entry in &mut recipients.entries {
        entry.name.clear();
    }
    if events.len() > 200
        || events.iter().any(|e| {
            e.verify().is_err()
                || e.kind.as_u16() != 0
                || e.created_at.as_secs() > now.saturating_add(60)
                || !recipients
                    .entries
                    .iter()
                    .any(|r| r.key == e.pubkey.to_hex())
        })
    {
        return;
    }
    let mut latest: BTreeMap<String, (&Event, bool)> = BTreeMap::new();
    for event in events {
        let key = event.pubkey.to_hex();
        match latest.get_mut(&key) {
            Some((old, conflict)) if old.created_at == event.created_at => {
                if old.id != event.id {
                    *conflict = true;
                }
            }
            Some((old, _)) if old.created_at > event.created_at => {}
            _ => {
                latest.insert(key, (event, false));
            }
        }
    }
    for entry in &mut recipients.entries {
        if let Some((event, false)) = latest.get(&entry.key) {
            entry.name = name(event);
        }
    }
}
pub async fn fetch(
    relay: &str,
    keys: &Keys,
    trusted_signer: PublicKey,
    room: Uuid,
) -> Result<Recipients, &'static str> {
    let events = query(relay, keys, &QueryRequest::RoomMembers { room }).await?;
    let mut recipients = roster(
        room,
        keys.public_key(),
        trusted_signer,
        &events,
        Timestamp::now().as_secs(),
    )?;
    let authors = recipients
        .entries
        .iter()
        .map(|r| PublicKey::from_hex(&r.key).map_err(|_| "recipients_invalid_roster"))
        .collect::<Result<Vec<_>, _>>()?;
    // Profile unavailability never invents names or discards proven roster keys.
    match query(
        relay,
        keys,
        &QueryRequest::Profiles {
            authors: authors.clone(),
        },
    )
    .await
    {
        Ok(events) => profiles(&mut recipients, &events, Timestamp::now().as_secs()),
        Err(_) => {}
    }
    match query(
        relay,
        keys,
        &QueryRequest::AgentProfiles {
            authors: authors.clone(),
        },
    )
    .await
    {
        Ok(events) => {
            let roster_keys = recipients
                .entries
                .iter()
                .map(|r| r.key.clone())
                .collect::<Vec<_>>();
            if let Ok(agents) =
                crate::agents::project(&roster_keys, &events, Timestamp::now().as_secs())
            {
                recipients.agents = agents;
            }
        }
        Err(_) => {}
    }
    // One batched status read for the same roster; unavailable means unknown.
    if let Ok(events) = query(relay, keys, &QueryRequest::UserStatuses { authors }).await {
        let roster_keys = recipients
            .entries
            .iter()
            .map(|r| r.key.clone())
            .collect::<Vec<_>>();
        if let Ok(mut found) = status(&roster_keys, &events, Timestamp::now().as_secs()) {
            for entry in &mut recipients.entries {
                entry.status = found.remove(&entry.key);
            }
            recipients.statuses_known = true;
        }
    }
    Ok(recipients)
}
/// This identity's own status, through the same verification as any roster.
pub async fn own_status(relay: &str, keys: &Keys) -> Result<Option<UserStatus>, &'static str> {
    let own = keys.public_key();
    let events = query(
        relay,
        keys,
        &QueryRequest::UserStatuses { authors: vec![own] },
    )
    .await?;
    let key = own.to_hex();
    Ok(status(
        std::slice::from_ref(&key),
        &events,
        Timestamp::now().as_secs(),
    )?
    .remove(&key))
}
#[cfg(test)]
#[path = "recipients_tests.rs"]
mod tests;
