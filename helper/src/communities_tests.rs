//! Communities on loopback relays only: synthetic NIP-11, join-policy and
//! claim answers, and a NIP-42 WebSocket peer. No real relay, keyring or
//! network is involved; secrets go to `setup::tests::FakeSecrets`.
use super::*;
use crate::{enrollment::IdentitySecrets, setup::tests::fixture};
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

const CODE: &str = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

#[test]
fn every_input_shape_parses_like_desktop() {
    let invite = |relay: Option<&str>, code: &str| Target::Invite {
        relay: relay.map(str::to_owned),
        code: code.into(),
    };
    for (input, expected) in [
        (
            "https://team.example/invite/abc.def",
            invite(Some("wss://team.example/"), "abc.def"),
        ),
        (
            "  http://127.0.0.1:3000/invite/abc  ",
            invite(Some("ws://127.0.0.1:3000/"), "abc"),
        ),
        (
            "buzz://join?relay=wss%3A%2F%2FTeam.example&code=abc",
            invite(Some("wss://team.example/"), "abc"),
        ),
        (CODE, invite(None, CODE)),
        ("abc_DEF-1", invite(None, "abc_DEF-1")),
        (
            "https://Team.Example",
            Target::Relay("wss://team.example/".into()),
        ),
        (
            "https://team.example:4433/",
            Target::Relay("wss://team.example:4433/".into()),
        ),
        (
            "wss://team.example//",
            Target::Relay("wss://team.example/".into()),
        ),
        ("team.example", Target::Relay("wss://team.example/".into())),
        (
            "team.communities.buzz.xyz",
            Target::Relay("wss://team.communities.buzz.xyz/".into()),
        ),
        (
            "localhost:3000",
            Target::Relay("wss://localhost:3000/".into()),
        ),
        (
            "http://localhost:3000",
            Target::Relay("ws://localhost:3000/".into()),
        ),
    ] {
        assert_eq!(parse_input(input), Ok(expected), "{input}");
    }
}

#[test]
fn hostile_inputs_are_refused_before_any_request() {
    for input in [
        "",
        "   ",
        "http://team.example", // plain ws:// to another computer
        "ws://team.example",
        "wss://user:pass@team.example",
        "https://user@team.example/invite/abc",
        "https://team.example/invite/abc#x",
        "https://team.example/rooms",
        "https://team.example/?a=1",
        "https://team.example/invite/",
        "https://team.example/invite/a/b",
        "buzz://join?relay=https://team.example&code=abc",
        "buzz://join?relay=wss://u@team.example&code=abc",
        "buzz://other?relay=wss://team.example&code=abc",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://team.example",
        "team.example/path",
        "abc def",
        "a\u{202e}b.example",
        "line\nbreak",
        "nul\u{0}",
        "https://team.example/invite/abc%00",
        "https://team.example/invite/<script>",
    ] {
        assert_eq!(parse_input(input), Err("join_invalid"), "{input:?}");
    }
    assert_eq!(parse_input(&"a".repeat(4097)), Err("join_invalid"));
    assert_eq!(
        parse_input(&format!("https://team.example/invite/{}", "a".repeat(1025))),
        Err("join_invalid")
    );
}

#[test]
fn nip11_names_are_untrusted_bounded_labels() {
    let signer = Keys::generate().public_key();
    let profile = |doc: Value| crate::catalog::info_profile(doc.to_string().as_bytes(), None);
    let p = profile(json!({"self": signer.to_hex(), "name": " Team\u{202e}\u{0007}Room ", "supported_nips": [1, 42, 43]})).unwrap();
    assert_eq!(p.signer, signer);
    assert_eq!(p.name.as_deref(), Some("Team  Room"));
    assert!(p.membership);
    let long = profile(json!({"self": signer.to_hex(), "name": "é".repeat(200)})).unwrap();
    assert!(long.name.unwrap().len() <= config::NAME_BYTES);
    for doc in [
        json!({"self": signer.to_hex(), "name": 7, "supported_nips": "43"}),
        json!({"self": signer.to_hex(), "name": "   ", "supported_nips": [42]}),
        json!({"self": signer.to_hex(), "icon": "data:image/png;base64,AAAA"}),
    ] {
        let p = profile(doc).unwrap();
        assert_eq!((p.name, p.membership), (None, false));
    }
    assert!(profile(json!({"name": "no signer"})).is_err());
}

/// What the loopback community relay answers and what it saw.
#[derive(Clone)]
struct Script {
    info: Value,
    /// `GET /api/join-policy`: status and body.
    policy: (u16, String),
    /// `POST /api/invites/claim`: status and body.
    claim: (u16, String),
    /// NIP-42: `None` accepts, `Some(message)` refuses.
    auth_refusal: Option<String>,
    /// The `OK` for any EVENT after authentication.
    event_ok: (bool, String),
}
impl Script {
    fn new(signer: &Keys) -> Self {
        Self {
            info: json!({"self": signer.public_key().to_hex(), "name": "Fixture Team", "supported_nips": [1, 11, 42, 43]}),
            policy: (404, "{}".into()),
            claim: (200, crate::join::tests::CLAIMED.into()),
            auth_refusal: None,
            event_ok: (true, String::new()),
        }
    }
}
#[derive(Default)]
struct Seen {
    paths: Vec<String>,
    claims: Vec<(String, Vec<u8>)>,
    auths: Vec<Event>,
    events: Vec<Event>,
}

async fn head_of(stream: &TcpStream) -> String {
    let mut buffer = vec![0; 16384];
    loop {
        let n = stream.peek(&mut buffer).await.unwrap();
        let text = String::from_utf8_lossy(&buffer[..n]).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            return text[..end + 4].to_owned();
        }
        assert!(n < buffer.len(), "header too large");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// A loopback community relay serving HTTP (NIP-11, join policy, claim) and a
/// NIP-42 WebSocket on one origin, for every connection until dropped.
async fn community_relay(
    script: Script,
) -> (
    String,
    Arc<std::sync::Mutex<Seen>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(Seen::default()));
    let record = seen.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let script = script.clone();
            let seen = record.clone();
            tokio::spawn(async move {
                let head = head_of(&stream).await;
                let lower = head.to_ascii_lowercase();
                if lower.contains("upgrade: websocket") {
                    let mut ws = accept_async(stream).await.unwrap();
                    ws.send(Message::Text(json!(["AUTH", "fixture"]).to_string().into()))
                        .await
                        .unwrap();
                    while let Some(Ok(frame)) = ws.next().await {
                        let Message::Text(text) = frame else { continue };
                        let value: Value = serde_json::from_str(&text).unwrap();
                        let event: Event = serde_json::from_value(value[1].clone()).unwrap();
                        event.verify().unwrap();
                        let id = event.id.to_hex();
                        let answer = if value[0] == "AUTH" {
                            seen.lock().unwrap().auths.push(event);
                            match &script.auth_refusal {
                                None => json!(["OK", id, true, ""]),
                                Some(message) => json!(["OK", id, false, message]),
                            }
                        } else {
                            assert_eq!(value[0], "EVENT");
                            seen.lock().unwrap().events.push(event);
                            json!(["OK", id, script.event_ok.0, script.event_ok.1])
                        };
                        let _ = ws.send(Message::Text(answer.to_string().into())).await;
                    }
                    return;
                }
                let mut consumed = vec![0; head.len()];
                stream.read_exact(&mut consumed).await.unwrap();
                let length = lower
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: ").map(str::to_owned))
                    .map_or(0, |v| v.trim().parse::<usize>().unwrap());
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let path = head
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                seen.lock().unwrap().paths.push(path.clone());
                let (status, text) = match path.as_str() {
                    "/info" => (200, script.info.to_string()),
                    "/api/join-policy" => script.policy.clone(),
                    "/api/invites/claim" => {
                        seen.lock().unwrap().claims.push((head.clone(), body));
                        script.claim.clone()
                    }
                    _ => (404, "{}".into()),
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (origin, seen, task)
}

/// A configured identity on `relays` (first active), its secret stored.
fn member_of(setup: &Setup, secrets: &crate::setup::tests::FakeSecrets, relays: &[&str]) -> Keys {
    let keys = Keys::generate();
    let config = Config {
        relay: Some(relays[0].into()),
        identity: Some(keys.public_key().to_hex()),
        communities: relays
            .iter()
            .map(|r| Community {
                relay: (*r).into(),
                name: config::derive_name(r),
                joined_at: 1,
            })
            .collect(),
    };
    config::save_to(setup.dir.as_ref().unwrap(), &config).unwrap();
    secrets
        .store(
            &config::account(&config).unwrap(),
            &Zeroizing::new(keys.secret_key().to_secret_hex()),
        )
        .unwrap();
    keys
}

#[tokio::test]
async fn an_invite_to_a_new_relay_is_claimed_with_the_same_identity() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://home.example/"]);
    let (relay, seen, server) = community_relay(Script::new(&Keys::generate())).await;
    let link = format!("{}invite/{CODE}", relay.replacen("ws://", "http://", 1));
    let done = perform(&setup, Request::Join(link), Some(&keys))
        .await
        .unwrap();
    assert!(done.switched && !done.claimed && done.policy.is_none());
    // One NIP-98 claim by the same key, bound to the claim URL.
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.paths,
        ["/info", "/api/join-policy", "/api/invites/claim"]
    );
    let (head, body) = &seen.claims[0];
    assert_eq!(
        serde_json::from_slice::<Value>(body).unwrap(),
        json!({"code": CODE})
    );
    crate::join::tests::check_nip98(
        head,
        body,
        &format!("{}api/invites/claim", relay.replacen("ws://", "http://", 1)),
        &keys,
    );
    drop(seen);
    let c = setup.load().unwrap();
    assert_eq!(c.relay.as_deref(), Some(relay.as_str()));
    assert_eq!(c.identity, Some(keys.public_key().to_hex()));
    assert_eq!(c.communities.len(), 2);
    assert_eq!(c.communities[1].name, "Local Dev");
    // The same secret under the new relay's account; the old one is kept.
    let items = secrets.items.lock().unwrap();
    assert_eq!(items.len(), 2);
    for secret in items.values() {
        assert_eq!(secret.as_str(), keys.secret_key().to_secret_hex());
    }
    drop(items);
    // The NIP-11 name is cached as a hint, never as the label.
    let shown = view(&c);
    assert_eq!(shown.entries[1].hint.as_deref(), Some("Fixture Team"));
    assert_eq!(shown.entries[1].name, "Local Dev");
    assert!(shown.entries[1].active && !shown.entries[0].active);
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn refused_claims_and_relay_signer_keys_add_nothing() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://home.example/"]);
    let before = setup.load().unwrap();
    for (status, body, expected) in [
        (403, r#"{"error":"invite_expired"}"#, "join_rejected"),
        (429, "{}", "join_rate_limited"),
        (500, "{}", "relay_unavailable"),
    ] {
        let mut script = Script::new(&Keys::generate());
        script.claim = (status, body.into());
        let (relay, _, server) = community_relay(script).await;
        *setup.joins.lock().unwrap() = None;
        let link = format!("buzz://join?relay={relay}&code={CODE}");
        assert_eq!(
            perform(&setup, Request::Join(link), Some(&keys))
                .await
                .unwrap_err(),
            expected
        );
        server.abort();
    }
    // A relay whose signer is this identity is refused before any claim.
    let (relay, seen, server) = community_relay(Script::new(&keys)).await;
    *setup.joins.lock().unwrap() = None;
    assert_eq!(
        perform(
            &setup,
            Request::Join(format!("buzz://join?relay={relay}&code={CODE}")),
            Some(&keys)
        )
        .await
        .unwrap_err(),
        "identity_unavailable"
    );
    assert_eq!(seen.lock().unwrap().paths, ["/info"]);
    server.abort();
    // Nothing listens here.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gone = format!("ws://{}/", closed.local_addr().unwrap());
    drop(closed);
    *setup.joins.lock().unwrap() = None;
    assert_eq!(
        perform(&setup, Request::Join(gone), Some(&keys))
            .await
            .unwrap_err(),
        "relay_unavailable"
    );
    assert_eq!(setup.load().unwrap(), before);
    assert_eq!(secrets.items.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn joins_are_rate_limited() {
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://home.example/"]);
    assert_eq!(
        perform(
            &setup,
            Request::Join("no spaces allowed".into()),
            Some(&keys)
        )
        .await
        .unwrap_err(),
        "join_invalid"
    );
    // Even a refused attempt starts the gap.
    assert_eq!(
        perform(
            &setup,
            Request::Join("wss://home.example".into()),
            Some(&keys)
        )
        .await
        .unwrap_err(),
        "join_rate_limited"
    );
    *setup.joins.lock().unwrap() = Some(Instant::now() - JOIN_GAP);
    // A known community without an invite is a switch to it (here: already
    // active), with no request to the relay.
    let done = perform(
        &setup,
        Request::Join("https://Home.example/".into()),
        Some(&keys),
    )
    .await
    .unwrap();
    assert!(!done.switched && !done.claimed);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn terms_are_left_to_accept_in_the_new_community() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://home.example/"]);
    let mut script = Script::new(&Keys::generate());
    script.policy = (
        200,
        r#"{"policy":{"terms_markdown":"Be kind.","age_attestation_required":false,"version":"v1"}}"#.into(),
    );
    let (relay, seen, server) = community_relay(script).await;
    let link = format!("buzz://join?relay={relay}&code={CODE}");
    let done = perform(&setup, Request::Join(link), Some(&keys))
        .await
        .unwrap();
    assert!(done.switched);
    let (code, policy) = done.policy.unwrap();
    assert_eq!((code.as_str(), policy.version.as_str()), (CODE, "v1"));
    // Nothing was claimed: acceptance comes first.
    assert!(seen.lock().unwrap().claims.is_empty());
    assert_eq!(setup.load().unwrap().relay.as_deref(), Some(relay.as_str()));
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_community_url_joins_only_when_the_relay_lets_this_identity_in() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://home.example/"]);
    let mut script = Script::new(&Keys::generate());
    script.auth_refusal = Some("restricted: not a relay member".into());
    let (relay, seen, server) = community_relay(script).await;
    let url = relay.replacen("ws://", "http://", 1);
    assert_eq!(
        perform(&setup, Request::Join(url.clone()), Some(&keys))
            .await
            .unwrap_err(),
        "join_rejected"
    );
    assert_eq!(seen.lock().unwrap().auths[0].pubkey, keys.public_key());
    assert_eq!(setup.load().unwrap().communities.len(), 1);
    server.abort();
    let (relay, seen, server) = community_relay(Script::new(&Keys::generate())).await;
    *setup.joins.lock().unwrap() = None;
    let done = perform(&setup, Request::Join(relay.clone()), Some(&keys))
        .await
        .unwrap();
    assert!(done.switched);
    assert!(seen.lock().unwrap().claims.is_empty());
    let c = setup.load().unwrap();
    assert_eq!(c.relay.as_deref(), Some(relay.as_str()));
    assert_eq!(c.communities.len(), 2);
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn first_setup_saves_the_relay_and_keeps_the_invite_for_later() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, _, dir) = fixture();
    let (relay, seen, server) = community_relay(Script::new(&Keys::generate())).await;
    let link = format!("{}invite/{CODE}", relay.replacen("ws://", "http://", 1));
    let done = perform(&setup, Request::Join(link), None).await.unwrap();
    assert!(done.switched && done.pending_invite);
    // Only the relay was checked; nothing can be claimed without an identity.
    assert_eq!(seen.lock().unwrap().paths, ["/info"]);
    let c = setup.load().unwrap();
    assert_eq!(
        (c.relay.as_deref(), c.identity.as_deref()),
        (Some(relay.as_str()), None)
    );
    // A bare code needs an active community: there is one now, but no identity.
    *setup.joins.lock().unwrap() = None;
    let done = perform(&setup, Request::Join(relay.clone()), None)
        .await
        .unwrap();
    assert!(!done.switched && !done.pending_invite);
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
    let (setup, _, dir) = fixture();
    assert_eq!(
        perform(&setup, Request::Join(CODE.into()), None)
            .await
            .unwrap_err(),
        "join_invalid"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn switch_and_rename_change_only_the_list() {
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://a.example/", "wss://b.example/"]);
    assert_eq!(
        perform(
            &setup,
            Request::Switch("wss://c.example".into()),
            Some(&keys)
        )
        .await
        .unwrap_err(),
        "community_unknown"
    );
    assert!(
        !perform(
            &setup,
            Request::Switch("wss://A.example".into()),
            Some(&keys)
        )
        .await
        .unwrap()
        .switched
    );
    let done = perform(
        &setup,
        Request::Switch("wss://b.example".into()),
        Some(&keys),
    )
    .await
    .unwrap();
    assert!(done.switched);
    let c = setup.load().unwrap();
    assert_eq!(c.relay.as_deref(), Some("wss://b.example/"));
    assert!(secrets
        .items
        .lock()
        .unwrap()
        .contains_key(&config::account(&c).unwrap()));
    // Without a loaded key only the choice is saved.
    perform(&setup, Request::Switch("wss://a.example/".into()), None)
        .await
        .unwrap();
    assert_eq!(
        setup.load().unwrap().relay.as_deref(),
        Some("wss://a.example/")
    );
    // Rename: sanitized, bounded, never empty.
    for bad in ["   ", "\u{202e}\u{0007}"] {
        assert_eq!(
            perform(
                &setup,
                Request::Rename("wss://a.example/".into(), bad.into()),
                None
            )
            .await
            .unwrap_err(),
            "name_invalid"
        );
    }
    assert_eq!(
        perform(
            &setup,
            Request::Rename("wss://z.example/".into(), "Z".into()),
            None
        )
        .await
        .unwrap_err(),
        "community_unknown"
    );
    perform(
        &setup,
        Request::Rename(
            "wss://b.example/".into(),
            format!(" Night\u{202e} shift {}", "x".repeat(100)),
        ),
        None,
    )
    .await
    .unwrap();
    let c = setup.load().unwrap();
    assert!(c.communities[1].name.starts_with("Night  shift xx"));
    assert!((60..=config::NAME_BYTES).contains(&c.communities[1].name.len()));
    assert_eq!(c.relay.as_deref(), Some("wss://a.example/"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn leave_publishes_the_nip43_request_and_never_the_last_community() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (setup, secrets, dir) = fixture();
    let (relay, seen, server) = community_relay(Script::new(&Keys::generate())).await;
    let keys = member_of(&setup, &secrets, &[&relay, "wss://other.example/"]);
    let done = perform(&setup, Request::Leave(relay.clone()), Some(&keys))
        .await
        .unwrap();
    assert!(done.switched && done.notice.is_none());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.auths.len(), 1);
    let event = &seen.events[0];
    assert_eq!(event.kind.as_u16(), LEAVE_KIND);
    assert_eq!(event.pubkey, keys.public_key());
    assert_eq!(event.content, "");
    let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
    assert_eq!(tags, vec![vec!["-".to_owned()]]);
    drop(seen);
    let c = setup.load().unwrap();
    assert_eq!(c.relay.as_deref(), Some("wss://other.example/"));
    assert_eq!(c.communities.len(), 1);
    assert_eq!(c.identity, Some(keys.public_key().to_hex()));
    // The identity's secret is never deleted, the new active account has it.
    let items = secrets.items.lock().unwrap();
    assert!(items.contains_key(&config::account(&c).unwrap()));
    assert!(items.contains_key(&format!("{relay}|{}", keys.public_key().to_hex())));
    drop(items);
    // The last community cannot be left.
    assert_eq!(
        perform(
            &setup,
            Request::Leave("wss://other.example/".into()),
            Some(&keys)
        )
        .await
        .unwrap_err(),
        "join_last"
    );
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn leave_refusals_map_to_fixed_outcomes() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    for (auth, ok, expected) in [
        (
            None,
            (false, "invalid: you are not a relay member"),
            Ok(Some("already_absent")),
        ),
        (
            Some("restricted: not a relay member"),
            (true, ""),
            Ok(Some("already_absent")),
        ),
        (
            None,
            (false, "invalid: relay owner cannot leave"),
            Err("leave_owner"),
        ),
        (
            None,
            (false, "error: something else"),
            Err("leave_rejected"),
        ),
        (
            Some("auth-required: bad"),
            (true, ""),
            Err("leave_rejected"),
        ),
    ] {
        let (setup, secrets, dir) = fixture();
        let mut script = Script::new(&Keys::generate());
        script.auth_refusal = auth.map(str::to_owned);
        script.event_ok = (ok.0, ok.1.into());
        let (relay, _, server) = community_relay(script).await;
        let keys = member_of(&setup, &secrets, &["wss://other.example/", &relay]);
        let result = perform(&setup, Request::Leave(relay.clone()), Some(&keys)).await;
        match expected {
            Ok(notice) => {
                let done = result.unwrap();
                assert_eq!(done.notice, notice);
                assert!(!done.switched);
                assert_eq!(setup.load().unwrap().communities.len(), 1);
            }
            Err(category) => {
                assert_eq!(result.unwrap_err(), category);
                assert_eq!(setup.load().unwrap().communities.len(), 2);
            }
        }
        server.abort();
        std::fs::remove_dir_all(dir).unwrap();
    }
    // A relay without NIP 43 has no membership to revoke: removed, nothing sent.
    let (setup, secrets, dir) = fixture();
    let mut script = Script::new(&Keys::generate());
    script.info =
        json!({"self": Keys::generate().public_key().to_hex(), "supported_nips": [1, 11, 42]});
    let (relay, seen, server) = community_relay(script).await;
    let keys = member_of(&setup, &secrets, &["wss://other.example/", &relay]);
    perform(&setup, Request::Leave(relay), Some(&keys))
        .await
        .unwrap();
    assert!(seen.lock().unwrap().auths.is_empty());
    assert_eq!(setup.load().unwrap().communities.len(), 1);
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_switch_bumps_the_generation_and_clears_the_old_relays_state() {
    let (setup, secrets, dir) = fixture();
    let keys = member_of(&setup, &secrets, &["wss://a.example/", "wss://b.example/"]);
    let mut status = crate::protocol::Status::new(&setup.load().unwrap());
    status.connection = "disconnected".into();
    status.catalog = crate::protocol::Catalog::loading();
    status.setup = crate::join::failed("invite_rejected");
    status.invites.state = "minted".into();
    let generation = status.generation;
    let (tx, rx) = tokio::sync::watch::channel(status);
    // A request made in another scope is refused.
    let (reply, answer) = tokio::sync::oneshot::channel();
    let request = Request::Switch("wss://b.example/".into());
    assert!(
        !crate::auth::offline_command(
            crate::protocol::Command::Community(request, Some(generation + 1), reply),
            &tx,
            &setup,
            Some(("wss://a.example/", &keys)),
        )
        .await
    );
    assert_eq!(answer.await.unwrap(), Some("join_busy"));
    let (reply, answer) = tokio::sync::oneshot::channel();
    let request = Request::Switch("wss://b.example/".into());
    // Reconnects at once.
    assert!(
        crate::auth::offline_command(
            crate::protocol::Command::Community(request, Some(generation), reply),
            &tx,
            &setup,
            Some(("wss://a.example/", &keys)),
        )
        .await
    );
    assert_eq!(answer.await.unwrap(), None);
    let s = rx.borrow();
    assert_eq!(s.generation, generation + 1);
    assert_eq!(s.relay.as_deref(), Some("wss://b.example/"));
    assert_eq!(s.catalog.state, "unavailable");
    assert_eq!(s.setup.state, "idle");
    assert_eq!(s.invites.state, "idle");
    assert_eq!(s.communities.state, "ready");
    assert_eq!(s.communities.active.as_deref(), Some("wss://b.example/"));
    assert!(s.communities.entries[1].active);
    drop(s);
    // A failure keeps the generation and shows a fixed category.
    let (reply, answer) = tokio::sync::oneshot::channel();
    assert!(
        !crate::auth::offline_command(
            crate::protocol::Command::Community(
                Request::Leave("wss://z.example/".into()),
                Some(generation + 1),
                reply
            ),
            &tx,
            &setup,
            Some(("wss://b.example/", &keys)),
        )
        .await
    );
    assert_eq!(answer.await.unwrap(), Some("community_unknown"));
    let s = rx.borrow();
    assert_eq!(s.generation, generation + 1);
    assert_eq!(
        (
            s.communities.state.as_str(),
            s.communities.category.as_deref()
        ),
        ("failed", Some("community_unknown"))
    );
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn categories_are_fixed() {
    for category in [
        claim_category("invite_invalid"),
        claim_category("invite_rejected"),
        claim_category("invite_rate_limited"),
        claim_category("policy_required"),
        claim_category("relay_unavailable"),
        claim_category("anything"),
    ] {
        assert!(CATEGORIES.contains(&category), "{category}");
    }
}

#[test]
fn request_shapes_are_strict() {
    let id = "00000000-0000-4000-8000-0000000000c1";
    let ok = |v: Value| crate::protocol::request(v.to_string().as_bytes()).map(|r| r.kind);
    let scoped = |kind: &str, extra: Value| {
        let mut v =
            json!({"version": 1, "id": id, "type": kind, "generation": 3, "instanceId": "x-1"});
        for (k, value) in extra.as_object().unwrap() {
            v[k] = value.clone();
        }
        v
    };
    assert!(ok(scoped(
        "join_community",
        json!({"input": "https://a.example"})
    ))
    .is_ok());
    assert!(ok(scoped(
        "switch_community",
        json!({"relay": "wss://a.example/"})
    ))
    .is_ok());
    assert!(ok(scoped(
        "leave_community",
        json!({"relay": "wss://a.example/"})
    ))
    .is_ok());
    assert!(ok(json!({"version": 1, "id": id, "type": "rename_community", "relay": "wss://a.example/", "name": "A"})).is_ok());
    for bad in [
        // Scope missing, wrong id shape, missing or extra fields.
        json!({"version": 1, "id": id, "type": "join_community", "input": "x"}),
        json!({"version": 1, "id": "ui-1", "type": "switch_community", "relay": "wss://a/", "generation": 3, "instanceId": "x"}),
        scoped("join_community", json!({})),
        scoped("join_community", json!({"input": "   "})),
        scoped("join_community", json!({"input": "a", "relay": "wss://a/"})),
        scoped("switch_community", json!({})),
        scoped("switch_community", json!({"relay": "wss://a/\n"})),
        scoped("leave_community", json!({"relay": "wss://a/", "name": "x"})),
        scoped(
            "rename_community",
            json!({"relay": "wss://a/", "name": "x"}),
        ),
        json!({"version": 1, "id": id, "type": "rename_community", "relay": "wss://a/"}),
        json!({"version": 1, "id": id, "type": "rename_community", "relay": "wss://a/", "name": " "}),
        json!({"version": 1, "id": id, "type": "rename_community", "relay": "wss://a/", "name": "a\u{0}"}),
        json!({"version": 1, "id": id, "type": "rename_community", "relay": "wss://a/", "name": "a".repeat(1025)}),
        json!({"version": 1, "id": id, "type": "get_snapshot", "relay": "wss://a/"}),
        json!({"version": 1, "id": id, "type": "get_snapshot", "name": "a"}),
    ] {
        assert!(ok(bad.clone()).is_err(), "{bad}");
    }
}
