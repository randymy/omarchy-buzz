use super::*;
use nostr::{EventBuilder, Kind, Tag};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn tags(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}
const NOW: u64 = 1_800_000_000;
/// A relay snapshot event as `synthesize_presence` signs it.
fn snapshot(signer: &Keys, content: &str, p: &[&[&str]], at: u64) -> Event {
    EventBuilder::new(Kind::Custom(KIND), content)
        .tags(p.iter().map(|t| Tag::parse(t.iter().copied()).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(signer)
        .unwrap()
}

#[test]
fn event_matches_the_pinned_sdk_builder() {
    let user = key(2);
    for state in ["online", "away", "offline"] {
        let event = build_event(state, &user, Some(Timestamp::from(NOW))).unwrap();
        event.verify().unwrap();
        assert_eq!(event.kind.as_u16(), 20001);
        assert_eq!(event.pubkey, user.public_key());
        assert_eq!(event.content, state);
        assert_eq!(tags(&event), vec![vec!["status".to_string(), state.into()]]);
        let upstream = buzz_sdk::build_presence_update(state)
            .unwrap()
            .custom_created_at(Timestamp::from(NOW))
            .sign_with_keys(&user)
            .unwrap();
        assert_eq!(upstream.id, event.id);
    }
    for bad in ["", "busy", "Online", "{\"status\":\"online\"}"] {
        assert_eq!(
            build_event(bad, &user, None).err(),
            Some("presence_invalid")
        );
    }
}

#[test]
fn derivation_table() {
    let table = [
        (Mode::Auto, true, "online"),
        (Mode::Auto, false, "away"),
        (Mode::Away, true, "away"),
        (Mode::Away, false, "away"),
        (Mode::Offline, true, "offline"),
        (Mode::Offline, false, "offline"),
    ];
    for (mode, active, state) in table {
        assert_eq!(derive(mode, active), state, "{mode:?} {active}");
    }
    for mode in [Mode::Auto, Mode::Away, Mode::Offline] {
        assert_eq!(Mode::parse(mode.as_str()), Some(mode));
    }
    assert_eq!(Mode::parse("online"), None);
    assert_eq!(Mode::parse("Auto"), None);
}

/// Publishes whatever is due at `now` through `gate`, as the observer does,
/// and returns the signed state.
fn tick(
    publisher: &mut Publisher,
    gate: &mut crate::user_status::Gate,
    now: Instant,
) -> Option<String> {
    let at = publisher.due(now, gate.ready_at(GAP), HEARTBEAT)?;
    if at > now || !gate.admit(now, GAP) {
        return None;
    }
    Some(publisher.prepare(&key(2), now).unwrap().content)
}
fn ack(publisher: &mut Publisher, accepted: bool) {
    let id = publisher.pending_id().unwrap();
    assert!(publisher.acknowledge(&id, accepted, NOW));
}

#[test]
fn heartbeat_and_rate_limit_with_a_fake_clock() {
    let mut gate = crate::user_status::Gate::new();
    let mut p = Publisher::default();
    let t0 = Instant::now();
    // Nothing is published before the panel asks.
    assert_eq!(p.due(t0, None, HEARTBEAT), None);
    assert!(p.set(Mode::Auto, true));
    assert_eq!(p.due(t0, None, HEARTBEAT), Some(t0));
    assert_eq!(tick(&mut p, &mut gate, t0).as_deref(), Some("online"));
    // One in flight: nothing else is due until it is answered.
    assert_eq!(p.due(t0 + HEARTBEAT * 2, None, HEARTBEAT), None);
    ack(&mut p, true);
    assert_eq!(p.published(), Some("online"));
    // The same preference again is a no-op.
    assert!(!p.set(Mode::Auto, true));
    assert_eq!(
        p.due(t0, gate.ready_at(GAP), HEARTBEAT),
        Some(t0 + HEARTBEAT)
    );
    // Going idle 1 s later changes the state, but the gate holds it to 5 s.
    let t1 = t0 + Duration::from_secs(1);
    assert!(p.set(Mode::Auto, false));
    assert_eq!(p.due(t1, gate.ready_at(GAP), HEARTBEAT), Some(t0 + GAP));
    assert_eq!(tick(&mut p, &mut gate, t1), None);
    assert_eq!(
        tick(&mut p, &mut gate, t0 + GAP - Duration::from_millis(1)),
        None
    );
    let t5 = t0 + GAP;
    assert_eq!(tick(&mut p, &mut gate, t5).as_deref(), Some("away"));
    ack(&mut p, true);
    // Away mode while idle derives the same state: nothing new to send.
    assert!(p.set(Mode::Away, false));
    assert!(p.set(Mode::Away, true));
    assert_eq!(
        p.due(t5, gate.ready_at(GAP), HEARTBEAT),
        Some(t5 + HEARTBEAT)
    );
    // The heartbeat re-publishes every 60 s while away.
    assert_eq!(
        tick(&mut p, &mut gate, t5 + HEARTBEAT - Duration::from_millis(1)),
        None
    );
    assert_eq!(
        tick(&mut p, &mut gate, t5 + HEARTBEAT).as_deref(),
        Some("away")
    );
    // A refused heartbeat is a category; the next one still comes.
    ack(&mut p, false);
    assert_eq!(p.view(true).state, "failed");
    assert_eq!(p.view(true).category.as_deref(), Some("presence_rejected"));
    assert_eq!(p.published(), Some("away"));
    let t65 = t5 + HEARTBEAT;
    assert_eq!(
        tick(&mut p, &mut gate, t65 + HEARTBEAT).as_deref(),
        Some("away")
    );
    ack(&mut p, true);
    assert_eq!(p.view(true).state, "ready");
    // Offline: published once, then no heartbeat at all.
    let t2 = t65 + HEARTBEAT + Duration::from_secs(10);
    assert!(p.set(Mode::Offline, true));
    assert_eq!(tick(&mut p, &mut gate, t2).as_deref(), Some("offline"));
    ack(&mut p, true);
    assert_eq!(
        p.due(t2 + HEARTBEAT * 10, gate.ready_at(GAP), HEARTBEAT),
        None
    );
    assert!(!p.set(Mode::Offline, true));
    assert!(p.set(Mode::Offline, false));
    assert_eq!(
        p.due(t2 + HEARTBEAT * 10, gate.ready_at(GAP), HEARTBEAT),
        None
    );
}

#[test]
fn the_gate_never_admits_two_publications_within_five_seconds() {
    let mut gate = crate::user_status::Gate::new();
    let mut p = Publisher::default();
    let t0 = Instant::now();
    let mut sent = Vec::new();
    // A panel flipping idleness every 500 ms for 30 s.
    for step in 0..60_u32 {
        let now = t0 + Duration::from_millis(500) * step;
        p.set(Mode::Auto, step % 2 == 0);
        if p.is_pending() {
            ack(&mut p, true);
        }
        if tick(&mut p, &mut gate, now).is_some() {
            sent.push(now);
        }
    }
    assert!(sent.len() >= 2);
    assert!(sent.windows(2).all(|w| w[1] - w[0] >= GAP), "{sent:?}");
}

#[test]
fn offline_is_published_once_when_the_last_panel_leaves_or_on_shutdown() {
    let mut gate = crate::user_status::Gate::new();
    let t0 = Instant::now();
    // Never published: nothing owed.
    let mut p = Publisher::default();
    p.detach();
    assert_eq!(p.due(t0, None, HEARTBEAT), None);
    // Online, then detached: offline once, then nothing.
    p.set(Mode::Auto, true);
    assert_eq!(tick(&mut p, &mut gate, t0).as_deref(), Some("online"));
    ack(&mut p, true);
    p.detach();
    assert!(!p.configured());
    assert_eq!(p.due(t0, gate.ready_at(GAP), HEARTBEAT), Some(t0 + GAP));
    assert_eq!(
        tick(&mut p, &mut gate, t0 + GAP).as_deref(),
        Some("offline")
    );
    ack(&mut p, true);
    assert_eq!(p.published(), Some("offline"));
    assert_eq!(p.due(t0 + HEARTBEAT * 5, None, HEARTBEAT), None);
    // A heartbeat in flight at shutdown is abandoned for the offline one
    // (the observer calls `unknown` first), still through the gate.
    let mut p = Publisher::default();
    let mut gate = crate::user_status::Gate::new();
    p.set(Mode::Away, true);
    assert_eq!(tick(&mut p, &mut gate, t0).as_deref(), Some("away"));
    p.detach();
    assert!(p.unknown());
    let later = t0 + GAP;
    assert_eq!(tick(&mut p, &mut gate, later).as_deref(), Some("offline"));
    // Already offline (mode): nothing more is owed.
    let mut p = Publisher::default();
    let mut gate = crate::user_status::Gate::new();
    p.set(Mode::Offline, true);
    assert_eq!(tick(&mut p, &mut gate, t0).as_deref(), Some("offline"));
    ack(&mut p, true);
    p.detach();
    assert_eq!(p.due(t0 + HEARTBEAT, None, HEARTBEAT), None);
}

#[test]
fn only_the_exact_ok_resolves_and_reauthentication_starts_over() {
    let mut gate = crate::user_status::Gate::new();
    let mut p = Publisher::default();
    let t0 = Instant::now();
    p.set(Mode::Auto, true);
    assert_eq!(p.view(true).state, "unavailable");
    assert_eq!(p.view(true).mode.as_deref(), Some("auto"));
    tick(&mut p, &mut gate, t0).unwrap();
    assert!(!p.acknowledge(&"0".repeat(64), true, NOW));
    assert!(p.is_pending());
    assert_eq!(p.deadline(), t0 + TIMEOUT);
    assert!(p.unknown());
    assert!(!p.unknown());
    let view = p.view(true);
    assert_eq!(
        (
            view.state.as_str(),
            view.category.as_deref(),
            view.published
        ),
        ("failed", Some("relay_unavailable"), None)
    );
    // Same state, so the next attempt is the heartbeat.
    assert_eq!(
        p.due(t0, gate.ready_at(GAP), HEARTBEAT),
        Some(t0 + HEARTBEAT)
    );
    // A re-authenticated session writes the state again at once (gate permitting).
    p.reauthenticated();
    assert_eq!(p.due(t0, gate.ready_at(GAP), HEARTBEAT), Some(t0 + GAP));
    assert_eq!(tick(&mut p, &mut gate, t0 + GAP).as_deref(), Some("online"));
    ack(&mut p, true);
    let view = p.view(true);
    assert_eq!(
        (
            view.state.as_str(),
            view.published.as_deref(),
            view.last_published_at,
            view.category
        ),
        ("ready", Some("online"), Some(NOW), None)
    );
    assert_eq!(p.view(false), crate::protocol::PresenceView::unavailable());
}

#[test]
fn content_accepts_bare_states_and_bounded_legacy_json() {
    for state in ["online", "away", "offline"] {
        assert_eq!(content_state(state), Some(state));
        assert_eq!(
            content_state(&format!("{{\"status\":\"{state}\"}}")),
            Some(state)
        );
    }
    let padded = format!("{{\"status\":\"away\",\"x\":\"{}\"}}", "a".repeat(120));
    assert!(padded.len() > LEGACY_BYTES);
    assert_eq!(content_state(&padded), None);
    for bad in [
        "",
        "Online",
        "busy",
        " online",
        "{\"status\":1}",
        "{bad",
        "{\"state\":\"online\"}",
    ] {
        assert_eq!(content_state(bad), None, "{bad}");
    }
}

#[test]
fn verification_rejects_every_unverified_shape() {
    let relay = key(9);
    let (alex, sam, outsider) = (key(3), key(4), key(5));
    let subjects = vec![alex.public_key().to_hex(), sam.public_key().to_hex()];
    let a = alex.public_key().to_hex();
    let good = snapshot(&relay, "online", &[&["p", &a]], NOW);
    let read = verify(
        &relay.public_key(),
        &subjects,
        std::slice::from_ref(&good),
        NOW,
    )
    .unwrap();
    // Absent from the snapshot: offline (the relay's answer is authoritative).
    assert_eq!(read.get(&a), Some(&"online"));
    assert_eq!(read.get(&sam.public_key().to_hex()), Some(&"offline"));

    let mut forged = good.clone();
    forged.content = "away".into();
    let upper = a.to_uppercase();
    let rejected = [
        ("bad signature", forged),
        // A peer's own live event (no relay signature) is not used here.
        ("self-signed", snapshot(&alex, "online", &[&["p", &a]], NOW)),
        (
            "non-relay signer",
            snapshot(&key(8), "online", &[&["p", &a]], NOW),
        ),
        (
            "missing p",
            snapshot(&relay, "online", &[&["status", "online"]], NOW),
        ),
        (
            "extra p",
            snapshot(
                &relay,
                "online",
                &[&["p", &a], &["p", &sam.public_key().to_hex()]],
                NOW,
            ),
        ),
        ("bare p", snapshot(&relay, "online", &[&["p"]], NOW)),
        (
            "non-canonical p",
            snapshot(&relay, "online", &[&["p", &upper]], NOW),
        ),
        (
            "short p",
            snapshot(&relay, "online", &[&["p", &a[..63]]], NOW),
        ),
        (
            "outside roster",
            snapshot(
                &relay,
                "online",
                &[&["p", &outsider.public_key().to_hex()]],
                NOW,
            ),
        ),
        (
            "wrong kind",
            EventBuilder::new(Kind::Custom(40902), "online")
                .tags([Tag::parse(["p", a.as_str()]).unwrap()])
                .sign_with_keys(&relay)
                .unwrap(),
        ),
    ];
    for (name, event) in rejected {
        // One bad event rejects the whole read, even beside a good one.
        assert_eq!(
            verify(&relay.public_key(), &subjects, &[good.clone(), event], NOW).err(),
            Some("presence_invalid"),
            "{name}"
        );
    }
    // Stale (either direction beyond 240 s): offline, not the content.
    for at in [NOW - FRESH_SECS - 1, NOW + FRESH_SECS + 1] {
        let stale = snapshot(&relay, "online", &[&["p", &a]], at);
        let read = verify(&relay.public_key(), &subjects, &[stale], NOW).unwrap();
        assert_eq!(read.get(&a), Some(&"offline"), "{at}");
    }
    let edge = snapshot(&relay, "away", &[&["p", &a]], NOW - FRESH_SECS);
    assert_eq!(
        verify(&relay.public_key(), &subjects, &[edge], NOW)
            .unwrap()
            .get(&a),
        Some(&"away")
    );
    // Malformed content: that subject stays unknown; the others are still read.
    for content in ["busy", "", "{\"status\":\"lurking\"}"] {
        let odd = snapshot(&relay, content, &[&["p", &a]], NOW);
        let read = verify(&relay.public_key(), &subjects, &[odd], NOW).unwrap();
        assert_eq!(read.get(&a), None, "{content:?}");
        assert_eq!(read.get(&sam.public_key().to_hex()), Some(&"offline"));
    }
    // Legacy JSON content is read; per subject the newest event wins.
    let older = snapshot(&relay, "{\"status\":\"away\"}", &[&["p", &a]], NOW - 30);
    let newer = snapshot(&relay, "online", &[&["p", &a]], NOW - 1);
    let read = verify(&relay.public_key(), &subjects, &[newer, older.clone()], NOW).unwrap();
    assert_eq!(read.get(&a), Some(&"online"));
    let read = verify(&relay.public_key(), &subjects, &[older], NOW).unwrap();
    assert_eq!(read.get(&a), Some(&"away"));
    // An empty snapshot: everyone offline.
    let read = verify(&relay.public_key(), &subjects, &[], NOW).unwrap();
    assert!(read.values().all(|s| *s == "offline") && read.len() == 2);
}

#[test]
fn categories_are_a_fixed_list() {
    let mut p = Publisher::default();
    let mut gate = crate::user_status::Gate::new();
    p.set(Mode::Auto, true);
    tick(&mut p, &mut gate, Instant::now()).unwrap();
    ack(&mut p, false);
    assert!(CATEGORIES.contains(&p.view(true).category.unwrap().as_str()));
    tick(&mut p, &mut gate, Instant::now() + HEARTBEAT).unwrap();
    p.unknown();
    assert!(CATEGORIES.contains(&p.view(true).category.unwrap().as_str()));
}
