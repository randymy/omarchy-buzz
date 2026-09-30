//! One live subscription for the selected room, used only as a refetch trigger.
//!
//! Nothing from a live frame is ever shown: a verified event only schedules a
//! head-page (and, for an open thread, a thread) refetch through the verified
//! HTTP page paths in `history` and `thread`. Missing an event therefore only
//! delays an update to the next poll. See `helper/WS_UPSTREAM.md` for why the
//! subscription is closed during every re-authentication.
use nostr::{Event, PublicKey};
use serde_json::{json, Value};
use std::collections::VecDeque;
use tokio::time::{Duration, Instant};

pub const PREFIX: &str = "omarchy-buzz-live-";
/// Messages, edits, deletions, reactions and relay thread summaries, as in
/// Desktop's live filter minus kinds the helper never renders.
pub const KINDS: [u16; 7] = [9, 40002, 40003, 5, 9005, 7, 39005];
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
/// At most one triggered refetch per this interval; bursts coalesce.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
/// Spacing of triggered refetches while more than `BUSY_EVENTS` verified events
/// arrived within `BUSY_WINDOW`.
pub const BUSY_GAP: Duration = Duration::from_secs(2);
const BUSY_EVENTS: usize = 20;
const BUSY_WINDOW: Duration = Duration::from_secs(10);
/// More than `FLOOD_FRAMES` unverifiable frames within `FLOOD_WINDOW` close the
/// subscription for `FLOOD_PAUSE`; polling continues meanwhile.
const FLOOD_FRAMES: usize = 200;
const FLOOD_WINDOW: Duration = Duration::from_secs(60);
pub const FLOOD_PAUSE: Duration = Duration::from_secs(300);
/// Re-arm delays after successive relay `CLOSED` answers for the live subscription.
const CLOSED_BACKOFF: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(300),
];

struct Active {
    id: String,
    room: String,
    primed: bool,
}

#[derive(Default)]
pub struct Live {
    active: Option<Active>,
    /// Last room a subscription was requested for; backoff is per room.
    room: Option<String>,
    closed_failures: usize,
    closed_until: Option<Instant>,
    flood_until: Option<Instant>,
    verified: VecDeque<Instant>,
    rejected: VecDeque<Instant>,
}

pub enum Frame {
    /// Not a frame of the current live subscription.
    Other,
    /// Ours but not a valid trigger; counted, never inspected further.
    Ignored,
    /// A verified event: refetch the head page, and the open thread when `thread`.
    Trigger { thread: bool },
    /// Too many unverifiable frames: send this CLOSE and fall back to polling.
    Flood(Value),
}

fn record(window: &mut VecDeque<Instant>, now: Instant, span: Duration, cap: usize) -> usize {
    while window
        .front()
        .is_some_and(|at| now.saturating_duration_since(*at) > span)
    {
        window.pop_front();
    }
    window.push_back(now);
    while window.len() > cap + 1 {
        window.pop_front();
    }
    window.len()
}

impl Live {
    pub fn is(&self, id: &str) -> bool {
        self.active.as_ref().is_some_and(|a| a.id == id)
    }
    pub fn room(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.room.as_str())
    }
    pub fn primed_room(&self) -> Option<&str> {
        self.active
            .as_ref()
            .filter(|a| a.primed)
            .map(|a| a.room.as_str())
    }
    pub fn primed_for(&self, room: Option<&str>) -> bool {
        room.is_some() && self.primed_room() == room
    }
    /// A room was (re)selected: a different room starts without relay backoff.
    /// A flood pause is about the relay, so it survives room changes.
    pub fn select(&mut self, room: &str) {
        if self.room.as_deref() != Some(room) {
            self.room = Some(room.to_owned());
            self.closed_failures = 0;
            self.closed_until = None;
        }
    }
    pub fn can_arm(&self, now: Instant) -> bool {
        self.active.is_none()
            && self.closed_until.is_none_or(|t| now >= t)
            && self.flood_until.is_none_or(|t| now >= t)
    }
    /// The REQ frame for a new subscription starting now; the caller sends it.
    pub fn arm(&mut self, room: &str, since: u64) -> Value {
        let id = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        self.select(room);
        self.verified.clear();
        self.rejected.clear();
        let frame = json!(["REQ", id, {"kinds": KINDS, "#h": [room], "since": since}]);
        self.active = Some(Active {
            id,
            room: room.to_owned(),
            primed: false,
        });
        frame
    }
    /// Take the subscription down; returns the CLOSE frame when one was open.
    pub fn close(&mut self) -> Option<Value> {
        self.active.take().map(|a| json!(["CLOSE", a.id]))
    }
    /// End of stored events for the live subscription: it is now primed.
    pub fn eose(&mut self, id: &str) -> bool {
        match self.active.as_mut() {
            Some(a) if a.id == id && !a.primed => {
                a.primed = true;
                self.closed_failures = 0;
                true
            }
            _ => false,
        }
    }
    /// The relay closed the live subscription: fall back to polling and re-arm
    /// after 5 s, 30 s, then 5 min.
    pub fn closed_by_relay(&mut self, now: Instant) {
        self.active = None;
        let delay = CLOSED_BACKOFF[self.closed_failures.min(CLOSED_BACKOFF.len() - 1)];
        self.closed_failures = self.closed_failures.saturating_add(1);
        self.closed_until = Some(now + delay);
    }
    /// Minimum spacing between triggered refetches right now.
    pub fn gap(&mut self, now: Instant) -> Duration {
        while self
            .verified
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > BUSY_WINDOW)
        {
            self.verified.pop_front();
        }
        if self.verified.len() > BUSY_EVENTS {
            BUSY_GAP
        } else {
            DEBOUNCE
        }
    }
    /// Classify an EVENT frame. `held` answers whether an event id is a row the
    /// helper currently shows for the room.
    pub fn event(
        &mut self,
        id: &str,
        event: &Event,
        relay: Option<PublicKey>,
        held: impl Fn(&str) -> bool,
        now: Instant,
    ) -> Frame {
        let Some(room) = self
            .active
            .as_ref()
            .filter(|a| a.id == id)
            .map(|a| a.room.clone())
        else {
            return Frame::Other;
        };
        if trigger(event, &room, relay, held) {
            record(&mut self.verified, now, BUSY_WINDOW, BUSY_EVENTS);
            let thread = event
                .tags
                .iter()
                .any(|t| t.as_slice().first().is_some_and(|k| k == "e"));
            return Frame::Trigger { thread };
        }
        if record(&mut self.rejected, now, FLOOD_WINDOW, FLOOD_FRAMES) > FLOOD_FRAMES {
            self.flood_until = Some(now + FLOOD_PAUSE);
            self.rejected.clear();
            if let Some(close) = self.close() {
                return Frame::Flood(close);
            }
        }
        Frame::Ignored
    }
}

/// Minimal checks that make an event a trigger. Cheap checks run before the
/// signature. Deletions and reactions may lack `h` (the relay matches them by
/// stored channel); such an event triggers only when an `e` tag names a row
/// the helper holds for this room.
fn trigger(
    event: &Event,
    room: &str,
    relay: Option<PublicKey>,
    held: impl Fn(&str) -> bool,
) -> bool {
    let kind = event.kind.as_u16();
    if !KINDS.contains(&kind) {
        return false;
    }
    if kind == 39005 && Some(event.pubkey) != relay {
        return false;
    }
    fn tag<'a>(event: &'a Event, name: &'static str) -> impl Iterator<Item = &'a [String]> {
        event
            .tags
            .iter()
            .map(|t| t.as_slice())
            .filter(move |t| t.first().is_some_and(|k| k == name))
    }
    let rooms: Vec<&[String]> = tag(event, "h").collect();
    let scoped = match rooms.as_slice() {
        [h] => h.len() >= 2 && h[1] == room,
        [] => {
            matches!(kind, 5 | 9005 | 7)
                && tag(event, "e").any(|e| e.get(1).is_some_and(|id| held(id)))
        }
        _ => false,
    };
    if !scoped {
        return false;
    }
    if serde_json::to_vec(event).map_or(true, |bytes| bytes.len() > MAX_EVENT_BYTES) {
        return false;
    }
    event.verify().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};
    const ROOM: &str = "00000000-0000-4000-8000-000000000001";
    fn signed(keys: &Keys, kind: u16, content: &str, tags: Vec<Tag>) -> Event {
        EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags)
            .sign_with_keys(keys)
            .unwrap()
    }
    fn h(room: &str) -> Tag {
        Tag::parse(["h", room]).unwrap()
    }
    fn armed() -> (Live, String) {
        let mut live = Live::default();
        let frame = live.arm(ROOM, 1_700_000_000);
        let id = frame[1].as_str().unwrap().to_owned();
        assert!(id.starts_with(PREFIX) && id.len() <= 64);
        assert_eq!(
            frame,
            json!(["REQ", id, {"kinds":[9,40002,40003,5,9005,7,39005],"#h":[ROOM],"since":1_700_000_000}])
        );
        (live, id)
    }
    #[test]
    fn only_verified_scoped_events_trigger() {
        let user = Keys::generate();
        let relay = Keys::generate();
        let (mut live, id) = armed();
        let now = Instant::now();
        let none = |_: &str| false;
        let mut check = |event: &Event, held: &dyn Fn(&str) -> bool| {
            matches!(
                live.event(&id, event, Some(relay.public_key()), held, now),
                Frame::Trigger { .. }
            )
        };
        assert!(check(&signed(&user, 40002, "x", vec![h(ROOM)]), &none));
        assert!(!check(
            &signed(
                &user,
                40002,
                "x",
                vec![h("00000000-0000-4000-8000-000000000002")]
            ),
            &none
        ));
        assert!(!check(
            &signed(&user, 40002, "x", vec![h(ROOM), h(ROOM)]),
            &none
        ));
        assert!(!check(&signed(&user, 40002, "x", vec![]), &none));
        assert!(!check(&signed(&user, 1, "x", vec![h(ROOM)]), &none));
        assert!(!check(
            &signed(&user, 40002, &"x".repeat(MAX_EVENT_BYTES), vec![h(ROOM)]),
            &none
        ));
        // Relay thread summaries only from the pinned relay.
        assert!(!check(&signed(&user, 39005, "{}", vec![h(ROOM)]), &none));
        assert!(check(&signed(&relay, 39005, "{}", vec![h(ROOM)]), &none));
        // A deletion without `h` triggers only for a held row.
        let target = "a".repeat(64);
        let deletion = signed(&user, 5, "", vec![Tag::parse(["e", &target]).unwrap()]);
        assert!(!check(&deletion, &none));
        assert!(check(&deletion, &|id: &str| id == target));
        assert!(!check(
            &signed(&user, 40002, "x", vec![Tag::parse(["e", &target]).unwrap()]),
            &|_: &str| true
        ));
        // A tampered body fails the signature.
        let mut forged = signed(&user, 40002, "x", vec![h(ROOM)]);
        forged.content = "y".into();
        assert!(!check(&forged, &none));
        // Another subscription's frame is not ours.
        assert!(matches!(
            live.event(
                "other",
                &signed(&user, 40002, "x", vec![h(ROOM)]),
                None,
                none,
                now
            ),
            Frame::Other
        ));
    }
    #[test]
    fn thread_flag_follows_e_tags() {
        let user = Keys::generate();
        let (mut live, id) = armed();
        let now = Instant::now();
        let reply = signed(
            &user,
            40002,
            "x",
            vec![h(ROOM), Tag::parse(["e", &"b".repeat(64)]).unwrap()],
        );
        assert!(matches!(
            live.event(&id, &reply, None, |_| false, now),
            Frame::Trigger { thread: true }
        ));
        let top = signed(&user, 40002, "x", vec![h(ROOM)]);
        assert!(matches!(
            live.event(&id, &top, None, |_| false, now),
            Frame::Trigger { thread: false }
        ));
    }
    #[test]
    fn flood_closes_and_pauses() {
        let user = Keys::generate();
        let (mut live, id) = armed();
        let now = Instant::now();
        let bad = signed(&user, 1, "x", vec![h(ROOM)]);
        for _ in 0..FLOOD_FRAMES {
            assert!(matches!(
                live.event(&id, &bad, None, |_| false, now),
                Frame::Ignored
            ));
        }
        match live.event(&id, &bad, None, |_| false, now) {
            Frame::Flood(close) => assert_eq!(close, json!(["CLOSE", id])),
            _ => panic!("flood not detected"),
        }
        assert!(live.room().is_none());
        assert!(!live.can_arm(now + Duration::from_secs(299)));
        live.select("00000000-0000-4000-8000-000000000002");
        assert!(
            !live.can_arm(now + Duration::from_secs(299)),
            "room change skipped the flood pause"
        );
        assert!(live.can_arm(now + FLOOD_PAUSE));
    }
    #[test]
    fn busy_rooms_widen_the_gap() {
        let user = Keys::generate();
        let (mut live, id) = armed();
        let now = Instant::now();
        let event = signed(&user, 40002, "x", vec![h(ROOM)]);
        for _ in 0..BUSY_EVENTS {
            live.event(&id, &event, None, |_| false, now);
        }
        assert_eq!(live.gap(now), DEBOUNCE);
        live.event(&id, &event, None, |_| false, now);
        assert_eq!(live.gap(now), BUSY_GAP);
        assert_eq!(live.gap(now + Duration::from_secs(11)), DEBOUNCE);
    }
    #[test]
    fn closed_backs_off_and_eose_primes() {
        let (mut live, id) = armed();
        assert!(!live.primed_for(Some(ROOM)));
        assert!(live.eose(&id));
        assert!(live.primed_for(Some(ROOM)));
        let now = Instant::now();
        live.closed_by_relay(now);
        assert!(!live.primed_for(Some(ROOM)));
        assert!(!live.can_arm(now + Duration::from_secs(4)));
        assert!(live.can_arm(now + Duration::from_secs(5)));
        live.arm(ROOM, 1);
        live.closed_by_relay(now);
        assert!(!live.can_arm(now + Duration::from_secs(29)));
        assert!(live.can_arm(now + Duration::from_secs(30)));
        live.arm(ROOM, 1);
        live.closed_by_relay(now);
        live.arm(ROOM, 1);
        live.closed_by_relay(now);
        assert!(!live.can_arm(now + Duration::from_secs(299)));
        // A different room starts without backoff.
        live.select("00000000-0000-4000-8000-000000000002");
        assert!(live.can_arm(now));
        assert!(live.close().is_none());
    }
}
