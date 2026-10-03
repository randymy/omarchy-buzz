//! Recent stream pages: the head page plus older pages the reader asks for.
//! No send API and no completeness claim.
use crate::query::{query, QueryRequest};
use nostr::{Event, EventId, Keys, PublicKey, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const EVENTS: usize = 200;
const BYTES: usize = 512 * 1024;
const ROWS: usize = 20;
/// Rows held for one room: the head page plus up to `OLDER_PAGES` older pages.
pub const HELD_ROWS: usize = 100;
pub const OLDER_PAGES: usize = 4;
/// Cumulative budget for the older pages held for one room. Each page keeps
/// its own `EVENTS`/`BYTES` budget; together they may not exceed these.
const OLDER_EVENTS: usize = 800;
const OLDER_BYTES: usize = 2 * 1024 * 1024;
const SUMMARY_REPLIES: u64 = 1_000_000;
const SUMMARY_PARTICIPANTS: usize = 10;
// 9999-12-31T23:59:59Z; the panel rejects later instants as unrepresentable.
const SUMMARY_TIME: u64 = 253_402_300_799;

/// Distinct emoji shown under one message, and the largest count shown.
pub const CHIPS: usize = 16;
pub const CHIP_COUNT: usize = 1000;
/// Observed live reactions in this bounded page, not authoritative agent state.
/// `seen`/`working` are the agent 👀/💬 acknowledgements; every other emoji is
/// a chip. The agent emoji are never chips: a person must not mimic agent work
/// states, and `sending` refuses them.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Reactions {
    pub seen: usize,
    pub working: usize,
    pub chips: Vec<Chip>,
}
/// One emoji's reactors on a message in this page.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Chip {
    pub emoji: String,
    pub count: usize,
    /// This identity reacted with it.
    pub mine: bool,
    /// This identity's reaction event, for removal. Never leaves the helper.
    #[serde(skip)]
    pub mine_id: Option<String>,
}
/// The agent acknowledgement emoji, shown as counts and never as chips.
pub fn agent_emoji(value: &str) -> bool {
    matches!(value, "👀" | "💬")
}
/// A reaction a person may show or send: a native glyph sequence, not agent state.
/// `:shortcode:` reactions need an emoji URL tag and are not supported.
pub fn chip_emoji(value: &str) -> bool {
    !agent_emoji(value) && !value.starts_with(':') && crate::user_status::valid_emoji(value)
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
    /// At most `attachments::MAX`, from the event supplying the content.
    pub attachments: Vec<crate::attachments::Attachment>,
    /// The content's `imeta` tags were malformed: none are shown.
    pub attachments_unavailable: bool,
    /// Tags the activity tracker classifies by; never sent to the panel.
    #[serde(skip)]
    pub signals: Signals,
}
/// Mentions and thread position of the original event (`activity.rs` only).
#[derive(Clone, Debug, Default)]
pub struct Signals {
    /// Lowercase hex keys of `p` tags, at most `MENTIONS`.
    pub mentions: Vec<String>,
    pub parent: Option<String>,
    pub root: Option<String>,
    pub broadcast: bool,
}
const MENTIONS: usize = 64;
/// Buzz Desktop's `getThreadReference`: the last `reply` marker is the parent,
/// the `root` marker (else the parent) is the root.
fn signals(event: &Event) -> Signals {
    let mut out = Signals::default();
    let mut root = None;
    for tag in event.tags.iter() {
        let t = tag.as_slice();
        match (t.first().map(String::as_str), t.get(1)) {
            (Some("p"), Some(key)) if out.mentions.len() < MENTIONS => {
                if let Ok(k) = PublicKey::from_hex(key) {
                    let k = k.to_hex();
                    if !out.mentions.contains(&k) {
                        out.mentions.push(k);
                    }
                }
            }
            (Some("e"), Some(id)) => match t.get(3).map(String::as_str) {
                Some("root") if root.is_none() => root = hex_id(id).ok(),
                Some("reply") => out.parent = hex_id(id).ok(),
                _ => {}
            },
            (Some("broadcast"), Some(v)) if v == "1" => out.broadcast = true,
            _ => {}
        }
    }
    out.root = out
        .parent
        .as_ref()
        .map(|p| root.unwrap_or_else(|| p.clone()));
    out
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
    /// The signed scan position to continue from; `Some` iff `has_more`.
    pub next_cursor: Option<Cursor>,
    /// Deletion markers on this page naming events that are not on it, as
    /// (target, signer). They may name rows held from other pages.
    pub outside_deletions: Vec<(String, PublicKey)>,
    /// Page cost, for cumulative budgets: events and serialized bytes.
    pub events: usize,
    pub bytes: usize,
}
/// A relay scan position from signed `kind:39006` bounds (NIP-CW), echoed
/// verbatim as `until` + `before_id`. Never derived from displayed rows.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    #[serde(rename(serialize = "createdAt"))]
    pub created_at: u64,
    pub id: String,
}
impl Cursor {
    /// True when `(at, id)` lies strictly past this position in the relay's
    /// `created_at DESC, id ASC` order, i.e. on a later (older) page. Lowercase
    /// hex compares like the bytes it encodes.
    pub fn precedes(&self, at: u64, id: &str) -> bool {
        at < self.created_at || at == self.created_at && id > self.id.as_str()
    }
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
#[cfg(test)]
pub fn reduce(
    room: Uuid,
    relay: PublicKey,
    events: &[Event],
    now: u64,
) -> Result<History, &'static str> {
    reduce_page(room, relay, events, now, None)
}
/// Media origin used by reducer tests (`attachments::origin` of `wss://relay.example`).
#[cfg(test)]
pub const TEST_ORIGIN: &str = "https://relay.example";
#[cfg(test)]
pub fn reduce_page(
    room: Uuid,
    relay: PublicKey,
    events: &[Event],
    now: u64,
    request: Option<&Cursor>,
) -> Result<History, &'static str> {
    reduce_at(room, relay, TEST_ORIGIN, events, now, request, None)
}
#[cfg(test)]
pub fn reduce_as(
    room: Uuid,
    relay: PublicKey,
    events: &[Event],
    now: u64,
    me: PublicKey,
) -> Result<History, &'static str> {
    reduce_at(room, relay, TEST_ORIGIN, events, now, None, Some(me))
}
/// `request` is the cursor this page was asked for (`None` for the head). The
/// bounds must echo it, and every row must lie strictly past it. `origin` is
/// the configured relay's media origin (`attachments::origin`).
/// `me` marks this identity's own reactions.
pub fn reduce_at(
    room: Uuid,
    relay: PublicKey,
    origin: &str,
    events: &[Event],
    now: u64,
    request: Option<&Cursor>,
    me: Option<PublicKey>,
) -> Result<History, &'static str> {
    if events.len() > EVENTS {
        return Err("history_oversized");
    }
    let bytes = serde_json::to_vec(events).map_err(|_| "history_invalid_shape")?;
    if bytes.len() > BYTES {
        return Err("history_oversized");
    }
    let scope = room.to_string();
    let binding = match request {
        None => format!("{scope}:head"),
        Some(cursor) => format!("{scope}:{}:{}", cursor.created_at, cursor.id),
    };
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
                    || one(event, "d")? != Some(binding.as_str())
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
    // Keyset integrity: a continued page lies strictly past its request cursor,
    // and every row is at or before the page's own scan position. This is what
    // lets held pages be joined without overlaps or silent gaps.
    for (id, event) in &originals {
        let at = event.created_at.as_secs();
        if request.is_some_and(|cursor| !cursor.precedes(at, id))
            || bounds
                .next_cursor
                .as_ref()
                .is_some_and(|next| next.precedes(at, id))
        {
            return Err("history_invalid_cursor");
        }
    }
    if let (Some(cursor), Some(next)) = (request, &bounds.next_cursor) {
        if !cursor.precedes(next.created_at, &next.id) {
            return Err("history_invalid_cursor");
        }
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
    let mut outside_deletions = Vec::new();
    for marker in deletions {
        for target in targets(marker)? {
            // One marker may reference targets outside this page. They do not
            // establish scope or a content claim here; the caller may apply
            // them to rows it already holds from other pages.
            let Some(event) = index.get(&target) else {
                outside_deletions.push((target, marker.pubkey));
                continue;
            };
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
    // Per target: agent 👀 and 💬 reactors, then each other emoji's reactors
    // and this identity's reaction event for it.
    type Emoji = BTreeMap<String, (BTreeSet<PublicKey>, Option<String>)>;
    let mut observed: BTreeMap<String, (BTreeSet<PublicKey>, BTreeSet<PublicKey>, Emoji)> =
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
            other if chip_emoji(other) => {
                let who = author(reaction, relay)?;
                let entry = counts.2.entry(other.to_owned()).or_default();
                entry.0.insert(who);
                if Some(who) == me {
                    entry.1 = Some(reaction.id.to_hex());
                }
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
        let (attachments, attachments_unavailable, (body, truncated)) = if unavailable {
            (Vec::new(), false, (String::new(), false))
        } else {
            let (list, broken, content) =
                crate::attachments::project(edit.copied().unwrap_or(original), origin);
            (list, broken, text(&content))
        };
        let reactions = if unavailable {
            None
        } else {
            let (seen, working, emoji) = observed.remove(&id).unwrap_or_default();
            let mut chips: Vec<Chip> = emoji
                .into_iter()
                .map(|(emoji, (who, mine_id))| Chip {
                    emoji,
                    count: who.len().min(CHIP_COUNT),
                    mine: me.is_some_and(|me| who.contains(&me)),
                    mine_id,
                })
                .collect();
            chips.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.emoji.cmp(&b.emoji)));
            chips.truncate(CHIPS);
            Some(Reactions {
                seen: seen.len(),
                working: working.len(),
                chips,
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
            attachments,
            attachments_unavailable,
            signals: signals(original),
        });
    }
    rows.sort_by(oldest_first);
    Ok(History {
        room: scope,
        category: "history_completeness_unknown",
        has_more: bounds.has_more,
        rows,
        next_cursor: bounds.next_cursor,
        outside_deletions,
        events: events.len(),
        bytes: bytes.len(),
    })
}
/// The exact reverse of the relay's `created_at DESC, id ASC` scan order, so an
/// older page always sorts before the page it continues.
fn oldest_first(a: &Row, b: &Row) -> std::cmp::Ordering {
    a.timestamp.cmp(&b.timestamp).then_with(|| b.id.cmp(&a.id))
}
/// Apply another page's deletion markers to held rows with the reducer's rule:
/// the proven author's marker hides the row; anyone else's makes it unavailable.
fn apply_deletions(rows: &mut Vec<Row>, deletions: &[(String, PublicKey)]) {
    for (target, signer) in deletions {
        let Some(index) = rows.iter().position(|row| &row.id == target) else {
            continue;
        };
        if rows[index].author_pubkey == signer.to_hex() {
            rows.remove(index);
        } else {
            let row = &mut rows[index];
            row.unavailable = true;
            row.text.clear();
            row.truncated = false;
            row.reactions = None;
            row.attachments.clear();
            row.attachments_unavailable = false;
        }
    }
}

/// One selected room's rows: the latest head page plus the older pages the
/// reader asked for. The observer owns one per (relay, identity, generation,
/// room, connection) and replaces it with `Held::default()` whenever the room's
/// history is cleared, re-selected, re-authenticated or revoked.
///
/// Reconciliation with the periodic head refresh: the new head replaces the old
/// one. Held rows the new head's scan range covers (at or before its
/// `next_cursor`) are dropped: they are either on the new head, or were
/// deleted or withheld since. Rows past that position stay, including rows an
/// earlier head showed that newer messages pushed off the head; those keep
/// their last verified projection and are not refreshed. Deletion markers on
/// any later page that name a held row are applied to it.
pub struct Held {
    head: Option<History>,
    /// Rows strictly past the head's scan position, oldest first.
    older: Vec<Row>,
    /// Older pages accepted; zero means only the head is held.
    pages: usize,
    /// Continuation after the oldest held page; `None` when exhausted or unheld.
    tail: Option<Cursor>,
    /// Older rows exist but are not held (trimmed to `HELD_ROWS`).
    trimmed: bool,
    events: usize,
    bytes: usize,
    /// `idle`, `loading` or `unavailable` for the latest older-page request.
    pub older_state: &'static str,
}
impl Default for Held {
    fn default() -> Self {
        Self {
            head: None,
            older: Vec::new(),
            pages: 0,
            tail: None,
            trimmed: false,
            events: 0,
            bytes: 0,
            older_state: "idle",
        }
    }
}
impl Held {
    fn clear_older(&mut self) {
        let head = self.head.take();
        *self = Self {
            head,
            ..Self::default()
        };
    }
    /// Accept a new head page for the held room.
    pub fn head(&mut self, page: History) {
        let previous = self.head.replace(page);
        let head = self.head.as_ref().expect("head just set");
        if previous.as_ref().is_some_and(|p| p.room != head.room) {
            self.clear_older();
            return;
        }
        if self.pages == 0 {
            return;
        }
        let Some(position) = head.next_cursor.clone() else {
            // The head reaches the start of the room: nothing older exists.
            self.clear_older();
            return;
        };
        if let Some(previous) = previous {
            self.older.extend(previous.rows);
        }
        self.older
            .retain(|row| position.precedes(row.timestamp, &row.id));
        let deletions = head.outside_deletions.clone();
        apply_deletions(&mut self.older, &deletions);
        if self.older.is_empty() {
            // The head covers everything held; continue from the head again.
            let state = self.older_state;
            self.clear_older();
            self.older_state = state;
            return;
        }
        self.older.sort_by(oldest_first);
        let room = HELD_ROWS.saturating_sub(self.head.as_ref().map_or(0, |h| h.rows.len()));
        if self.older.len() > room {
            // Drop the oldest rows; the cursor past them is gone with them.
            self.older.drain(..self.older.len() - room);
            self.tail = None;
            self.trimmed = true;
        }
    }
    /// Where the next older page would start, ignoring the row cap.
    fn next(&self) -> Option<&Cursor> {
        let head = self.head.as_ref()?;
        if self.pages == 0 {
            // `reduce` guarantees `has_more` exactly when a cursor is present.
            head.next_cursor.as_ref().filter(|_| head.has_more)
        } else {
            self.tail.as_ref()
        }
    }
    fn room_for_page(&self) -> bool {
        let shown = self.head.as_ref().map_or(0, |h| h.rows.len()) + self.older.len();
        !self.trimmed && self.pages < OLDER_PAGES && shown + ROWS <= HELD_ROWS
    }
    /// The cursor for the next older page, when one may be requested and held.
    pub fn continuation(&self) -> Option<&Cursor> {
        self.next().filter(|_| self.room_for_page())
    }
    /// Accept an older page fetched for `request`. Stale requests, over-budget
    /// pages and pages repeating a held row are refused whole.
    pub fn older(&mut self, request: &Cursor, page: History) -> Result<(), &'static str> {
        if self.continuation() != Some(request) {
            return Err("history_stale_cursor");
        }
        let head = self.head.as_mut().expect("continuation implies a head");
        if page.room != head.room {
            return Err("history_invalid_scope");
        }
        if self.events + page.events > OLDER_EVENTS || self.bytes + page.bytes > OLDER_BYTES {
            return Err("history_oversized");
        }
        // Keyset order makes an overlap impossible for a conforming relay. A
        // repeated id is refused rather than merged: which copy's auxiliary
        // closure is current cannot be told, and silently keeping either could
        // resurrect an edited or deleted body.
        let held: BTreeSet<&str> = head
            .rows
            .iter()
            .chain(&self.older)
            .map(|row| row.id.as_str())
            .collect();
        if page.rows.iter().any(|row| held.contains(row.id.as_str())) {
            return Err("history_duplicate_event");
        }
        apply_deletions(&mut head.rows, &page.outside_deletions);
        apply_deletions(&mut self.older, &page.outside_deletions);
        self.older.splice(0..0, page.rows);
        self.older.sort_by(oldest_first);
        self.pages += 1;
        self.tail = page.next_cursor;
        self.events += page.events;
        self.bytes += page.bytes;
        Ok(())
    }
    /// The panel view: held rows oldest first, `next_cursor` only while another
    /// page may be loaded, and `history_older_unheld` when older rows exist that
    /// will not be held.
    pub fn project(&self) -> Option<crate::protocol::History> {
        let head = self.head.as_ref()?;
        let more = self.trimmed || self.next().is_some();
        let unheld = more && self.continuation().is_none();
        let mut rows: Vec<crate::protocol::HistoryRow> = self
            .older
            .iter()
            .chain(&head.rows)
            .map(|r| crate::protocol::HistoryRow {
                reactions: r.reactions.clone(),
                thread: r.thread.clone(),
                id: r.id.clone(),
                author: r.author_pubkey.clone(),
                time: r.timestamp,
                text: r.text.clone(),
                edited: r.edited,
                truncated: r.truncated,
                unavailable: r.unavailable,
                attachments: r.attachments.clone(),
                attachments_unavailable: r.attachments_unavailable,
            })
            .collect();
        crate::attachments::bound(rows.iter_mut(), crate::attachments::FRAME_HISTORY);
        Some(crate::protocol::History {
            state: "snapshot".into(),
            room_id: Some(head.room.clone()),
            has_more: Some(more),
            category: Some(
                if unheld {
                    "history_older_unheld"
                } else {
                    head.category
                }
                .into(),
            ),
            next_cursor: self.continuation().cloned(),
            older_state: self.older_state.into(),
            live: false,
            rows,
        })
    }
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
            before: None,
        },
    )
    .await?;
    let origin = crate::attachments::origin(relay).map_err(|_| "history_invalid_shape")?;
    reduce_at(
        room,
        trusted_signer,
        &origin,
        &events,
        Timestamp::now().as_secs(),
        None,
        Some(keys.public_key()),
    )
}
/// Recent thread replies for the notification tracker only: the newest
/// `ROWS` messages of the room read without `top_level`, keeping the replies.
/// Signatures, kind, scope and time are checked like `reduce_at`; edits,
/// deletions and summaries are not read, so rows are never shown to the panel.
pub async fn fetch_replies(
    relay: &str,
    keys: &Keys,
    trusted_signer: PublicKey,
    room: Uuid,
) -> Result<Vec<Row>, &'static str> {
    let events = query(
        relay,
        keys,
        &QueryRequest::RoomRecent {
            room,
            limit: ROWS as u16,
        },
    )
    .await?;
    reply_rows(room, trusted_signer, &events, Timestamp::now().as_secs())
}
pub fn reply_rows(
    room: Uuid,
    relay: PublicKey,
    events: &[Event],
    now: u64,
) -> Result<Vec<Row>, &'static str> {
    if events.len() > ROWS {
        return Err("history_oversized");
    }
    let scope = room.to_string();
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for event in events {
        event.verify().map_err(|_| "history_invalid_signature")?;
        if event.created_at.as_secs() > now.saturating_add(60) {
            return Err("history_invalid_shape");
        }
        if !seen.insert(event.id) {
            return Err("history_duplicate_event");
        }
        if !matches!(event.kind.as_u16(), 9 | 40002) {
            return Err("history_invalid_kind");
        }
        if one(event, "h")? != Some(scope.as_str()) {
            return Err("history_invalid_scope");
        }
        let signals = signals(event);
        if signals.parent.is_none() {
            continue;
        }
        rows.push(Row {
            reactions: None,
            thread: None,
            id: event.id.to_hex(),
            author_pubkey: author(event, relay)?.to_hex(),
            timestamp: event.created_at.as_secs(),
            text: text(&event.content).0,
            edited: false,
            truncated: false,
            unavailable: false,
            attachments: Vec::new(),
            attachments_unavailable: false,
            signals,
        });
    }
    rows.sort_by(oldest_first);
    Ok(rows)
}
/// One older page continuing from `cursor`, verified exactly like the head
/// plus its request binding. Reads only, so a busy query slot is retried.
pub async fn fetch_older(
    relay: &str,
    keys: &Keys,
    trusted_signer: PublicKey,
    room: Uuid,
    cursor: &Cursor,
) -> Result<History, &'static str> {
    let id = EventId::from_hex(&cursor.id).map_err(|_| "history_invalid_cursor")?;
    if id.to_hex() != cursor.id {
        return Err("history_invalid_cursor");
    }
    let request = QueryRequest::RoomHistory {
        room,
        limit: ROWS as u16,
        before: Some((cursor.created_at, id)),
    };
    let mut attempts = 0;
    let events = loop {
        attempts += 1;
        match query(relay, keys, &request).await {
            Err("query_busy") if attempts < 3 => {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await
            }
            other => break other?,
        }
    };
    let origin = crate::attachments::origin(relay).map_err(|_| "history_invalid_shape")?;
    reduce_at(
        room,
        trusted_signer,
        &origin,
        &events,
        Timestamp::now().as_secs(),
        Some(cursor),
        Some(keys.public_key()),
    )
}
#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
