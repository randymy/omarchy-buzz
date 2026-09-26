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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let public = user.public_key();
    let task = tokio::spawn(async move {
        timeout(Duration::from_secs(3), async move {
            for index in 0..2 {
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
                    } else {
                        serde_json::json!([{"kinds":[0],"authors":[public.to_hex()],"limit":1}])
                    }
                );
                let response = if index == 0 {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
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
    assert!(result.partial);
    task.await.unwrap();
}
