//! Verified roster keys with optional self-asserted plaintext profile names.
//! Names are display hints, never ownership, classification, or authority.
use crate::query::{query, QueryRequest};
use nostr::{Event, Keys, PublicKey, Timestamp};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const LIMIT: usize = 20;
#[derive(Clone, Debug)]
pub struct Recipient {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct Recipients {
    pub room: String,
    pub partial: bool,
    pub entries: Vec<Recipient>,
    pub agents: Vec<crate::agents::AgentHint>,
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
            })
            .collect(),
        agents: Vec::new(),
    })
}
fn name(event: &Event) -> String {
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
    let mut out = String::new();
    for ch in value.chars() {
        // Profiles are untrusted labels displayed beside an identity key. Bidi
        // formatting must not reorder that surrounding key; preserve emoji ZWJ.
        let ch = if ch.is_control()
            || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > 64 {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
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
    match query(relay, keys, &QueryRequest::AgentProfiles { authors }).await {
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
    Ok(recipients)
}
#[cfg(test)]
#[path = "recipients_tests.rs"]
mod tests;
