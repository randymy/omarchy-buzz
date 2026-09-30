// A relay that rejects a mid-session re-authentication is retried with the
// network backoff; a rejected first authentication still waits for Retry,
// unless the relay's `Date` header shows the local clock is off (`clock_skew`),
// which is retried with the same budget whether first or re-authentication.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
/// A loopback relay. WebSocket sessions are handed to the test in order; plain
/// HTTP requests on the same port (the rejection's `HEAD` for the clock, the
/// catalog's NIP-11 `GET`) are answered at once with 404 and the configured
/// `Date` header (none when unset), and their request lines are recorded.
struct Relay {
    sessions: tokio::sync::Mutex<mpsc::UnboundedReceiver<Ws>>,
    date: Arc<Mutex<Option<String>>>,
    http: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Relay {
    fn set_date(&self, date: Option<String>) {
        *self.date.lock().unwrap() = date;
    }
    fn http(&self) -> Vec<String> {
        self.http.lock().unwrap().clone()
    }
}
/// The request head, peeked so a WebSocket upgrade can still read it.
async fn peek_head(tcp: &TcpStream) -> Option<String> {
    let mut buf = [0_u8; 4096];
    for _ in 0..400 {
        let n = tcp.peek(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        if text.contains("\r\n\r\n") {
            return Some(text);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    None
}
async fn serve_http(mut tcp: TcpStream, head: String, date: Option<String>) {
    let end = head.find("\r\n\r\n").unwrap() + 4;
    let mut consumed = vec![0_u8; end];
    if tcp.read_exact(&mut consumed).await.is_err() {
        return;
    }
    let date = date.map_or(String::new(), |d| format!("Date: {d}\r\n"));
    let answer =
        format!("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n{date}\r\n");
    let _ = tcp.write_all(answer.as_bytes()).await;
    let _ = tcp.shutdown().await;
}
/// The next WebSocket session.
async fn next_ws(relay: &Relay) -> Ws {
    relay
        .sessions
        .lock()
        .await
        .recv()
        .await
        .expect("relay fixture stopped")
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
async fn authenticated_then_rejected(listener: &Relay) {
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
                &crate::setup::Setup::system(),
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
async fn listener() -> (Relay, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let (sessions, receiver) = mpsc::unbounded_channel();
    let date = Arc::new(Mutex::new(None::<String>));
    let http = Arc::new(Mutex::new(Vec::new()));
    let (shared_date, shared_http) = (date.clone(), http.clone());
    let task = tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let Some(head) = peek_head(&tcp).await else {
                continue;
            };
            if head.to_ascii_lowercase().contains("upgrade: websocket") {
                if let Ok(ws) = accept_async(tcp).await {
                    let _ = sessions.send(ws);
                }
            } else {
                let line = head.lines().next().unwrap_or_default().to_owned();
                shared_http.lock().unwrap().push(line);
                let date = shared_date.lock().unwrap().clone();
                tokio::spawn(serve_http(tcp, head, date));
            }
        }
    });
    let relay = Relay {
        sessions: tokio::sync::Mutex::new(receiver),
        date,
        http,
        task,
    };
    (relay, url)
}
/// An IMF-fixdate `offset` seconds from the local clock (the relay's view
/// when the local clock is `offset` seconds behind).
fn http_date(offset: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let t = crate::clock::now() + offset;
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        DAYS[days.rem_euclid(7) as usize],
        day,
        MONTHS[(month - 1) as usize],
        year,
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}
/// The local clock is 73 minutes behind the relay.
const BEHIND: i64 = 73 * 60;
fn near(skew: Option<i64>, expected: i64) -> bool {
    skew.is_some_and(|s| (s - expected).abs() <= 2)
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
    // One HEAD measured the clock; without a `Date` the offset stays unknown.
    assert_eq!(listener.http(), ["HEAD /info HTTP/1.1"]);
    assert_eq!(client.tx.borrow().clock_skew_seconds, None);
}

#[test]
fn the_panel_accepts_exactly_the_connection_categories() {
    // An older panel refuses a frame with an unknown category: both change together.
    let service = include_str!("../../plugin/Service.qml");
    let line = service
        .lines()
        .find(|l| {
            l.trim_start()
                .starts_with("var categories = [\"identity_access_pending\"")
        })
        .expect("panel category list");
    let listed: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    assert_eq!(listed, crate::protocol::CONNECTION_CATEGORIES);
}

#[test]
fn clock_skew_retries_with_the_network_budget() {
    let mut b = Backoff::default();
    for seconds in [1, 2, 4] {
        assert_eq!(b.delay("clock_skew"), Some(Duration::from_secs(seconds)));
        assert_eq!(b.retrying(), Some("clock_skew"));
    }
    // A network failure in between shares the budget and is shown as such.
    assert_eq!(b.delay("relay_unavailable"), Some(Duration::from_secs(8)));
    assert_eq!(b.retrying(), None);
    assert_eq!(b.delay("clock_skew"), Some(Duration::from_secs(16)));
    assert!(b.delay("clock_skew").is_none());
    assert_eq!(b.retrying(), None);
    b.reset();
    assert_eq!(b.delay("clock_skew"), Some(Duration::from_secs(1)));
    b.reset();
    assert_eq!(b.retrying(), None);
}

#[tokio::test]
async fn skewed_first_authentication_reads_clock_skew_and_recovers() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    listener.set_date(Some(http_date(BEHIND)));
    let client = Client::start(url, Duration::from_millis(200));
    let mut ws = next_ws(&listener).await;
    challenge(&mut ws, "initial", false).await;
    client.reaches("connecting", Some("clock_skew")).await;
    assert!(near(client.tx.borrow().clock_skew_seconds, BEHIND));
    // The clock is fixed meanwhile: the automatic attempt is accepted.
    listener.set_date(Some(http_date(0)));
    let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
        .await
        .expect("no automatic retry");
    challenge(&mut ws, "retry", true).await;
    let server = tokio::spawn(serve_counts(ws));
    client.reaches("authenticated", None).await;
    let seen = client.seen();
    assert!(!seen.iter().any(|e| e.1.as_deref() == Some("auth_rejected")));
    assert_eq!(client.commands.capacity(), 4);
    // Exactly one measurement for the one rejection.
    assert_eq!(
        listener
            .http()
            .iter()
            .filter(|l| l.starts_with("HEAD "))
            .count(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn skewed_reauthentication_reads_clock_skew() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    // The relay's clock is behind this one (the local clock ran fast).
    listener.set_date(Some(http_date(-BEHIND)));
    let client = Client::start(url, Duration::from_millis(200));
    authenticated_then_rejected(&listener).await;
    client.reaches("connecting", Some("clock_skew")).await;
    assert!(near(client.tx.borrow().clock_skew_seconds, -BEHIND));
    let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
        .await
        .expect("no automatic retry");
    challenge(&mut ws, "retry", true).await;
    let server = tokio::spawn(serve_counts(ws));
    client.reaches("authenticated", None).await;
    assert!(!client
        .seen()
        .iter()
        .any(|e| e.1.as_deref() == Some("auth_rejected")));
    server.abort();
}

#[tokio::test]
async fn small_skew_keeps_auth_rejected() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    listener.set_date(Some(http_date(30)));
    let client = Client::start(url, Duration::from_millis(10));
    let mut ws = next_ws(&listener).await;
    challenge(&mut ws, "initial", false).await;
    client.reaches("disconnected", Some("auth_rejected")).await;
    assert!(near(client.tx.borrow().clock_skew_seconds, 30));
    assert!(
        timeout(Duration::from_millis(500), next_ws(&listener))
            .await
            .is_err(),
        "a rejection the clock does not explain was retried"
    );
    assert!(!client
        .seen()
        .iter()
        .any(|e| e.1.as_deref() == Some("clock_skew")));
}

#[tokio::test]
async fn five_skewed_retries_stop_and_wait_for_retry() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (listener, url) = listener().await;
    listener.set_date(Some(http_date(BEHIND)));
    let client = Client::start(url, Duration::from_millis(20));
    let mut ws = next_ws(&listener).await;
    challenge(&mut ws, "initial", false).await;
    for attempt in 0..5 {
        let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
            .await
            .unwrap_or_else(|_| panic!("automatic retry {attempt} missing"));
        challenge(&mut ws, "retry", false).await;
    }
    client.reaches("disconnected", Some("clock_skew")).await;
    assert!(
        timeout(Duration::from_millis(1000), next_ws(&listener))
            .await
            .is_err(),
        "retried past the budget"
    );
    let seen = client.seen();
    assert!(seen.contains(&entry("connecting", Some("clock_skew"))));
    assert_eq!(
        seen.last(),
        Some(&entry("disconnected", Some("clock_skew")))
    );
    assert!(!seen.iter().any(|e| e.1.as_deref() == Some("auth_rejected")));
    // One HEAD per rejection, never more.
    assert_eq!(listener.http(), vec!["HEAD /info HTTP/1.1"; 6]);
    // Retry starts a fresh budget; the clock is fixed and it authenticates.
    listener.set_date(Some(http_date(0)));
    client.commands.send(Command::Retry).await.unwrap();
    let mut ws = timeout(Duration::from_secs(3), next_ws(&listener))
        .await
        .expect("Retry did not reconnect");
    challenge(&mut ws, "after-retry", true).await;
    let server = tokio::spawn(serve_counts(ws));
    client.reaches("authenticated", None).await;
    server.abort();
}
