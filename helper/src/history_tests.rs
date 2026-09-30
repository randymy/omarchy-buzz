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
fn sdk_reaction_without_h_stays_auxiliary_and_wrong_h_or_forgery_fails() {
    let relay = key(1);
    let user = key(2);
    let original = row(&user, "message");
    let reaction = buzz_sdk::build_reaction(original.id, "+")
        .unwrap()
        .custom_created_at(Timestamp::from(150))
        .sign_with_keys(&user)
        .unwrap();
    let page = [original.clone(), reaction.clone(), head(&relay)];
    let result = reduce(room(), relay.public_key(), &page, 200).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].id, original.id.to_hex());
    let unrelated =
        buzz_sdk::build_reaction(nostr::EventId::from_hex(&"aa".repeat(32)).unwrap(), "+")
            .unwrap()
            .custom_created_at(Timestamp::from(150))
            .sign_with_keys(&user)
            .unwrap();
    assert_eq!(
        reduce(
            room(),
            relay.public_key(),
            &[original.clone(), unrelated, head(&relay)],
            200
        )
        .unwrap_err(),
        "history_invalid_scope"
    );
    let wrong = event(
        &user,
        7,
        "+",
        tags(&[
            &["h", &Uuid::new_v4().to_string()],
            &["e", &original.id.to_hex()],
        ]),
        150,
    );
    assert_eq!(
        reduce(
            room(),
            relay.public_key(),
            &[original.clone(), wrong, head(&relay)],
            200
        )
        .unwrap_err(),
        "history_invalid_scope"
    );
    let mut forged = reaction;
    forged.content = "forged".into();
    assert_eq!(
        reduce(
            room(),
            relay.public_key(),
            &[original, forged, head(&relay)],
            200
        )
        .unwrap_err(),
        "history_invalid_signature"
    );
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
            assert_eq!(actual,serde_json::json!([{"kinds":[9,40002],"#h":[room().to_string()],"limit":20,"top_level":true,"include_aux":true,"include_summaries":true}]));
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

#[test]
fn observed_agent_reactions_deduplicate_and_apply_deletions() {
    let relay = key(1);
    let user = key(2);
    let bot = key(3);
    let other = key(4);
    let original = row(&user, "task");
    let reaction = |signer: &Keys, emoji: &str, at| {
        buzz_sdk::build_reaction(original.id, emoji)
            .unwrap()
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(signer)
            .unwrap()
    };
    let seen = reaction(&bot, "👀", 150);
    let duplicate = reaction(&bot, "👀", 151);
    let working = reaction(&bot, "💬", 152);
    let page = vec![
        original.clone(),
        seen.clone(),
        duplicate.clone(),
        working.clone(),
        head(&relay),
    ];
    let result = reduce(room(), relay.public_key(), &page, 200).unwrap();
    let counts = result.rows[0].reactions.as_ref().unwrap();
    assert_eq!((counts.seen, counts.working), (1, 1));
    let mut removed = page.clone();
    removed.extend([
        deletion(&bot, &seen),
        deletion(&bot, &duplicate),
        deletion(&bot, &working),
    ]);
    let result = reduce(room(), relay.public_key(), &removed, 200).unwrap();
    let counts = result.rows[0].reactions.as_ref().unwrap();
    assert_eq!((counts.seen, counts.working), (0, 0));
    // Unknown moderation authority is conservatively suppressed, not shown as live.
    let mut uncertain = page;
    uncertain.push(deletion(&other, &working));
    let result = reduce(room(), relay.public_key(), &uncertain, 200).unwrap();
    assert_eq!(result.rows[0].reactions.as_ref().unwrap().working, 0);
    assert!(!result.rows[0].unavailable);
}
fn summary_with(relay: &Keys, tag_values: &[&[&str]], content: &str, at: u64) -> Event {
    event(relay, 39005, content, tags(tag_values), at)
}
fn summary(relay: &Keys, target: &Event, content: &str, at: u64) -> Event {
    let id = target.id.to_hex();
    summary_with(
        relay,
        &[&["e", &id], &["d", &id], &["h", &room().to_string()]],
        content,
        at,
    )
}
fn two_replies(user: &Keys) -> String {
    format!(
        r#"{{"reply_count":2,"descendant_count":3,"last_reply_at":150,"participants":["{}"]}}"#,
        user.public_key().to_hex()
    )
}
#[test]
fn thread_summary_projects_onto_its_row_and_tolerates_unknown_fields() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let replied = row(&user, "replied");
    let quiet = event(&user, 9, "quiet", tags(&[&["h", &room().to_string()]]), 110);
    let content = format!(
        r#"{{"reply_count":2,"descendant_count":3,"last_reply_at":150,"participants":["{}","{}"],"future":{{"x":1}}}}"#,
        other.public_key().to_hex(),
        user.public_key().to_hex()
    );
    let page = [
        replied.clone(),
        quiet,
        summary(&relay, &replied, &content, 199),
        head(&relay),
    ];
    let result = reduce(room(), relay.public_key(), &page, 200).unwrap();
    assert_eq!(
        result.rows[0].thread,
        Some(ThreadSummary {
            replies: 2,
            last_reply_at: Some(150),
            participants: vec![other.public_key().to_hex(), user.public_key().to_hex()],
        })
    );
    assert_eq!(result.rows[1].thread, None);
    assert_eq!(
        serde_json::to_value(&result.rows[0].thread).unwrap(),
        serde_json::json!({"replies":2,"lastReplyAt":150,"participants":[other.public_key().to_hex(),user.public_key().to_hex()]})
    );
    // Null recency and zero participants are valid, typed values.
    let empty = r#"{"reply_count":0,"descendant_count":0,"last_reply_at":null,"participants":[]}"#;
    let result = reduce(
        room(),
        relay.public_key(),
        &[
            replied.clone(),
            summary(&relay, &replied, empty, 199),
            head(&relay),
        ],
        200,
    )
    .unwrap();
    assert_eq!(
        result.rows[0].thread,
        Some(ThreadSummary {
            replies: 0,
            last_reply_at: None,
            participants: vec![],
        })
    );
    // Relay-authored reply metadata survives unresolved content authority.
    let result = reduce(
        room(),
        relay.public_key(),
        &[
            replied.clone(),
            deletion(&other, &replied),
            summary(&relay, &replied, &two_replies(&user), 199),
            head(&relay),
        ],
        200,
    )
    .unwrap();
    assert!(result.rows[0].unavailable);
    assert_eq!(result.rows[0].thread.as_ref().unwrap().replies, 2);
}
#[test]
fn thread_summary_signer_tags_and_scope_are_exact() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "root");
    let id = r.id.to_hex();
    let other_id = "ab".repeat(32);
    let scope = room().to_string();
    let other_room = Uuid::new_v4().to_string();
    let content = two_replies(&user);
    let reject = |overlay: Event| {
        reduce(
            room(),
            relay.public_key(),
            &[r.clone(), overlay, head(&relay)],
            200,
        )
        .unwrap_err()
    };
    assert_eq!(
        reject(summary(&user, &r, &content, 199)),
        "history_invalid_summary"
    );
    for bad in [
        vec![vec!["e", &id], vec!["h", &scope]],
        vec![vec!["d", &id], vec!["h", &scope]],
        vec![
            vec!["e", &id],
            vec!["d", &id],
            vec!["h", &scope],
            vec!["p", &id],
        ],
        vec![
            vec!["e", &id],
            vec!["e", &id],
            vec!["d", &id],
            vec!["h", &scope],
        ],
        vec![
            vec!["e", &id, "wss://relay"],
            vec!["d", &id],
            vec!["h", &scope],
        ],
        vec![vec!["e", &id], vec!["d", &other_id], vec!["h", &scope]],
        vec![
            vec!["e", &id.to_uppercase()],
            vec!["d", &id.to_uppercase()],
            vec!["h", &scope],
        ],
        vec![vec!["e", "root"], vec!["d", "root"], vec!["h", &scope]],
    ] {
        let rows: Vec<&[&str]> = bad.iter().map(Vec::as_slice).collect();
        assert_eq!(
            reject(summary_with(&relay, &rows, &content, 199)),
            "history_invalid_summary",
            "{bad:?}"
        );
    }
    assert_eq!(
        reject(summary_with(
            &relay,
            &[&["e", &id], &["d", &id], &["h", &other_room]],
            &content,
            199
        )),
        "history_invalid_scope"
    );
    assert_eq!(
        reject(summary_with(
            &relay,
            &[&["e", &id], &["d", &id]],
            &content,
            199
        )),
        "history_invalid_scope"
    );
}
#[test]
fn thread_summary_content_is_typed_and_bounded() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "root");
    let p = user.public_key().to_hex();
    let eleven: Vec<String> = (10..21).map(|n| key(n).public_key().to_hex()).collect();
    for content in [
        "not json".to_string(),
        "[]".to_string(),
        r#"{"reply_count":-1,"descendant_count":0,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":1.5,"descendant_count":2,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":"2","descendant_count":2,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":1,"descendant_count":-1,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":1,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"participants":[]}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":null}"#.into(),
        r#"{"descendant_count":1,"last_reply_at":null,"participants":[]}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":-5,"participants":[]}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":253402300800,"participants":[]}"#
            .into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":null,"participants":"x"}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":null,"participants":[7]}"#.into(),
        r#"{"reply_count":1,"descendant_count":1,"last_reply_at":null,"participants":["zz"]}"#
            .into(),
        format!(
            r#"{{"reply_count":1,"descendant_count":1,"last_reply_at":null,"participants":["{}"]}}"#,
            p.to_uppercase()
        ),
        format!(
            r#"{{"reply_count":2,"descendant_count":2,"last_reply_at":null,"participants":["{p}","{p}"]}}"#
        ),
        serde_json::json!({"reply_count":11,"descendant_count":11,"last_reply_at":1,"participants":eleven})
            .to_string(),
    ] {
        assert_eq!(
            reduce(
                room(),
                relay.public_key(),
                &[r.clone(), summary(&relay, &r, &content, 199), head(&relay)],
                200
            )
            .unwrap_err(),
            "history_invalid_summary",
            "{content}"
        );
    }
    // Caps: an implausible count is bounded; ten distinct participants pass.
    let ten = &eleven[..10];
    let content = serde_json::json!({"reply_count":5_000_000u64,"descendant_count":u64::MAX,"last_reply_at":253402300799u64,"participants":ten}).to_string();
    let result = reduce(
        room(),
        relay.public_key(),
        &[r.clone(), summary(&relay, &r, &content, 199), head(&relay)],
        200,
    )
    .unwrap();
    let thread = result.rows[0].thread.as_ref().unwrap();
    assert_eq!(thread.replies, 1_000_000);
    assert_eq!(thread.last_reply_at, Some(253_402_300_799));
    assert_eq!(thread.participants, ten);
}
#[test]
fn newest_thread_summary_per_row_wins_and_ties_fail() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "root");
    let older = summary(&relay, &r, &two_replies(&user), 190);
    let newer_content =
        r#"{"reply_count":3,"descendant_count":3,"last_reply_at":180,"participants":[]}"#;
    let newer = summary(&relay, &r, newer_content, 195);
    for page in [
        [r.clone(), older.clone(), newer.clone(), head(&relay)],
        [r.clone(), newer.clone(), older.clone(), head(&relay)],
    ] {
        let result = reduce(room(), relay.public_key(), &page, 200).unwrap();
        let thread = result.rows[0].thread.as_ref().unwrap();
        assert_eq!((thread.replies, thread.last_reply_at), (3, Some(180)));
    }
    let tie = summary(&relay, &r, newer_content, 190);
    assert_eq!(
        reduce(
            room(),
            relay.public_key(),
            &[r.clone(), older, tie, head(&relay)],
            200
        )
        .unwrap_err(),
        "history_invalid_summary"
    );
}
#[test]
fn thread_summary_for_a_row_not_on_the_page_fails_the_page() {
    let relay = key(1);
    let user = key(2);
    let r = row(&user, "root");
    let absent = row(&user, "not returned");
    let changed = edit(&user, &r, "edited", 150);
    for target in [&absent, &changed] {
        assert_eq!(
            reduce(
                room(),
                relay.public_key(),
                &[
                    r.clone(),
                    changed.clone(),
                    summary(&relay, target, &two_replies(&user), 199),
                    head(&relay)
                ],
                200
            )
            .unwrap_err(),
            "history_invalid_scope"
        );
    }
    // Summaries count against the page's event budget like any other event.
    let mut page: Vec<Event> = (0..200)
        .map(|n| summary(&relay, &r, &two_replies(&user), n))
        .collect();
    page.push(r.clone());
    page.push(head(&relay));
    assert_eq!(
        reduce(room(), relay.public_key(), &page, 200).unwrap_err(),
        "history_oversized"
    );
}
