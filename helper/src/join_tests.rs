use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use nostr::hashes::{sha256, Hash};
use nostr::JsonUtil;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

pub(crate) const CODE: &str = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

#[test]
fn invite_forms_parse_like_desktop() {
    let cases = [
        (
            format!("https://relay.example/invite/{CODE}"),
            Some("wss://relay.example"),
        ),
        (
            format!("  https://relay.example/invite/{CODE}/ "),
            Some("wss://relay.example"),
        ),
        (
            format!("http://127.0.0.1:7000/invite/{CODE}"),
            Some("ws://127.0.0.1:7000"),
        ),
        (
            format!("https://relay.example:8443/invite/v2%2E{}", &CODE[3..]),
            Some("wss://relay.example:8443"),
        ),
        (
            format!("buzz://join?relay=wss://relay.example&code={CODE}"),
            Some("wss://relay.example"),
        ),
        (
            format!("buzz://join?code={CODE}&relay=wss%3A%2F%2Frelay.example&code=ignored"),
            Some("wss://relay.example"),
        ),
        (format!("\n{CODE}\t"), None),
        ("abc.DEF_ghi-123".to_string(), None),
    ];
    for (input, relay) in cases {
        let invite = parse_invite(&input).unwrap_or_else(|e| panic!("{input}: {e}"));
        let expected = if input.contains("abc.DEF") {
            "abc.DEF_ghi-123"
        } else {
            CODE
        };
        assert_eq!(invite.code, expected, "{input}");
        assert_eq!(invite.relay.as_deref(), relay, "{input}");
    }
}

#[test]
fn malformed_invites_are_refused_before_any_request() {
    for input in [
        "".to_string(),
        "   ".to_string(),
        format!("https://user:pw@relay.example/invite/{CODE}"),
        format!("https://relay.example/invite/{CODE}#x"),
        format!("https://relay.example/other/{CODE}"),
        "https://relay.example/invite/".to_string(),
        format!("https://relay.example/invite/{CODE}/extra"),
        format!("https://relay.example/invite/{CODE}//"),
        format!("buzz://other?relay=wss://relay.example&code={CODE}"),
        format!("buzz://join?code={CODE}"),
        "buzz://join?relay=wss://relay.example".to_string(),
        format!("buzz://join?relay=https://relay.example&code={CODE}"),
        format!("buzz://join?relay=wss://u@relay.example&code={CODE}"),
        format!("buzz://join?relay=wss://relay.example%23x&code={CODE}"),
        format!("buzz://join?relay=wss://relay.example&code={CODE}#x"),
        format!("wss://relay.example/invite/{CODE}"),
        format!("ftp://relay.example/invite/{CODE}"),
        format!("relay.example/invite/{CODE}"),
        "has space".to_string(),
        "percent%20code".to_string(),
        "x:y".to_string(),
        "a".repeat(CODE_BYTES + 1),
        format!("https://relay.example/invite/{CODE}%0A"),
        "https://relay.example/invite/%zz".to_string(),
    ] {
        assert_eq!(
            parse_invite(&input).unwrap_err(),
            "invite_invalid",
            "{input}"
        );
    }
}

#[test]
fn an_invite_never_switches_relays() {
    let configured = "wss://relay.example/";
    for input in [
        CODE.to_string(),
        format!("https://relay.example/invite/{CODE}"),
        format!("https://RELAY.example:443/invite/{CODE}"),
        format!("buzz://join?relay=wss://relay.example/&code={CODE}"),
    ] {
        assert_eq!(
            check_relay(&parse_invite(&input).unwrap(), configured),
            Ok(())
        );
    }
    for input in [
        format!("https://other.example/invite/{CODE}"),
        format!("https://relay.example:8443/invite/{CODE}"),
        format!("http://relay.example/invite/{CODE}"),
        format!("buzz://join?relay=ws://relay.example&code={CODE}"),
        format!("buzz://join?relay=wss://relay.example/path&code={CODE}"),
    ] {
        assert_eq!(
            check_relay(&parse_invite(&input).unwrap(), configured),
            Err("invite_relay_mismatch"),
            "{input}"
        );
    }
}

#[test]
fn policy_responses_are_strict() {
    assert_eq!(parse_policy(b"{}").unwrap(), None);
    assert_eq!(parse_policy(br#"{"policy":null}"#).unwrap(), None);
    let full = parse_policy(
        br##"{"policy":{"terms_markdown":"# Terms\u0007","privacy_markdown":"Privacy","age_attestation_required":true,"version":"v1"}}"##,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        full,
        JoinPolicy {
            text: "# Terms \n\nPrivacy".into(),
            version: "v1".into(),
            age_required: true,
            truncated: false,
        }
    );
    // Serialized `None` documents are null; a policy may have no documents.
    let bare = parse_policy(
        br#"{"policy":{"terms_markdown":null,"privacy_markdown":null,"age_attestation_required":false,"version":"2"}}"#,
    )
    .unwrap()
    .unwrap();
    assert_eq!((bare.text.as_str(), bare.age_required), ("", false));
    let long = serde_json::json!({"policy":{"terms_markdown":"é".repeat(POLICY_TEXT),"age_attestation_required":false,"version":"v"}});
    let cut = parse_policy(&serde_json::to_vec(&long).unwrap())
        .unwrap()
        .unwrap();
    assert!(cut.truncated && cut.text.len() <= POLICY_TEXT && cut.text.len() > POLICY_TEXT - 2);
    for bad in [
        &br#"[]"#[..],
        br#"{"policy":{}}"#,
        br#"{"policy":"v1"}"#,
        br#"{"policy":null,"extra":1}"#,
        br#"{"policy":{"age_attestation_required":false}}"#,
        br#"{"policy":{"version":"v1"}}"#,
        br#"{"policy":{"age_attestation_required":"no","version":"v1"}}"#,
        br#"{"policy":{"age_attestation_required":false,"version":""}}"#,
        br#"{"policy":{"age_attestation_required":false,"version":"a\nb"}}"#,
        br#"{"policy":{"age_attestation_required":false,"version":"v1","terms_markdown":7}}"#,
        br#"{"policy":{"age_attestation_required":false,"version":"v1","extra":""}}"#,
        br#"not json"#,
    ] {
        assert_eq!(
            parse_policy(bad).unwrap_err(),
            "relay_unavailable",
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
}

#[test]
fn claim_responses_are_strict() {
    let good = r#"{"status":"joined","community_id":"11111111-1111-4111-8111-111111111111","host":"relay.example","role":"member"}"#;
    assert_eq!(
        parse_claim(good.as_bytes()).unwrap(),
        ClaimResult {
            status: "joined".into(),
            community_id: "11111111-1111-4111-8111-111111111111".into(),
            host: "relay.example".into(),
            role: "member".into(),
        }
    );
    assert_eq!(
        parse_claim(good.replace("\"joined\"", "\"already_member\"").as_bytes())
            .unwrap()
            .status,
        "already_member"
    );
    for bad in [
        good.replace("\"joined\"", "\"pending\""),
        good.replace("community_id", "communityId"),
        good.replace(
            "11111111-1111-4111-8111-111111111111",
            "11111111-1111-4111-8111-11111111111A",
        ),
        good.replace("11111111-1111-4111-8111-111111111111", "community"),
        good.replace("relay.example", "relay example"),
        good.replace("relay.example", ""),
        good.replace("\"member\"", "\"Member\""),
        good.replace("\"member\"", "7"),
        good.replace("}", ",\"extra\":true}"),
        good.replace(",\"role\":\"member\"", ""),
        "[]".into(),
        "".into(),
    ] {
        assert_eq!(
            parse_claim(bad.as_bytes()).unwrap_err(),
            "relay_unavailable",
            "{bad}"
        );
    }
}

/// One scripted loopback HTTP origin: answers each request in order with
/// `(status, body)` and returns what it received (lower-cased head, body).
pub(crate) async fn http_fixture(
    script: Vec<(u16, String)>,
) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
    http_fixture_for(|_| script).await
}

/// `http_fixture` whose script is built from the relay address it serves.
pub(crate) async fn http_fixture_for(
    script: impl FnOnce(&str) -> Vec<(u16, String)>,
) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}/", listener.local_addr().unwrap());
    let script = script(&relay);
    let task = tokio::spawn(async move {
        let mut seen = Vec::new();
        for (status, body) in script {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut byte = [0; 1];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                head.push(byte[0]);
                assert!(head.len() < 16384);
            }
            let head = String::from_utf8(head).unwrap();
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .map_or(0, |v| v.trim().parse::<usize>().unwrap());
            let mut request = vec![0; length];
            stream.read_exact(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push((head, request));
        }
        seen
    });
    (relay, task)
}

pub(crate) fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// The NIP-98 proof on a claim: signed by the joining key, kind 27235, bound to
/// the exact URL, method and body hash.
pub(crate) fn check_nip98(head: &str, body: &[u8], url: &str, keys: &Keys) {
    let value = header(head, "authorization").expect("authorization");
    let encoded = value.strip_prefix("Nostr ").unwrap();
    let event = nostr::Event::from_json(STANDARD.decode(encoded).unwrap()).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey, keys.public_key());
    assert_eq!(event.kind.as_u16(), 27235);
    let tag = |name: &str| {
        event
            .tags
            .iter()
            .find(|t| t.as_slice()[0] == name)
            .map(|t| t.as_slice()[1].clone())
    };
    assert_eq!(tag("u").as_deref(), Some(url));
    assert_eq!(tag("method").as_deref(), Some("POST"));
    assert_eq!(tag("payload"), Some(sha256::Hash::hash(body).to_string()));
}

pub(crate) const CLAIMED: &str = r#"{"status":"joined","community_id":"11111111-1111-4111-8111-111111111111","host":"127.0.0.1","role":"member"}"#;

#[tokio::test]
async fn policy_is_accepted_before_a_signed_claim() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let policy = r#"{"policy":{"terms_markdown":"Be kind.","age_attestation_required":true,"version":"2026-09"}}"#;
    let (relay, server) = http_fixture(vec![
        (200, policy.into()),
        (200, r#"{"receipt":"cmVjZWlwdA.c2ln"}"#.into()),
        (200, CLAIMED.into()),
    ])
    .await;
    let origin = relay.trim_end_matches('/').replace("ws://", "http://");
    let (code, shown) = check_invite(&relay, &format!("{origin}/invite/{CODE}"))
        .await
        .unwrap();
    assert_eq!(code, CODE);
    let shown = shown.unwrap();
    assert_eq!(
        (shown.text.as_str(), shown.age_required),
        ("Be kind.", true)
    );
    // A different version than the one shown is refused without a request.
    assert_eq!(
        redeem(&relay, &keys, &code, Some(&shown), Some("2026-10"))
            .await
            .unwrap_err(),
        "policy_required"
    );
    assert_eq!(
        redeem(&relay, &keys, &code, Some(&shown), None)
            .await
            .unwrap_err(),
        "policy_required"
    );
    let claim = redeem(&relay, &keys, &code, Some(&shown), Some("2026-09"))
        .await
        .unwrap();
    assert_eq!(claim.status, "joined");
    let seen = server.await.unwrap();
    assert!(
        seen[0].0.starts_with("GET /api/join-policy "),
        "{}",
        seen[0].0
    );
    assert!(header(&seen[0].0, "authorization").is_none());
    assert!(seen[1].0.starts_with("POST /api/invites/accept-policy "));
    // The relay reads no NIP-98 proof on acceptance; none is sent.
    assert!(header(&seen[1].0, "authorization").is_none());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&seen[1].1).unwrap(),
        serde_json::json!({"code": CODE, "policy_version": "2026-09", "age_confirmed": true})
    );
    assert!(seen[2].0.starts_with("POST /api/invites/claim "));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&seen[2].1).unwrap(),
        serde_json::json!({"code": CODE, "policy_receipt": "cmVjZWlwdA.c2ln"})
    );
    check_nip98(
        &seen[2].0,
        &seen[2].1,
        &format!("{origin}/api/invites/claim"),
        &keys,
    );
    let text = seen
        .iter()
        .map(|(head, body)| format!("{head}{}", String::from_utf8_lossy(body)))
        .collect::<String>();
    assert!(!text.contains(&keys.secret_key().to_secret_hex()));
}

#[tokio::test]
async fn relays_without_a_policy_are_claimed_directly() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    for missing in [(404, String::new()), (200, "{}".to_string())] {
        let (relay, server) = http_fixture(vec![
            missing,
            (200, CLAIMED.replace("joined", "already_member")),
        ])
        .await;
        let (code, shown) = check_invite(&relay, CODE).await.unwrap();
        assert!(shown.is_none());
        assert_eq!(
            redeem(&relay, &keys, &code, None, None)
                .await
                .unwrap()
                .status,
            "already_member"
        );
        let seen = server.await.unwrap();
        assert!(seen[1].0.starts_with("POST /api/invites/claim "));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&seen[1].1).unwrap(),
            serde_json::json!({"code": CODE})
        );
    }
    // A version for a policy that was never shown is refused.
    let (relay, server) = http_fixture(vec![]).await;
    assert_eq!(
        redeem(&relay, &keys, CODE, None, Some("v1"))
            .await
            .unwrap_err(),
        "policy_required"
    );
    assert!(server.await.unwrap().is_empty());
}

#[tokio::test]
async fn refusals_map_to_fixed_categories() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let cases = [
        (403, r#"{"error":"invite_expired"}"#, "invite_rejected"),
        (403, r#"{"error":"invite_invalid"}"#, "invite_rejected"),
        (
            403,
            r#"{"error":"join_policy_required"}"#,
            "policy_required",
        ),
        (
            429,
            r#"{"error":"too many invite claim attempts, slow down"}"#,
            "invite_rate_limited",
        ),
        (401, r#"{"error":"nip98"}"#, "invite_rejected"),
        (
            404,
            r#"{"error":"relay: no community is configured for this host"}"#,
            "relay_unavailable",
        ),
        (500, "", "relay_unavailable"),
        (302, "", "relay_unavailable"),
        // Accepted, but not the documented answer: never reported as joined.
        (
            200,
            r#"{"status":"joined","communityId":"11111111-1111-4111-8111-111111111111","host":"h","role":"member"}"#,
            "relay_unavailable",
        ),
        (200, "not json", "relay_unavailable"),
    ];
    for (status, body, category) in cases {
        let (relay, server) = http_fixture(vec![(status, body.into())]).await;
        assert_eq!(
            claim(&relay, &keys, CODE, None).await.unwrap_err(),
            category,
            "{status} {body}"
        );
        server.await.unwrap();
    }
    let shown = JoinPolicy {
        text: String::new(),
        version: "v1".into(),
        age_required: false,
        truncated: false,
    };
    for (status, body, category) in [
        (
            400,
            r#"{"error":"join_policy_not_accepted"}"#,
            "policy_required",
        ),
        (
            404,
            r#"{"error":"join_policy_not_configured"}"#,
            "policy_required",
        ),
        (429, "", "invite_rate_limited"),
        (200, r#"{"receipt":"a","extra":1}"#, "relay_unavailable"),
        (200, r#"{"receipt":"has space"}"#, "relay_unavailable"),
    ] {
        let (relay, server) = http_fixture(vec![(status, body.into())]).await;
        assert_eq!(
            redeem(&relay, &keys, CODE, Some(&shown), Some("v1"))
                .await
                .unwrap_err(),
            category,
            "accept {status} {body}"
        );
        assert_eq!(
            server.await.unwrap().len(),
            1,
            "claimed after a refused acceptance"
        );
    }
    // Policy read failures and malformed policies stop before any claim.
    for (status, body) in [(500, ""), (200, r#"{"policy":{"version":"v1"}}"#)] {
        let (relay, server) = http_fixture(vec![(status, body.into())]).await;
        assert_eq!(
            check_invite(&relay, CODE).await.unwrap_err(),
            "relay_unavailable"
        );
        server.await.unwrap();
    }
    // A mismatched relay or malformed invite makes no request at all.
    let (relay, server) = http_fixture(vec![]).await;
    assert_eq!(
        check_invite(&relay, &format!("https://other.example/invite/{CODE}"))
            .await
            .unwrap_err(),
        "invite_relay_mismatch"
    );
    assert_eq!(
        check_invite(&relay, "a/b").await.unwrap_err(),
        "invite_invalid"
    );
    assert!(server.await.unwrap().is_empty());
    // Nothing listening.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}/", closed.local_addr().unwrap());
    drop(closed);
    assert_eq!(
        check_invite(&relay, CODE).await.unwrap_err(),
        "relay_unavailable"
    );
    assert_eq!(
        claim(&relay, &keys, CODE, None).await.unwrap_err(),
        "relay_unavailable"
    );
}

fn joined_status(keys: &Keys) -> Status {
    let config = crate::config::Config {
        relay: Some("wss://relay.example/".into()),
        identity: Some(keys.public_key().to_hex()),
    };
    let mut status = Status::new(&config);
    status.connection = "authenticated".into();
    status.catalog.state = "partial".into();
    status.catalog.rooms.push(crate::protocol::Room {
        id: JOINED.into(),
        name: "joined".into(),
        description: String::new(),
        kind: "stream".into(),
        participants: Vec::new(),
        hidden: false,
    });
    status.catalog.rooms.push(crate::protocol::Room {
        id: DM.into(),
        name: "dm".into(),
        description: String::new(),
        kind: "dm".into(),
        participants: Vec::new(),
        hidden: false,
    });
    status.open_rooms = crate::protocol::OpenRooms {
        state: "snapshot".into(),
        rooms: vec![crate::protocol::OpenRoom {
            id: OPEN.into(),
            name: "open".into(),
            description: String::new(),
            kind: "stream".into(),
        }],
        category: None,
    };
    status
}
const JOINED: &str = "11111111-1111-4111-8111-111111111111";
const OPEN: &str = "aaaaaaaa-2222-4222-8222-22222222222b";
const DM: &str = "33333333-3333-4333-8333-333333333333";
const REQUEST: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn room_actions_sign_exact_events_and_resolve_only_by_ok() {
    let keys = Keys::generate();
    let status = joined_status(&keys);
    let mut actions = RoomActions::default();
    let refused = |action, room: &str, status: &Status, fresh, trusted| {
        RoomActions::default()
            .prepare(action, REQUEST, room, &keys, status, fresh, trusted)
            .unwrap_err()
    };
    assert_eq!(
        refused(Action::Join, JOINED, &status, true, true),
        "room_not_open"
    );
    assert_eq!(
        refused(Action::Join, DM, &status, true, true),
        "room_not_open"
    );
    assert_eq!(
        refused(Action::Join, &OPEN.to_uppercase(), &status, true, true),
        "room_not_open"
    );
    assert_eq!(
        refused(Action::Leave, OPEN, &status, true, true),
        "leave_rejected"
    );
    assert_eq!(
        refused(Action::Leave, DM, &status, true, true),
        "leave_rejected"
    );
    assert_eq!(
        refused(Action::Join, OPEN, &status, false, true),
        "relay_unavailable"
    );
    assert_eq!(
        refused(Action::Join, OPEN, &status, true, false),
        "relay_unavailable"
    );
    let mut stale = status.clone();
    stale.open_rooms.state = "loading".into();
    assert_eq!(
        refused(Action::Join, OPEN, &stale, true, true),
        "room_not_open"
    );

    let (view, event) = actions
        .prepare(Action::Join, REQUEST, OPEN, &keys, &status, true, true)
        .unwrap();
    assert_eq!(
        (
            view.state.as_str(),
            view.action.as_deref(),
            view.room_id.as_deref()
        ),
        ("sending", Some("join"), Some(OPEN))
    );
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 9021);
    assert_eq!(event.pubkey, keys.public_key());
    assert_eq!(event.content, "");
    let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
    assert_eq!(tags, vec![vec!["h".to_string(), OPEN.to_string()]]);
    // One action at a time; other IDs never resolve it.
    assert_eq!(
        actions
            .prepare(
                Action::Leave,
                "00000000-0000-4000-8000-000000000002",
                JOINED,
                &keys,
                &status,
                true,
                true
            )
            .unwrap_err(),
        "setup_busy"
    );
    assert!(actions.acknowledge(&"0".repeat(64), true).is_none());
    let done = actions.acknowledge(&event.id.to_hex(), true).unwrap();
    assert_eq!((done.state.as_str(), done.category), ("acknowledged", None));
    assert!(actions.acknowledge(&event.id.to_hex(), true).is_none());

    let leave = "00000000-0000-4000-8000-000000000003";
    let (_, event) = actions
        .prepare(Action::Leave, leave, JOINED, &keys, &status, true, true)
        .unwrap();
    assert_eq!(event.kind.as_u16(), 9022);
    let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
    assert_eq!(tags, vec![vec!["h".to_string(), JOINED.to_string()]]);
    let refused = actions.acknowledge(&event.id.to_hex(), false).unwrap();
    assert_eq!(
        (refused.state.as_str(), refused.category.as_deref()),
        ("rejected", Some("leave_rejected"))
    );
    // A reported request ID is not signed again.
    let mut reported = status.clone();
    reported.room_action = refused;
    assert_eq!(
        actions
            .prepare(Action::Leave, leave, JOINED, &keys, &reported, true, true)
            .unwrap_err(),
        "setup_busy"
    );
    let (_, _) = actions
        .prepare(
            Action::Join,
            "00000000-0000-4000-8000-000000000004",
            OPEN,
            &keys,
            &status,
            true,
            true,
        )
        .unwrap();
    let lost = actions.unknown().unwrap();
    assert_eq!(
        (lost.state.as_str(), lost.category.as_deref()),
        ("unknown", Some("relay_unavailable"))
    );
    assert!(!actions.is_pending());
}

#[test]
fn categories_are_fixed() {
    for category in [
        "invite_invalid",
        "invite_relay_mismatch",
        "invite_rejected",
        "invite_rate_limited",
        "policy_required",
        "relay_unavailable",
        "setup_busy",
        "room_not_open",
        "join_rejected",
        "leave_rejected",
    ] {
        assert!(CATEGORIES.contains(&category));
    }
}
