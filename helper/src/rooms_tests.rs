use super::*;
use crate::join::RoomActions;
use crate::protocol::{RoomDetailView, RoomMember, Status};
use nostr::{EventBuilder, Kind, Tag};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn event(author: &Keys, kind: u16, rows: &[Vec<String>], at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), "")
        .tags(rows.iter().map(|t| Tag::parse(t.clone()).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn row(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn tags_of(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}
const ROOM: &str = "11111111-1111-4111-8111-111111111111";
const REQUEST: &str = "00000000-0000-4000-8000-000000000001";
fn room() -> Uuid {
    Uuid::parse_str(ROOM).unwrap()
}

#[test]
fn event_shapes_match_desktop() {
    let id = room();
    let create = Change::Create {
        name: "  #Team room ".into(),
        about: "About us".into(),
        private: true,
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(create.kind.as_u16(), 9007);
    assert_eq!(create.content, "");
    assert_eq!(
        tags_of(&create),
        vec![
            row(&["h", ROOM]),
            // The relay stores names without the display `#` (`canonical_channel_name`).
            row(&["name", "Team room"]),
            row(&["visibility", "private"]),
            row(&["channel_type", "stream"]),
            row(&["about", "About us"]),
        ]
    );
    // No description: no `about` tag; open rooms say so.
    let open = Change::Create {
        name: "Open".into(),
        about: String::new(),
        private: false,
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(
        tags_of(&open),
        vec![
            row(&["h", ROOM]),
            row(&["name", "Open"]),
            row(&["visibility", "open"]),
            row(&["channel_type", "stream"]),
        ]
    );
    let details = Change::Details {
        room: id,
        name: "Renamed".into(),
        about: String::new(),
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(details.kind.as_u16(), 9002);
    assert_eq!(
        tags_of(&details),
        vec![
            row(&["h", ROOM]),
            row(&["name", "Renamed"]),
            row(&["about", ""])
        ]
    );
    let topic = Change::Topic {
        room: id,
        topic: "Ship it".into(),
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(topic.kind.as_u16(), 9002);
    assert_eq!(
        tags_of(&topic),
        vec![row(&["h", ROOM]), row(&["topic", "Ship it"])]
    );
    let other = key(3).public_key();
    let add = Change::AddMember {
        room: id,
        key: other,
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(add.kind.as_u16(), 9000);
    assert_eq!(
        tags_of(&add),
        vec![row(&["h", ROOM]), row(&["p", &other.to_hex()])]
    );
    let remove = Change::RemoveMember {
        room: id,
        key: other,
    }
    .build(id)
    .unwrap()
    .sign_with_keys(&key(2))
    .unwrap();
    assert_eq!(remove.kind.as_u16(), 9001);
    assert_eq!(
        tags_of(&remove),
        vec![row(&["h", ROOM]), row(&["p", &other.to_hex()])]
    );
}

#[test]
fn text_is_bounded_and_plain() {
    let id = room();
    let create = |name: &str, about: &str| Change::Create {
        name: name.into(),
        about: about.into(),
        private: false,
    };
    assert!(create("ok", "").build(id).is_ok());
    for (name, about) in [
        ("", ""),
        ("   ", ""),
        ("###", ""),
        ("two\nlines", ""),
        ("tab\there", ""),
        (&"n".repeat(NAME_BYTES + 1), ""),
        ("ok", &"a".repeat(ABOUT_BYTES + 1)),
        ("ok", "bell\u{7}"),
    ] {
        assert_eq!(
            create(name, about).build(id).err(),
            Some("room_invalid"),
            "{name:?}"
        );
    }
    assert!(create(&"n".repeat(NAME_BYTES), &"a".repeat(ABOUT_BYTES))
        .build(id)
        .is_ok());
    let topic = |t: &str| {
        Change::Topic {
            room: id,
            topic: t.into(),
        }
        .build(id)
    };
    assert!(topic("").is_ok(), "an empty topic clears it");
    assert!(topic(&"t".repeat(TOPIC_BYTES)).is_ok());
    assert!(topic(&"t".repeat(TOPIC_BYTES + 1)).is_err());
    assert!(topic("a\u{0}b").is_err());
}

fn catalog_status(keys: &Keys) -> Status {
    let mut status = Status::new(&crate::config::Config::default());
    status.connection = "authenticated".into();
    status.catalog.state = "partial".into();
    for (id, kind) in [
        (ROOM, "stream"),
        ("22222222-2222-4222-8222-222222222222", "dm"),
    ] {
        status.catalog.rooms.push(Room {
            id: id.into(),
            name: "r".into(),
            description: String::new(),
            kind: kind.into(),
            participants: Vec::new(),
            hidden: false,
        });
    }
    let _ = keys;
    status
}
fn with_roster(status: &mut Status, members: &[&Keys]) {
    status.room_detail = RoomDetailView {
        state: "snapshot".into(),
        room_id: Some(ROOM.into()),
        members: members
            .iter()
            .map(|k| RoomMember {
                key: k.public_key().to_hex(),
                name: String::new(),
                role: "member".into(),
            })
            .collect(),
        ..RoomDetailView::unavailable(None, None)
    };
}

#[test]
fn changes_are_checked_signed_and_resolved_only_by_ok() {
    let me = key(2);
    let (a, b) = (key(3), key(4));
    let mut status = catalog_status(&me);
    with_roster(&mut status, &[&me, &a]);
    let prepare = |change: Change, status: &Status, fresh, trusted| {
        RoomActions::default().prepare_change(&change, REQUEST, &me, status, fresh, trusted)
    };
    let create = || Change::Create {
        name: "New".into(),
        about: String::new(),
        private: false,
    };
    assert_eq!(
        prepare(create(), &status, false, true).unwrap_err(),
        "relay_unavailable"
    );
    assert_eq!(
        prepare(create(), &status, true, false).unwrap_err(),
        "relay_unavailable"
    );
    let mut offline = status.clone();
    offline.connection = "connecting".into();
    assert_eq!(
        prepare(create(), &offline, true, true).unwrap_err(),
        "relay_unavailable"
    );
    // Management targets a joined stream room only.
    let dm = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let unknown = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    for target in [dm, unknown] {
        let topic = Change::Topic {
            room: target,
            topic: "x".into(),
        };
        assert_eq!(
            prepare(topic, &status, true, true).unwrap_err(),
            "room_invalid"
        );
    }
    // Removal names a rostered member, never this identity; adding never a
    // member the roster already shows, nor this identity.
    let remove = |key: &Keys| Change::RemoveMember {
        room: room(),
        key: key.public_key(),
    };
    let add = |key: &Keys| Change::AddMember {
        room: room(),
        key: key.public_key(),
    };
    assert_eq!(
        prepare(remove(&b), &status, true, true).unwrap_err(),
        "room_invalid"
    );
    assert_eq!(
        prepare(remove(&me), &status, true, true).unwrap_err(),
        "room_invalid"
    );
    assert_eq!(
        prepare(add(&a), &status, true, true).unwrap_err(),
        "room_invalid"
    );
    assert_eq!(
        prepare(add(&me), &status, true, true).unwrap_err(),
        "room_invalid"
    );
    assert!(prepare(add(&b), &status, true, true).is_ok());
    assert!(prepare(remove(&a), &status, true, true).is_ok());
    // A roster of another room proves nothing.
    let mut elsewhere = status.clone();
    elsewhere.room_detail.room_id = Some("33333333-3333-4333-8333-333333333333".into());
    assert_eq!(
        prepare(remove(&a), &elsewhere, true, true).unwrap_err(),
        "room_invalid"
    );

    let mut actions = RoomActions::default();
    let (view, event) = actions
        .prepare_change(&create(), REQUEST, &me, &status, true, true)
        .unwrap();
    assert_eq!(event.kind.as_u16(), 9007);
    event.verify().unwrap();
    assert_eq!(event.pubkey, me.public_key());
    // The view names the new room, which the event's `h` tag carries.
    let new_room = view.room_id.clone().unwrap();
    assert_eq!(tags_of(&event)[0], row(&["h", &new_room]));
    assert_ne!(new_room, ROOM);
    assert_eq!(
        (view.state.as_str(), view.action.as_deref()),
        ("sending", Some("create"))
    );
    // One action at a time; an unrelated OK resolves nothing.
    assert_eq!(
        actions
            .prepare_change(
                &create(),
                "00000000-0000-4000-8000-000000000002",
                &me,
                &status,
                true,
                true
            )
            .unwrap_err(),
        "setup_busy"
    );
    assert!(actions
        .acknowledge_with(&"0".repeat(64), true, "")
        .is_none());
    let done = actions
        .acknowledge_with(&event.id.to_hex(), true, "")
        .unwrap();
    assert_eq!(
        (done.state.as_str(), done.category.as_deref(), done.detail),
        ("acknowledged", None, None)
    );
    // The relay's refusal arrives as it was said, with the action's category.
    let topic = Change::Topic {
        room: room(),
        topic: "t".into(),
    };
    let (_, event) = actions
        .prepare_change(
            &topic,
            "00000000-0000-4000-8000-000000000003",
            &me,
            &status,
            true,
            true,
        )
        .unwrap();
    let refused = actions
        .acknowledge_with(
            &event.id.to_hex(),
            false,
            "restricted: actor not authorized\u{7}\n",
        )
        .unwrap();
    assert_eq!(
        (
            refused.state.as_str(),
            refused.category.as_deref(),
            refused.detail.as_deref()
        ),
        (
            "rejected",
            Some("edit_rejected"),
            Some("restricted: actor not authorized")
        )
    );
    // No OK in time: the relay may have applied it.
    let (_, _) = actions
        .prepare_change(
            &add(&b),
            "00000000-0000-4000-8000-000000000004",
            &me,
            &status,
            true,
            true,
        )
        .unwrap();
    let lost = actions.unknown().unwrap();
    assert_eq!(
        (lost.state.as_str(), lost.category.as_deref(), lost.detail),
        ("unknown", Some("relay_unavailable"), None)
    );
}

#[test]
fn refusal_text_is_plain_and_bounded() {
    assert_eq!(crate::join::refusal(""), None);
    assert_eq!(crate::join::refusal("  \u{7} "), None);
    assert_eq!(
        crate::join::refusal("blocked: only owners\tand admins").as_deref(),
        Some("blocked: only owners and admins")
    );
    let long = crate::join::refusal(&"x".repeat(1000)).unwrap();
    assert_eq!(long.len(), crate::join::REFUSAL_BYTES);
    assert_eq!(
        crate::join::refusal("a\u{202e}b").as_deref(),
        Some("a b"),
        "bidi controls cannot reorder text"
    );
}

fn roster(relay: &Keys, members: &[(&Keys, &str)], at: u64) -> Event {
    let mut rows = vec![row(&["d", ROOM])];
    rows.extend(
        members
            .iter()
            .map(|(k, role)| row(&["p", &k.public_key().to_hex(), "", role])),
    );
    event(relay, 39002, &rows, at)
}
fn meta(relay: &Keys, extra: &[&[&str]]) -> Event {
    let mut rows = vec![
        row(&["d", ROOM]),
        row(&["name", "Lobby"]),
        row(&["t", "stream"]),
    ];
    rows.extend(extra.iter().map(|r| row(r)));
    event(relay, 39000, &rows, 100)
}

#[test]
fn detail_is_verified_with_roles_and_topic() {
    let (relay, me, admin, plain_member, bot) = (key(1), key(2), key(3), key(4), key(5));
    let roster = roster(
        &relay,
        &[
            (&plain_member, "member"),
            (&bot, "bot"),
            (&me, "owner"),
            (&admin, "admin"),
        ],
        100,
    );
    let meta = meta(&relay, &[&["public"], &["topic", "Ship\u{7} it"]]);
    let d = detail(
        room(),
        me.public_key(),
        relay.public_key(),
        &[roster.clone()],
        &[meta.clone()],
        1000,
    )
    .unwrap();
    assert_eq!(
        (d.role.as_str(), d.visibility.as_str(), d.topic.as_str()),
        ("owner", "open", "Ship  it")
    );
    // Owners and admins first, then the rest by key.
    let order: Vec<&str> = d.members.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(&order[..2], ["owner", "admin"]);
    assert!(!d.truncated);
    let private = detail(
        room(),
        me.public_key(),
        relay.public_key(),
        &[roster.clone()],
        &[self::meta(&relay, &[&["private"]])],
        1000,
    )
    .unwrap();
    assert_eq!(
        (private.visibility.as_str(), private.topic.as_str()),
        ("private", "")
    );

    let go = |rosters: &[Event], metas: &[Event], viewer: &Keys| {
        detail(
            room(),
            viewer.public_key(),
            relay.public_key(),
            rosters,
            metas,
            1000,
        )
        .err()
    };
    // Not the relay's signature, not a member, wrong or missing events.
    let forged = self::roster(&me, &[(&me, "owner")], 100);
    assert_eq!(
        go(&[forged], &[meta.clone()], &me),
        Some("room_detail_invalid")
    );
    assert_eq!(
        go(&[roster.clone()], &[meta.clone()], &key(9)),
        Some("room_detail_access_denied")
    );
    assert_eq!(go(&[], &[meta.clone()], &me), Some("room_detail_invalid"));
    assert_eq!(go(&[roster.clone()], &[], &me), Some("room_detail_invalid"));
    assert_eq!(
        go(&[roster.clone()], &[self::meta(&me, &[&["public"]])], &me),
        Some("room_detail_invalid")
    );
    // Both or neither visibility flag is not a shape the relay emits.
    assert_eq!(
        go(
            &[roster.clone()],
            &[self::meta(&relay, &[&["public"], &["private"]])],
            &me
        ),
        Some("room_detail_invalid")
    );
    assert_eq!(
        go(&[roster.clone()], &[self::meta(&relay, &[])], &me),
        Some("room_detail_invalid")
    );
    // A future timestamp, a repeated member and an unreadable key reject it.
    assert_eq!(
        go(
            &[self::roster(&relay, &[(&me, "owner")], 5000)],
            &[meta.clone()],
            &me
        ),
        Some("room_detail_invalid")
    );
    assert_eq!(
        go(
            &[self::roster(
                &relay,
                &[(&me, "owner"), (&me, "member")],
                100
            )],
            &[meta.clone()],
            &me
        ),
        Some("room_detail_invalid")
    );
    let bad = event(
        &relay,
        39002,
        &[row(&["d", ROOM]), row(&["p", "nothex"])],
        100,
    );
    assert_eq!(
        go(&[bad], &[meta.clone()], &me),
        Some("room_detail_invalid")
    );
    // A roster of another room is not this room's.
    let elsewhere = event(
        &relay,
        39002,
        &[
            row(&["d", "33333333-3333-4333-8333-333333333333"]),
            row(&["p", &me.public_key().to_hex()]),
        ],
        100,
    );
    assert_eq!(go(&[elsewhere], &[meta], &me), Some("room_detail_invalid"));
}

#[test]
fn unknown_roles_are_not_invented_and_large_rosters_are_cut() {
    let (relay, me) = (key(1), key(2));
    let many: Vec<Keys> = (10..10 + MEMBERS as u8 + 5).map(key).collect();
    let mut members: Vec<(&Keys, &str)> = many.iter().map(|k| (k, "weird")).collect();
    members.push((&me, "admin"));
    let d = detail(
        room(),
        me.public_key(),
        relay.public_key(),
        &[roster(&relay, &members, 100)],
        &[meta(&relay, &[&["public"]])],
        1000,
    )
    .unwrap();
    assert_eq!(d.members.len(), MEMBERS);
    assert!(d.truncated);
    assert_eq!(
        d.members[0].role, "admin",
        "the viewer's admin role sorts first"
    );
    assert!(d.members[1..].iter().all(|m| m.role == "unknown"));
}

#[test]
fn merge_keeps_every_listed_room() {
    let row = |id: &str, name: &str| Room {
        id: id.into(),
        name: name.into(),
        description: String::new(),
        kind: "stream".into(),
        participants: Vec::new(),
        hidden: false,
    };
    let held = vec![row("a", "A"), row("b", "B"), row("c", "C")];
    let merged = merge(&held, vec![row("b", "B2"), row("d", "D")]);
    let names: Vec<(&str, &str)> = merged
        .iter()
        .map(|r| (r.id.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(names, [("a", "A"), ("c", "C"), ("b", "B2"), ("d", "D")]);
    assert_eq!(merge(&held, Vec::new()).len(), 3);
    assert_eq!(more(false, 1), "none");
    assert_eq!(more(true, 1), "available");
    assert_eq!(more(true, crate::catalog::MAX_PAGES), "limit");
}

mod paging {
    use super::*;
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn room_uuid(n: usize) -> String {
        format!("{n:08x}-0000-4000-8000-000000000000")
    }
    fn membership(relay: &Keys, me: &Keys, n: usize, at: u64) -> Event {
        event(
            relay,
            39002,
            &[
                row(&["d", &room_uuid(n)]),
                row(&["p", &me.public_key().to_hex(), "", "member"]),
            ],
            at,
        )
    }
    fn metadata(relay: &Keys, n: usize) -> Event {
        event(
            relay,
            39000,
            &[
                row(&["d", &room_uuid(n)]),
                row(&["name", &format!("room {n}")]),
                row(&["t", "stream"]),
                row(&["public"]),
            ],
            100,
        )
    }
    /// Answers each request in turn; the filters it saw come back at the end.
    async fn serve(
        responses: Vec<String>,
    ) -> (String, tokio::task::JoinHandle<Vec<serde_json::Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut filters = Vec::new();
            for (index, payload) in responses.into_iter().enumerate() {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                loop {
                    let mut byte = [0; 1];
                    assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                    head.push(byte[0]);
                    if head.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                if index > 0 {
                    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    filters.push(
                        serde_json::from_slice::<serde_json::Value>(&body).unwrap()[0].clone(),
                    );
                }
                let response = if payload.starts_with("HTTP/") {
                    payload
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    )
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            filters
        });
        (origin, task)
    }
    fn json(events: &[Event]) -> String {
        serde_json::to_string(events).unwrap()
    }
    fn info(relay: &Keys) -> String {
        serde_json::json!({"self": relay.public_key().to_hex()}).to_string()
    }
    async fn finished(
        task: tokio::task::JoinHandle<Vec<serde_json::Value>>,
    ) -> Vec<serde_json::Value> {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn pages_continue_from_the_last_snapshot_and_report_more() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        let page = crate::catalog::PAGE as usize;
        // Newest first: page one at 2000.., page two at 1000.. (one short of full).
        let first: Vec<Event> = (0..page)
            .map(|n| membership(&relay, &me, n, 2000 - n as u64))
            .collect();
        let second: Vec<Event> = (page..page + 7)
            .map(|n| membership(&relay, &me, n, 1000 - n as u64))
            .collect();
        let cursor = first.last().unwrap();
        let (origin, task) = serve(vec![
            info(&relay),
            json(&first),
            json(&second),
            json(&(0..page).map(|n| metadata(&relay, n)).collect::<Vec<_>>()),
            json(
                &(page..page + 7)
                    .map(|n| metadata(&relay, n))
                    .collect::<Vec<_>>(),
            ),
        ])
        .await;
        let catalog =
            crate::catalog::discover_pages(&origin, &me, Some(relay.public_key()), 2, &[])
                .await
                .unwrap();
        assert_eq!(catalog.rooms.len(), page + 7);
        assert!(
            !catalog.has_more && catalog.next.is_none(),
            "a short page is the end"
        );
        let filters = finished(task).await;
        assert_eq!(filters[0]["limit"], page);
        assert!(filters[0].get("until").is_none() && filters[0].get("before_id").is_none());
        assert_eq!(filters[1]["until"], cursor.created_at.as_secs());
        assert_eq!(filters[1]["before_id"], cursor.id.to_hex());
        assert_eq!(filters[1]["kinds"][0], 39002);
        // One metadata read per page, within the request bound.
        assert_eq!(filters[2]["#d"].as_array().unwrap().len(), page);
        assert_eq!(filters[3]["#d"].as_array().unwrap().len(), 7);
    }

    #[tokio::test]
    async fn a_full_page_offers_more_with_its_cursor_and_loading_it_continues() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        let page = crate::catalog::PAGE as usize;
        let first: Vec<Event> = (0..page)
            .map(|n| membership(&relay, &me, n, 2000 - n as u64))
            .collect();
        let (origin, task) = serve(vec![
            info(&relay),
            json(&first),
            json(&(0..page).map(|n| metadata(&relay, n)).collect::<Vec<_>>()),
        ])
        .await;
        let head = crate::catalog::discover_pages(&origin, &me, Some(relay.public_key()), 1, &[])
            .await
            .unwrap();
        finished(task).await;
        let last = first.last().unwrap();
        assert!(head.has_more);
        assert_eq!(head.next, Some((last.created_at.as_secs(), last.id)));

        let more: Vec<Event> = (page..page + 3)
            .map(|n| membership(&relay, &me, n, 500 - n as u64))
            .collect();
        let (origin, task) = serve(vec![
            info(&relay),
            json(&more),
            json(
                &(page..page + 3)
                    .map(|n| metadata(&relay, n))
                    .collect::<Vec<_>>(),
            ),
        ])
        .await;
        let next =
            crate::catalog::discover_more(&origin, &me, relay.public_key(), head.next.unwrap())
                .await
                .unwrap();
        let filters = finished(task).await;
        assert_eq!(filters[0]["until"], last.created_at.as_secs());
        assert_eq!(filters[0]["before_id"], last.id.to_hex());
        assert_eq!(next.rooms.len(), 3);
        assert!(!next.has_more);
        // The caller's merge keeps the first page's rooms.
        let held: Vec<Room> = head.rooms.into_iter().map(project).collect();
        let merged = merge(&held, next.rooms.into_iter().map(project).collect());
        assert_eq!(merged.len(), page + 3);
    }

    #[tokio::test]
    async fn a_page_newer_than_its_cursor_or_oversized_is_refused() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        let (origin, task) = serve(vec![
            info(&relay),
            json(&[membership(&relay, &me, 1, 5000)]),
        ])
        .await;
        let cursor = (1000, membership(&relay, &me, 2, 1000).id);
        assert_eq!(
            crate::catalog::discover_more(&origin, &me, relay.public_key(), cursor)
                .await
                .err(),
            Some("catalog_invalid_shape")
        );
        finished(task).await;
    }

    #[tokio::test]
    async fn a_listed_room_pushed_past_the_pages_is_confirmed_not_dropped() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        // Room 1 was listed before; the page now holds only room 0. Room 1's
        // own roster still names this identity, room 2's no longer does.
        let page = vec![membership(&relay, &me, 0, 2000)];
        let still = membership(&relay, &me, 1, 100);
        let gone = event(
            &relay,
            39002,
            &[
                row(&["d", &room_uuid(2)]),
                row(&["p", &key(9).public_key().to_hex(), "", "member"]),
            ],
            100,
        );
        let known = vec![room_uuid(0), room_uuid(1), room_uuid(2)];
        let (origin, task) = serve(vec![
            info(&relay),
            json(&page),
            json(&[still, gone]),
            json(&[metadata(&relay, 0), metadata(&relay, 1)]),
        ])
        .await;
        let catalog =
            crate::catalog::discover_pages(&origin, &me, Some(relay.public_key()), 1, &known)
                .await
                .unwrap();
        let ids: Vec<&str> = catalog.rooms.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, [room_uuid(0), room_uuid(1)]);
        let filters = finished(task).await;
        // Only the missing rooms are asked about, by room.
        let asked: Vec<&str> = filters[1]["#d"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(asked, [room_uuid(1), room_uuid(2)]);
        assert_eq!(filters[1]["kinds"][0], 39002);
    }

    #[tokio::test]
    async fn the_same_room_on_two_pages_keeps_its_newer_snapshot() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        let page = crate::catalog::PAGE as usize;
        let mut first: Vec<Event> = (0..page - 1)
            .map(|n| membership(&relay, &me, n, 2000 - n as u64))
            .collect();
        first.push(membership(&relay, &me, 900, 1500));
        // The relay replaced room 900's snapshot between the two reads.
        let second = vec![membership(&relay, &me, 900, 1400)];
        let (origin, task) = serve(vec![
            info(&relay),
            json(&first),
            json(&second),
            json(
                &(0..page - 1)
                    .map(|n| metadata(&relay, n))
                    .chain([metadata(&relay, 900)])
                    .collect::<Vec<_>>(),
            ),
        ])
        .await;
        let catalog =
            crate::catalog::discover_pages(&origin, &me, Some(relay.public_key()), 2, &[])
                .await
                .unwrap();
        assert_eq!(catalog.rooms.len(), page);
        finished(task).await;
    }

    #[tokio::test]
    async fn a_refused_roster_batch_is_asked_room_by_room() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (relay, me) = (key(1), key(2));
        let forbidden = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let known = vec![room_uuid(0), room_uuid(1), room_uuid(2)];
        let (origin, task) = serve(vec![
            info(&relay),
            json(&[membership(&relay, &me, 0, 2000)]),
            // Rooms 1 and 2 together: refused because one is no longer ours.
            forbidden.into(),
            json(&[membership(&relay, &me, 1, 100)]),
            forbidden.into(),
            json(&[metadata(&relay, 0), metadata(&relay, 1)]),
        ])
        .await;
        let catalog =
            crate::catalog::discover_pages(&origin, &me, Some(relay.public_key()), 1, &known)
                .await
                .unwrap();
        let ids: Vec<&str> = catalog.rooms.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, [room_uuid(0), room_uuid(1)]);
        let filters = finished(task).await;
        assert_eq!(filters[1]["#d"].as_array().unwrap().len(), 2);
        assert_eq!(filters[2]["#d"], serde_json::json!([room_uuid(1)]));
        assert_eq!(filters[3]["#d"], serde_json::json!([room_uuid(2)]));
    }
}

fn request(value: serde_json::Value) -> Result<crate::protocol::Request, &'static str> {
    crate::protocol::request(&serde_json::to_vec(&value).unwrap())
}
fn frame(kind: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut value = serde_json::json!({"version":1,"id":REQUEST,"type":kind});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

#[test]
fn room_requests_have_exact_shapes_and_map_to_changes() {
    let member = key(3).public_key().to_hex();
    let ok = |kind: &str, extra: serde_json::Value| {
        crate::protocol::room_change(&request(frame(kind, extra)).unwrap())
    };
    assert_eq!(
        ok(
            "create_room",
            serde_json::json!({"name":"New","about":"Hi","visibility":"private"})
        ),
        Some(Change::Create {
            name: "New".into(),
            about: "Hi".into(),
            private: true
        })
    );
    // About is optional when creating.
    assert_eq!(
        ok(
            "create_room",
            serde_json::json!({"name":"New","visibility":"open"})
        ),
        Some(Change::Create {
            name: "New".into(),
            about: String::new(),
            private: false
        })
    );
    assert_eq!(
        ok(
            "update_room",
            serde_json::json!({"roomId":ROOM,"name":"N","about":""})
        ),
        Some(Change::Details {
            room: room(),
            name: "N".into(),
            about: String::new()
        })
    );
    assert_eq!(
        ok(
            "set_room_topic",
            serde_json::json!({"roomId":ROOM,"topic":"T"})
        ),
        Some(Change::Topic {
            room: room(),
            topic: "T".into()
        })
    );
    assert!(matches!(
        ok(
            "add_room_member",
            serde_json::json!({"roomId":ROOM,"key":member})
        ),
        Some(Change::AddMember { .. })
    ));
    assert!(matches!(
        ok(
            "remove_room_member",
            serde_json::json!({"roomId":ROOM,"key":member})
        ),
        Some(Change::RemoveMember { .. })
    ));
    // Text past the room bounds passes the frame check but never becomes a change.
    assert_eq!(
        ok(
            "create_room",
            serde_json::json!({"name":"n".repeat(NAME_BYTES + 1),"visibility":"open"})
        ),
        None
    );
    assert_eq!(
        ok(
            "set_room_topic",
            serde_json::json!({"roomId":ROOM,"topic":"a\nb"})
        ),
        None
    );

    for bad in [
        frame("create_room", serde_json::json!({"visibility":"open"})),
        frame("create_room", serde_json::json!({"name":"x"})),
        frame(
            "create_room",
            serde_json::json!({"name":"x","visibility":"secret"}),
        ),
        frame(
            "create_room",
            serde_json::json!({"name":"x","visibility":"open","roomId":ROOM}),
        ),
        frame(
            "create_room",
            serde_json::json!({"name":"x","visibility":"open","about":null}),
        ),
        frame(
            "create_room",
            serde_json::json!({"name":"x","visibility":"open","key":member}),
        ),
        frame("update_room", serde_json::json!({"roomId":ROOM,"name":"x"})),
        frame("update_room", serde_json::json!({"name":"x","about":""})),
        frame(
            "update_room",
            serde_json::json!({"roomId":ROOM,"name":"x","about":"","visibility":"open"}),
        ),
        frame("set_room_topic", serde_json::json!({"roomId":ROOM})),
        frame(
            "set_room_topic",
            serde_json::json!({"roomId":ROOM,"topic":"a\u{0}"}),
        ),
        frame(
            "add_room_member",
            serde_json::json!({"roomId":ROOM,"key":member.to_uppercase()}),
        ),
        frame(
            "add_room_member",
            serde_json::json!({"roomId":ROOM,"key":"abc"}),
        ),
        frame("remove_room_member", serde_json::json!({"roomId":ROOM})),
        frame("join_room", serde_json::json!({"roomId":ROOM,"topic":"x"})),
        frame("load_more_rooms", serde_json::json!({"roomId":ROOM})),
        frame("refresh_rooms", serde_json::json!({"key":member})),
        // Publishing requests carry a correlation UUID.
        serde_json::json!({"version":1,"id":"plain","type":"set_room_topic","roomId":ROOM,"topic":"x"}),
    ] {
        assert!(request(bad.clone()).is_err(), "{bad}");
    }
    assert!(request(frame(
        "fetch_room_detail",
        serde_json::json!({"roomId":ROOM})
    ))
    .is_ok());
    assert!(request(frame("fetch_room_detail", serde_json::json!({}))).is_err());
    assert!(request(frame("load_more_rooms", serde_json::json!({}))).is_ok());
    assert!(request(frame("refresh_rooms", serde_json::json!({}))).is_ok());
}
