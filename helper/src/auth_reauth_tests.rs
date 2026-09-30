// A relay that rejects a mid-session re-authentication is retried with the
// network backoff; a rejected first authentication still waits for Retry.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

type Ws = WebSocketStream<TcpStream>;

async fn send(ws: &mut Ws, frame: Value) {
    ws.send(Message::Text(frame.to_string().into()))
        .await
        .unwrap();
}
async fn frame(ws: &mut Ws) -> Option<Value> {
    loop {
        match ws.next().await? {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}
/// The next WebSocket session. Catalog discovery's plain HTTP requests to the
/// same port fail the upgrade and are skipped.
async fn next_ws(listener: &TcpListener) -> Ws {
    loop {
        let (tcp, _) = listener.accept().await.unwrap();
        if let Ok(ws) = accept_async(tcp).await {
            return ws;
        }
    }
}
/// Sends a challenge and answers the signed AUTH event.
async fn challenge(ws: &mut Ws, name: &str, accept: bool) {
    send(ws, json!(["AUTH", name])).await;
    loop {
        let value = frame(ws).await.expect("AUTH expected");
        match value[0].as_str().unwrap() {
            "AUTH" => {
                let event: Event = serde_json::from_value(value[1].clone()).unwrap();
                event.verify().unwrap();
                assert!(event
                    .tags
                    .iter()
                    .any(|t| t.as_slice() == ["challenge", name]));
                let reason = if accept {
                    ""
                } else {
                    "auth-required: rejected"
                };
                send(ws, json!(["OK", event.id.to_hex(), accept, reason])).await;
                return;
            }
            // Liveness probes or a CLOSE may precede a mid-session answer.
            "COUNT" | "CLOSE" | "REQ" => {}
            other => panic!("unexpected frame {other}"),
        }
    }
}
/// Answers liveness probes until the client goes away.
async fn serve_counts(mut ws: Ws) {
    while let Some(value) = frame(&mut ws).await {
        if value[0] == "COUNT" {
            send(&mut ws, json!(["COUNT", value[1], {"count": 0}])).await;
        }
    }
}
/// First session: authenticated and fresh, then a rejected re-authentication.
async fn authenticated_then_rejected(listener: &TcpListener) {
    let mut ws = next_ws(listener).await;
    challenge(&mut ws, "initial", true).await;
    loop {
        let value = frame(&mut ws).await.expect("COUNT expected");
        if value[0] == "COUNT" {
            send(&mut ws, json!(["COUNT", value[1], {"count": 0}])).await;
            break;
        }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    challenge(&mut ws, "again", false).await;
}
fn policy() -> FreshnessPolicy {
    FreshnessPolicy {
        interval: Duration::from_secs(1),
        response: Duration::from_secs(1),
        ..FRESHNESS
    }
}
struct Client {
    tx: Arc<watch::Sender<Status>>,
    seen: Arc<Mutex<Vec<(String, Option<String>)>>>,
    commands: mpsc::Sender<Command>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Client {
    /// Runs the connection loop exactly as `run` does after keys are loaded.
    fn start(url: String, unit: Duration) -> Self {
        let keys = Keys::generate();
        let tx = Arc::new(watch::channel(Status::new(&config::Config::default())).0);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut rx = tx.subscribe();
        let record = seen.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let status = rx.borrow_and_update();
                let entry = (status.connection.clone(), status.category.clone());
                let mut seen = record.lock().unwrap();
                if seen.last() != Some(&entry) {
                    seen.push(entry);
                }
            }
        });
        let (commands, mut retry) = mpsc::channel(4);
        let client_tx = tx.clone();
        let task = tokio::spawn(async move {
            let mut backoff = Backoff {
                unit,
                ..Backoff::default()
            };
            let mut sender = crate::sending::Sender::new(None);
            let mut pin = None;
            while connect_and_observe(
                &url,
                &keys,
                &mut pin,
                &client_tx,
                &mut retry,
                &mut backoff,
                policy(),
                &mut sender,
            )
            .await
            {}
        });
        Self {
            tx,
            seen,
            commands,
            task,
        }
    }
    async fn reaches(&self, connection: &str, category: Option<&str>) {
        let mut rx = self.tx.subscribe();
        timeout(
            Duration::from_secs(5),
            rx.wait_for(|s| s.connection == connection && s.category.as_deref() == category),
        )
        .await
        .unwrap_or_else(|_| panic!("never reached {connection}/{category:?}"))
        .unwrap();
    }
    fn seen(&self) -> Vec<(String, Option<String>)> {
        self.seen.lock().unwrap().clone()
    }
}
fn entry(connection: &str, category: Option<&str>) -> (String, Option<String>) {
    (connection.into(), category.map(str::to_owned))
}
async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    (listener, url)
}

#[test]
fn rejected_reauthentication_uses_the_network_budget() {
    let mut b = Backoff::default();
    assert!(b.delay("auth_rejected").is_none());
    assert_eq!(b.retrying(), None);
    b.reauth_rejected = true;
    assert_eq!(b.retrying(), Some("auth_rejected"));
    for seconds in [1, 2, 4, 8, 16] {
        assert_eq!(b.delay("auth_rejected"), Some(Duration::from_secs(seconds)));
    }
    assert!(b.delay("auth_rejected").is_none());
    // Exhaustion ends the automatic chain until Retry.
    assert_eq!(b.retrying(), None);
    b.reset();
    assert!(b.delay("auth_rejected").is_none());
}

#[tokio::test]
async fn rejected_reauthentication_recovers_without_a_command() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    let client = Client::start(url, Duration::from_millis(200));
    authenticated_then_rejected(&listener).await;
    // One automatic attempt after the first delay, accepted this time.
    let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
        .await
        .expect("no automatic retry");
    challenge(&mut ws, "retry", true).await;
    let server = tokio::spawn(serve_counts(ws));
    client.reaches("authenticated", None).await;
    let seen = client.seen();
    let rejected = seen
        .iter()
        .position(|e| e == &entry("connecting", Some("auth_rejected")))
        .expect("retry not published as connecting/auth_rejected");
    assert!(seen[..rejected].contains(&entry("authenticated", None)));
    assert!(!seen.contains(&entry("disconnected", Some("auth_rejected"))));
    assert_eq!(seen.last(), Some(&entry("authenticated", None)));
    // No Retry was sent: the command channel was never used.
    assert_eq!(client.commands.capacity(), 4);
    server.abort();
}

#[tokio::test]
async fn five_rejected_retries_stop_and_wait_for_retry() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    let client = Client::start(url, Duration::from_millis(20));
    authenticated_then_rejected(&listener).await;
    for attempt in 0..5 {
        let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
            .await
            .unwrap_or_else(|_| panic!("automatic retry {attempt} missing"));
        challenge(&mut ws, "retry", false).await;
    }
    client.reaches("disconnected", Some("auth_rejected")).await;
    // 1+2+4+8+16 units elapsed; a sixth attempt would come after at most 32.
    assert!(
        timeout(Duration::from_millis(1000), next_ws(&listener))
            .await
            .is_err(),
        "retried past the budget"
    );
    let seen = client.seen();
    assert!(seen.contains(&entry("connecting", Some("auth_rejected"))));
    assert_eq!(
        seen.last(),
        Some(&entry("disconnected", Some("auth_rejected")))
    );
    // Retry makes a first authentication again: rejected, it stops at once.
    client.commands.send(Command::Retry).await.unwrap();
    let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
        .await
        .expect("Retry did not reconnect");
    challenge(&mut ws, "after-retry", false).await;
    client.reaches("disconnected", Some("auth_rejected")).await;
    assert!(timeout(Duration::from_millis(300), next_ws(&listener))
        .await
        .is_err());
}

#[tokio::test]
async fn rejected_first_authentication_is_not_retried() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    let client = Client::start(url, Duration::from_millis(10));
    let mut ws = next_ws(&listener).await;
    challenge(&mut ws, "initial", false).await;
    client.reaches("disconnected", Some("auth_rejected")).await;
    assert!(
        timeout(Duration::from_millis(500), next_ws(&listener))
            .await
            .is_err(),
        "first-authentication rejection was retried"
    );
    assert!(!client
        .seen()
        .contains(&entry("connecting", Some("auth_rejected"))));
}
