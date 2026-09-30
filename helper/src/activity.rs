//! Bounded, session-local observed activity. Not Buzz synchronized unread state.
use crate::history::Row;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub room_id: String,
    pub epoch: u64,
    pub observed: u32,
}
struct Room {
    ids: BTreeSet<String>,
    previous: BTreeSet<String>,
    floor: u64,
    epoch: u64,
    observed: u32,
}
#[derive(Default)]
pub struct Tracker {
    rooms: BTreeMap<String, Room>,
    epoch: u64,
}
impl Tracker {
    pub fn retain(&mut self, rooms: &[crate::protocol::Room]) {
        self.rooms.retain(|id, _| rooms.iter().any(|r| r.id == *id));
    }
    pub fn forget(&mut self, room: &str) {
        self.rooms.remove(room);
    }
    pub fn summaries(&self) -> Vec<Summary> {
        self.rooms
            .iter()
            .map(|(id, r)| Summary {
                room_id: id.clone(),
                epoch: r.epoch,
                observed: r.observed,
            })
            .collect()
    }
    pub fn observe(&mut self, room: &str, rows: &[Row], own: &str, now: u64) {
        if rows.len() > 20 || (!self.rooms.contains_key(room) && self.rooms.len() >= 20) {
            return;
        }
        let ids: BTreeSet<_> = rows.iter().map(|r| r.id.clone()).collect();
        let baseline = self.rooms.get(room).is_none_or(|r| {
            r.ids.len() + ids.len() > 512
                || (!r.previous.is_empty() && !ids.is_empty() && r.previous.is_disjoint(&ids))
        });
        if baseline {
            self.epoch = self.epoch.saturating_add(1);
            self.rooms.insert(
                room.into(),
                Room {
                    ids: ids.clone(),
                    previous: ids,
                    floor: now.saturating_sub(1),
                    epoch: self.epoch,
                    observed: 0,
                },
            );
            return;
        }
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
            }
        }
        r.previous = ids;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        }
    }
    #[test]
    fn baseline_dedup_own_edits_gaps_and_revocation() {
        let mut t = Tracker::default();
        t.observe("a", &[row("1")], "self", 100);
        assert_eq!(t.summaries()[0].observed, 0);
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
        t.forget("a");
        assert!(t.summaries().is_empty());
        t.observe("a", &[row("gap"), row("new")], "self", 100);
        assert_eq!(t.summaries()[0].observed, 0);
        t.retain(&[]);
        assert!(t.summaries().is_empty());
    }
    #[test]
    fn bounded_rooms_and_dedup_storage() {
        let mut t = Tracker::default();
        for i in 0..25 {
            t.observe(&i.to_string(), &[], "self", 100);
        }
        assert_eq!(t.summaries().len(), 20);
        for i in 0..2000 {
            t.observe("0", &[row("anchor"), row(&i.to_string())], "self", 100);
        }
        assert!(t.rooms["0"].ids.len() <= 512);
    }
}
