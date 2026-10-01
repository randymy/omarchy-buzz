use super::*;
use nostr::{EventBuilder, Kind, Tag};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn event(author: &Keys, kind: u16, tags: Vec<Vec<String>>, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), "")
        .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn tags(values: &[&[&str]]) -> Vec<Vec<String>> {
    values
        .iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect()
}
const ID: &str = "11111111-1111-4111-8111-111111111111";
fn membership(author: &Keys, member: &Keys) -> Event {
    event(
        author,
        39002,
        tags(&[
            &["d", ID],
            &["p", &member.public_key().to_hex(), "", "member"],
        ]),
        100,
    )
}
fn metadata(author: &Keys, kind: &str) -> Event {
    event(
        author,
        39000,
        tags(&[
            &["d", ID],
            &["name", "Lobby"],
            &["t", kind],
            &["about", "Welcome"],
        ]),
        100,
    )
}

#[test]
fn nip11_self_is_authority_and_rotation_is_refused() {
    let relay = key(1).public_key();
    let contact = key(2).public_key();
    let document =
        serde_json::to_vec(&serde_json::json!({"self":relay.to_hex(),"pubkey":contact.to_hex()}))
            .unwrap();
    assert_eq!(info_signer(&document, Some(relay)).unwrap(), relay);
    assert_eq!(
        info_signer(&document, Some(contact)).unwrap_err(),
        "relay_identity_changed"
    );
    assert_eq!(
        info_signer(br#"{"pubkey":"ignored"}"#, None).unwrap_err(),
        "discovery_signer_unavailable"
    );
    assert_eq!(
        info_signer(&vec![b' '; INFO_BYTES + 1], None).unwrap_err(),
        "discovery_oversized"
    );
}
#[test]
fn presence_signer_is_the_nip11_self_key_and_absent_means_unavailable() {
    // `presence::verify` checks snapshot signatures against the pinned key
    // this returns. A document with only an operator `pubkey` (even a valid
    // key), or a malformed `self`, yields no signer: presence reads (like
    // every relay-signed read) are unavailable, never trusted.
    let relay = key(1).public_key();
    for document in [
        serde_json::json!({"pubkey": relay.to_hex()}),
        serde_json::json!({"self": null, "pubkey": relay.to_hex()}),
        serde_json::json!({"self": relay.to_hex()[..63]}),
        serde_json::json!({"self": 7}),
        serde_json::json!({}),
    ] {
        assert_eq!(
            info_signer(&serde_json::to_vec(&document).unwrap(), None).unwrap_err(),
            "discovery_signer_unavailable",
            "{document}"
        );
    }
    let document = serde_json::json!({"self": relay.to_hex()});
    assert_eq!(
        info_signer(&serde_json::to_vec(&document).unwrap(), None).unwrap(),
        relay
    );
}
#[test]
fn inline_icon_metadata_is_bounded_and_does_not_change_authority() {
    let relay = key(1).public_key();
    let other = key(2).public_key();
    let mut document = serde_json::json!({"self": relay.to_hex(), "icon": ""});
    let overhead = serde_json::to_vec(&document).unwrap().len();
    document["icon"] = serde_json::Value::String("x".repeat(INFO_BYTES - overhead));
    let bytes = serde_json::to_vec(&document).unwrap();
    assert_eq!(bytes.len(), INFO_BYTES);
    assert_eq!(info_signer(&bytes, Some(relay)).unwrap(), relay);
    assert_eq!(
        info_signer(&bytes, Some(other)).unwrap_err(),
        "relay_identity_changed"
    );
    document["icon"] = serde_json::Value::String("x".repeat(INFO_BYTES - overhead + 1));
    assert_eq!(
        info_signer(&serde_json::to_vec(&document).unwrap(), None).unwrap_err(),
        "discovery_oversized"
    );
}

#[test]
fn valid_snapshot_partial_missing_metadata_and_unsupported_are_honest() {
    let relay = key(1);
    let member = key(2);
    let m = membership(&relay, &member);
    let c = reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[metadata(&relay, "stream")],
        1000,
    )
    .unwrap();
    assert_eq!(c.state, "partial");
    assert_eq!(c.category, "room_catalog_partial");
    assert_eq!(c.rooms.len(), 1);
    assert_eq!(c.rooms[0].name, "Lobby");
    assert_eq!(c.rooms[0].description, "Welcome");
    assert!(reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[],
        1000
    )
    .unwrap()
    .rooms
    .is_empty());
    assert!(reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[metadata(&relay, "forum")],
        1000
    )
    .unwrap()
    .rooms
    .is_empty());
    let empty = reconcile(member.public_key(), relay.public_key(), &[], &[], 1000).unwrap();
    assert_eq!(empty.state, "partial");
}
#[test]
fn valid_signature_does_not_grant_membership_authority() {
    let relay = key(1);
    let member = key(2);
    let attacker = key(3);
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&attacker, &member)],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_untrusted_author"
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&relay, &attacker)],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_membership"
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&relay, &member)],
            &[metadata(&attacker, "stream")],
            1000
        )
        .unwrap_err(),
        "catalog_untrusted_author"
    );
}
#[test]
fn conflicts_duplicate_identifier_future_and_bad_signature_fail_closed() {
    let relay = key(1);
    let member = key(2);
    let good = membership(&relay, &member);
    let duplicate = event(
        &relay,
        39002,
        tags(&[
            &["d", ID],
            &["d", ID],
            &["p", &member.public_key().to_hex()],
        ]),
        100,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[duplicate],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_shape"
    );
    let later = event(
        &relay,
        39002,
        tags(&[&["d", ID], &["p", &member.public_key().to_hex()]]),
        101,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[good.clone(), later],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_conflicting_snapshot"
    );
    let future = event(
        &relay,
        39002,
        tags(&[&["d", ID], &["p", &member.public_key().to_hex()]]),
        1061,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[future],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_shape"
    );
    let mut tampered = good;
    tampered.content = "tampered".into();
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[tampered],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_signature"
    );
}
#[test]
fn presentation_text_is_byte_bounded_and_controls_are_removed() {
    let relay = key(1);
    let member = key(2);
    let long = "é\n".repeat(600);
    let data = event(
        &relay,
        39000,
        tags(&[
            &["d", ID],
            &["name", &long],
            &["t", "stream"],
            &["about", &long],
        ]),
        100,
    );
    let c = reconcile(
        member.public_key(),
        relay.public_key(),
        &[membership(&relay, &member)],
        &[data],
        1000,
    )
    .unwrap();
    assert!(c.rooms[0].name.len() <= 128);
    assert!(c.rooms[0].description.len() <= 512);
    assert!(!c.rooms[0].description.chars().any(char::is_control));
}
#[tokio::test]
async fn loopback_info_uses_info_without_credentials_and_rejects_redirect() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let relay = key(1).public_key();
    for redirect in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let payload =
            serde_json::json!({"self":relay.to_hex(), "icon": "x".repeat(36 * 1024)}).to_string();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            loop {
                let mut byte = [0; 1];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                head.push(byte[0]);
                assert!(head.len() < 8192);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            assert!(head.starts_with("get /info "));
            assert!(head.contains("accept: application/nostr+json"));
            assert!(!head.contains("authorization:"));
            let response = if redirect {
                "HTTP/1.1 302 Found\r\nLocation: https://example.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else {
                // The same response's `Date` measures the clock offset.
                format!(
                    "HTTP/1.1 200 OK\r\nDate: Thu, 01 Jan 1970 00:00:00 GMT\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let (result, skew) = relay_info(&origin, Some(relay)).await;
        if redirect {
            assert_eq!(result.unwrap_err(), "discovery_redirect_rejected");
            assert_eq!(skew, None);
        } else {
            assert_eq!(result.unwrap(), relay);
            assert_eq!(skew, Some(-crate::clock::BOUND));
        }
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn complete_loopback_discovery_and_trust_failures() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let relay = key(1);
    let member = key(2);
    for mode in ["valid", "rotation", "contact_only"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let info = if mode == "contact_only" {
            serde_json::json!({"pubkey":relay.public_key().to_hex()})
        } else {
            serde_json::json!({"self":relay.public_key().to_hex()})
        };
        let responses = vec![
            info.to_string(),
            serde_json::to_string(&vec![membership(&relay, &member)]).unwrap(),
            serde_json::to_string(&vec![metadata(&relay, "stream")]).unwrap(),
        ];
        let requests = if mode == "valid" { 3 } else { 1 };
        let task = tokio::spawn(async move {
            for (index, payload) in responses.into_iter().take(requests).enumerate() {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                loop {
                    let mut byte = [0; 1];
                    assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                    head.push(byte[0]);
                    assert!(head.len() < 16384);
                    if head.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                if index == 0 {
                    assert!(head.starts_with("get /info "));
                } else {
                    assert!(head.starts_with("post /query "));
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    assert!(length < 8192);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let filter: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        filter[0]["kinds"][0],
                        if index == 1 { 39002 } else { 39000 }
                    );
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            if requests == 1 {
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err(),
                    "trust failure still queried private data"
                );
            }
        });
        let pin = Some(if mode == "rotation" {
            key(3).public_key()
        } else {
            relay.public_key()
        });
        let result = discover(&origin, &member, pin).await;
        match mode {
            "valid" => {
                let catalog = result.unwrap();
                assert_eq!(catalog.signer, relay.public_key());
                assert_eq!(catalog.trust, "loopback_development");
                assert_eq!(catalog.state, "partial");
                assert_eq!(catalog.rooms.len(), 1);
                assert_eq!(catalog.rooms[0].id, ID);
            }
            "rotation" => assert_eq!(result.unwrap_err(), "relay_identity_changed"),
            _ => assert_eq!(result.unwrap_err(), "discovery_signer_unavailable"),
        }
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
    }
}

const DM: &str = "22222222-2222-4222-8222-222222222222";
fn joined(author: &Keys, member: &Keys, id: &str) -> Event {
    event(
        author,
        39002,
        tags(&[&["d", id], &["p", &member.public_key().to_hex()]]),
        100,
    )
}
/// Shape of pinned Buzz's DM kind 39000 (side_effects.rs:1200-1240).
fn dm_metadata(author: &Keys, id: &str, people: &[String], extra: &[&[&str]]) -> Event {
    let mut rows = tags(&[
        &["d", id],
        &["name", "DM"],
        &["private"],
        &["hidden"],
        &["closed"],
        &["t", "dm"],
    ]);
    rows.extend(people.iter().map(|p| vec!["p".to_string(), p.clone()]));
    rows.extend(tags(extra));
    event(author, 39000, rows, 100)
}
fn hexes(keys: &[&Keys]) -> Vec<String> {
    keys.iter().map(|k| k.public_key().to_hex()).collect()
}
fn prefix(k: &Keys) -> String {
    format!("{}…", &k.public_key().to_hex()[..12])
}
fn dm_catalog(relay: &Keys, member: &Keys, people: &[String]) -> Result<Catalog, &'static str> {
    reconcile(
        member.public_key(),
        relay.public_key(),
        &[joined(relay, member, ID), joined(relay, member, DM)],
        &[
            metadata(relay, "stream"),
            dm_metadata(relay, DM, people, &[]),
        ],
        1000,
    )
}
fn snapshot(author: &Keys, rows: &[&[&str]], content: &str) -> Event {
    EventBuilder::new(Kind::Custom(30622), content)
        .tags(tags(rows).into_iter().map(|t| Tag::parse(t).unwrap()))
        .custom_created_at(Timestamp::from(100))
        .sign_with_keys(author)
        .unwrap()
}
fn profile(author: &Keys, content: &str, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(0), content)
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}

#[test]
fn dm_is_admitted_with_participants_and_streams_are_unchanged() {
    let (relay, member, other) = (key(1), key(2), key(3));
    let c = dm_catalog(&relay, &member, &hexes(&[&other, &member])).unwrap();
    assert_eq!(c.rooms.len(), 2);
    let stream = c.rooms.iter().find(|r| r.id == ID).unwrap();
    assert_eq!(
        (
            stream.kind,
            stream.name.as_str(),
            stream.description.as_str()
        ),
        ("stream", "Lobby", "Welcome")
    );
    assert!(stream.participants.is_empty() && !stream.hidden);
    let dm = c.rooms.iter().find(|r| r.id == DM).unwrap();
    assert_eq!(dm.kind, "dm");
    // The upstream hidden hint is ignored for DMs; sorted keys, self included.
    assert!(!dm.hidden);
    let mut sorted = hexes(&[&member, &other]);
    sorted.sort();
    assert_eq!(dm.participants, sorted);
    // The relay's literal "DM" name never reaches the projection.
    assert_eq!(dm.name, prefix(&other));
}
#[test]
fn group_dm_names_three_others_then_a_count() {
    let relay = key(1);
    let member = key(2);
    let others: Vec<Keys> = (3..8).map(key).collect();
    let mut people = hexes(&others.iter().collect::<Vec<_>>());
    people.push(member.public_key().to_hex());
    let mut c = dm_catalog(&relay, &member, &people).unwrap();
    let dm = c.rooms.iter().find(|r| r.id == DM).unwrap();
    assert_eq!(dm.participants.len(), 6);
    let mut ordered = others.clone();
    ordered.sort_by_key(|k| k.public_key().to_hex());
    assert_eq!(
        dm.name,
        format!(
            "{}, {}, {}, +2",
            prefix(&ordered[0]),
            prefix(&ordered[1]),
            prefix(&ordered[2])
        )
    );
    let names = BTreeMap::from([(ordered[1].public_key().to_hex(), "Bea".to_string())]);
    apply_names(&mut c, member.public_key(), &names);
    let dm = c.rooms.iter().find(|r| r.id == DM).unwrap();
    assert_eq!(
        dm.name,
        format!("{}, Bea, {}, +2", prefix(&ordered[0]), prefix(&ordered[2]))
    );
}
#[test]
fn archived_dm_and_hidden_stream_are_dropped() {
    let (relay, member, other) = (key(1), key(2), key(3));
    let archived = dm_metadata(&relay, DM, &hexes(&[&member, &other]), &[&["archived"]]);
    let hidden_stream = event(
        &relay,
        39000,
        tags(&[
            &["d", ID],
            &["name", "Lobby"],
            &["t", "stream"],
            &["hidden"],
        ]),
        100,
    );
    let c = reconcile(
        member.public_key(),
        relay.public_key(),
        &[joined(&relay, &member, ID), joined(&relay, &member, DM)],
        &[hidden_stream, archived],
        1000,
    )
    .unwrap();
    assert!(c.rooms.is_empty());
}
#[test]
fn dm_participant_sets_outside_upstream_bounds_reject_the_catalog() {
    let (relay, member, other) = (key(1), key(2), key(3));
    let own = member.public_key().to_hex();
    let ten: Vec<String> = (3..12)
        .map(|n| key(n).public_key().to_hex())
        .chain([own.clone()])
        .collect();
    let bad: Vec<Vec<String>> = vec![
        vec![own.clone()],                                             // self only
        ten,                                                           // ten participants
        vec![own.clone(), own.clone()],                                // repeated key
        hexes(&[&other, &key(4)]),                                     // viewer absent
        vec![own.clone(), other.public_key().to_hex().to_uppercase()], // not canonical
        vec![own.clone(), "zz".repeat(32)],                            // not hex
    ];
    for people in bad {
        assert_eq!(
            dm_catalog(&relay, &member, &people).unwrap_err(),
            "catalog_invalid_shape",
            "{people:?}"
        );
    }
    let nine: Vec<String> = (3..11)
        .map(|n| key(n).public_key().to_hex())
        .chain([own])
        .collect();
    assert_eq!(
        dm_catalog(&relay, &member, &nine).unwrap().rooms[1]
            .participants
            .len(),
        9
    );
}
#[test]
fn signed_visibility_snapshot_marks_only_listed_dms() {
    let (relay, member, other) = (key(1), key(2), key(3));
    let own = member.public_key().to_hex();
    let people = hexes(&[&member, &other]);
    let mut c = dm_catalog(&relay, &member, &people).unwrap();
    apply_visibility(&mut c, member.public_key(), &[], 1000).unwrap();
    assert!(
        c.rooms.iter().all(|r| !r.hidden),
        "absent snapshot hides nothing"
    );
    let valid = snapshot(
        &relay,
        &[
            &["d", &own],
            &["p", &own],
            &["h", DM],
            &["h", DM],
            &["h", ID],
        ],
        "",
    );
    apply_visibility(&mut c, member.public_key(), &[valid], 1000).unwrap();
    assert!(c.rooms.iter().find(|r| r.id == DM).unwrap().hidden);
    // Non-DM channels are never affected (NIP-DV.md:108).
    assert!(!c.rooms.iter().find(|r| r.id == ID).unwrap().hidden);
    assert_eq!(c.rooms.len(), 2, "hidden DMs stay in the catalog");
    let empty = snapshot(&relay, &[&["d", &own], &["p", &own]], "");
    apply_visibility(&mut c, member.public_key(), &[empty], 1000).unwrap();
    assert!(c.rooms.iter().all(|r| !r.hidden));
}
#[test]
fn wrong_signer_or_malformed_snapshot_rejects_the_catalog() {
    let (relay, member, other) = (key(1), key(2), key(3));
    let own = member.public_key().to_hex();
    let theirs = other.public_key().to_hex();
    let good: &[&[&str]] = &[&["d", &own], &["p", &own], &["h", DM]];
    let mut tampered = snapshot(&relay, good, "");
    tampered.created_at = Timestamp::from(101);
    let bad = vec![
        vec![snapshot(&other, good, "")],
        vec![snapshot(&relay, good, "note")],
        vec![snapshot(&relay, good, ""), snapshot(&relay, good, "")],
        vec![tampered],
        vec![snapshot(&relay, &[&["p", &own], &["h", DM]], "")],
        vec![snapshot(&relay, &[&["d", &own], &["h", DM]], "")],
        vec![snapshot(&relay, &[&["d", &theirs], &["p", &own]], "")],
        vec![snapshot(
            &relay,
            &[&["d", &own], &["p", &own], &["p", &own]],
            "",
        )],
        vec![snapshot(
            &relay,
            &[&["d", &own], &["p", &own], &["h", "room"]],
            "",
        )],
        vec![snapshot(
            &relay,
            &[
                &["d", &own],
                &["p", &own],
                &["h", "AAAAAAAA-BBBB-4CCC-8DDD-EEEEEEEEEEEE"],
            ],
            "",
        )],
        vec![snapshot(
            &relay,
            &[&["d", &own], &["p", &own], &["h", DM, "x"]],
            "",
        )],
        vec![snapshot(
            &relay,
            &[&["d", &own], &["p", &own], &["t", "x"]],
            "",
        )],
        vec![event(
            &relay,
            30622,
            tags(&[&["d", &own], &["p", &own]]),
            2000,
        )],
    ];
    for (index, events) in bad.into_iter().enumerate() {
        let mut c = dm_catalog(&relay, &member, &hexes(&[&member, &other])).unwrap();
        assert_eq!(
            apply_visibility(&mut c, member.public_key(), &events, 1000),
            Err("catalog_invalid_shape"),
            "case {index}"
        );
    }
}
#[test]
fn dm_names_use_present_profiles_and_fall_back_for_missing_or_blank() {
    let (relay, member) = (key(1), key(2));
    let (named, blank, missing, bidi) = (key(3), key(4), key(5), key(6));
    let wanted: BTreeSet<String> = hexes(&[&named, &blank, &missing, &bidi])
        .into_iter()
        .collect();
    let events = vec![
        profile(&named, r#"{"name":"Old"}"#, 100),
        profile(&named, r#"{"display_name":"Ada","name":"ada"}"#, 200),
        profile(&blank, r#"{"name":"   "}"#, 100),
        profile(&bidi, "{\"name\":\"Eve\u{202e}\\n\"}", 100),
    ];
    let names = profile_names(&wanted, &events, 1000);
    assert_eq!(names.len(), 2);
    for (other, expected) in [
        (&named, "Ada".to_string()),
        (&blank, prefix(&blank)),
        (&missing, prefix(&missing)),
        (&bidi, "Eve".to_string()),
    ] {
        let mut c = dm_catalog(&relay, &member, &hexes(&[&member, other])).unwrap();
        apply_names(&mut c, member.public_key(), &names);
        let dm = c.rooms.iter().find(|r| r.id == DM).unwrap();
        assert_eq!(dm.name, expected);
        assert_eq!(c.rooms.iter().find(|r| r.id == ID).unwrap().name, "Lobby");
    }
    // One unrequested author or future event discards the batch's names.
    let stranger = profile(&key(9), r#"{"name":"X"}"#, 100);
    assert!(profile_names(&wanted, &[events[1].clone(), stranger], 1000).is_empty());
    let future = profile(&named, r#"{"name":"Later"}"#, 1061);
    assert!(profile_names(&wanted, &[events[1].clone(), future], 1000).is_empty());
    // A same-second conflict leaves that author unnamed.
    let rival = profile(&named, r#"{"name":"Rival"}"#, 200);
    assert!(profile_names(&wanted, &[events[1].clone(), rival], 1000).is_empty());
}

#[tokio::test]
async fn loopback_discovery_marks_hidden_dms_and_names_them() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let (relay, member, other) = (key(1), key(2), key(3));
    let own = member.public_key().to_hex();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let responses = vec![
        serde_json::json!({"self":relay.public_key().to_hex()}).to_string(),
        serde_json::to_string(&vec![
            joined(&relay, &member, ID),
            joined(&relay, &member, DM),
        ])
        .unwrap(),
        serde_json::to_string(&vec![
            metadata(&relay, "stream"),
            dm_metadata(&relay, DM, &hexes(&[&member, &other]), &[]),
        ])
        .unwrap(),
        serde_json::to_string(&vec![snapshot(
            &relay,
            &[&["d", &own], &["p", &own], &["h", DM]],
            "",
        )])
        .unwrap(),
        serde_json::to_string(&vec![profile(&other, r#"{"name":"Otto"}"#, 100)]).unwrap(),
    ];
    let task = tokio::spawn(async move {
        let mut kinds = Vec::new();
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
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            if index > 0 {
                let length = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let filter: serde_json::Value = serde_json::from_slice(&body).unwrap();
                kinds.push(filter[0].clone());
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        kinds
    });
    let catalog = discover(&origin, &member, Some(relay.public_key()))
        .await
        .unwrap();
    let filters = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        filters[2],
        serde_json::json!({"kinds":[30622],"#p":[own],"limit":1})
    );
    assert_eq!(
        filters[3],
        serde_json::json!({"kinds":[0],"authors":[other.public_key().to_hex()],"limit":1})
    );
    let dm = catalog.rooms.iter().find(|r| r.id == DM).unwrap();
    assert_eq!((dm.kind, dm.name.as_str(), dm.hidden), ("dm", "Otto", true));
    assert_eq!(catalog.rooms.len(), 2);
}

fn open_meta(author: &Keys, id: &str, name: &str, extra: &[&[&str]]) -> Event {
    let (d, n) = (["d", id], ["name", name]);
    let mut rows: Vec<&[&str]> = vec![&d, &n, &["closed"]];
    rows.extend_from_slice(extra);
    event(author, 39000, tags(&rows), 100)
}

#[test]
fn open_rooms_keep_only_signed_public_unjoined_streams() {
    let relay = key(1);
    let joined_id = "22222222-2222-4222-8222-222222222222";
    let events = vec![
        open_meta(
            &relay,
            ID,
            "zeta",
            &[&["public"], &["t", "stream"], &["about", "Z\u{7}"]],
        ),
        open_meta(
            &relay,
            "33333333-3333-4333-8333-333333333333",
            "Alpha",
            &[&["public"], &["t", "stream"]],
        ),
        // Joined already, private, a DM, a forum, archived: all left out.
        open_meta(
            &relay,
            joined_id,
            "joined",
            &[&["public"], &["t", "stream"]],
        ),
        open_meta(
            &relay,
            "44444444-4444-4444-8444-444444444444",
            "secret",
            &[&["private"], &["t", "stream"]],
        ),
        open_meta(
            &relay,
            "55555555-5555-4555-8555-555555555555",
            "DM",
            &[&["public"], &["hidden"], &["t", "dm"]],
        ),
        open_meta(
            &relay,
            "66666666-6666-4666-8666-666666666666",
            "forum",
            &[&["public"], &["t", "forum"]],
        ),
        open_meta(
            &relay,
            "77777777-7777-4777-8777-777777777777",
            "old",
            &[&["public"], &["t", "stream"], &["archived", "true"]],
        ),
    ];
    let joined: BTreeSet<String> = [joined_id.to_string()].into();
    let rooms = open_rooms(relay.public_key(), &events, &joined, 1000).unwrap();
    let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "zeta"]);
    assert_eq!(rooms[1].description, "Z ");
    assert!(rooms.iter().all(|r| r.kind == "stream"));
    // Fifty at most, by name.
    let many: Vec<Event> = (0..60u32)
        .map(|n| {
            let id = format!("00000000-0000-4000-8000-{n:012}");
            open_meta(
                &relay,
                &id,
                &format!("room {n:02}"),
                &[&["public"], &["t", "stream"]],
            )
        })
        .collect();
    let rooms = open_rooms(relay.public_key(), &many, &BTreeSet::new(), 1000).unwrap();
    assert_eq!(rooms.len(), OPEN_ROOMS);
    assert_eq!(rooms[0].name, "room 00");
}

#[test]
fn open_rooms_reject_the_whole_page_on_any_unexpected_value() {
    let relay = key(1);
    let other = key(2);
    let good = open_meta(&relay, ID, "Lobby", &[&["public"], &["t", "stream"]]);
    let two = "3333333c-3333-4333-8333-33333333333d";
    let cases: Vec<(Vec<Event>, &str)> = vec![
        // Not the pinned relay signer.
        (
            vec![
                good.clone(),
                open_meta(&other, two, "x", &[&["public"], &["t", "stream"]]),
            ],
            "catalog_untrusted_author",
        ),
        // No visibility, both, or a valued visibility tag.
        (
            vec![open_meta(&relay, two, "x", &[&["t", "stream"]])],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                two,
                "x",
                &[&["public"], &["private"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                two,
                "x",
                &[&["public", "yes"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                two,
                "x",
                &[&["public"], &["public"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        // Doubled name, bad kind, blank name, non-canonical id, future event.
        (
            vec![open_meta(
                &relay,
                two,
                "x",
                &[&["public"], &["name", "y"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                two,
                "x",
                &[&["public"], &["t", "str eam"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                two,
                " ",
                &[&["public"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![open_meta(
                &relay,
                &two.to_uppercase(),
                "x",
                &[&["public"], &["t", "stream"]],
            )],
            "catalog_invalid_shape",
        ),
        (
            vec![event(
                &relay,
                39000,
                tags(&[&["d", two], &["name", "x"], &["public"], &["t", "stream"]]),
                5000,
            )],
            "catalog_invalid_shape",
        ),
        // Wrong kind, content, and a conflicting second version of one room.
        (
            vec![event(&relay, 39002, tags(&[&["d", two]]), 100)],
            "catalog_invalid_shape",
        ),
        (
            vec![
                good.clone(),
                open_meta(&relay, ID, "Lobby 2", &[&["public"], &["t", "stream"]]),
            ],
            "catalog_conflicting_snapshot",
        ),
    ];
    for (events, expected) in cases {
        assert_eq!(
            open_rooms(relay.public_key(), &events, &BTreeSet::new(), 1000).unwrap_err(),
            expected
        );
    }
    let content = EventBuilder::new(Kind::Custom(39000), "x")
        .tags(
            tags(&[&["d", two], &["name", "x"], &["public"], &["t", "stream"]])
                .into_iter()
                .map(|t| Tag::parse(t).unwrap()),
        )
        .custom_created_at(Timestamp::from(100))
        .sign_with_keys(&relay)
        .unwrap();
    assert_eq!(
        open_rooms(relay.public_key(), &[content], &BTreeSet::new(), 1000).unwrap_err(),
        "catalog_invalid_shape"
    );
    // The same event twice is one room.
    assert_eq!(
        open_rooms(
            relay.public_key(),
            &[good.clone(), good],
            &BTreeSet::new(),
            1000
        )
        .unwrap()
        .len(),
        1
    );
}
