//! Desktop's legacy oldest-first thread read (NIP-CW "Legacy Oldest-first
//! Threads"): nested replies under the selected root, fetched in bounded pages.
//!
//! This path has no relay-signed bounds. Every event is still verified, scoped
//! and chained to the root; completeness and "more exist" remain heuristics.
use crate::query::{
    query, QueryRequest, THREAD_DEPTH, THREAD_PAGE_BYTES, THREAD_PAGE_EVENTS, THREAD_PAGE_ROWS,
};
use nostr::{Event, EventId, Keys, PublicKey, Timestamp};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use uuid::Uuid;

const PAGE: usize = THREAD_PAGE_ROWS as usize;
const PAGES: usize = 4;
/// Replies shown at most; a full last page means more may exist.
pub const ROWS: usize = PAGE * PAGES;
const EVENTS: usize = THREAD_PAGE_EVENTS;
const BYTES: usize = THREAD_PAGE_BYTES;
/// Whole-thread budget; root auxiliaries repeat on every page and count again.
const TOTAL_EVENTS: usize = 1000;
const TOTAL_BYTES: usize = 2 * 1024 * 1024;
const BUSY_ATTEMPTS: usize = 3;

#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub author_pubkey: String,
    pub timestamp: u64,
    pub text: String,
    pub edited: bool,
    pub truncated: bool,
    pub unavailable: bool,
    /// 1 for a direct reply to the root.
    pub depth: u8,
    /// The root for depth 1, else an earlier displayed row.
    pub parent: String,
    pub attachments: Vec<crate::attachments::Attachment>,
    pub attachments_unavailable: bool,
}

#[derive(Clone, Debug)]
pub struct Thread {
    pub room: String,
    pub root: String,
    pub category: &'static str,
    pub has_more: bool,
    pub rows: Vec<Row>,
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

type Cursor = (u64, EventId);

/// Accumulated, individually verified pages of one thread read.
#[derive(Default)]
struct Pages {
    /// Reply rows in relay order with their NIP-10 parent.
    rows: Vec<(Event, String)>,
    ids: BTreeSet<String>,
    aux: BTreeMap<String, Event>,
    events: usize,
    bytes: usize,
}

/// The NIP-10 parent of a reply under `root`, as the relay's ingest resolves it:
/// a direct reply names the root as `reply` (a `root` marker, if any, is the
/// root); a nested reply names its parent as `reply` and the root as `root`.
fn parent(event: &Event, root: &str) -> Result<String, &'static str> {
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
        let slot = match parts[3].as_str() {
            "reply" => &mut reply,
            "root" => &mut marked_root,
            _ => continue,
        };
        if slot.is_some() {
            return Err("thread_invalid_shape");
        }
        *slot = Some(id(&parts[1])?);
    }
    match (reply, marked_root) {
        (Some(reply), None) if reply == root => Ok(reply),
        (Some(reply), Some(marked)) if marked == root => Ok(reply),
        _ => Err("thread_invalid_scope"),
    }
}

impl Pages {
    /// Verify one page and append it, or reject it whole. Returns the
    /// continuation cursor when the page is full.
    fn add(
        &mut self,
        room: Uuid,
        root: EventId,
        after: Option<Cursor>,
        events: Vec<Event>,
        now: u64,
    ) -> Result<Option<Cursor>, &'static str> {
        let bytes = serde_json::to_vec(&events)
            .map_err(|_| "thread_invalid_shape")?
            .len();
        if events.len() > EVENTS
            || bytes > BYTES
            || self.events + events.len() > TOTAL_EVENTS
            || self.bytes + bytes > TOTAL_BYTES
        {
            return Err("thread_oversized");
        }
        let room_hex = room.to_string();
        let root_hex = root.to_hex();
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        let mut aux = Vec::new();
        let mut last = after.map(|(at, id)| (at, id.to_hex()));
        let count = events.len();
        for event in events {
            event.verify().map_err(|_| "thread_invalid_signature")?;
            if event.created_at.as_secs() > now.saturating_add(60) {
                return Err("thread_invalid_shape");
            }
            let event_id = event.id.to_hex();
            if !seen.insert(event_id.clone()) || self.ids.contains(&event_id) {
                return Err("thread_duplicate_event");
            }
            let kind = event.kind.as_u16();
            let h = one(&event, "h")?;
            if matches!(kind, 5 | 9005 | 7) {
                if h.is_some_and(|h| h != room_hex) {
                    return Err("thread_invalid_scope");
                }
            } else if h != Some(room_hex.as_str()) {
                return Err("thread_invalid_scope");
            }
            match kind {
                9 | 40002 => {
                    if event.id == root {
                        continue; // The root is displayed from the channel snapshot.
                    }
                    let parent = parent(&event, &root_hex)?;
                    // Relay order is (created_at, id) ascending, strictly after the cursor.
                    let key = (event.created_at.as_secs(), event_id);
                    if last.as_ref().is_some_and(|last| key <= *last) {
                        return Err("thread_invalid_order");
                    }
                    last = Some(key);
                    rows.push((event, parent));
                }
                40003 => {
                    if targets(&event)?.len() != 1 {
                        return Err("thread_invalid_shape");
                    }
                    aux.push(event);
                }
                5 | 9005 | 7 => {
                    targets(&event)?;
                    aux.push(event);
                }
                _ => return Err("thread_invalid_kind"),
            }
        }
        if rows.len() > PAGE {
            return Err("thread_oversized");
        }
        for event in &aux {
            if self.ids.contains(&event.id.to_hex()) {
                return Err("thread_duplicate_event");
            }
        }
        let next = if rows.len() == PAGE {
            rows.last()
                .map(|(event, _)| (event.created_at.as_secs(), event.id))
        } else {
            None
        };
        self.events += count;
        self.bytes += bytes;
        for (event, parent) in rows {
            self.ids.insert(event.id.to_hex());
            self.rows.push((event, parent));
        }
        for event in aux {
            // Root auxiliaries are returned again with every page.
            self.aux.entry(event.id.to_hex()).or_insert(event);
        }
        Ok(next)
    }

    /// Apply the auxiliary closure and lay out the tree. A reply whose parent is
    /// neither the root nor an earlier displayed reply is hidden, never re-parented.
    fn project(
        &self,
        relay: PublicKey,
        origin: &str,
        room: Uuid,
        root: EventId,
        capped: bool,
    ) -> Result<Thread, &'static str> {
        let root_hex = root.to_hex();
        let originals: BTreeMap<String, &Event> = self
            .rows
            .iter()
            .map(|(event, _)| (event.id.to_hex(), event))
            .collect();
        let mut edits = Vec::new();
        let mut deletes = Vec::new();
        for event in self.aux.values() {
            match event.kind.as_u16() {
                40003 => edits.push(event),
                5 | 9005 => deletes.push(event),
                _ => {
                    if !targets(event)?
                        .iter()
                        .any(|target| target == &root_hex || originals.contains_key(target))
                    {
                        return Err("thread_invalid_scope");
                    }
                }
            }
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
        edits.sort_by_key(|edit| (edit.created_at, edit.id));
        let mut latest: BTreeMap<String, &Event> = BTreeMap::new();
        for edit in edits {
            let target = targets(edit)?.pop().ok_or("thread_invalid_shape")?;
            let Some(original) = originals.get(&target) else {
                continue;
            }; // Root auxiliary.
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
        let mut depths: BTreeMap<String, u8> = BTreeMap::new();
        let mut hidden = false;
        let mut rows = Vec::new();
        for (original, parent) in &self.rows {
            let id = original.id.to_hex();
            if deleted.contains(&id) {
                continue;
            }
            let depth = if *parent == root_hex {
                1
            } else if let Some(depth) = depths.get(parent) {
                depth + 1
            } else {
                // Parent deleted, withheld, or not yet loaded (Desktop guesses a depth).
                hidden = true;
                continue;
            };
            if u16::from(depth) > THREAD_DEPTH {
                return Err("thread_invalid_shape");
            }
            depths.insert(id.clone(), depth);
            let unavailable = uncertain.contains(&id);
            let edit = latest.get(&id);
            let (attachments, attachments_unavailable, (body, truncated)) = if unavailable {
                (Vec::new(), false, (String::new(), false))
            } else {
                let (list, broken, content) =
                    crate::attachments::project(edit.copied().unwrap_or(original), origin);
                (list, broken, text(&content))
            };
            rows.push(Row {
                author_pubkey: author(original, relay)?.to_hex(),
                timestamp: original.created_at.as_secs(),
                text: body,
                edited: edit.is_some(),
                truncated,
                unavailable,
                depth,
                parent: parent.clone(),
                id,
                attachments,
                attachments_unavailable,
            });
        }
        Ok(Thread {
            room: room.to_string(),
            root: root_hex,
            category: if hidden {
                "thread_replies_hidden"
            } else if capped {
                "thread_more_unshown"
            } else {
                "thread_completeness_unknown"
            },
            has_more: capped,
            rows,
        })
    }
}

async fn page(
    relay_url: &str,
    keys: &Keys,
    request: &QueryRequest,
) -> Result<Vec<Event>, &'static str> {
    // Reads only: other views may briefly hold both query slots.
    for _ in 1..BUSY_ATTEMPTS {
        match query(relay_url, keys, request).await {
            Err("query_busy") => tokio::time::sleep(Duration::from_millis(250)).await,
            other => return other,
        }
    }
    query(relay_url, keys, request).await
}

/// Oldest-first pages of 50 until a short page or 200 replies. Any invalid
/// page fails the whole read; nothing partial is returned.
pub async fn fetch(
    relay_url: &str,
    keys: &Keys,
    signer: PublicKey,
    room: Uuid,
    root_hex: &str,
) -> Result<Thread, &'static str> {
    let root = EventId::from_hex(root_hex).map_err(|_| "thread_invalid_root")?;
    if root.to_hex() != root_hex {
        return Err("thread_invalid_root");
    }
    let origin = crate::attachments::origin(relay_url).map_err(|_| "thread_invalid_shape")?;
    let mut pages = Pages::default();
    let mut after = None;
    for _ in 0..PAGES {
        let request = QueryRequest::ThreadReplies { room, root, after };
        let events = page(relay_url, keys, &request).await?;
        match pages.add(room, root, after, events, Timestamp::now().as_secs())? {
            Some(next) => after = Some(next),
            None => return pages.project(signer, &origin, room, root, false),
        }
    }
    pages.project(signer, &origin, room, root, true)
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
