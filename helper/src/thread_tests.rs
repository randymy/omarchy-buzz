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
/// A direct reply: the root is its NIP-10 `reply` target.
fn reply(user: &Keys, root: &Event, content: &str, at: u64) -> Event {
    event(
        user,
        9,
        content,
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "reply"],
        ],
        at,
    )
}
/// A nested reply, tagged as upstream Desktop and the SDK tag one.
fn nested(user: &Keys, root: &Event, parent: &Event, content: &str, at: u64) -> Event {
    event(
        user,
        9,
        content,
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "root"],
            &["e", &parent.id.to_hex(), "", "reply"],
        ],
        at,
    )
}
fn replies(user: &Keys, root: &Event, first: u64, count: u64) -> Vec<Event> {
    (first..first + count)
        .map(|at| reply(user, root, &format!("reply {at}"), at))
        .collect()
}
/// Pages through the same accumulator `fetch` uses, with the fetch stop rule.
fn read(relay: &Keys, root: &Event, pages: Vec<Vec<Event>>) -> Result<Thread, &'static str> {
    let mut accumulated = Pages::default();
    let mut after = None;
    for (n, page) in pages.into_iter().enumerate() {
        let full = accumulated.add(room(), root.id, after, page, 1000)?;
        if full.is_none() || n + 1 == PAGES {
            return accumulated.project(
                relay.public_key(),
                crate::history::TEST_ORIGIN,
                room(),
                root.id,
                full.is_some(),
            );
        }
        after = full;
    }
    accumulated.project(
        relay.public_key(),
        crate::history::TEST_ORIGIN,
        room(),
        root.id,
        false,
    )
}
fn one_page(relay: &Keys, root: &Event, page: Vec<Event>) -> Result<Thread, &'static str> {
    read(relay, root, vec![page])
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
fn nested_replies_get_depth_and_parent_in_relay_order() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let root = root(&user);
    let a = reply(&user, &root, "direct", 110);
    let b = nested(&other, &root, &a, "to a", 111);
    let c = nested(&user, &root, &b, "to b", 112);
    let d = event(
        &other,
        40002,
        "second direct",
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "root"],
            &["e", &root.id.to_hex(), "", "reply"],
        ],
        113,
    );
    let root_reaction = buzz_sdk::build_reaction(root.id, "+")
        .unwrap()
        .custom_created_at(Timestamp::from(150))
        .sign_with_keys(&other)
        .unwrap();
    let result = one_page(
        &relay,
        &root,
        vec![a.clone(), b.clone(), c.clone(), d.clone(), root_reaction],
    )
    .unwrap();
    let shape: Vec<_> = result
        .rows
        .iter()
        .map(|row| (row.id.clone(), row.depth, row.parent.clone()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (a.id.to_hex(), 1, root.id.to_hex()),
            (b.id.to_hex(), 2, a.id.to_hex()),
            (c.id.to_hex(), 3, b.id.to_hex()),
            (d.id.to_hex(), 1, root.id.to_hex()),
        ]
    );
    assert_eq!(result.rows[1].author_pubkey, other.public_key().to_hex());
    assert_eq!(result.rows[2].text, "to b");
    assert_eq!(result.room, room().to_string());
    assert_eq!(result.root, root.id.to_hex());
    assert!(!result.has_more);
    assert_eq!(result.category, "thread_completeness_unknown");
}

#[test]
fn two_pages_accumulate_in_order_and_a_short_page_stops() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let first = replies(&user, &root, 1000 - 200, 50);
    let later = nested(&user, &root, &first[3], "late nested", 900);
    let second = vec![reply(&user, &root, "tail", 899), later.clone()];
    let result = read(&relay, &root, vec![first.clone(), second]).unwrap();
    assert_eq!(result.rows.len(), 52);
    assert!(result
        .rows
        .windows(2)
        .all(|pair| pair[0].timestamp < pair[1].timestamp));
    assert_eq!(result.rows[0].id, first[0].id.to_hex());
    assert_eq!(result.rows[51].id, later.id.to_hex());
    assert_eq!(result.rows[51].depth, 2);
    assert_eq!(result.rows[51].parent, first[3].id.to_hex());
    assert!(!result.has_more);
    assert_eq!(result.category, "thread_completeness_unknown");
}

#[test]
fn cap_at_two_hundred_reports_more_unshown() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let pages: Vec<_> = (0..4)
        .map(|page| replies(&user, &root, 300 + page * 50, 50))
        .collect();
    let result = read(&relay, &root, pages).unwrap();
    assert_eq!(result.rows.len(), ROWS);
    assert!(result.has_more);
    assert_eq!(result.category, "thread_more_unshown");
    assert!(ROWS == 200 && PAGE == 50);
}

#[test]
fn orphans_are_hidden_never_reparented() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    // The relay drops soft-deleted and inaccessible parents but keeps children.
    let missing = reply(&user, &root, "deleted parent", 105);
    let orphan = nested(&user, &root, &missing, "orphan", 110);
    let grandchild = nested(&user, &root, &orphan, "under orphan", 111);
    let sibling = reply(&user, &root, "visible", 112);
    let result = one_page(&relay, &root, vec![orphan, grandchild, sibling.clone()]).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].id, sibling.id.to_hex());
    assert_eq!(result.category, "thread_replies_hidden");
    assert!(!result.has_more);
    // A child ordered before its parent cannot name an earlier row.
    let parent = reply(&user, &root, "parent", 120);
    let early = nested(&user, &root, &parent, "clock skew", 119);
    let result = one_page(&relay, &root, vec![early, parent.clone()]).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].id, parent.id.to_hex());
    assert_eq!(result.category, "thread_replies_hidden");
}

#[test]
fn rows_require_room_scope_root_chain_and_valid_signatures() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let mut tampered = reply(&user, &root, "valid", 110);
    tampered.content = "tampered".into();
    assert_eq!(
        one_page(&relay, &root, vec![tampered]).unwrap_err(),
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
        110,
    );
    assert_eq!(
        one_page(&relay, &root, vec![wrong_room]).unwrap_err(),
        "thread_invalid_scope"
    );
    let unscoped = event(
        &user,
        9,
        "no room",
        &[&["e", &root.id.to_hex(), "", "reply"]],
        110,
    );
    assert_eq!(
        one_page(&relay, &root, vec![unscoped]).unwrap_err(),
        "thread_invalid_scope"
    );
    let other_root = event(&user, 9, "other", &[&["h", &room().to_string()]], 100);
    let foreign = nested(&user, &other_root, &root, "wrong root marker", 110);
    assert_eq!(
        one_page(&relay, &root, vec![foreign]).unwrap_err(),
        "thread_invalid_scope"
    );
    let unmarked_parent = event(
        &user,
        9,
        "nested without root marker",
        &[
            &["h", &room().to_string()],
            &["e", &"a".repeat(64), "", "reply"],
        ],
        110,
    );
    assert_eq!(
        one_page(&relay, &root, vec![unmarked_parent]).unwrap_err(),
        "thread_invalid_scope"
    );
    let top_level = event(&user, 9, "top", &[&["h", &room().to_string()]], 110);
    assert_eq!(
        one_page(&relay, &root, vec![top_level]).unwrap_err(),
        "thread_invalid_scope"
    );
    let unsupported = event(
        &user,
        40008,
        "unrendered kind",
        &[
            &["h", &room().to_string()],
            &["e", &root.id.to_hex(), "", "reply"],
        ],
        110,
    );
    assert_eq!(
        one_page(&relay, &root, vec![unsupported]).unwrap_err(),
        "thread_invalid_kind"
    );
}

#[test]
fn duplicates_and_order_fail_the_page() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let first = replies(&user, &root, 200, 50);
    let repeat = first[49].clone();
    assert_eq!(
        read(&relay, &root, vec![first.clone(), vec![repeat]]).unwrap_err(),
        "thread_duplicate_event"
    );
    let row = reply(&user, &root, "okay", 110);
    assert_eq!(
        one_page(&relay, &root, vec![row.clone(), row.clone()]).unwrap_err(),
        "thread_duplicate_event"
    );
    // Root auxiliaries legitimately return with every page.
    let reaction = event(&user, 7, "+", &[&["e", &root.id.to_hex()]], 400);
    let mut page_one = first.clone();
    page_one.push(reaction.clone());
    let result = read(
        &relay,
        &root,
        vec![page_one, vec![reply(&user, &root, "next", 260), reaction]],
    )
    .unwrap();
    assert_eq!(result.rows.len(), 51);
    let backwards = vec![
        reply(&user, &root, "later", 120),
        reply(&user, &root, "earlier", 110),
    ];
    assert_eq!(
        one_page(&relay, &root, backwards).unwrap_err(),
        "thread_invalid_order"
    );
    let stale = reply(&user, &root, "before cursor", 210);
    assert_eq!(
        read(&relay, &root, vec![first, vec![stale]]).unwrap_err(),
        "thread_invalid_order"
    );
}

#[test]
fn sdk_reaction_without_h_stays_auxiliary_and_wrong_h_or_forgery_fails() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let original = reply(&user, &root, "message", 120);
    let reaction = buzz_sdk::build_reaction(original.id, "+")
        .unwrap()
        .custom_created_at(Timestamp::from(150))
        .sign_with_keys(&user)
        .unwrap();
    let result = one_page(&relay, &root, vec![original.clone(), reaction.clone()]).unwrap();
    assert_eq!(result.rows.len(), 1);
    let unrelated = buzz_sdk::build_reaction(EventId::from_hex(&"aa".repeat(32)).unwrap(), "+")
        .unwrap()
        .custom_created_at(Timestamp::from(150))
        .sign_with_keys(&user)
        .unwrap();
    assert_eq!(
        one_page(&relay, &root, vec![original.clone(), unrelated]).unwrap_err(),
        "thread_invalid_scope"
    );
    let wrong = event(
        &user,
        7,
        "+",
        &[
            &["h", &Uuid::new_v4().to_string()],
            &["e", &original.id.to_hex()],
        ],
        150,
    );
    assert_eq!(
        one_page(&relay, &root, vec![original.clone(), wrong]).unwrap_err(),
        "thread_invalid_scope"
    );
    let mut forged = reaction;
    forged.content = "forged".into();
    assert_eq!(
        one_page(&relay, &root, vec![original, forged]).unwrap_err(),
        "thread_invalid_signature"
    );
}

#[test]
fn edits_deletions_and_uncertain_authority_are_conservative() {
    let relay = key(1);
    let user = key(2);
    let other = key(3);
    let root = root(&user);
    let parent = reply(&user, &root, "parent", 110);
    let original = nested(&user, &root, &parent, "before", 120);
    let child = nested(&other, &root, &original, "child", 125);
    let edit = event(
        &user,
        40003,
        "after",
        &[&["h", &room().to_string()], &["e", &original.id.to_hex()]],
        130,
    );
    let base = vec![parent.clone(), original.clone()];
    let result = one_page(&relay, &root, [base.clone(), vec![edit]].concat()).unwrap();
    assert_eq!(result.rows[1].text, "after");
    assert!(result.rows[1].edited);
    assert_eq!(result.rows[1].depth, 2);
    let hostile = event(
        &other,
        40003,
        "wrong author",
        &[&["h", &room().to_string()], &["e", &original.id.to_hex()]],
        140,
    );
    let result = one_page(&relay, &root, [base.clone(), vec![hostile]].concat()).unwrap();
    assert!(result.rows[1].unavailable);
    assert!(result.rows[1].text.is_empty());
    // Deleting a nested leaf removes only it.
    let delete = event(&user, 5, "", &[&["e", &original.id.to_hex()]], 150);
    let result = one_page(&relay, &root, [base.clone(), vec![delete.clone()]].concat()).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].id, parent.id.to_hex());
    assert_eq!(result.category, "thread_completeness_unknown");
    // Its replies lose their visible parent and are hidden, not re-parented.
    let result = one_page(
        &relay,
        &root,
        [base.clone(), vec![child.clone(), delete]].concat(),
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.category, "thread_replies_hidden");
    let unknown_delete = event(&other, 5, "", &[&["e", &original.id.to_hex()]], 150);
    let result = one_page(&relay, &root, [base, vec![child, unknown_delete]].concat()).unwrap();
    assert!(result.rows[1].unavailable);
    assert_eq!(result.rows[2].depth, 3);
}

#[test]
fn page_and_thread_budgets_fail_whole_page() {
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let reactions = |count: usize, at: u64| -> Vec<Event> {
        (0..count)
            .map(|n| event(&user, 7, &format!("{n}"), &[&["e", &root.id.to_hex()]], at))
            .collect()
    };
    assert_eq!(
        one_page(&relay, &root, reactions(EVENTS + 1, 150)).unwrap_err(),
        "thread_oversized"
    );
    let huge = reply(&user, &root, &"x".repeat(BYTES), 110);
    assert_eq!(
        one_page(&relay, &root, vec![huge]).unwrap_err(),
        "thread_oversized"
    );
    assert_eq!(
        one_page(&relay, &root, replies(&user, &root, 110, 51)).unwrap_err(),
        "thread_oversized"
    );
    // Three pages of 50 rows and 350 auxiliaries exceed the 1000-event thread budget.
    let page = |first: u64| -> Vec<Event> {
        [replies(&user, &root, first, 50), reactions(350, first + 60)].concat()
    };
    let mut accumulated = Pages::default();
    let after = accumulated
        .add(room(), root.id, None, page(200), 1000)
        .unwrap();
    let after = accumulated
        .add(room(), root.id, after, page(300), 1000)
        .unwrap();
    assert_eq!(
        accumulated
            .add(room(), root.id, after, page(400), 1000)
            .unwrap_err(),
        "thread_oversized"
    );
    // The rejected page left nothing behind.
    assert_eq!(accumulated.rows.len(), 100);
}

/// Serves one canned page per connection and returns each request body.
async fn serve(
    pages: Vec<Vec<Event>>,
    reader: PublicKey,
) -> (String, tokio::task::JoinHandle<Vec<serde_json::Value>>) {
    use nostr::nips::nip98::{verify_auth_header, HttpMethod};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::{timeout, Duration},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let endpoint = origin.replacen("ws://", "http://", 1) + "query";
    let server = tokio::spawn(async move {
        timeout(Duration::from_secs(8), async move {
            let mut bodies = Vec::new();
            for page in pages {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut all = Vec::new();
                loop {
                    let mut b = [0; 1024];
                    let n = stream.read(&mut b).await.unwrap();
                    assert!(n > 0);
                    all.extend_from_slice(&b[..n]);
                    assert!(all.len() < 16384);
                    if all.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let end = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                let head = String::from_utf8(all[..end].to_vec()).unwrap();
                assert!(head.starts_with("POST /query HTTP/1.1\r\n"));
                let header = |name: &str| {
                    head.lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                                .map(|(_, v)| v.trim().to_owned())
                        })
                        .unwrap()
                };
                let len: usize = header("content-length").parse().unwrap();
                assert!(len <= 8192);
                let mut body = all[end..].to_vec();
                while body.len() < len {
                    let mut b = [0; 1024];
                    let n = stream.read(&mut b).await.unwrap();
                    assert!(n > 0);
                    body.extend_from_slice(&b[..n]);
                }
                assert_eq!(
                    verify_auth_header(
                        &header("authorization"),
                        &nostr::Url::parse(&endpoint).unwrap(),
                        HttpMethod::POST,
                        Timestamp::now(),
                        Some(&body)
                    )
                    .unwrap(),
                    reader
                );
                bodies.push(serde_json::from_slice(&body).unwrap());
                let payload = serde_json::to_vec(&page).unwrap();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(&payload).await.unwrap();
            }
            bodies
        })
        .await
        .unwrap()
    });
    (origin, server)
}

fn filter(root: &Event, after: Option<&Event>) -> serde_json::Value {
    let mut value = serde_json::json!([{"#h":[room().to_string()],"#e":[root.id.to_hex()],"kinds":[9,40002],"depth_limit":64,"limit":50,"include_aux":true}]);
    if let Some(last) = after {
        value[0]["thread_cursor"] = serde_json::json!(last.created_at.as_secs());
        value[0]["thread_cursor_id"] = serde_json::json!(last.id.to_hex());
    }
    value
}

#[tokio::test]
async fn fetch_sends_desktop_filter_then_cursor_and_stops_on_short_page() {
    use tokio::time::{timeout, Duration};
    let _network = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let first = replies(&user, &root, 200, 50);
    let second = vec![nested(&user, &root, &first[0], "AE-ACK:7", 300)];
    let (origin, server) = serve(vec![first.clone(), second], user.public_key()).await;
    let result = timeout(
        Duration::from_secs(8),
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
    assert_eq!(result.rows.len(), 51);
    assert_eq!(result.rows[50].text, "AE-ACK:7");
    assert_eq!(result.rows[50].depth, 2);
    assert!(!result.has_more);
    assert_eq!(
        server.await.unwrap(),
        vec![filter(&root, None), filter(&root, first.last())]
    );
}

#[tokio::test]
async fn fetch_stops_after_four_full_pages() {
    use tokio::time::{timeout, Duration};
    let _network = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = key(1);
    let user = key(2);
    let root = root(&user);
    let pages: Vec<_> = (0..4)
        .map(|page| replies(&user, &root, 300 + page * 50, 50))
        .collect();
    let expected: Vec<_> = std::iter::once(filter(&root, None))
        .chain(pages[..3].iter().map(|page| filter(&root, page.last())))
        .collect();
    // Only four pages are served; a fifth request would fail the read.
    let (origin, server) = serve(pages, user.public_key()).await;
    let result = timeout(
        Duration::from_secs(8),
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
    assert_eq!(result.rows.len(), 200);
    assert!(result.has_more);
    assert_eq!(result.category, "thread_more_unshown");
    assert_eq!(server.await.unwrap(), expected);
}
