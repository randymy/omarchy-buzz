use super::*;
use crate::join::tests::{check_nip98, header, http_fixture, CODE};

const NOW: u64 = 1_790_000_000;

fn answer(relay: &str, uses: u32, hours: u32) -> serde_json::Value {
    serde_json::json!({
        "code": CODE,
        "expires_at": NOW + u64::from(hours) * 3600,
        "max_uses": uses,
        "uses_remaining": uses,
        "url": landing_url(relay, CODE).unwrap(),
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn only_canonical_v2_codes_are_accepted() {
    assert!(valid_v2_code(CODE));
    for bad in [
        "",
        "v2.",
        &CODE[3..],
        "v1.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
        "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh",
        "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh9",
        "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8A",
        "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdH+8",
    ] {
        assert!(!valid_v2_code(bad), "{bad}");
    }
}

#[test]
fn the_request_body_is_desktops_with_both_limits() {
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request_body(5, 168)).unwrap(),
        serde_json::json!({"max_uses": 5, "ttl_secs": 604800})
    );
    assert_eq!(
        String::from_utf8(request_body(1, 24)).unwrap(),
        r#"{"max_uses":1,"ttl_secs":86400}"#
    );
}

#[test]
fn mint_answers_are_strict() {
    let relay = "wss://relay.example/";
    assert_eq!(
        landing_url(relay, CODE).unwrap(),
        format!("https://relay.example/invite/{CODE}")
    );
    assert_eq!(
        landing_url("ws://127.0.0.1:7000/", CODE).unwrap(),
        format!("http://127.0.0.1:7000/invite/{CODE}")
    );
    let good = answer(relay, 5, 24);
    let parse = |value: &serde_json::Value| {
        parse_minted(&serde_json::to_vec(value).unwrap(), relay, 5, 24, NOW)
    };
    assert_eq!(
        parse(&good).unwrap(),
        Minted {
            code: CODE.into(),
            expires_at: NOW + 86400,
            max_uses: 5
        }
    );
    // Skew within fifteen minutes is tolerated.
    let mut skewed = good.clone();
    skewed["expires_at"] = serde_json::json!(NOW + 86400 - 900);
    assert!(parse(&skewed).is_ok());
    let edits: Vec<(&str, serde_json::Value)> = vec![
        ("code", serde_json::json!("abc.def")),
        ("code", serde_json::json!(7)),
        ("expires_at", serde_json::json!(NOW + 86400 + 901)),
        ("expires_at", serde_json::json!(NOW)),
        ("expires_at", serde_json::json!("soon")),
        ("expires_at", serde_json::json!(-1)),
        ("max_uses", serde_json::json!(null)),
        ("max_uses", serde_json::json!(6)),
        ("uses_remaining", serde_json::json!(4)),
        ("uses_remaining", serde_json::json!(null)),
        (
            "url",
            serde_json::json!(format!("https://other.example/invite/{CODE}")),
        ),
        (
            "url",
            serde_json::json!(format!("http://relay.example/invite/{CODE}")),
        ),
        (
            "url",
            serde_json::json!("https://relay.example/invite/other"),
        ),
        ("extra", serde_json::json!(true)),
    ];
    for (key, value) in edits {
        let mut bad = good.clone();
        bad[key] = value.clone();
        assert_eq!(
            parse(&bad).unwrap_err(),
            "relay_unavailable",
            "{key}={value}"
        );
    }
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("url");
    assert_eq!(parse(&missing).unwrap_err(), "relay_unavailable");
    for raw in [&b"[]"[..], b"", b"not json", b"null"] {
        assert_eq!(
            parse_minted(raw, relay, 5, 24, NOW).unwrap_err(),
            "relay_unavailable"
        );
    }
}

#[tokio::test]
async fn an_owner_mints_with_a_signed_request() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let (relay, server) = http_fixture_with(|relay| {
        let mut value = answer(relay, 25, 168);
        value["expires_at"] = serde_json::json!(now_secs() + 168 * 3600);
        vec![(200, value.to_string())]
    })
    .await;
    let minted = mint(&relay, &keys, 25, 168).await.unwrap();
    assert_eq!((minted.code.as_str(), minted.max_uses), (CODE, 25));
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("POST /api/invites "), "{}", seen[0].0);
    assert_eq!(header(&seen[0].0, "content-type"), Some("application/json"));
    assert_eq!(
        String::from_utf8(seen[0].1.clone()).unwrap(),
        r#"{"max_uses":25,"ttl_secs":604800}"#
    );
    let origin = relay.trim_end_matches('/').replace("ws://", "http://");
    check_nip98(
        &seen[0].0,
        &seen[0].1,
        &format!("{origin}/api/invites"),
        &keys,
    );
    assert!(!seen[0].0.contains(&keys.secret_key().to_secret_hex()));
    // The view published to the panel.
    let view = minted_view(minted);
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        serde_json::json!({"state":"minted","code":CODE,"expiresAt":view.expires_at,"maxUses":25,"role":"member","category":null})
    );
}

fn minted_view(m: Minted) -> crate::protocol::Invites {
    minted(m)
}

/// `http_fixture` whose script depends on the relay address it listens on.
async fn http_fixture_with(
    script: impl FnOnce(&str) -> Vec<(u16, String)>,
) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
    crate::join::tests::http_fixture_for(script).await
}

#[tokio::test]
async fn refusals_map_to_fixed_categories() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let cases = [
        (
            403,
            r#"{"error":"only relay owners and admins can create invites"}"#.to_string(),
            "invite_forbidden",
        ),
        (
            403,
            "authorization denied\n".to_string(),
            "invite_forbidden",
        ),
        (
            429,
            r#"{"error":"rate limited"}"#.to_string(),
            "invite_rate_limited",
        ),
        (
            400,
            r#"{"error":"max_uses must be between 1 and 10000"}"#.to_string(),
            "invite_rejected",
        ),
        (
            401,
            "authentication required\n".to_string(),
            "invite_rejected",
        ),
        (
            404,
            r#"{"error":"relay: no community is configured for this host"}"#.to_string(),
            "relay_unavailable",
        ),
        (503, String::new(), "relay_unavailable"),
        (500, String::new(), "relay_unavailable"),
        (302, String::new(), "relay_unavailable"),
        // Accepted, but not the documented answer: never shown as an invite.
        (200, r#"{"code":"v2.x"}"#.to_string(), "relay_unavailable"),
        (200, "not json".to_string(), "relay_unavailable"),
    ];
    for (status, body, category) in cases {
        let (relay, server) = http_fixture(vec![(status, body.clone())]).await;
        assert_eq!(
            mint(&relay, &keys, 1, 24).await.unwrap_err(),
            category,
            "{status} {body}"
        );
        server.await.unwrap();
    }
    // Out-of-range limits never reach the relay.
    let (relay, server) = http_fixture(vec![]).await;
    for (uses, hours) in [(0, 24), (101, 24), (1, 0), (1, 721)] {
        assert_eq!(
            mint(&relay, &keys, uses, hours).await.unwrap_err(),
            "invite_rejected"
        );
    }
    assert!(server.await.unwrap().is_empty());
    for category in CATEGORIES {
        assert_eq!(
            serde_json::to_value(failed(category)).unwrap(),
            serde_json::json!({"state":"failed","code":null,"expiresAt":null,"maxUses":null,"role":null,"category":category})
        );
    }
}
