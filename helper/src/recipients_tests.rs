use super::*;
use nostr::{EventBuilder, Kind, Tag};
fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn room() -> Uuid {
    Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
}
fn event(author: &Keys, kind: u16, content: &str, tags: Vec<Tag>, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags)
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn membership(relay: &Keys, members: &[PublicKey]) -> Event {
    let mut tags = vec![Tag::parse(["d", &room().to_string()]).unwrap()];
    tags.extend(
        members
            .iter()
            .map(|p| Tag::parse(["p", &p.to_hex(), "", "member"]).unwrap()),
    );
    event(relay, 39002, "", tags, 100)
}
fn profile(user: &Keys, body: &str, at: u64) -> Event {
    event(user, 0, body, vec![], at)
}
#[test]
fn signed_ordinary_identity_cannot_forge_the_authoritative_roster() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    assert!(roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[membership(&other, &[user.public_key()])],
        200
    )
    .is_err());
    assert_eq!(
        roster(
            room(),
            user.public_key(),
            relay.public_key(),
            &[membership(&relay, &[other.public_key()])],
            200
        )
        .unwrap_err(),
        "recipients_access_denied"
    );
    let mut tampered = membership(&relay, &[user.public_key()]);
    tampered.content = "tampered".into();
    assert!(roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[tampered],
        200
    )
    .is_err());
}
#[test]
fn malformed_duplicate_wrong_room_or_ambiguous_rosters_are_rejected() {
    let relay = key(1);
    let user = key(2);
    let good = membership(&relay, &[user.public_key()]);
    assert!(roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[good.clone(), good.clone()],
        200
    )
    .is_err());
    assert!(roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[membership(&relay, &[user.public_key(), user.public_key()])],
        200
    )
    .is_err());
    let bad = event(
        &relay,
        39002,
        "",
        vec![
            Tag::parse(["d", &room().to_string()]).unwrap(),
            Tag::parse(["p", "not-a-key"]).unwrap(),
        ],
        100,
    );
    assert!(roster(room(), user.public_key(), relay.public_key(), &[bad], 200).is_err());
    assert!(roster(
        Uuid::new_v4(),
        user.public_key(),
        relay.public_key(),
        &[good],
        200
    )
    .is_err());
}
#[test]
fn entire_roster_is_validated_before_the_sorted_bounded_subset() {
    let relay = key(1);
    let users: Vec<PublicKey> = (2..=25).map(|n| key(n).public_key()).collect();
    let result = roster(
        room(),
        users[0],
        relay.public_key(),
        &[membership(&relay, &users)],
        200,
    )
    .unwrap();
    assert!(result.partial);
    assert_eq!(result.entries.len(), 20);
    assert!(result.entries.windows(2).all(|r| r[0].key < r[1].key));
    assert!(result.entries.iter().all(|r| r.name.is_empty()));
    assert!(result.agents.is_empty());
}
#[test]
fn profiles_are_self_asserted_and_conflicts_or_invalid_names_fall_back() {
    let relay = key(1);
    let user = key(2);
    let m = membership(&relay, &[user.public_key()]);
    let mut result = roster(room(), user.public_key(), relay.public_key(), &[m], 200).unwrap();
    profiles(
        &mut result,
        &[
            profile(&user, r#"{"name":"old"}"#, 100),
            profile(
                &user,
                r#"{"name":"ignored","display_name":"Preferred"}"#,
                150,
            ),
        ],
        200,
    );
    assert_eq!(result.entries[0].name, "Preferred");
    profiles(
        &mut result,
        &[
            profile(&user, r#"{"name":"A"}"#, 150),
            profile(&user, r#"{"name":"B"}"#, 150),
        ],
        200,
    );
    assert!(result.entries[0].name.is_empty());
    for body in [
        "[]",
        "invalid",
        r#"{"name":5}"#,
        r#"{"picture":"https://example.invalid/avatar","nip05":"somebody@example.invalid"}"#,
    ] {
        profiles(&mut result, &[profile(&user, body, 150)], 200);
        assert!(result.entries[0].name.is_empty());
    }
    let long = serde_json::json!({"display_name":"é\n\0".repeat(200)}).to_string();
    profiles(&mut result, &[profile(&user, &long, 150)], 200);
    assert!(result.entries[0].name.len() <= 64);
    assert!(!result.entries[0].name.chars().any(char::is_control));
}
#[test]
fn profile_names_cannot_reorder_the_adjacent_identity_key() {
    let relay = key(1);
    let user = key(2);
    let mut result = roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[membership(&relay, &[user.public_key()])],
        200,
    )
    .unwrap();
    let hostile = "A\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}B 👩\u{200d}💻";
    let body = serde_json::json!({"display_name": hostile}).to_string();
    profiles(&mut result, &[profile(&user, &body, 150)], 200);
    assert_eq!(result.entries[0].name, "A            B 👩\u{200d}💻");
    assert_eq!(result.entries[0].key, user.public_key().to_hex());
    assert!(result.entries[0].name.len() <= 64);
}
#[test]
fn profile_scope_signature_and_future_failure_preserves_only_verified_keys() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let mut result = roster(
        room(),
        user.public_key(),
        relay.public_key(),
        &[membership(&relay, &[user.public_key()])],
        200,
    )
    .unwrap();
    let mut tampered = profile(&user, r#"{"name":"secret bogus name"}"#, 150);
    tampered.content = "invalid".into();
    for bad in [
        tampered,
        profile(&other, r#"{"name":"wrong author"}"#, 150),
        profile(&user, r#"{"name":"future"}"#, 261),
    ] {
        profiles(
            &mut result,
            &[
                profile(&user, r#"{"name":"valid but page rejected"}"#, 100),
                bad,
            ],
            200,
        );
        assert_eq!(result.entries[0].key, user.public_key().to_hex());
        assert!(result.entries[0].name.is_empty());
    }
}
#[tokio::test]
async fn loopback_fetch_uses_fixed_roster_and_profile_queries_and_falls_back_on_profile_failure() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{timeout, Duration},
    };
    let relay = key(1);
    let user = key(2);
    let payload = serde_json::to_string(&vec![membership(&relay, &[user.public_key()])]).unwrap();
    let agent_payload = serde_json::to_string(&vec![event(
        &user,
        10100,
        r#"{"name":"Synthetic agent","status":"online","pubkey":"forged"}"#,
        vec![],
        100,
    )])
    .unwrap();
    let now = Timestamp::now().as_secs();
    let expires = (now + 3600).to_string();
    let status_payload = serde_json::to_string(&vec![event(
        &user,
        30315,
        "In a \u{202e}meeting",
        vec![
            Tag::parse(["d", "general"]).unwrap(),
            Tag::parse(["emoji", "🗣️"]).unwrap(),
            Tag::parse(["expiration", expires.as_str()]).unwrap(),
        ],
        now - 5,
    )])
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let public = user.public_key();
    let task = tokio::spawn(async move {
        timeout(Duration::from_secs(3), async move {
            for index in 0..4 {
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
                assert!(head.starts_with("post /query "));
                assert!(head.contains("authorization: nostr "));
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
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(
                    body,
                    if index == 0 {
                        serde_json::json!([{"kinds":[39002],"#d":[room().to_string()],"limit":1}])
                    } else if index == 1 {
                        serde_json::json!([{"kinds":[0],"authors":[public.to_hex()],"limit":1}])
                    } else if index == 2 {
                        serde_json::json!([{"kinds":[10100],"authors":[public.to_hex()],"limit":1}])
                    } else {
                        serde_json::json!([{"kinds":[30315],"authors":[public.to_hex()],"#d":["general"],"limit":1}])
                    }
                );
                let response = if index == 0 {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    )
                } else if index >= 2 {
                    let body = if index == 2 { &agent_payload } else { &status_payload };
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else {
                    "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .into()
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        })
        .await
        .unwrap();
    });
    let result = timeout(
        Duration::from_secs(4),
        fetch(&origin, &user, relay.public_key(), room()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].key, public.to_hex());
    assert!(result.entries[0].name.is_empty());
    assert_eq!(result.agents.len(), 1);
    assert_eq!(result.agents[0].key, public.to_hex());
    assert_eq!(result.agents[0].name, "Synthetic agent");
    assert_eq!(result.agents[0].execution_state, "unknown");
    // The batched status read: verified, sanitized, with its expiry.
    assert!(result.statuses_known);
    assert_eq!(
        result.entries[0].status,
        Some(UserStatus {
            text: "In a  meeting".into(),
            emoji: Some("🗣️".into()),
            expires_at: Some(now + 3600)
        })
    );
    assert!(result.partial);
    task.await.unwrap();
}

#[tokio::test]
async fn optional_profile_access_denial_preserves_the_verified_roster() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{timeout, Duration},
    };
    let relay = key(1);
    let user = key(2);
    let roster_body =
        serde_json::to_string(&vec![membership(&relay, &[user.public_key()])]).unwrap();
    for denied_kind in [0, 10100, 30315] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let roster_body = roster_body.clone();
        let task = tokio::spawn(async move {
            timeout(Duration::from_secs(3), async move {
                for index in 0..4 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    loop {
                        let mut byte = [0; 1];
                        assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                        head.push(byte[0]);
                        if head.ends_with(b"\r\n\r\n") {
                            break;
                        }
                        assert!(head.len() < 16384);
                    }
                    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                    let length = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    assert!(length < 8192);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        request[0]["kinds"][0],
                        serde_json::Value::from([39002, 0, 10100, 30315][index])
                    );
                    let reply = if index == 0 {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            roster_body.len(),
                            roster_body
                        )
                    } else if [39002, 0, 10100, 30315][index] == denied_kind {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]".into()
                    };
                    stream.write_all(reply.as_bytes()).await.unwrap();
                }
            })
            .await
            .unwrap();
        });
        let result = timeout(
            Duration::from_secs(4),
            fetch(&origin, &user, relay.public_key(), room()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.room, room().to_string());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].key, user.public_key().to_hex());
        assert!(result.entries[0].name.is_empty());
        assert!(result.agents.is_empty());
        // A denied status read is unknown, not "no status".
        assert_eq!(result.statuses_known, denied_kind != 30315);
        assert!(result.entries[0].status.is_none());
        task.await.unwrap();
    }
}

fn listed(user: &Keys, body: &str, at: u64) -> Event {
    profile(user, body, at)
}
fn names(found: &[Person]) -> Vec<&str> {
    found.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn people_directory_lists_by_name_dedupes_and_excludes_self() {
    let me = key(1);
    let (ann, bob, cy) = (key(2), key(3), key(4));
    let events = [
        listed(&cy, r#"{"display_name":"cy"}"#, 10),
        listed(&bob, r#"{"name":"Bob"}"#, 10),
        listed(&ann, r#"{"display_name":"Old Ann"}"#, 10),
        listed(&ann, r#"{"display_name":"Ann"}"#, 20),
        listed(&me, r#"{"display_name":"Me"}"#, 10),
    ];
    let found = people(me.public_key(), "", &events, 100).unwrap();
    // The newest kind 0 per author, in name order, without the viewer.
    assert_eq!(names(&found), ["Ann", "Bob", "cy"]);
    assert_eq!(found[0].key, ann.public_key().to_hex());
    // An equal time keeps the lower event id, whatever the relay order.
    let a = listed(&bob, r#"{"display_name":"B1"}"#, 30);
    let b = listed(&bob, r#"{"display_name":"B2"}"#, 30);
    let lower = if a.id < b.id { "B1" } else { "B2" };
    for events in [[a.clone(), b.clone()], [b, a]] {
        let found = people(me.public_key(), "", &events, 100).unwrap();
        assert_eq!(names(&found), [lower]);
    }
}

#[test]
fn people_without_a_name_stay_listed_and_names_are_sanitized() {
    let me = key(1);
    let (plain, odd) = (key(2), key(3));
    let events = [
        listed(&plain, "not json", 10),
        listed(&odd, "{\"display_name\":\"A\\u202eB\\nC\"}", 10),
    ];
    let found = people(me.public_key(), "", &events, 100).unwrap();
    assert_eq!(found.len(), 2);
    let unnamed = found.iter().find(|p| p.key == plain.public_key().to_hex());
    assert_eq!(unnamed.unwrap().name, "");
    let cleaned = found.iter().find(|p| p.key == odd.public_key().to_hex());
    assert_eq!(cleaned.unwrap().name, "A B C");
    let long = format!(r#"{{"display_name":"{}"}}"#, "x".repeat(200));
    let found = people(me.public_key(), "", &[listed(&plain, &long, 10)], 100).unwrap();
    assert_eq!(found[0].name.len(), 64);
}

#[test]
fn people_search_ranks_like_desktop_and_drops_non_matches() {
    let me = key(1);
    let (a, b, c, d, e) = (key(2), key(3), key(4), key(5), key(6));
    let events = [
        listed(&a, r#"{"display_name":"Big Tyler"}"#, 10),
        listed(&b, r#"{"display_name":"Tyler"}"#, 10),
        listed(&c, r#"{"display_name":"Tylerson"}"#, 10),
        listed(&d, r#"{"display_name":"Zed","about":"I know Tyler"}"#, 10),
        listed(
            &e,
            r#"{"display_name":"Zoe","nip05":"tyler@example.com"}"#,
            10,
        ),
    ];
    let found = people(me.public_key(), "tyler", &events, 100).unwrap();
    // Exact name, name prefix, name substring, nip05 prefix; `about` never matches.
    assert_eq!(names(&found), ["Tyler", "Tylerson", "Big Tyler", "Zoe"]);
    let by_key = people(me.public_key(), &a.public_key().to_hex()[..6], &events, 100).unwrap();
    assert_eq!(names(&by_key), ["Big Tyler"]);
    assert!(people(me.public_key(), "nobody", &events, 100)
        .unwrap()
        .is_empty());
}

#[test]
fn people_reject_unverified_or_out_of_scope_reads_whole() {
    let me = key(1);
    let other = key(2);
    let good = listed(&other, r#"{"display_name":"Ok"}"#, 10);
    let mut forged = listed(&other, r#"{"display_name":"Forged"}"#, 11);
    forged.content = r#"{"display_name":"Changed"}"#.into();
    for bad in [
        forged,
        event(&other, 1, "note", vec![], 10),
        listed(&other, "{}", 1000),
    ] {
        assert_eq!(
            people(me.public_key(), "", &[good.clone(), bad], 100).unwrap_err(),
            "people_invalid"
        );
    }
    let many: Vec<Event> = (0..=PEOPLE as u8)
        .map(|n| listed(&key(n + 2), "{}", 10))
        .collect();
    assert!(people(me.public_key(), "", &many, 100).is_err());
    assert_eq!(
        people(me.public_key(), "", &many[..PEOPLE], 100)
            .unwrap()
            .len(),
        PEOPLE
    );
}

#[test]
fn people_query_is_bounded_and_plain() {
    assert_eq!(people_query("  ann  "), "ann");
    assert_eq!(people_query("a\u{202e}b\nc"), "a b c");
    assert_eq!(people_query(" \t "), "");
    assert_eq!(people_query(&"é".repeat(100)).len(), QUERY_BYTES);
}
