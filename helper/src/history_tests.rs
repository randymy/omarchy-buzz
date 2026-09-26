use super::*;
use nostr::{EventBuilder, Kind, Tag};
fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn event(author: &Keys, kind: u16, content: &str, tags: Vec<Vec<String>>, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn tags(values: &[&[&str]]) -> Vec<Vec<String>> {
    values
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}
fn room() -> Uuid {
    Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
}
fn row(author: &Keys, body: &str) -> Event {
    event(
        author,
        40002,
        body,
        tags(&[&["h", &room().to_string()]]),
        100,
    )
}
fn bounds(relay: &Keys, content: &str) -> Event {
    event(
        relay,
        39006,
        content,
        tags(&[
            &["h", &room().to_string()],
            &["d", &format!("{}:head", room())],
        ]),
        200,
    )
}
fn head(relay: &Keys) -> Event {
    bounds(relay, r#"{"has_more":false,"next_cursor":null}"#)
}
fn edit(author: &Keys, target: &Event, body: &str, at: u64) -> Event {
    event(
        author,
        40003,
        body,
        tags(&[&["h", &room().to_string()], &["e", &target.id.to_hex()]]),
        at,
    )
}
fn deletion(author: &Keys, target: &Event) -> Event {
    event(author, 5, "", tags(&[&["e", &target.id.to_hex()]]), 190)
}
#[test]
fn signed_bounds_required_typed_and_authoritative() {
    let relay = key(1);
    let user = key(2);
    assert_eq!(
        reduce(room(), relay.public_key(), &[row(&user, "body")], 200).unwrap_err(),
        "history_missing_bounds"
    );
    for payload in [
        r#"{"has_more":true,"next_cursor":null}"#,
        r#"{"has_more":false}"#,
        r#"{"has_more":"false","next_cursor":null}"#,
        r#"{"has_more":true,"next_cursor":{"created_at":1,"id":"bad"}}"#,
    ] {
        assert_eq!(
            reduce(room(), relay.public_key(), &[bounds(&relay, payload)], 200).unwrap_err(),
            "history_invalid_bounds"
        );
    }
    assert_eq!(
        reduce(room(), relay.public_key(), &[head(&user)], 200).unwrap_err(),
        "history_invalid_bounds"
    );
    let wrong = event(
        &relay,
        39006,
        r#"{"has_more":false,"next_cursor":null}"#,
        tags(&[
            &["h", &room().to_string()],
            &["d", &format!("{}:123:cursor", room())],
        ]),
        200,
    );
    assert_eq!(
        reduce(room(), relay.public_key(), &[wrong], 200).unwrap_err(),
        "history_invalid_bounds"
    );
    assert!(reduce(room(), relay.public_key(), &[head(&relay)], 200)
        .unwrap()
        .rows
        .is_empty());
    assert_eq!(
        reduce(room(), relay.public_key(), &[head(&relay)], 261).unwrap_err(),
        "history_stale_bounds"
    );
}
#[test]
fn edit_and_delete_projection_does_not_show_aux_or_stale_original() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "old");
    let e = edit(&user, &r, "new", 150);
    let result = reduce(
        room(),
        relay.public_key(),
        &[r.clone(), e.clone(), head(&relay)],
        200,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].text, "new");
    assert!(result.rows[0].edited);
    assert_eq!(result.rows[0].id, r.id.to_hex());
    assert_eq!(result.rows[0].timestamp, 100);
    let deleted_edit = reduce(
        room(),
        relay.public_key(),
        &[r.clone(), e.clone(), deletion(&user, &e), head(&relay)],
        200,
    )
    .unwrap();
    assert_eq!(deleted_edit.rows[0].text, "old");
    assert!(!deleted_edit.rows[0].edited);
    assert!(reduce(
        room(),
        relay.public_key(),
        &[r.clone(), e, deletion(&user, &r), head(&relay)],
        200
    )
    .unwrap()
    .rows
    .is_empty());
}
#[test]
fn unresolved_owner_or_moderator_authority_hides_affected_content() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let r = row(&user, "must not leak old body");
    for auxiliary in [edit(&other, &r, "unproven edit", 150), deletion(&other, &r)] {
        let result = reduce(
            room(),
            relay.public_key(),
            &[r.clone(), auxiliary, head(&relay)],
            200,
        )
        .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert!(result.rows[0].unavailable);
        assert!(result.rows[0].text.is_empty());
    }
    let e = edit(&user, &r, "new", 150);
    let result = reduce(
        room(),
        relay.public_key(),
        &[r, e.clone(), deletion(&other, &e), head(&relay)],
        200,
    )
    .unwrap();
    assert!(result.rows[0].unavailable);
    assert!(result.rows[0].text.is_empty());
}
#[test]
fn delegated_actor_requires_relay_signature_and_mentions_never_attribute() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    for author in [&relay, &other] {
        let r = event(
            author,
            9,
            "body",
            tags(&[
                &["h", &room().to_string()],
                &["actor", &user.public_key().to_hex()],
                &["p", &user.public_key().to_hex()],
            ]),
            100,
        );
        let result = reduce(room(), relay.public_key(), &[r, head(&relay)], 200).unwrap();
        assert_eq!(
            result.rows[0].author_pubkey,
            if author.public_key() == relay.public_key() {
                user.public_key().to_hex()
            } else {
                other.public_key().to_hex()
            }
        );
    }
    let r = event(
        &relay,
        9,
        "body",
        tags(&[
            &["h", &room().to_string()],
            &["p", &user.public_key().to_hex()],
        ]),
        100,
    );
    assert_eq!(
        reduce(room(), relay.public_key(), &[r, head(&relay)], 200)
            .unwrap()
            .rows[0]
            .author_pubkey,
        relay.public_key().to_hex()
    );
}
#[test]
fn page_limits_scope_and_signatures_fail_whole_page() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "body");
    assert_eq!(
        reduce(
            room(),
            relay.public_key(),
            &[r.clone(), r.clone(), head(&relay)],
            200
        )
        .unwrap_err(),
        "history_duplicate_event"
    );
    let mut bad = r;
    bad.content = "tampered".into();
    assert_eq!(
        reduce(room(), relay.public_key(), &[bad, head(&relay)], 200).unwrap_err(),
        "history_invalid_signature"
    );
    let wrong = event(
        &user,
        40002,
        "wrong room",
        tags(&[&["h", &Uuid::new_v4().to_string()]]),
        100,
    );
    assert_eq!(
        reduce(room(), relay.public_key(), &[wrong, head(&relay)], 200).unwrap_err(),
        "history_invalid_scope"
    );
    assert_eq!(
        reduce(room(), relay.public_key(), &vec![head(&relay); 201], 200).unwrap_err(),
        "history_oversized"
    );
    let huge = row(&user, &"x".repeat(BYTES));
    assert_eq!(
        reduce(room(), relay.public_key(), &[huge, head(&relay)], 200).unwrap_err(),
        "history_oversized"
    );
}
#[test]
fn bounded_plaintext_snapshot_preserves_explicit_truncation_and_partiality() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, &"é\n\0".repeat(900));
    let result = reduce(room(), relay.public_key(), &[r, head(&relay)], 200).unwrap();
    assert!(result.rows[0].truncated);
    assert!(result.rows[0].text.len() <= 768);
    assert!(!result.rows[0].text.chars().any(char::is_control));
    assert_eq!(result.category, "history_completeness_unknown");
    assert!(!result.has_more);
}
#[test]
fn signed_cursor_is_not_inferred_from_displayed_rows_and_row_cap_is_enforced() {
    let relay = key(1);
    let user = key(2);
    let cursor =
        serde_json::json!({"has_more":true,"next_cursor":{"created_at":10,"id":"01".repeat(32)}})
            .to_string();
    let result = reduce(
        room(),
        relay.public_key(),
        &[row(&user, "body"), bounds(&relay, &cursor)],
        200,
    )
    .unwrap();
    assert!(result.has_more);
    assert_eq!(result.rows.len(), 1);
    let mut page = vec![head(&relay)];
    for n in 0..21 {
        page.push(row(&user, &format!("row {n}")));
    }
    assert_eq!(
        reduce(room(), relay.public_key(), &page, 200).unwrap_err(),
        "history_oversized"
    );
}
#[tokio::test]
async fn loopback_fetch_uses_one_bounded_nip_cw_query_and_projects_only_rows() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{timeout, Duration},
    };
    let relay = key(1);
    let user = key(2);
    let original = row(&user, "before");
    let fresh = event(
        &relay,
        39006,
        r#"{"has_more":false,"next_cursor":null}"#,
        tags(&[
            &["h", &room().to_string()],
            &["d", &format!("{}:head", room())],
        ]),
        Timestamp::now().as_secs(),
    );
    let page = vec![
        original.clone(),
        edit(&user, &original, "after", 150),
        fresh,
    ];
    let payload = serde_json::to_vec(&page).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        timeout(Duration::from_secs(3),async move {
            let (mut stream,_)=listener.accept().await.unwrap();let mut head=Vec::new();
            loop {
                let mut byte=[0;1];assert_eq!(stream.read(&mut byte).await.unwrap(),1);head.push(byte[0]);assert!(head.len()<16384);
                if head.ends_with(b"\r\n\r\n") {break;}
            }
            let head=String::from_utf8(head).unwrap().to_ascii_lowercase();assert!(head.starts_with("post /query "));
            assert!(head.contains("authorization: nostr "));
            let length=head.lines().find_map(|l|l.strip_prefix("content-length: ")).unwrap().trim().parse::<usize>().unwrap();
            assert!(length<=8192);let mut body=vec![0;length];stream.read_exact(&mut body).await.unwrap();
            let actual:serde_json::Value=serde_json::from_slice(&body).unwrap();
            assert_eq!(actual,serde_json::json!([{"kinds":[9,40002],"#h":[room().to_string()],"limit":20,"top_level":true,"include_aux":true,"include_summaries":false}]));
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len()).as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
        }).await.unwrap();
    });
    let result = timeout(
        Duration::from_secs(4),
        fetch(&origin, &user, relay.public_key(), room()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].text, "after");
    assert!(result.rows[0].edited);
    assert_eq!(result.rows[0].id, original.id.to_hex());
    server.await.unwrap();
}
