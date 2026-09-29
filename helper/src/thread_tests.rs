use super::*;
use nostr::{EventBuilder, Kind, Tag};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn room() -> Uuid {
    Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
}
fn event(key: &Keys, kind: u16, content: &str, tags: &[&[&str]], at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags.iter().map(|v| Tag::parse(v.iter().copied()).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(key)
        .unwrap()
}
fn root(user: &Keys) -> Event {
    event(user, 9, "prompt", &[&["h", &room().to_string()]], 100)
}
fn reply(user: &Keys, root: &Event, content: &str) -> Event {
    event(
        user,
        9,
        content,
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "reply"],
        ],
        120,
    )
}
fn bounds(relay: &Keys, user: &Keys, root: &Event, content: &str) -> Event {
    event(
        relay,
        39007,
        content,
        &[
            &[
                "d",
                &binding("ws://localhost:3000", user.public_key(), room(), root.id).unwrap(),
            ],
            &["h", &room().to_string()],
            &["e", &root.id.to_hex()],
        ],
        200,
    )
}
fn closed(relay: &Keys, user: &Keys, root: &Event) -> Event {
    bounds(
        relay,
        user,
        root,
        r#"{"version":1,"direction":"older","has_more":false,"next_cursor":null}"#,
    )
}
fn run(user: &Keys, relay: &Keys, root: &Event, events: &[Event]) -> Result<Thread, &'static str> {
    reduce(
        "ws://localhost:3000",
        user.public_key(),
        room(),
        root.id,
        relay.public_key(),
        events,
        200,
    )
}

#[test]
fn binding_matches_independent_pinned_request_vector_with_port() {
    let root = EventId::from_hex(&"cd".repeat(32)).unwrap();
    let expected = binding("ws://localhost:3000", key(1).public_key(), room(), root).unwrap();
    assert_eq!(
        expected,
        "tw:1:6a0a4f9f03435c00d68e46e435a4185304132b38a0ccd884d088a8ccf5f0d992"
    );
    assert_ne!(
        expected,
        binding("ws://localhost:3001", key(1).public_key(), room(), root).unwrap()
    );
    assert_ne!(
        expected,
        binding("ws://localhost:3000", key(2).public_key(), room(), root).unwrap()
    );
    assert_ne!(
        expected,
        binding(
            "ws://localhost:3000",
            key(1).public_key(),
            Uuid::nil(),
            root
        )
        .unwrap()
    );
    assert_ne!(
        expected,
        binding(
            "ws://localhost:3000",
            key(1).public_key(),
            room(),
            EventId::from_hex(&"ef".repeat(32)).unwrap()
        )
        .unwrap()
    );
}

#[tokio::test]
async fn fetch_rejects_noncanonical_root_before_transport() {
    let user = key(2);
    assert_eq!(
        fetch(
            "ws://localhost:3000",
            &user,
            key(1).public_key(),
            room(),
            &"AB".repeat(32)
        )
        .await
        .unwrap_err(),
        "thread_invalid_root"
    );
}

#[test]
fn signed_direct_reply_and_root_aux_project_only_reply() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let reply = reply(&user, &root, "AE-ACK:7");
    let result = run(
        &user,
        &relay,
        &root,
        &[root.clone(), reply.clone(), closed(&relay, &user, &root)],
    )
    .unwrap();
    assert_eq!(result.room, room().to_string());
    assert_eq!(result.root, root.id.to_hex());
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].id, reply.id.to_hex());
    assert_eq!(result.rows[0].text, "AE-ACK:7");
    assert!(!result.has_more);
    assert_eq!(result.category, "thread_completeness_unknown");
}

#[test]
fn bounds_are_fresh_unique_signed_and_exactly_bound() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let good = closed(&relay, &user, &root);
    assert_eq!(
        run(&user, &relay, &root, &[]).unwrap_err(),
        "thread_missing_bounds"
    );
    assert_eq!(
        run(&user, &relay, &root, &[good.clone(), good.clone()]).unwrap_err(),
        "thread_duplicate_event"
    );
    let second = bounds(
        &relay,
        &user,
        &root,
        r#"{"version":1,"direction":"older","has_more":true,"next_cursor":{"created_at":120,"id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
    );
    assert_eq!(
        run(&user, &relay, &root, &[good.clone(), second]).unwrap_err(),
        "thread_invalid_bounds"
    );
    let forged = bounds(&user, &user, &root, &good.content);
    assert_eq!(
        run(&user, &relay, &root, &[forged]).unwrap_err(),
        "thread_invalid_bounds"
    );
    let stale = event(
        &relay,
        39007,
        &good.content,
        &[
            &[
                "d",
                &binding("ws://localhost:3000", user.public_key(), room(), root.id).unwrap(),
            ],
            &["h", &room().to_string()],
            &["e", &root.id.to_hex()],
        ],
        100,
    );
    assert_eq!(
        run(&user, &relay, &root, &[stale]).unwrap_err(),
        "thread_stale_bounds"
    );
    let wrong = event(
        &relay,
        39007,
        &good.content,
        &[
            &[
                "d",
                &binding("ws://localhost:3001", user.public_key(), room(), root.id).unwrap(),
            ],
            &["h", &room().to_string()],
            &["e", &root.id.to_hex()],
        ],
        200,
    );
    assert_eq!(
        run(&user, &relay, &root, &[wrong]).unwrap_err(),
        "thread_invalid_bounds"
    );
    for content in [
        r#"{"version":2,"direction":"older","has_more":false,"next_cursor":null}"#,
        r#"{"version":1,"direction":"newer","has_more":false,"next_cursor":null}"#,
        r#"{"version":1,"direction":"older","has_more":true,"next_cursor":null}"#,
    ] {
        assert_eq!(
            run(
                &user,
                &relay,
                &root,
                &[bounds(&relay, &user, &root, content)]
            )
            .unwrap_err(),
            "thread_invalid_bounds"
        );
    }
}

#[test]
fn rows_require_direct_scope_and_valid_signatures() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let good = closed(&relay, &user, &root);
    let mut tampered = reply(&user, &root, "valid");
    tampered.content = "tampered".into();
    assert_eq!(
        run(&user, &relay, &root, &[tampered, good.clone()]).unwrap_err(),
        "thread_invalid_signature"
    );
    let wrong_room = event(
        &user,
        9,
        "cross room",
        &[
            &["h", &Uuid::new_v4().to_string()],
            &["e", &root.id.to_hex(), "", "reply"],
        ],
        120,
    );
    assert_eq!(
        run(&user, &relay, &root, &[wrong_room, good.clone()]).unwrap_err(),
        "thread_invalid_scope"
    );
    let nested = event(
        &user,
        9,
        "nested",
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "root"],
            &["e", &"a".repeat(64), "", "reply"],
        ],
        120,
    );
    assert_eq!(
        run(&user, &relay, &root, &[nested, good.clone()]).unwrap_err(),
        "thread_invalid_scope"
    );
    let row = reply(&user, &root, "okay");
    assert_eq!(
        run(&user, &relay, &root, &[row.clone(), row, good]).unwrap_err(),
        "thread_duplicate_event"
    );
}

#[test]
fn edits_deletions_and_uncertain_authority_are_conservative() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let root = root(&user);
    let original = reply(&user, &root, "before");
    let edit = event(
        &user,
        40003,
        "after",
        &[&["h", &room().to_string()], &["e", &original.id.to_hex()]],
        130,
    );
    let base = closed(&relay, &user, &root);
    let result = run(
        &user,
        &relay,
        &root,
        &[original.clone(), edit.clone(), base.clone()],
    )
    .unwrap();
    assert_eq!(result.rows[0].text, "after");
    assert!(result.rows[0].edited);
    let hostile = event(
        &other,
        40003,
        "wrong author",
        &[&["h", &room().to_string()], &["e", &original.id.to_hex()]],
        140,
    );
    let result = run(
        &user,
        &relay,
        &root,
        &[original.clone(), hostile, base.clone()],
    )
    .unwrap();
    assert!(result.rows[0].unavailable);
    assert!(result.rows[0].text.is_empty());
    let delete = event(&user, 5, "", &[&["e", &original.id.to_hex()]], 150);
    assert!(run(
        &user,
        &relay,
        &root,
        &[original.clone(), delete, base.clone()]
    )
    .unwrap()
    .rows
    .is_empty());
    let unknown_delete = event(&other, 5, "", &[&["e", &original.id.to_hex()]], 150);
    assert!(
        run(&user, &relay, &root, &[original, unknown_delete, base])
            .unwrap()
            .rows[0]
            .unavailable
    );
}

#[test]
fn page_budgets_fail_whole_page() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    assert_eq!(
        run(
            &user,
            &relay,
            &root,
            &vec![closed(&relay, &user, &root); 201]
        )
        .unwrap_err(),
        "thread_oversized"
    );
    let huge = reply(&user, &root, &"x".repeat(BYTES));
    assert_eq!(
        run(&user, &relay, &root, &[huge, closed(&relay, &user, &root)]).unwrap_err(),
        "thread_oversized"
    );
    let mut rows = vec![closed(&relay, &user, &root)];
    for n in 0..9 {
        rows.push(reply(&user, &root, &format!("reply {n}")));
    }
    assert_eq!(
        run(&user, &relay, &root, &rows).unwrap_err(),
        "thread_oversized"
    );
}

#[tokio::test]
async fn fetch_uses_exact_thread_filter_and_signed_post() {
    use nostr::nips::nip98::{verify_auth_header, HttpMethod};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{timeout, Duration},
    };
    let _network = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let page = vec![
        reply(&user, &root, "AE-ACK:7"),
        event(
            &relay,
            39007,
            r#"{"version":1,"direction":"older","has_more":false,"next_cursor":null}"#,
            &[
                &[
                    "d",
                    &binding(&origin, user.public_key(), room(), root.id).unwrap(),
                ],
                &["h", &room().to_string()],
                &["e", &root.id.to_hex()],
            ],
            Timestamp::now().as_secs(),
        ),
    ];
    let payload = serde_json::to_vec(&page).unwrap();
    let expected_origin = origin.clone();
    let expected_root = root.id.to_hex();
    let expected_reader = user.public_key();
    let server = tokio::spawn(async move {
        timeout(Duration::from_secs(3),async move {
            let (mut stream,_) = listener.accept().await.unwrap();
            let mut all = Vec::new();
            loop { let mut b=[0;1024]; let n=stream.read(&mut b).await.unwrap(); assert!(n>0); all.extend_from_slice(&b[..n]); assert!(all.len()<16384); if all.windows(4).any(|w|w==b"\r\n\r\n") {break;} }
            let end=all.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;
            let head=String::from_utf8(all[..end].to_vec()).unwrap();
            assert!(head.starts_with("POST /query HTTP/1.1\r\n"));
            let header=|name:&str| head.lines().find_map(|line|line.split_once(':').filter(|(key,_)|key.eq_ignore_ascii_case(name)).map(|(_,v)|v.trim().to_owned())).unwrap();
            let len:usize=header("content-length").parse().unwrap(); assert!(len<=8192);
            let mut body=all[end..].to_vec(); while body.len()<len {let mut b=[0;1024];let n=stream.read(&mut b).await.unwrap();assert!(n>0);body.extend_from_slice(&b[..n]);}
            assert_eq!(body.len(),len);
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap(),serde_json::json!([{"thread_window":true,"#h":[room().to_string()],"#e":[expected_root],"kinds":[9],"depth_limit":1,"limit":8,"include_aux":true}]));
            let endpoint=expected_origin.replacen("ws://","http://",1)+"query";
            assert_eq!(verify_auth_header(&header("authorization"),&nostr::Url::parse(&endpoint).unwrap(),HttpMethod::POST,Timestamp::now(),Some(&body)).unwrap(),expected_reader);
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len()).as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
        }).await.unwrap();
    });
    let result = timeout(
        Duration::from_secs(4),
        fetch(
            &origin,
            &user,
            relay.public_key(),
            room(),
            &root.id.to_hex(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].text, "AE-ACK:7");
    server.await.unwrap();
}
