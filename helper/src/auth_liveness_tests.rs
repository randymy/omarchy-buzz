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
fn backoff_is_bounded_and_network_only() {
    let mut b = Backoff::default();
    for seconds in [1, 2, 4, 8, 16] {
        assert_eq!(b.delay("relay_timeout"), Some(Duration::from_secs(seconds)));
    }
    assert!(b.delay("relay_timeout").is_none());
    b.reset();
    assert_eq!(b.delay("relay_unavailable"), Some(Duration::from_secs(1)));
    for failure in [
        "auth_rejected",
        "identity_unavailable",
        "relay_protocol_error",
        "invalid_config",
    ] {
        assert!(b.delay(failure).is_none());
    }
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
