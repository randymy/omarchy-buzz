//! Bounded, session-local observed activity. Not Buzz synchronized unread state.
//! Each room also carries one coalesced notice for desktop notifications,
//! classified like Buzz Desktop's `shouldNotify` (mention, DM, thread, room).
use crate::history::Row;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
/// Rooms tracked: every room the catalog can hold (`catalog::MAX_ROOMS`), each
/// with bounded state. A room that leaves the catalog is evicted (`retain`,
/// `forget`).
pub const ROOMS: usize = crate::catalog::MAX_ROOMS;
/// A catalog of up to this many rooms is polled one room per cycle, as ever.
const SMALL: usize = 20;
/// Most rooms one poll cycle reads (a head read and a replies read each) once
/// the catalog is larger than `SMALL`: the relay load stays within a few
/// queries per cycle however many rooms are joined.
pub const BUDGET: usize = 4;
/// How many rooms a poll cycle reads for a catalog of `total` rooms: one up to
/// `SMALL` rooms (the load before paging), then `ceil(total / SMALL)` up to `BUDGET`.
pub fn per_cycle(total: usize) -> usize {
    if total <= SMALL {
        usize::from(total > 0)
    } else {
        total.div_ceil(SMALL).min(BUDGET)
    }
}
/// The next `per_cycle` rooms in rotation from `cursor` (advanced past them):
/// every room is reached within `ceil(total / per_cycle)` cycles.
pub fn next_rooms(rooms: &[String], cursor: &mut usize) -> Vec<String> {
    let count = per_cycle(rooms.len());
    let picked: Vec<String> = (0..count)
        .map(|i| rooms[(*cursor + i) % rooms.len()].clone())
        .collect();
    *cursor = cursor.wrapping_add(count);
    picked
}
/// At most one notice per room per window; later rows wait for the next one.
const WINDOW: u64 = 10;
const SNIPPET: usize = 100;
const NAMES: usize = 256;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub room_id: String,
    pub epoch: u64,
    pub observed: u32,
    /// The latest notice; a higher `seq` than before means a new one.
    pub notice: Option<Notice>,
}
/// A bounded, sanitized description of messages observed since the last notice.
/// Observations, not synced unread counts: `count` is what this session saw.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub seq: u64,
    /// `mention`, `dm`, `thread` or `room`: the strongest in the burst.
    pub kind: &'static str,
    /// Observed messages coalesced into this notice, at most 999.
    pub count: u32,
    pub room_name: String,
    /// Self-asserted profile name; empty when unknown, never a raw key.
    pub sender: String,
    /// One plain line of at most `SNIPPET` characters; empty without text.
    pub snippet: String,
    pub event_id: String,
    /// The thread the message belongs to, when it is a reply.
    pub thread_root: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Room,
    Thread,
    Mention,
    Dm,
}
impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Room => "room",
            Kind::Thread => "thread",
            Kind::Mention => "mention",
            Kind::Dm => "dm",
        }
    }
}
/// What the catalog says about a room, for classifying and titling.
struct Meta {
    name: String,
    dm: bool,
    participants: Vec<String>,
}
struct Pending {
    kind: Kind,
    count: u32,
    row: Row,
}
struct Room {
    ids: BTreeSet<String>,
    previous: BTreeSet<String>,
    /// Messages of mine and the thread roots I replied under.
    mine: BTreeSet<String>,
    floor: u64,
    epoch: u64,
    observed: u32,
    seq: u64,
    notice: Option<Notice>,
    pending: Option<Pending>,
    emitted_at: u64,
}
#[derive(Default)]
pub struct Tracker {
    rooms: BTreeMap<String, Room>,
    meta: BTreeMap<String, Meta>,
    names: BTreeMap<String, String>,
    epoch: u64,
}
fn rank(kind: Kind, row: &Row) -> (Kind, u64, &str) {
    (kind, row.timestamp, row.id.as_str())
}
impl Tracker {
    pub fn retain(&mut self, rooms: &[crate::protocol::Room]) {
        self.rooms.retain(|id, _| rooms.iter().any(|r| r.id == *id));
        self.meta = rooms
            .iter()
            .map(|r| {
                let meta = Meta {
                    name: crate::recipients::sanitize(&r.name, 128),
                    dm: r.kind == "dm",
                    participants: r.participants.clone(),
                };
                (r.id.clone(), meta)
            })
            .collect();
    }
    pub fn forget(&mut self, room: &str) {
        self.rooms.remove(room);
    }
    /// Profile names from a verified roster; hints for titles only.
    pub fn note_names(&mut self, entries: &[crate::recipients::Recipient]) {
        for e in entries.iter().filter(|e| !e.name.trim().is_empty()) {
            if self.names.len() >= NAMES && !self.names.contains_key(&e.key) {
                self.names.clear();
            }
            self.names.insert(e.key.clone(), e.name.clone());
        }
    }
    pub fn summaries(&self) -> Vec<Summary> {
        self.rooms
            .iter()
            .map(|(id, r)| Summary {
                room_id: id.clone(),
                epoch: r.epoch,
                observed: r.observed,
                notice: r.notice.clone(),
            })
            .collect()
    }
    fn sender(&self, room: &str, key: &str) -> String {
        if let Some(name) = self.names.get(key) {
            return crate::recipients::sanitize(name, 64);
        }
        // A one-to-one DM is titled by the other person's profile name.
        match self.meta.get(room) {
            Some(m)
                if m.dm && m.participants.len() == 2 && m.participants.iter().any(|p| p == key) =>
            {
                let name = crate::recipients::sanitize(&m.name, 64);
                if name.ends_with('…') {
                    String::new()
                } else {
                    name
                }
            }
            _ => String::new(),
        }
    }
    fn classify(
        &self,
        room: &str,
        row: &Row,
        own: &str,
        rows: &[Row],
        mine: &BTreeSet<String>,
    ) -> Kind {
        if self.meta.get(room).is_some_and(|m| m.dm) {
            return Kind::Dm;
        }
        if row.signals.mentions.iter().any(|k| k == own) {
            return Kind::Mention;
        }
        let in_thread = row.signals.root.as_ref().is_some_and(|root| {
            mine.contains(root)
                || rows.iter().any(|r| {
                    r.id == *root
                        && (r.author_pubkey == own
                            || r.thread
                                .as_ref()
                                .is_some_and(|t| t.participants.iter().any(|p| p == own)))
                })
        });
        // Desktop: a reply outside my threads is quiet unless it is broadcast.
        if row.signals.broadcast || in_thread {
            Kind::Thread
        } else {
            Kind::Room
        }
    }
    pub fn observe(&mut self, room: &str, rows: &[Row], own: &str, now: u64) {
        if rows.len() > 20 || (!self.rooms.contains_key(room) && self.rooms.len() >= ROOMS) {
            return;
        }
        let ids: BTreeSet<_> = rows.iter().map(|r| r.id.clone()).collect();
        let baseline = self.rooms.get(room).is_none_or(|r| {
            r.ids.len() + ids.len() > 512
                || (!r.previous.is_empty() && !ids.is_empty() && r.previous.is_disjoint(&ids))
        });
        let mut mine = if baseline {
            BTreeSet::new()
        } else {
            self.rooms[room].mine.clone()
        };
        for row in rows {
            if row.author_pubkey == own
                || row
                    .thread
                    .as_ref()
                    .is_some_and(|t| t.participants.iter().any(|p| p == own))
            {
                mine.insert(row.id.clone());
            }
            if row.author_pubkey == own {
                mine.extend(row.signals.root.clone());
            }
        }
        if mine.len() > 512 {
            mine.clear();
        }
        if baseline {
            self.epoch = self.epoch.saturating_add(1);
            self.rooms.insert(
                room.into(),
                Room {
                    ids: ids.clone(),
                    previous: ids,
                    mine,
                    floor: now.saturating_sub(1),
                    epoch: self.epoch,
                    observed: 0,
                    seq: 0,
                    notice: None,
                    pending: None,
                    emitted_at: 0,
                },
            );
            return;
        }
        let mut fresh: Vec<&Row> = Vec::new();
        {
            let r = self.rooms.get_mut(room).unwrap();
            for row in rows {
                if r.ids.insert(row.id.clone())
                    && row.author_pubkey != own
                    && !row.edited
                    && !row.unavailable
                    && row.timestamp >= r.floor
                    && row.timestamp <= now.saturating_add(60)
                {
                    r.observed = r.observed.saturating_add(1).min(1_000_000_000);
                    fresh.push(row);
                }
            }
            r.previous = ids;
        }
        let fresh: Vec<Row> = fresh.into_iter().cloned().collect();
        self.queue(room, &fresh, rows, own, now, mine);
    }
    /// Thread replies from a read without `top_level` (`history::fetch_replies`),
    /// which the head page never holds. Only for a room already baselined;
    /// replies older than that baseline or already seen are not new.
    pub fn observe_replies(&mut self, room: &str, replies: &[Row], own: &str, now: u64) {
        let Some(r) = self.rooms.get_mut(room) else {
            return;
        };
        if replies.len() > 20 || r.ids.len() + replies.len() > 512 {
            return;
        }
        let mut mine = std::mem::take(&mut r.mine);
        for row in replies.iter().filter(|r| r.author_pubkey == own) {
            mine.insert(row.id.clone());
            mine.extend(row.signals.root.clone());
        }
        let mut fresh = Vec::new();
        for row in replies {
            if r.ids.insert(row.id.clone())
                && row.author_pubkey != own
                && row.timestamp >= r.floor
                && row.timestamp <= now.saturating_add(60)
            {
                r.observed = r.observed.saturating_add(1).min(1_000_000_000);
                fresh.push(row.clone());
            }
        }
        self.queue(room, &fresh, replies, own, now, mine);
    }
    fn queue(
        &mut self,
        room: &str,
        fresh: &[Row],
        rows: &[Row],
        own: &str,
        now: u64,
        mine: BTreeSet<String>,
    ) {
        // The strongest kind in the burst, then its latest message, speaks for it.
        let mut best: Option<(Kind, &Row)> = None;
        for row in fresh {
            let kind = self.classify(room, row, own, rows, &mine);
            if best.is_none_or(|(k, b)| rank(kind, row) > rank(k, b)) {
                best = Some((kind, row));
            }
        }
        let due;
        let taken;
        {
            let r = self.rooms.get_mut(room).unwrap();
            r.mine = if mine.len() > 512 {
                BTreeSet::new()
            } else {
                mine
            };
            if let Some((kind, row)) = best {
                let count = u32::try_from(fresh.len()).unwrap_or(999);
                r.pending = Some(match r.pending.take() {
                    Some(p) => {
                        let total = p.count.saturating_add(count).min(999);
                        if rank(p.kind, &p.row) > rank(kind, row) {
                            Pending { count: total, ..p }
                        } else {
                            Pending {
                                kind,
                                count: total,
                                row: row.clone(),
                            }
                        }
                    }
                    None => Pending {
                        kind,
                        count: count.min(999),
                        row: row.clone(),
                    },
                });
            }
            due = r.emitted_at == 0 || now >= r.emitted_at.saturating_add(WINDOW);
            taken = if due { r.pending.take() } else { None };
        }
        if let Some(p) = taken {
            let sender = self.sender(room, &p.row.author_pubkey);
            let room_name = self
                .meta
                .get(room)
                .map(|m| m.name.clone())
                .unwrap_or_default();
            let r = self.rooms.get_mut(room).unwrap();
            r.seq = r.seq.saturating_add(1);
            r.emitted_at = now.max(1);
            r.notice = Some(Notice {
                seq: r.seq,
                kind: p.kind.label(),
                count: p.count,
                room_name,
                sender,
                snippet: snippet(&p.row.text),
                event_id: p.row.id.clone(),
                thread_root: p.row.signals.root.clone(),
            });
        }
    }
}
/// One plain line: controls and bidi marks already blanked, runs of spaces
/// collapsed, at most `SNIPPET` characters with an ellipsis when cut.
fn snippet(text: &str) -> String {
    let clean = crate::recipients::sanitize(text, 512);
    let line = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= SNIPPET {
        return line;
    }
    let cut: String = line.chars().take(SNIPPET - 1).collect();
    format!("{}…", cut.trim_end())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Signals;
    fn row(id: &str) -> Row {
        Row {
            reactions: None,
            thread: None,
            id: id.into(),
            author_pubkey: "other".into(),
            timestamp: 100,
            text: "not retained".into(),
            edited: false,
            truncated: false,
            unavailable: false,
            attachments: vec![],
            attachments_unavailable: false,
            signals: Signals::default(),
        }
    }
    fn room(id: &str, kind: &str, name: &str, participants: &[&str]) -> crate::protocol::Room {
        crate::protocol::Room {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            kind: kind.into(),
            participants: participants.iter().map(|p| p.to_string()).collect(),
            hidden: false,
        }
    }
    /// A tracker with a baseline for "a" and the given catalog.
    fn ready(rooms: &[crate::protocol::Room]) -> Tracker {
        let mut t = Tracker::default();
        t.retain(rooms);
        t.observe("a", &[row("0")], "self", 100);
        t
    }
    fn notice(t: &Tracker) -> Notice {
        t.summaries()[0].notice.clone().expect("notice")
    }
    #[test]
    fn baseline_dedup_own_edits_gaps_and_revocation() {
        let mut t = Tracker::default();
        t.observe("a", &[row("1")], "self", 100);
        assert_eq!(t.summaries()[0].observed, 0);
        assert!(t.summaries()[0].notice.is_none());
        t.observe("a", &[row("1"), row("2")], "self", 100);
        assert_eq!(t.summaries()[0].observed, 1);
        let mut own = row("3");
        own.author_pubkey = "self".into();
        let mut edit = row("4");
        edit.edited = true;
        t.observe("a", &[row("2"), own, edit], "self", 100);
        assert_eq!(t.summaries()[0].observed, 1);
        let epoch = t.summaries()[0].epoch;
        t.observe("a", &[row("gap")], "self", 100);
        assert!(t.summaries()[0].epoch > epoch);
        assert_eq!(t.summaries()[0].observed, 0);
        assert!(t.summaries()[0].notice.is_none());
        t.forget("a");
        assert!(t.summaries().is_empty());
        t.observe("a", &[row("gap"), row("new")], "self", 100);
        assert_eq!(t.summaries()[0].observed, 0);
        t.retain(&[]);
        assert!(t.summaries().is_empty());
    }
    /// More than 20 joined rooms: the tracker takes all of them (it used to
    /// refuse every room past the twentieth, so those never had indicators).
    #[test]
    fn more_than_twenty_rooms_are_all_tracked() {
        let mut t = Tracker::default();
        for i in 0..50 {
            t.observe(&format!("room-{i}"), &[row("a")], "self", 100);
        }
        assert_eq!(t.summaries().len(), 50);
        assert!(t.summaries().iter().all(|s| s.observed == 0));
    }
    #[test]
    fn a_cycle_reads_a_bounded_number_of_rooms_and_rotation_reaches_all() {
        // Small catalogs are polled one room per cycle, as before paging.
        assert_eq!((per_cycle(0), per_cycle(1), per_cycle(20)), (0, 1, 1));
        // Larger ones: ceil(n / 20) rooms, never more than BUDGET.
        assert_eq!((per_cycle(21), per_cycle(50), per_cycle(61)), (2, 3, 4));
        assert_eq!((per_cycle(100), per_cycle(200)), (4, BUDGET));
        for total in [1_usize, 7, 20, 21, 50, 99, 150, 200] {
            let rooms: Vec<String> = (0..total).map(|i| format!("r{i}")).collect();
            let mut cursor = 0;
            let mut seen = BTreeSet::new();
            let cycles = total.div_ceil(per_cycle(total));
            for _ in 0..cycles {
                let batch = next_rooms(&rooms, &mut cursor);
                assert!(
                    batch.len() <= BUDGET,
                    "{total}: {} rooms in one cycle",
                    batch.len()
                );
                assert_eq!(batch.len(), batch.iter().collect::<BTreeSet<_>>().len());
                seen.extend(batch);
            }
            assert_eq!(
                seen.len(),
                total,
                "{total} rooms not all reached in {cycles} cycles"
            );
        }
    }
    #[test]
    fn bounded_rooms_and_dedup_storage() {
        let mut t = Tracker::default();
        for i in 0..ROOMS + 5 {
            t.observe(&i.to_string(), &[], "self", 100);
        }
        assert_eq!(t.summaries().len(), ROOMS);
        for i in 0..2000 {
            t.observe("0", &[row("anchor"), row(&i.to_string())], "self", 100);
        }
        assert!(t.rooms["0"].ids.len() <= 512);
        assert!(t.rooms["0"].mine.len() <= 512);
    }
    #[test]
    fn plain_room_message_is_a_room_notice() {
        let mut t = ready(&[room("a", "stream", "general", &[])]);
        t.names.insert("other".into(), "Alex".into());
        let mut m = row("1");
        m.text = "hello there".into();
        t.observe("a", &[row("0"), m], "self", 100);
        let n = notice(&t);
        assert_eq!((n.kind, n.count, n.seq), ("room", 1, 1));
        assert_eq!(n.room_name, "general");
        assert_eq!(n.sender, "Alex");
        assert_eq!(n.snippet, "hello there");
        assert_eq!(n.event_id, "1");
        assert_eq!(n.thread_root, None);
    }
    #[test]
    fn mention_of_me_outranks_room_messages() {
        let mut t = ready(&[room("a", "stream", "general", &[])]);
        let mut mention = row("1");
        mention.signals.mentions = vec!["self".into()];
        mention.timestamp = 99;
        let mut later = row("2");
        later.timestamp = 100;
        let mut other = row("3");
        other.signals.mentions = vec!["someone".into()];
        t.observe("a", &[row("0"), mention, later, other], "self", 100);
        let n = notice(&t);
        assert_eq!((n.kind, n.count, n.event_id.as_str()), ("mention", 3, "1"));
    }
    #[test]
    fn dm_rooms_notify_as_dm_and_name_the_other_person() {
        let mut t = ready(&[room("a", "dm", "Sam", &["self", "other"])]);
        t.observe("a", &[row("0"), row("1")], "self", 100);
        let n = notice(&t);
        assert_eq!((n.kind, n.sender.as_str()), ("dm", "Sam"));
        // A key-prefix fallback name is not a person's name.
        let mut t = ready(&[room("a", "dm", "abcdef012345…", &["self", "other"])]);
        t.observe("a", &[row("0"), row("1")], "self", 100);
        assert_eq!(notice(&t).sender, "");
    }
    #[test]
    fn thread_replies_notify_only_in_my_threads() {
        let root = "r".repeat(64);
        let mut mine_root = row(&root);
        mine_root.author_pubkey = "self".into();
        mine_root.timestamp = 90;
        let reply = |id: &str, root: &str, broadcast: bool| {
            let mut r = row(id);
            r.signals.parent = Some(root.into());
            r.signals.root = Some(root.into());
            r.signals.broadcast = broadcast;
            r
        };
        // The root is mine and on the page.
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        t.observe(
            "a",
            &[row("0"), mine_root.clone(), reply("1", &root, false)],
            "self",
            100,
        );
        let n = notice(&t);
        assert_eq!(
            (n.kind, n.thread_root.as_deref()),
            ("thread", Some(root.as_str()))
        );
        // A reply under someone else's root is quiet room activity...
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        t.observe("a", &[row("0"), reply("1", "x", false)], "self", 100);
        assert_eq!(notice(&t).kind, "room");
        // ...unless it is broadcast, or I took part (summary or earlier reply).
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        t.observe("a", &[row("0"), reply("1", "x", true)], "self", 100);
        assert_eq!(notice(&t).kind, "thread");
        let mut theirs = row("x");
        theirs.thread = Some(crate::history::ThreadSummary {
            replies: 2,
            last_reply_at: None,
            participants: vec!["self".into()],
        });
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        t.observe(
            "a",
            &[row("0"), theirs, reply("1", "x", false)],
            "self",
            100,
        );
        assert_eq!(notice(&t).kind, "thread");
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        let mut mine_reply = reply("1", "y", false);
        mine_reply.author_pubkey = "self".into();
        t.observe("a", &[row("0"), mine_reply], "self", 100);
        t.observe("a", &[row("1"), reply("2", "y", false)], "self", 100);
        assert_eq!(notice(&t).kind, "thread");
    }
    #[test]
    fn bursts_coalesce_per_window_and_never_overcount() {
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        let page = |n: usize| -> Vec<Row> { (0..n).map(|i| row(&i.to_string())).collect() };
        t.observe("a", &page(4), "self", 100);
        assert_eq!((notice(&t).count, notice(&t).seq), (3, 1));
        // Inside the window the new rows are held, not announced.
        t.observe("a", &page(6), "self", 103);
        assert_eq!(notice(&t).seq, 1);
        assert_eq!(t.summaries()[0].observed, 5);
        t.observe("a", &page(7), "self", 110);
        let n = notice(&t);
        assert_eq!((n.seq, n.count), (2, 3));
        // A quiet poll does not repeat the notice.
        t.observe("a", &page(7), "self", 130);
        assert_eq!(notice(&t).seq, 2);
    }
    #[test]
    fn sanitizes_names_and_truncates_snippets() {
        let mut t = ready(&[room("a", "stream", "ge\u{202e}neral", &[])]);
        t.names.insert("other".into(), "Al\u{202e}ex\n".into());
        let mut m = row("1");
        m.text = format!("<img src=x>  line\u{202e}  one {}", "é".repeat(300));
        t.observe("a", &[row("0"), m], "self", 100);
        let n = notice(&t);
        assert_eq!(n.sender, "Al ex");
        assert_eq!(n.room_name, "ge neral");
        assert_eq!(n.snippet.chars().count(), SNIPPET);
        assert!(n.snippet.ends_with('…'));
        assert!(n.snippet.starts_with("<img src=x> line one"));
        assert!(!n.snippet.contains('\u{202e}'));
        assert_eq!(snippet("short"), "short");
        assert_eq!(snippet(&"x".repeat(SNIPPET)).chars().count(), SNIPPET);
    }
    #[test]
    fn replies_need_a_baseline_and_are_counted_once() {
        let reply = |id: &str, at: u64| {
            let mut r = row(id);
            r.timestamp = at;
            r.signals.parent = Some("x".into());
            r.signals.root = Some("x".into());
            r.signals.mentions = vec!["self".into()];
            r
        };
        let mut t = Tracker::default();
        t.observe_replies("a", &[reply("1", 100)], "self", 100);
        assert!(t.summaries().is_empty(), "no baseline, no observation");
        let mut t = ready(&[room("a", "stream", "g", &[])]);
        // Older than the baseline floor (99) is history, not news.
        t.observe_replies("a", &[reply("old", 50), reply("1", 100)], "self", 100);
        let n = notice(&t);
        assert_eq!((n.kind, n.count, n.event_id.as_str()), ("mention", 1, "1"));
        t.observe_replies("a", &[reply("1", 100)], "self", 130);
        assert_eq!((notice(&t).seq, t.summaries()[0].observed), (1, 1));
    }
    #[test]
    fn bounded_name_cache() {
        let mut t = Tracker::default();
        let entries: Vec<_> = (0..300)
            .map(|i| crate::recipients::Recipient {
                key: format!("{i}"),
                name: "n".into(),
                status: None,
            })
            .collect();
        t.note_names(&entries);
        assert!(t.names.len() <= NAMES);
    }
}
