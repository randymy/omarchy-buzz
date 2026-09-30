use super::*;
use crate::protocol::{Recipient, RecipientsView, Room};

const ROOM: &str = "00000000-0000-4000-8000-000000000001";
const DM: &str = "00000000-0000-4000-8000-000000000002";

fn key(c: char) -> Keys {
    Keys::parse(&c.to_string().repeat(64)).unwrap()
}
fn hex(c: char) -> String {
    key(c).public_key().to_hex()
}
fn intent(id: u8, participants: Vec<String>) -> DmOpenIntent {
    DmOpenIntent {
        request_id: format!("00000000-0000-4000-8000-0000000000{id:02x}"),
        participants,
        generation: 1,
    }
}
/// Viewer `1`; room roster `1`,`2`; an existing DM with `1`,`3`.
fn status() -> Status {
    let mut s = Status::new(&crate::config::Config::default());
    s.connection = "authenticated".into();
    s.catalog.state = "partial".into();
    s.catalog.rooms = vec![
        Room {
            id: ROOM.into(),
            name: "room".into(),
            description: String::new(),
            kind: "stream".into(),
            participants: Vec::new(),
            hidden: false,
        },
        Room {
            id: DM.into(),
            name: "dm".into(),
            description: String::new(),
            kind: "dm".into(),
            participants: vec![hex('1'), hex('3')],
            hidden: true,
        },
    ];
    s.recipients = RecipientsView {
        state: "snapshot".into(),
        room_id: Some(ROOM.into()),
        entries: ['1', '2']
            .into_iter()
            .map(|c| Recipient {
                key: hex(c),
                name: String::new(),
            })
            .collect(),
        agents: Vec::new(),
        partial: false,
        category: None,
    };
    s
}

#[test]
fn event_is_exactly_the_sdk_dm_open() {
    let viewer = key('1');
    let others = vec![hex('3'), hex('2')];
    let at = nostr::Timestamp::from(1_700_000_000);
    let event = build_event(&others, &viewer, Some(at)).unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 41010);
    assert_eq!(event.pubkey, viewer.public_key());
    assert_eq!(event.content, "");
    assert_eq!(event.created_at, at);
    let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
    assert_eq!(
        tags,
        vec![
            vec!["p".to_string(), hex('3')],
            vec!["p".to_string(), hex('2')]
        ]
    );
    // Same signed identity as the upstream builder: no local tag was added.
    let upstream = buzz_sdk::build_dm_open(&[&hex('3'), &hex('2')])
        .unwrap()
        .custom_created_at(at)
        .sign_with_keys(&viewer)
        .unwrap();
    assert_eq!(event.id, upstream.id);
    assert!(build_event(&[], &viewer, None).is_err());
    let nine: Vec<String> = "23456789a".chars().map(hex).collect();
    assert!(build_event(&nine, &viewer, None).is_err());
}

#[test]
fn only_verified_roster_or_existing_dm_participants_are_accepted() {
    let viewer = key('1');
    let s = status();
    for allowed in [vec![hex('2')], vec![hex('3')], vec![hex('2'), hex('3')]] {
        let mut opener = Opener::default();
        let (view, event) = opener
            .prepare(&intent(1, allowed.clone()), &viewer, &s, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(view.state, "sending");
        assert!(view.channel_id.is_none() && view.created.is_none());
        let tags: Vec<String> = event.tags.iter().map(|t| t.as_slice()[1].clone()).collect();
        assert_eq!(tags, allowed);
    }
    let mut opener = Opener::default();
    let refused = |o: &mut Opener, s: &Status, i: DmOpenIntent, fresh, pinned| {
        o.prepare(&i, &viewer, s, fresh, pinned).unwrap_err()
    };
    // A key known nowhere, or mixed in with an allowed one.
    assert_eq!(
        refused(&mut opener, &s, intent(1, vec![hex('4')]), true, true),
        "dm_open_access_denied"
    );
    assert_eq!(
        refused(
            &mut opener,
            &s,
            intent(1, vec![hex('2'), hex('4')]),
            true,
            true
        ),
        "dm_open_access_denied"
    );
    // The viewer is never a named participant.
    assert_eq!(
        refused(&mut opener, &s, intent(1, vec![hex('1')]), true, true),
        "dm_open_invalid"
    );
    assert_eq!(
        refused(&mut opener, &s, intent(1, vec![]), true, true),
        "dm_open_invalid"
    );
    assert_eq!(
        refused(
            &mut opener,
            &s,
            intent(1, vec![hex('2'), hex('2')]),
            true,
            true
        ),
        "dm_open_invalid"
    );
    assert_eq!(
        refused(
            &mut opener,
            &s,
            intent(1, vec![hex('2').to_uppercase()]),
            true,
            true
        ),
        "dm_open_invalid"
    );
    let mut bad_id = intent(1, vec![hex('2')]);
    bad_id.request_id = "ui-1".into();
    assert_eq!(
        refused(&mut opener, &s, bad_id, true, true),
        "dm_open_invalid"
    );
    let mut stale = intent(1, vec![hex('2')]);
    stale.generation = 2;
    assert_eq!(
        refused(&mut opener, &s, stale, true, true),
        "dm_open_scope_changed"
    );
    assert_eq!(
        refused(&mut opener, &s, intent(1, vec![hex('2')]), false, true),
        "dm_open_unavailable"
    );
    assert_eq!(
        refused(&mut opener, &s, intent(1, vec![hex('2')]), true, false),
        "dm_open_unavailable"
    );
    let mut loading = s.clone();
    loading.catalog.state = "loading".into();
    assert_eq!(
        refused(&mut opener, &loading, intent(1, vec![hex('2')]), true, true),
        "dm_open_unavailable"
    );
    // A roster that is not a current snapshot of a joined room grants nothing.
    let mut stale_roster = s.clone();
    stale_roster.recipients.state = "loading".into();
    assert_eq!(
        refused(
            &mut opener,
            &stale_roster,
            intent(1, vec![hex('2')]),
            true,
            true
        ),
        "dm_open_access_denied"
    );
    let mut left = s.clone();
    left.catalog.rooms.retain(|r| r.id != ROOM);
    assert_eq!(
        refused(&mut opener, &left, intent(1, vec![hex('2')]), true, true),
        "dm_open_access_denied"
    );
    // A stream's (empty) participant list is not a DM's.
    let mut no_dm = s.clone();
    no_dm.catalog.rooms.retain(|r| r.id != DM);
    assert_eq!(
        refused(&mut opener, &no_dm, intent(1, vec![hex('3')]), true, true),
        "dm_open_access_denied"
    );
    assert!(!opener.is_pending());
}

#[test]
fn one_open_at_a_time_and_request_ids_are_never_resigned() {
    let viewer = key('1');
    let mut s = status();
    let mut opener = Opener::default();
    let first = intent(1, vec![hex('2')]);
    let (view, _) = opener
        .prepare(&first, &viewer, &s, true, true)
        .unwrap()
        .unwrap();
    assert!(opener.is_pending());
    // Identical replay: no new event, receipt kept.
    assert!(opener
        .prepare(&first, &viewer, &s, true, true)
        .unwrap()
        .is_none());
    assert_eq!(
        opener
            .prepare(&intent(2, vec![hex('2')]), &viewer, &s, true, true)
            .unwrap_err(),
        "dm_open_busy"
    );
    assert_eq!(
        opener
            .prepare(&intent(1, vec![hex('3')]), &viewer, &s, true, true)
            .unwrap_err(),
        "dm_open_request_reused"
    );
    s.dm_open = view;
    let done = opener.unknown().unwrap();
    assert_eq!(done.state, "unknown");
    assert_eq!(done.category.as_deref(), Some("dm_open_unknown"));
    assert_eq!(done.request_id, first.request_id.clone().into());
    assert!(opener.unknown().is_none());
    s.dm_open = done;
    // The reported request is finished; reusing its ID is refused, a new ID is fine.
    assert_eq!(
        opener.prepare(&first, &viewer, &s, true, true).unwrap_err(),
        "dm_open_request_reused"
    );
    assert!(opener
        .prepare(&intent(2, vec![hex('2')]), &viewer, &s, true, true)
        .unwrap()
        .is_some());
    opener.abandon();
    assert!(!opener.is_pending());
}

#[test]
fn ok_is_matched_by_exact_event_id_and_parsed_strictly() {
    let viewer = key('1');
    let s = status();
    let open = |opener: &mut Opener| {
        opener
            .prepare(&intent(1, vec![hex('2')]), &viewer, &s, true, true)
            .unwrap()
            .unwrap()
            .1
            .id
            .to_hex()
    };
    let mut opener = Opener::default();
    let id = open(&mut opener);
    let answer = format!("response:{{\"channel_id\":\"{DM}\",\"created\":true}}");
    assert!(opener.acknowledge(&"0".repeat(64), true, &answer).is_none());
    assert!(opener.is_pending());
    let view = opener.acknowledge(&id, true, &answer).unwrap();
    assert_eq!(view.state, "acknowledged");
    assert_eq!(view.channel_id.as_deref(), Some(DM));
    assert_eq!(view.created, Some(true));
    assert!(view.category.is_none());
    assert!(
        opener.acknowledge(&id, true, &answer).is_none(),
        "second OK ignored"
    );

    let id = open(&mut opener);
    let view = opener
        .acknowledge(&id, false, "restricted: arbitrary relay text")
        .unwrap();
    assert_eq!(view.state, "rejected");
    assert_eq!(view.category.as_deref(), Some("dm_open_rejected"));
    assert!(view.channel_id.is_none() && view.created.is_none());

    // Accepted without a usable channel: acknowledged, channel unknown.
    for message in [
        "duplicate: already processed",
        "",
        "response:{\"channel_id\":\"not-a-uuid\",\"created\":true}",
        "response:{\"channel_id\":\"00000000-0000-4000-8000-00000000000A\",\"created\":true}",
        "response:{\"channel_id\":\"00000000-0000-4000-8000-000000000002\"}",
        "response:{\"channel_id\":\"00000000-0000-4000-8000-000000000002\",\"created\":\"yes\"}",
    ] {
        let id = open(&mut opener);
        let view = opener.acknowledge(&id, true, message).unwrap();
        assert_eq!(view.state, "acknowledged", "{message}");
        assert!(
            view.channel_id.is_none() && view.created.is_none(),
            "{message}"
        );
        assert_eq!(view.category.as_deref(), Some("dm_open_response_unknown"));
    }
    let padded = format!(
        "response:{{\"channel_id\":\"{DM}\",\"created\":false,\"pad\":\"{}\"}}",
        "x".repeat(1024)
    );
    assert!(parse_response(&padded).is_none());
    assert_eq!(
        parse_response(&format!("{{\"channel_id\":\"{DM}\",\"created\":false}}")),
        Some((DM.to_string(), false))
    );
    assert_eq!(
        parse_response(&format!(
            "response:{{\"created\":false,\"channel_id\":\"{DM}\",\"extra\":1}}"
        )),
        Some((DM.to_string(), false))
    );
}
