use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

async fn send(ws: &mut WebSocketStream<TcpStream>, frame: Value) {
    ws.send(Message::Text(frame.to_string().into()))
        .await
        .unwrap();
}
async fn authenticated_listener() -> (String, tokio::task::JoinHandle<WebSocketStream<TcpStream>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        timeout(Duration::from_secs(2), async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            send(&mut ws, json!(["AUTH", "fixture"])).await;
            let frame = ws.next().await.unwrap().unwrap();
            let Message::Text(text) = frame else {
                panic!("AUTH expected")
            };
            let value: Value = serde_json::from_str(&text).unwrap();
            let event: Event = serde_json::from_value(value[1].clone()).unwrap();
            event.verify().unwrap();
            send(&mut ws, json!(["OK", event.id.to_hex(), true, ""])).await;
            ws
        })
        .await
        .unwrap()
    });
    (url, task)
}
fn status() -> (watch::Sender<Status>, watch::Receiver<Status>) {
    watch::channel(Status::new(&config::Config::default()))
}
fn fast() -> FreshnessPolicy {
    FreshnessPolicy {
        interval: Duration::from_secs(1),
        response: Duration::from_millis(50),
        ..FRESHNESS
    }
}
#[test]
fn network_backoff_is_unbounded_and_capped() {
    let mut b = Backoff::default();
    for seconds in [1, 2, 4, 8, 16, 30, 30] {
        assert_eq!(
            b.delay_jittered("relay_timeout", 0.5),
            Some(Duration::from_secs(seconds))
        );
    }
    // Hours of outage (a VPN switch, suspend) never exhaust it.
    for failure in [
        "relay_unavailable",
        "relay_resource_limit",
        "relay_protocol_error",
    ] {
        for _ in 0..500 {
            assert_eq!(
                b.delay_jittered(failure, 0.5),
                Some(Duration::from_secs(30))
            );
        }
    }
    b.reset();
    assert_eq!(
        b.delay_jittered("relay_unavailable", 0.5),
        Some(Duration::from_secs(1))
    );
}
#[test]
fn backoff_jitter_stays_within_a_quarter_and_the_cap() {
    let mut b = Backoff::default();
    assert_eq!(
        b.delay_jittered("relay_timeout", 0.0),
        Some(Duration::from_millis(750))
    );
    assert_eq!(
        b.delay_jittered("relay_timeout", 0.999_999),
        Some(Duration::from_secs(2).mul_f64(0.75 + 0.5 * 0.999_999))
    );
    for _ in 0..6 {
        b.delay_jittered("relay_timeout", 0.5);
    }
    // At the cap the jitter only shortens the delay.
    assert_eq!(
        b.delay_jittered("relay_timeout", 0.0),
        Some(Duration::from_millis(22_500))
    );
    assert_eq!(
        b.delay_jittered("relay_timeout", 0.999_999),
        Some(Duration::from_secs(30))
    );
    let mut b = Backoff::default();
    let mut seen = std::collections::BTreeSet::new();
    for attempt in 0..200_u32 {
        let base = Duration::from_secs(1 << attempt.min(16)).min(Duration::from_secs(30));
        let delay = b.delay("relay_unavailable").unwrap();
        assert!(delay >= base.mul_f64(0.75) && delay <= base.mul_f64(1.25).min(b.cap));
        seen.insert(delay);
    }
    assert!(seen.len() > 50, "delays are not jittered");
}
#[test]
fn backoff_resets_only_after_a_stable_session() {
    let start = tokio::time::Instant::now();
    let mut b = Backoff::default();
    for _ in 0..8 {
        b.delay_jittered("relay_unavailable", 0.5);
    }
    // A session that answers probes but drops again within a minute keeps
    // the long delay.
    b.healthy(start);
    b.healthy(start + Duration::from_secs(59));
    assert_eq!(
        b.delay_jittered("relay_unavailable", 0.5),
        Some(Duration::from_secs(30))
    );
    // A failure starts the stable period over.
    b.healthy(start + Duration::from_secs(70));
    b.healthy(start + Duration::from_secs(129));
    assert_eq!(b.failures, 9);
    b.healthy(start + Duration::from_secs(130));
    assert_eq!(b.failures, 0);
    assert_eq!(
        b.delay_jittered("relay_unavailable", 0.5),
        Some(Duration::from_secs(1))
    );
}
#[test]
fn fatal_categories_still_stop_the_backoff() {
    let mut b = Backoff::default();
    for failure in [
        "auth_rejected",
        "identity_unavailable",
        "identity_missing",
        "identity_locked",
        "invalid_config",
        "config_unavailable",
    ] {
        assert!(b.delay(failure).is_none(), "{failure} retried");
    }
    // Also after network failures, and they leave no retrying category.
    b.delay("relay_unavailable");
    assert!(b.delay("auth_rejected").is_none());
    assert_eq!(b.retrying(), None);
}
#[tokio::test]
async fn retried_failures_are_shown_as_reconnecting() {
    let (tx, rx) = status();
    let (_send, mut retry) = mpsc::channel(1);
    let setup = crate::setup::Setup::unavailable();
    let mut backoff = Backoff {
        unit: Duration::from_millis(1),
        ..Backoff::default()
    };
    assert!(
        wait_after_failure(
            "relay_unavailable",
            &mut backoff,
            &mut retry,
            &tx,
            &setup,
            None
        )
        .await
    );
    assert_eq!(rx.borrow().connection, "disconnected");
    assert_eq!(rx.borrow().category.as_deref(), Some("relay_unavailable"));
    assert!(rx.borrow().reconnecting);
    // The next attempt is still the reconnection; a session ends it.
    update(&tx, "connecting", None);
    assert!(rx.borrow().reconnecting);
    update(&tx, "authenticated", None);
    assert!(!rx.borrow().reconnecting);
    // A failure that is not retried waits for Retry and says so.
    assert!(wait_after_failure("relay_timeout", &mut backoff, &mut retry, &tx, &setup, None).await);
    assert!(rx.borrow().reconnecting);
    let waiting = timeout(
        Duration::from_millis(100),
        wait_after_failure("auth_rejected", &mut backoff, &mut retry, &tx, &setup, None),
    )
    .await;
    assert!(waiting.is_err(), "auth_rejected was retried");
    assert_eq!(rx.borrow().connection, "disconnected");
    assert!(!rx.borrow().reconnecting);
}
#[test]
fn probe_answers_are_exact_and_refusals_keep_the_session() {
    let count = |id: &str| RelayMessage::Count {
        subscription_id: id.into(),
        count: 0,
    };
    let closed = |id: &str, message: &str| RelayMessage::Closed {
        subscription_id: id.into(),
        message: message.into(),
    };
    let probe = Some("omarchy-buzz-liveness-1");
    assert_eq!(
        probe_answer(&count("omarchy-buzz-liveness-1"), probe),
        Some(ProbeAnswer::Counted)
    );
    for refusal in [
        "rate-limited: too many concurrent requests",
        "error: database error",
        "",
    ] {
        assert_eq!(
            probe_answer(&closed("omarchy-buzz-liveness-1", refusal), probe),
            Some(ProbeAnswer::Refused)
        );
    }
    assert_eq!(
        probe_answer(
            &closed("omarchy-buzz-liveness-1", "auth-required: sign in"),
            probe
        ),
        Some(ProbeAnswer::SignedOut)
    );
    assert_eq!(probe_answer(&count("other"), probe), None);
    assert_eq!(probe_answer(&closed("other", "error: x"), probe), None);
    assert_eq!(probe_answer(&count("omarchy-buzz-liveness-1"), None), None);
}
#[tokio::test]
async fn silent_authenticated_socket_loses_freshness() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let (url, server) = authenticated_listener().await;
    let mut conn = connect_identity(&url, &keys).await.unwrap();
    let mut ws = server.await.unwrap();
    let (tx, rx) = status();
    let (_send, mut retry) = mpsc::channel(1);
    let mut backoff = Backoff::default();
    let mut pin = None;
    let observe = observe_connection(
        &mut conn,
        &keys,
        &url,
        &mut pin,
        &tx,
        &mut retry,
        &mut backoff,
        fast(),
    );
    tokio::pin!(observe);
    tokio::select! {outcome=&mut observe=>assert!(matches!(outcome,ConnectionExit::Failure("relay_timeout"))),_ = async {let _=ws.next().await;tokio::time::sleep(Duration::from_secs(2)).await;}=>panic!("freshness timer failed")}
    assert_ne!(rx.borrow().connection, "authenticated");
}
#[tokio::test]
async fn exact_count_ack_authenticates_and_mismatched_chatter_does_not() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let public = keys.public_key().to_hex();
    let (url, server) = authenticated_listener().await;
    let mut conn = connect_identity(&url, &keys).await.unwrap();
    let mut ws = server.await.unwrap();
    let (tx, mut rx) = status();
    let (send_retry, mut retry) = mpsc::channel(1);
    let mut backoff = Backoff {
        failures: 4,
        ..Backoff::default()
    };
    let mut pin = None;
    let server = tokio::spawn(async move {
        let Message::Text(text) = ws.next().await.unwrap().unwrap() else {
            panic!("COUNT expected")
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(frame[0], "COUNT");
        assert_eq!(frame[2], json!({"kinds":[0],"authors":[public],"limit":1}));
        send(&mut ws, json!(["COUNT","unrelated",{"count":0}])).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        send(&mut ws, json!(["COUNT",frame[1],{"count":0}])).await;
        timeout(Duration::from_secs(1), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rx.borrow().connection, "authenticated");
        send_retry.send(Command::Retry).await.unwrap();
    });
    assert!(matches!(
        observe_connection(
            &mut conn,
            &keys,
            &url,
            &mut pin,
            &tx,
            &mut retry,
            &mut backoff,
            fast()
        )
        .await,
        ConnectionExit::Retry
    ));
    assert_eq!(backoff.failures, 0);
    server.await.unwrap();
}
#[tokio::test]
async fn unrelated_frames_cannot_extend_probe_deadline() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let (url, server) = authenticated_listener().await;
    let mut conn = connect_identity(&url, &keys).await.unwrap();
    let mut ws = server.await.unwrap();
    let (tx, _rx) = status();
    let (_send, mut retry) = mpsc::channel(1);
    let mut backoff = Backoff::default();
    let mut pin = None;
    let server = tokio::spawn(async move {
        let _ = ws.next().await;
        for _ in 0..30 {
            if ws
                .send(Message::Text(
                    json!(["COUNT","wrong",{"count":0}]).to_string().into(),
                ))
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    });
    let result = timeout(
        Duration::from_millis(200),
        observe_connection(
            &mut conn,
            &keys,
            &url,
            &mut pin,
            &tx,
            &mut retry,
            &mut backoff,
            fast(),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(result, ConnectionExit::Failure("relay_timeout")));
    drop(conn);
    server.await.unwrap();
}
#[tokio::test]
async fn refused_probe_and_undecodable_frames_keep_the_session() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let (url, server) = authenticated_listener().await;
    let mut conn = connect_identity(&url, &keys).await.unwrap();
    let mut ws = server.await.unwrap();
    let (tx, mut rx) = status();
    let (send_retry, mut retry) = mpsc::channel(1);
    let mut backoff = Backoff::default();
    let mut pin = None;
    let server = tokio::spawn(async move {
        // Two probes in a row are refused as a busy relay does, each after
        // undecodable frames; the session becomes and stays fresh.
        for _ in 0..2 {
            let Message::Text(text) = ws.next().await.unwrap().unwrap() else {
                panic!("COUNT expected")
            };
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(frame[0], "COUNT");
            ws.send(Message::Text("[\"NOT-NOSTR\",1]".into()))
                .await
                .unwrap();
            ws.send(Message::Text("{broken".into())).await.unwrap();
            send(
                &mut ws,
                json!([
                    "CLOSED",
                    frame[1],
                    "rate-limited: too many concurrent requests"
                ]),
            )
            .await;
            timeout(
                Duration::from_secs(1),
                rx.wait_for(|s| s.connection == "authenticated"),
            )
            .await
            .unwrap()
            .unwrap();
        }
        send_retry.send(Command::Retry).await.unwrap();
        ws
    });
    let result = timeout(
        Duration::from_secs(5),
        observe_connection(
            &mut conn,
            &keys,
            &url,
            &mut pin,
            &tx,
            &mut retry,
            &mut backoff,
            fast(),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(result, ConnectionExit::Retry));
    drop(server.await.unwrap());
}
#[tokio::test]
async fn auth_required_probe_refusal_reconnects_with_backoff() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let (url, server) = authenticated_listener().await;
    let mut conn = connect_identity(&url, &keys).await.unwrap();
    let mut ws = server.await.unwrap();
    let (tx, _rx) = status();
    let (_send, mut retry) = mpsc::channel(1);
    let mut backoff = Backoff::default();
    let mut pin = None;
    let server = tokio::spawn(async move {
        let Message::Text(text) = ws.next().await.unwrap().unwrap() else {
            panic!("COUNT expected")
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        send(
            &mut ws,
            json!(["CLOSED", frame[1], "auth-required: sign in"]),
        )
        .await;
        ws
    });
    let result = observe_connection(
        &mut conn,
        &keys,
        &url,
        &mut pin,
        &tx,
        &mut retry,
        &mut backoff,
        fast(),
    )
    .await;
    assert!(matches!(
        result,
        ConnectionExit::Failure("relay_protocol_error")
    ));
    assert!(backoff.delay("relay_protocol_error").is_some());
    drop(server.await.unwrap());
}
