//! First-page stream preview; no send API and no completeness claim.
use crate::query::{query, QueryRequest};
use nostr::{Event, EventId, Keys, PublicKey, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const EVENTS: usize = 200;
const BYTES: usize = 512 * 1024;
const ROWS: usize = 20;
const SUMMARY_REPLIES: u64 = 1_000_000;
const SUMMARY_PARTICIPANTS: usize = 10;
// 9999-12-31T23:59:59Z; the panel rejects later instants as unrepresentable.
const SUMMARY_TIME: u64 = 253_402_300_799;

/// Observed live reactions in this bounded page, not authoritative agent state.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Reactions {
    pub seen: usize,
    pub working: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub author_pubkey: String,
    pub timestamp: u64,
    pub text: String,
    pub edited: bool,
    pub truncated: bool,
    pub unavailable: bool,
    pub reactions: Option<Reactions>,
    pub thread: Option<ThreadSummary>,
}
/// Bounded projection of a relay-signed NIP-CW `kind:39005` thread summary.
/// Metadata about a row, never a row, a cursor input or a content claim.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub replies: u64,
    pub last_reply_at: Option<u64>,
    pub participants: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct History {
    pub room: String,
    pub category: &'static str,
    pub has_more: bool,
    pub rows: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    created_at: u64,
    id: String,
}
// NIP-CW: clients MUST ignore unknown overlay content fields, so no
// deny_unknown_fields here; the known fields are required and typed.
#[derive(Deserialize)]
struct SummaryContent {
    reply_count: u64,
    #[allow(dead_code)]
    descendant_count: u64,
    last_reply_at: Option<u64>,
    participants: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    has_more: bool,
    next_cursor: Option<Cursor>,
}

fn one<'a>(event: &'a Event, name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut tags = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == name));
    let value = tags.next().map(|t| t.as_slice().get(1).map(String::as_str));
    if tags.next().is_some() || value == Some(None) {
        return Err("history_invalid_shape");
    }
    Ok(value.flatten())
}
fn hex_id(value: &str) -> Result<String, &'static str> {
    let id = EventId::from_hex(value).map_err(|_| "history_invalid_shape")?;
    if id.to_hex() != value {
        return Err("history_invalid_shape");
    }
    Ok(value.to_owned())
}
fn targets(event: &Event) -> Result<Vec<String>, &'static str> {
    let mut values = Vec::new();
    for t in event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "e"))
    {
        values.push(hex_id(t.as_slice().get(1).ok_or("history_invalid_shape")?)?);
    }
    if values.is_empty() {
        return Err("history_invalid_shape");
    }
    Ok(values)
}
/// Verify a `kind:39005` overlay's exact tags and typed content. Returns the
/// summarized row id and the bounded projection. The signer is checked by the
/// caller; room scope (`h`) is checked with every other event.
fn summary(event: &Event, scope: &str) -> Result<(String, ThreadSummary), &'static str> {
    const INVALID: &str = "history_invalid_summary";
    // Exact tag cardinality: one `e`, one `d`, one `h`, nothing else.
    if event.tags.len() != 3 || event.tags.iter().any(|t| t.as_slice().len() != 2) {
        return Err(INVALID);
    }
    let e = one(event, "e").map_err(|_| INVALID)?.ok_or(INVALID)?;
    let d = one(event, "d").map_err(|_| INVALID)?.ok_or(INVALID)?;
    if one(event, "h").map_err(|_| INVALID)? != Some(scope) || e != d {
        return Err(INVALID);
    }
    let target = hex_id(e).map_err(|_| INVALID)?;
    let shape: serde_json::Value = serde_json::from_str(&event.content).map_err(|_| INVALID)?;
    // `last_reply_at` may be null but must be present; serde would default it.
    if shape.get("last_reply_at").is_none() {
        return Err(INVALID);
    }
    let content: SummaryContent = serde_json::from_value(shape).map_err(|_| INVALID)?;
    if content.last_reply_at.is_some_and(|at| at > SUMMARY_TIME)
        || content.participants.len() > SUMMARY_PARTICIPANTS
    {
        return Err(INVALID);
    }
    let mut distinct = BTreeSet::new();
    for participant in &content.participants {
        let key = PublicKey::from_hex(participant).map_err(|_| INVALID)?;
        if key.to_hex() != *participant || !distinct.insert(key) {
            return Err(INVALID);
        }
    }
    Ok((
        target,
        ThreadSummary {
            replies: content.reply_count.min(SUMMARY_REPLIES),
            last_reply_at: content.last_reply_at,
            participants: content
                .participants
                .into_iter()
                .take(SUMMARY_PARTICIPANTS)
                .collect(),
        },
    ))
}
fn author(event: &Event, relay: PublicKey) -> Result<PublicKey, &'static str> {
    // No ordinary mention (`p`) ever changes authorship. Only a relay-signed
    // actor claim is delegated attribution in this conservative preview.
    if event.pubkey == relay {
        if let Some(value) = one(event, "actor")? {
            return PublicKey::from_hex(value).map_err(|_| "history_invalid_shape");
        }
    }
    Ok(event.pubkey)
}
fn text(value: &str) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    for ch in value.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if output.len() + ch.len_utf8() > 768 {
            truncated = true;
            break;
        }
        output.push(ch);
    }
    (output, truncated)
}

/// Reject an over-budget page as a whole; never silently cut the auxiliary set.
/// The trusted relay's admission policy is not substituted for direct edit/delete
/// proof. Unknown owner/moderator authority suppresses affected content instead.
pub fn reduce(
    room: Uuid,
    relay: PublicKey,
    events: &[Event],
    now: u64,
) -> Result<History, &'static str> {
    if events.len() > EVENTS {
        return Err("history_oversized");
    }
    let bytes = serde_json::to_vec(events).map_err(|_| "history_invalid_shape")?;
    if bytes.len() > BYTES {
        return Err("history_oversized");
    }
    let scope = room.to_string();
    let mut index = BTreeMap::new();
    let mut bounds = None;
    let mut originals = BTreeMap::new();
    let mut edits = Vec::new();
    let mut deletions = Vec::new();
    let mut reactions = Vec::new();
    let mut summaries = Vec::new();
    for event in events {
        event.verify().map_err(|_| "history_invalid_signature")?;
        if event.created_at.as_secs() > now.saturating_add(60) {
            return Err("history_invalid_shape");
        }
        if index.insert(event.id.to_hex(), event).is_some() {
            return Err("history_duplicate_event");
        }
        let kind = event.kind.as_u16();
        let h = one(event, "h")?;
        if !matches!(kind, 5 | 9005 | 7) && h != Some(scope.as_str())
            || matches!(kind, 5 | 9005 | 7) && h.is_some_and(|h| h != scope)
        {
            return Err("history_invalid_scope");
        }
        match kind {
            9 | 40002 => {
                originals.insert(event.id.to_hex(), event);
            }
            40003 => {
                if targets(event)?.len() != 1 {
                    return Err("history_invalid_shape");
                }
                edits.push(event);
            }
            5 | 9005 => {
                targets(event)?;
                deletions.push(event);
            }
            7 => {
                targets(event)?;
                reactions.push(event);
            }
            39005 => {
                if event.pubkey != relay {
                    return Err("history_invalid_summary");
                }
                let (target, value) = summary(event, &scope)?;
                summaries.push((target, event, value));
            }
            39006 => {
                if now.saturating_sub(event.created_at.as_secs()) > 60 {
                    return Err("history_stale_bounds");
                }
                if bounds.is_some()
                    || event.pubkey != relay
                    || one(event, "d")? != Some(format!("{scope}:head").as_str())
                {
                    return Err("history_invalid_bounds");
                }
                let shape: serde_json::Value =
                    serde_json::from_str(&event.content).map_err(|_| "history_invalid_bounds")?;
                if shape.get("next_cursor").is_none() {
                    return Err("history_invalid_bounds");
                }
                let value: Bounds =
                    serde_json::from_value(shape).map_err(|_| "history_invalid_bounds")?;
                if value.has_more != value.next_cursor.is_some() {
                    return Err("history_invalid_bounds");
                }
                if let Some(cursor) = &value.next_cursor {
                    hex_id(&cursor.id).map_err(|_| "history_invalid_bounds")?;
                    if cursor.created_at > now.saturating_add(60) {
                        return Err("history_invalid_bounds");
                    }
                }
                bounds = Some(value);
            }
            _ => return Err("history_invalid_kind"),
        }
    }
    let bounds = bounds.ok_or("history_missing_bounds")?;
    if originals.len() > ROWS {
        return Err("history_oversized");
    }
    for reaction in &reactions {
        if !targets(reaction)?
            .iter()
            .any(|target| originals.contains_key(target))
        {
            return Err("history_invalid_scope");
        }
    }
    // The pinned relay emits a summary only while iterating the rows it is
    // returning (bridge.rs, step 3), so a summary naming anything else is a
    // relay contract violation. Buzz Desktop skips such a summary, but here it
    // is treated like an unmatched edit or reaction: the page is rejected as a
    // whole rather than partially trusted. NIP-CW: key by `d`, latest wins.
    let mut threads: BTreeMap<String, (&Event, ThreadSummary)> = BTreeMap::new();
    for (target, event, value) in summaries {
        if !originals.contains_key(&target) {
            return Err("history_invalid_scope");
        }
        match threads.get(&target) {
            Some((old, _)) if old.created_at == event.created_at => {
                // Two different same-second summaries give no latest one.
                return Err("history_invalid_summary");
            }
            Some((old, _)) if old.created_at > event.created_at => {}
            _ => {
                threads.insert(target, (event, value));
            }
        }
    }
    let mut deleted = BTreeSet::new();
    let mut uncertain = BTreeSet::new();
    for marker in deletions {
        for target in targets(marker)? {
            // One marker may reference targets outside this page. They do not
            // establish scope or a content claim, and are ignored here.
            if let Some(event) = index.get(&target) {
                if !matches!(event.kind.as_u16(), 9 | 40002 | 40003 | 7) {
                    continue;
                }
                if marker.pubkey == author(event, relay)? {
                    deleted.insert(target);
                } else {
                    uncertain.insert(target);
                }
            }
        }
    }
    let mut latest: BTreeMap<String, &Event> = BTreeMap::new();
    for edit in edits {
        let target = targets(edit)?.pop().unwrap();
        let original = originals.get(&target).ok_or("history_invalid_scope")?;
        if uncertain.contains(&edit.id.to_hex()) {
            uncertain.insert(target.clone());
        }
        if deleted.contains(&edit.id.to_hex()) || deleted.contains(&target) {
            continue;
        }
        if edit.created_at < original.created_at {
            return Err("history_invalid_shape");
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
    let mut observed: BTreeMap<String, (BTreeSet<PublicKey>, BTreeSet<PublicKey>)> =
        BTreeMap::new();
    for reaction in reactions {
        if deleted.contains(&reaction.id.to_hex()) || uncertain.contains(&reaction.id.to_hex()) {
            continue;
        }
        // Buzz's builder emits exactly one target. Ambiguous multi-target
        // reactions cannot establish a per-message acknowledgement.
        let targets = targets(reaction)?;
        if targets.len() != 1 {
            continue;
        }
        let counts = observed.entry(targets[0].clone()).or_default();
        match reaction.content.as_str() {
            "👀" => {
                counts.0.insert(author(reaction, relay)?);
            }
            "💬" => {
                counts.1.insert(author(reaction, relay)?);
            }
            _ => {}
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
        let reactions = if unavailable {
            None
        } else {
            let (seen, working) = observed.remove(&id).unwrap_or_default();
            Some(Reactions {
                seen: seen.len(),
                working: working.len(),
            })
        };
        // A relay-authored reply summary is kept even when the row's content
        // is unavailable: it does not depend on the unresolved edit/delete.
        let thread = threads.remove(&id).map(|(_, value)| value);
        rows.push(Row {
            reactions,
            thread,
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
    Ok(History {
        room: scope,
        category: "history_completeness_unknown",
        has_more: bounds.has_more,
        rows,
    })
}

pub async fn fetch(
    relay: &str,
    keys: &Keys,
    trusted_signer: PublicKey,
    room: Uuid,
) -> Result<History, &'static str> {
    let events = query(
        relay,
        keys,
        &QueryRequest::RoomHistory {
            room,
            limit: ROWS as u16,
        },
    )
    .await?;
    reduce(room, trusted_signer, &events, Timestamp::now().as_secs())
}
#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
