//! Bounded first-page, depth-one NIP-CW thread preview for the selected root.
use crate::query::{query, QueryRequest};
use nostr::{
    hashes::{sha256, Hash},
    Event, EventId, Keys, PublicKey, Timestamp,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const ROWS: usize = 8;
const EVENTS: usize = 200;
const BYTES: usize = 512 * 1024;

#[derive(Clone, Debug)]
pub struct Thread {
    pub room: String,
    pub root: String,
    pub category: &'static str,
    pub has_more: bool,
    pub rows: Vec<crate::history::Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    created_at: u64,
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    version: u8,
    direction: String,
    has_more: bool,
    next_cursor: Option<Cursor>,
}

fn one<'a>(event: &'a Event, name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut found = None;
    for tag in event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == name))
    {
        if found.is_some() || tag.as_slice().len() != 2 {
            return Err("thread_invalid_shape");
        }
        found = Some(tag.as_slice()[1].as_str());
    }
    Ok(found)
}
fn id(value: &str) -> Result<String, &'static str> {
    let parsed = EventId::from_hex(value).map_err(|_| "thread_invalid_shape")?;
    if value != parsed.to_hex() {
        return Err("thread_invalid_shape");
    }
    Ok(value.to_owned())
}
fn targets(event: &Event) -> Result<Vec<String>, &'static str> {
    let mut out = Vec::new();
    for tag in event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "e"))
    {
        out.push(id(tag.as_slice().get(1).ok_or("thread_invalid_shape")?)?);
    }
    if out.is_empty() {
        return Err("thread_invalid_shape");
    }
    Ok(out)
}
fn author(event: &Event, relay: PublicKey) -> Result<PublicKey, &'static str> {
    if event.pubkey == relay {
        if let Some(actor) = one(event, "actor")? {
            return PublicKey::from_hex(actor).map_err(|_| "thread_invalid_shape");
        }
    }
    Ok(event.pubkey)
}
fn text(value: &str) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for ch in value.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if out.len() + ch.len_utf8() > 768 {
            truncated = true;
            break;
        }
        out.push(ch);
    }
    (out, truncated)
}

/// Reproduce the pinned buzz-core `thread_window::Request::binding` array and
/// `tenant::relay_url_authority`, including non-default ports.
fn binding(
    relay_url: &str,
    reader: PublicKey,
    room: Uuid,
    root: EventId,
) -> Result<String, &'static str> {
    let canonical =
        crate::config::canonical_relay(relay_url).map_err(|_| "thread_invalid_origin")?;
    let url = url::Url::parse(&canonical).map_err(|_| "thread_invalid_origin")?;
    let host = match url.host().ok_or("thread_invalid_origin")? {
        url::Host::Domain(s) => s.to_string(),
        url::Host::Ipv4(a) => a.to_string(),
        url::Host::Ipv6(a) => format!("[{a}]"),
    };
    let mut host = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
    .to_ascii_lowercase();
    if let Some(value) = host
        .strip_suffix(":443")
        .or_else(|| host.strip_suffix(":80"))
    {
        host = value.to_owned();
    }
    if let Some(value) = host.strip_suffix('.') {
        host = value.to_owned();
    }
    let normalized = serde_json::json!([
        "tw",
        1,
        "older",
        host,
        reader.to_hex(),
        room.to_string(),
        root.to_hex(),
        ROWS,
        1,
        [9],
        null,
        true
    ]);
    Ok(format!(
        "tw:1:{}",
        sha256::Hash::hash(normalized.to_string().as_bytes())
    ))
}

/// Invalid or incomplete pages fail as a whole; no unsigned exhaustion.
pub fn reduce(
    relay_url: &str,
    reader: PublicKey,
    room: Uuid,
    root: EventId,
    relay: PublicKey,
    events: &[Event],
    now: u64,
) -> Result<Thread, &'static str> {
    if events.len() > EVENTS {
        return Err("thread_oversized");
    }
    if serde_json::to_vec(events)
        .map_err(|_| "thread_invalid_shape")?
        .len()
        > BYTES
    {
        return Err("thread_oversized");
    }
    let room_hex = room.to_string();
    let root_hex = root.to_hex();
    let expected = binding(relay_url, reader, room, root)?;
    let mut seen = BTreeSet::new();
    let mut bounds = None;
    let mut originals = BTreeMap::new();
    let mut edits = Vec::new();
    let mut deletes = Vec::new();
    for event in events {
        event.verify().map_err(|_| "thread_invalid_signature")?;
        if event.created_at.as_secs() > now.saturating_add(60) {
            return Err("thread_invalid_shape");
        }
        if !seen.insert(event.id.to_hex()) {
            return Err("thread_duplicate_event");
        }
        let kind = event.kind.as_u16();
        let h = one(event, "h")?;
        if matches!(kind, 5 | 9005) {
            if h.is_some_and(|h| h != room_hex) {
                return Err("thread_invalid_scope");
            }
        } else if h != Some(room_hex.as_str()) {
            return Err("thread_invalid_scope");
        }
        match kind {
            39007 => {
                if now.saturating_sub(event.created_at.as_secs()) > 60 {
                    return Err("thread_stale_bounds");
                }
                if bounds.is_some()
                    || event.pubkey != relay
                    || event.tags.len() != 3
                    || one(event, "d")? != Some(expected.as_str())
                    || one(event, "e")? != Some(root_hex.as_str())
                {
                    return Err("thread_invalid_bounds");
                }
                let raw: serde_json::Value =
                    serde_json::from_str(&event.content).map_err(|_| "thread_invalid_bounds")?;
                if raw.get("next_cursor").is_none() {
                    return Err("thread_invalid_bounds");
                }
                let value: Bounds =
                    serde_json::from_value(raw).map_err(|_| "thread_invalid_bounds")?;
                if value.version != 1
                    || value.direction != "older"
                    || value.has_more != value.next_cursor.is_some()
                {
                    return Err("thread_invalid_bounds");
                }
                if let Some(cursor) = &value.next_cursor {
                    id(&cursor.id).map_err(|_| "thread_invalid_bounds")?;
                    if cursor.created_at > now.saturating_add(60) {
                        return Err("thread_invalid_bounds");
                    }
                }
                bounds = Some(value);
            }
            9 => {
                if event.id == root {
                    continue;
                } // Root auxiliary is never a reply row.
                let mut reply = None;
                let mut marked_root = None;
                for tag in event
                    .tags
                    .iter()
                    .filter(|t| t.as_slice().first().is_some_and(|v| v == "e"))
                {
                    let parts = tag.as_slice();
                    if parts.len() < 4 {
                        continue;
                    }
                    match parts[3].as_str() {
                        "reply" => {
                            if reply.is_some() {
                                return Err("thread_invalid_shape");
                            }
                            reply = Some(id(&parts[1])?);
                        }
                        "root" => {
                            if marked_root.is_some() {
                                return Err("thread_invalid_shape");
                            }
                            marked_root = Some(id(&parts[1])?);
                        }
                        _ => {}
                    }
                }
                if reply.as_deref() != Some(root_hex.as_str())
                    || marked_root.is_some_and(|r| r != root_hex)
                {
                    return Err("thread_invalid_scope");
                }
                originals.insert(event.id.to_hex(), event);
            }
            40003 => {
                if targets(event)?.len() != 1 {
                    return Err("thread_invalid_shape");
                }
                edits.push(event);
            }
            5 | 9005 => {
                targets(event)?;
                deletes.push(event);
            }
            7 => {
                targets(event)?;
            }
            _ => return Err("thread_invalid_kind"),
        }
    }
    let bounds = bounds.ok_or("thread_missing_bounds")?;
    if originals.len() > ROWS {
        return Err("thread_oversized");
    }
    let mut deleted = BTreeSet::new();
    let mut uncertain = BTreeSet::new();
    let edit_ids: BTreeMap<_, _> = edits.iter().map(|e| (e.id.to_hex(), *e)).collect();
    for marker in deletes {
        for target in targets(marker)? {
            let original = originals
                .get(&target)
                .copied()
                .or_else(|| edit_ids.get(&target).copied());
            if let Some(original) = original {
                if marker.pubkey == author(original, relay)? {
                    deleted.insert(target);
                } else {
                    uncertain.insert(target);
                }
            }
        }
    }
    let mut latest: BTreeMap<String, &Event> = BTreeMap::new();
    for edit in edits {
        let target = targets(edit)?.pop().ok_or("thread_invalid_shape")?;
        let Some(original) = originals.get(&target) else {
            continue;
        }; // Root or unseen-row aux.
        if uncertain.contains(&edit.id.to_hex()) {
            uncertain.insert(target.clone());
        }
        if deleted.contains(&edit.id.to_hex()) || deleted.contains(&target) {
            continue;
        }
        if edit.created_at < original.created_at {
            return Err("thread_invalid_shape");
        }
        if edit.pubkey != author(original, relay)? {
            uncertain.insert(target);
            continue;
        }
        match latest.get(&target) {
            Some(old) if old.created_at == edit.created_at && old.id != edit.id => {
                uncertain.insert(target);
            }
            Some(old) if old.created_at > edit.created_at => {}
            _ => {
                latest.insert(target, edit);
            }
        }
    }
    let mut rows = Vec::new();
    for (id, original) in originals {
        if deleted.contains(&id) {
            continue;
        }
        let unavailable = uncertain.contains(&id);
        let edit = latest.get(&id);
        let (body, truncated) = if unavailable {
            (String::new(), false)
        } else {
            text(edit.map_or(original.content.as_str(), |e| e.content.as_str()))
        };
        rows.push(crate::history::Row {
            id,
            author_pubkey: author(original, relay)?.to_hex(),
            timestamp: original.created_at.as_secs(),
            text: body,
            edited: edit.is_some(),
            truncated,
            unavailable,
        });
    }
    rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
    Ok(Thread {
        room: room_hex,
        root: root_hex,
        category: "thread_completeness_unknown",
        has_more: bounds.has_more,
        rows,
    })
}

pub async fn fetch(
    relay_url: &str,
    keys: &Keys,
    signer: PublicKey,
    room: Uuid,
    root_hex: &str,
) -> Result<Thread, &'static str> {
    let root = EventId::from_hex(root_hex).map_err(|_| "thread_invalid_root")?;
    if root.to_hex() != root_hex {
        // Keep the request identity and signed bounds in one canonical form.
        return Err("thread_invalid_root");
    }
    let events = query(relay_url, keys, &QueryRequest::ThreadReplies { room, root }).await?;
    reduce(
        relay_url,
        keys.public_key(),
        room,
        root,
        signer,
        &events,
        Timestamp::now().as_secs(),
    )
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
