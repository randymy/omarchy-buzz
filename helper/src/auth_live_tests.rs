//! Live trigger subscription through the production observer: synthetic
//! same-origin HTTP/WS relay on loopback, synthetic keys and rooms only.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Keys, Kind, Tag};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::Instant,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct AbortTask<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// Short cadences: 1 s head poll without live, 3 s while primed.
const POLICY: FreshnessPolicy = FreshnessPolicy {
    interval: Duration::from_secs(2),
    response: Duration::from_secs(1),
    catalog: Duration::from_secs(30),
    head: Duration::from_secs(1),
    live_poll: Duration::from_secs(3),
};

fn event(key: &Keys, kind: u16, content: &str, tags: Vec<Tag>, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags)
        .custom_created_at(nostr::Timestamp::from(at))
        .sign_with_keys(key)
        .unwrap()
}
fn tag(values: &[&str]) -> Tag {
    Tag::parse(values.iter().copied()).unwrap()
}
async fn request(stream: &mut TcpStream) -> (String, Option<Value>) {
    let mut head = Vec::new();
    loop {
        let mut byte = [0; 1];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
        head.push(byte[0]);
        assert!(head.len() < 16384);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
    let length = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length: "))
        .map(|n| n.trim().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(length <= 8192);
    let body = if length > 0 {
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await.unwrap();
        Some(serde_json::from_slice(&bytes).unwrap())
    } else {
        None
    };
    (head, body)
}

struct Relay {
    user: Keys,
    rooms: [String; 2],
    root: Event,
    status: watch::Receiver<Status>,
    commands: mpsc::Sender<Command>,
    /// REQ/CLOSE frames the helper sent, and `["AUTH"]` for each AUTH event.
    seen: mpsc::UnboundedReceiver<Value>,
    inject: mpsc::UnboundedSender<Value>,
    heads: Arc<Mutex<Vec<String>>>,
    threads: Arc<AtomicUsize>,
    _tasks: Vec<AbortTask<()>>,
}

impl Relay {
    async fn start() -> Self {
        let user = Keys::generate();
        let relay = Keys::generate();
        let public = user.public_key();
        let signer = relay.public_key();
        let rooms = [
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ];
        let now = nostr::Timestamp::now().as_secs();
        let root = event(
            &user,
            9,
            "synthetic root",
            vec![tag(&["h", &rooms[0]])],
            now - 20,
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let members: Vec<Event> = rooms
            .iter()
            .map(|room| {
                event(
                    &relay,
                    39002,
                    "",
                    vec![tag(&["d", room]), tag(&["p", &public.to_hex()])],
                    now - 60,
                )
            })
            .collect();
        let metadata: Vec<Event> = rooms
            .iter()
            .map(|room| {
                event(
                    &relay,
                    39000,
                    "",
                    vec![
                        tag(&["d", room]),
                        tag(&["name", "Synthetic"]),
                        tag(&["t", "stream"]),
                    ],
                    now - 60,
                )
            })
            .collect();
        let heads = Arc::new(Mutex::new(Vec::new()));
        let threads = Arc::new(AtomicUsize::new(0));
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let (inject, mut inject_rx) = mpsc::unbounded_channel::<Value>();
        let (server_relay, server_rooms, server_root, server_origin) =
            (relay.clone(), rooms.clone(), root.clone(), origin.clone());
        let (head_log, thread_count, server_user) = (heads.clone(), threads.clone(), user.clone());
        let server = AbortTask(tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH", "initial"]).to_string().into()))
                .await
                .unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                panic!("AUTH expected")
            };
            let frame: Value = serde_json::from_str(&text).unwrap();
            let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
            auth.verify().unwrap();
            assert!(auth
                .tags
                .iter()
                .any(|t| t.as_slice() == ["relay", server_origin.as_str()]));
            ws.send(Message::Text(
                json!(["OK", auth.id.to_hex(), true, ""]).to_string().into(),
            ))
            .await
            .unwrap();
            let _websocket = AbortTask(tokio::spawn(async move {
                loop {
                    tokio::select! {
                        frame = inject_rx.recv() => {
                            let Some(frame) = frame else { break };
                            if ws.send(Message::Text(frame.to_string().into())).await.is_err() { break; }
                        }
                        message = ws.next() => {
                            let Some(Ok(Message::Text(text))) = message else { break };
                            let value: Value = serde_json::from_str(&text).unwrap();
                            match value[0].as_str().unwrap() {
                                "COUNT" => ws.send(Message::Text(json!(["COUNT", value[1], {"count": 0}]).to_string().into())).await.unwrap(),
                                "AUTH" => {
                                    let auth: Event = serde_json::from_value(value[1].clone()).unwrap();
                                    auth.verify().unwrap();
                                    let _ = seen_tx.send(json!(["AUTH"]));
                                    ws.send(Message::Text(json!(["OK", auth.id.to_hex(), true, ""]).to_string().into())).await.unwrap();
                                }
                                "REQ" | "CLOSE" => { let _ = seen_tx.send(value); }
                                other => panic!("unexpected observer frame {other}"),
                            }
                        }
                    }
                }
            }));
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (head, body) = request(&mut stream).await;
                let payload = if head.starts_with("get /info ") {
                    json!({"self": signer.to_hex()}).to_string()
                } else {
                    let body = body.unwrap();
                    let filter = &body[0];
                    if filter["kinds"] == json!([39002]) {
                        serde_json::to_string(&members).unwrap()
                    } else if filter["kinds"] == json!([39000]) {
                        serde_json::to_string(&metadata).unwrap()
                    } else if filter.get("#e").is_some() {
                        thread_count.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(filter["#e"], json!([server_root.id.to_hex()]));
                        let at = nostr::Timestamp::now().as_secs() - 5;
                        let reply = event(
                            &server_user,
                            9,
                            "synthetic reply",
                            vec![
                                tag(&["h", &server_rooms[0]]),
                                tag(&["e", &server_root.id.to_hex(), "", "reply"]),
                            ],
                            at,
                        );
                        serde_json::to_string(&vec![reply]).unwrap()
                    } else {
                        let room = filter["#h"][0].as_str().unwrap().to_owned();
                        assert_eq!(
                            body,
                            json!([{"kinds":[9,40002],"#h":[room],"limit":20,"top_level":true,"include_aux":true,"include_summaries":true}])
                        );
                        head_log.lock().unwrap().push(room.clone());
                        let row = if room == server_rooms[0] {
                            server_root.clone()
                        } else {
                            event(
                                &server_user,
                                9,
                                "synthetic other",
                                vec![tag(&["h", &room])],
                                now - 20,
                            )
                        };
                        let bounds = event(
                            &server_relay,
                            39006,
                            r#"{"has_more":false,"next_cursor":null}"#,
                            vec![tag(&["h", &room]), tag(&["d", &format!("{room}:head")])],
                            now - 1,
                        );
                        serde_json::to_string(&vec![row, bounds]).unwrap()
                    }
                };
                let _ = stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            payload.len(),
                            payload
                        )
                        .as_bytes(),
                    )
                    .await;
            }
        }));
        let config = config::Config {
            relay: Some(origin.clone()),
            identity: Some(public.to_hex()),
        };
        let (tx, status) = watch::channel(Status::new(&config));
        let (commands, mut command_rx) = mpsc::channel(8);
        let mut conn = connect_identity(&origin, &user).await.unwrap();
        let observer_user = user.clone();
        let observer = AbortTask(tokio::spawn(async move {
            let mut pin = None;
            let mut backoff = Backoff::default();
            observe_connection(
                &mut conn,
                &observer_user,
                &origin,
                &mut pin,
                &tx,
                &mut command_rx,
                &mut backoff,
                POLICY,
            )
            .await;
        }));
        let mut fixture = Relay {
            user,
            rooms,
            root,
            status,
            commands,
            seen,
            inject,
            heads,
            threads,
            _tasks: vec![observer, server],
        };
        fixture.wait(|s| s.catalog.rooms.len() == 2).await;
        fixture
    }
    async fn wait(&mut self, test: impl Fn(&Status) -> bool) {
        timeout(Duration::from_secs(8), async {
            loop {
                if test(&self.status.borrow()) {
                    break;
                }
                self.status.changed().await.unwrap();
            }
        })
        .await
        .expect("status deadline");
    }
    async fn until(&self, what: &str, test: impl Fn(&Self) -> bool) {
        timeout(Duration::from_secs(8), async {
            while !test(self) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("deadline: {what}"));
    }
    async fn frame(&mut self) -> Value {
        timeout(Duration::from_secs(8), self.seen.recv())
            .await
            .expect("helper frame")
            .unwrap()
    }
    fn quiet(&mut self) {
        assert!(self.seen.try_recv().is_err(), "unexpected helper frame");
    }
    fn heads(&self, room: usize) -> usize {
        self.heads
            .lock()
            .unwrap()
            .iter()
            .filter(|r| **r == self.rooms[room])
            .count()
    }
    fn threads(&self) -> usize {
        self.threads.load(Ordering::SeqCst)
    }
    fn send(&self, frame: Value) {
        self.inject.send(frame).unwrap();
    }
    /// Select a room and return the live subscription requested after its head page.
    async fn select(&mut self, room: usize) -> String {
        let id = self.rooms[room].clone();
        self.commands
            .send(Command::FetchRecent(id.clone()))
            .await
            .unwrap();
        self.wait(|s| {
            s.history.state == "snapshot" && s.history.room_id.as_deref() == Some(id.as_str())
        })
        .await;
        let request = self.frame().await;
        let sub = request[1].as_str().unwrap().to_owned();
        assert!(sub.starts_with("omarchy-buzz-live-") && sub.len() <= 64);
        let since = request[2]["since"].as_u64().unwrap();
        assert!(since.abs_diff(nostr::Timestamp::now().as_secs()) <= 5);
        assert_eq!(
            request,
            json!(["REQ", sub, {"kinds":[9,40002,40003,5,9005,7,39005],"#h":[id],"since":since}])
        );
        assert!(
            !self.status.borrow().history.live,
            "live claimed before EOSE"
        );
        sub
    }
    async fn prime(&mut self, sub: &str) {
        self.send(json!(["EOSE", sub]));
        self.wait(|s| s.history.live).await;
    }
    fn message(&self, room: usize, content: &str, extra: Vec<Tag>) -> Event {
        let mut tags = vec![tag(&["h", &self.rooms[room]])];
        tags.extend(extra);
        event(
            &self.user,
            40002,
            content,
            tags,
            nostr::Timestamp::now().as_secs(),
        )
    }
}

#[tokio::test]
async fn verified_events_trigger_debounced_refetches_and_floods_close() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(40), async {
        let mut relay = Relay::start().await;
        let sub = relay.select(0).await;
        relay.prime(&sub).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        // One event: one head refetch.
        let before = relay.heads(0);
        relay.send(json!([
            "EVENT",
            sub,
            relay.message(0, "synthetic live", vec![])
        ]));
        relay
            .until("triggered refetch", |r| r.heads(0) == before + 1)
            .await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(relay.heads(0), before + 1);
        // A burst coalesces into one refetch.
        for n in 0..10 {
            relay.send(json!([
                "EVENT",
                sub,
                relay.message(0, &format!("burst {n}"), vec![])
            ]));
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(relay.heads(0), before + 2, "burst was not coalesced");
        assert_eq!(relay.threads(), 0, "no thread is open");
        // Frames that are not valid triggers cause no work.
        let mut forged = relay.message(0, "synthetic", vec![]);
        forged.content = "tampered".into();
        let invalid = [
            forged,
            relay.message(1, "another room", vec![]),
            event(
                &relay.user,
                1,
                "unsubscribed kind",
                vec![tag(&["h", &relay.rooms[0]])],
                nostr::Timestamp::now().as_secs(),
            ),
            relay.message(0, &"x".repeat(crate::live::MAX_EVENT_BYTES), vec![]),
            event(
                &relay.user,
                39005,
                "{}",
                vec![tag(&["h", &relay.rooms[0]])],
                nostr::Timestamp::now().as_secs(),
            ),
        ];
        for frame in &invalid {
            relay.send(json!(["EVENT", sub, frame]));
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(
            relay.heads(0),
            before + 2,
            "an invalid frame triggered a refetch"
        );
        assert!(relay.status.borrow().history.live);
        relay.quiet();
        // More than 200 unverifiable frames close the subscription.
        let bad = &invalid[2];
        for _ in 0..196 {
            relay.send(json!(["EVENT", sub, bad]));
        }
        assert_eq!(relay.frame().await, json!(["CLOSE", sub]));
        relay
            .wait(|s| !s.history.live && s.history.state == "snapshot")
            .await;
        // Polling resumes at the fallback cadence; no re-arm during the pause.
        let after = relay.heads(0);
        relay
            .until("fallback polling", |r| r.heads(0) >= after + 2)
            .await;
        relay.quiet();
    })
    .await
    .expect("live trigger fixture deadline");
}

#[tokio::test]
async fn primed_subscription_slows_head_polling() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut relay = Relay::start().await;
        let sub = relay.select(0).await;
        // Not primed: the 1-second fallback cadence.
        let start = relay.heads(0);
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(
            relay.heads(0) >= start + 2,
            "fallback polling stopped before EOSE"
        );
        assert!(!relay.status.borrow().history.live);
        relay.prime(&sub).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let primed = relay.heads(0);
        tokio::time::sleep(Duration::from_millis(2000)).await;
        assert_eq!(relay.heads(0), primed, "primed head poll ran early");
        relay
            .until("primed head poll", |r| r.heads(0) == primed + 1)
            .await;
        assert!(relay.status.borrow().history.live);
        relay.quiet();
    })
    .await
    .expect("live cadence fixture deadline");
}

#[tokio::test]
async fn reauthentication_closes_before_auth_and_rearms_after_head() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut relay = Relay::start().await;
        let sub = relay.select(0).await;
        relay.prime(&sub).await;
        relay.send(json!(["AUTH", "again"]));
        assert_eq!(
            relay.frame().await,
            json!(["CLOSE", sub]),
            "CLOSE must precede AUTH"
        );
        assert_eq!(relay.frame().await, json!(["AUTH"]));
        relay.wait(|s| s.connection == "connecting").await;
        assert!(!relay.status.borrow().history.live);
        relay
            .wait(|s| s.connection == "authenticated" && s.catalog.rooms.len() == 2)
            .await;
        // Nothing is re-armed until a room's head page is read again.
        tokio::time::sleep(Duration::from_millis(500)).await;
        relay.quiet();
        let again = relay.select(0).await;
        assert_ne!(again, sub);
        relay.prime(&again).await;
    })
    .await
    .expect("live reauthentication fixture deadline");
}

#[tokio::test]
async fn relay_closed_falls_back_to_polling_and_retries_after_backoff() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut relay = Relay::start().await;
        let sub = relay.select(0).await;
        relay.prime(&sub).await;
        relay.send(json!(["CLOSED", sub, "error: synthetic"]));
        let closed = Instant::now();
        relay.wait(|s| !s.history.live).await;
        let after = relay.heads(0);
        relay
            .until("fallback polling", |r| r.heads(0) >= after + 2)
            .await;
        let request = relay.frame().await;
        assert!(
            closed.elapsed() >= Duration::from_secs(5),
            "re-armed before the 5 s backoff"
        );
        let retried = request[1].as_str().unwrap();
        assert_ne!(retried, sub);
        assert_eq!(request[0], "REQ");
        assert_eq!(request[2]["#h"], json!([relay.rooms[0]]));
    })
    .await
    .expect("live closed fixture deadline");
}

#[tokio::test]
async fn room_change_and_reselection_reissue_and_retry_closes() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut relay = Relay::start().await;
        let first = relay.select(0).await;
        relay.prime(&first).await;
        relay
            .commands
            .send(Command::FetchRecent(relay.rooms[1].clone()))
            .await
            .unwrap();
        assert_eq!(relay.frame().await, json!(["CLOSE", first]));
        let other = relay.rooms[1].clone();
        relay
            .wait(|s| {
                s.history.room_id.as_deref() == Some(other.as_str())
                    && s.history.state == "snapshot"
            })
            .await;
        let request = relay.frame().await;
        let second = request[1].as_str().unwrap().to_owned();
        assert_eq!(request[2]["#h"], json!([relay.rooms[1]]));
        relay
            .commands
            .send(Command::FetchRecent(relay.rooms[1].clone()))
            .await
            .unwrap();
        assert_eq!(relay.frame().await, json!(["CLOSE", second]));
        let request = relay.frame().await;
        let third = request[1].as_str().unwrap().to_owned();
        assert_ne!(third, second);
        relay.commands.send(Command::Retry).await.unwrap();
        assert_eq!(
            relay.frame().await,
            json!(["CLOSE", third]),
            "Retry left the subscription open"
        );
    })
    .await
    .expect("live room change fixture deadline");
}

#[tokio::test]
async fn e_tagged_event_refetches_open_thread_which_also_refreshes_while_live() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut relay = Relay::start().await;
        let sub = relay.select(0).await;
        relay.prime(&sub).await;
        let root = relay.root.id.to_hex();
        relay
            .commands
            .send(Command::FetchThread(relay.rooms[0].clone(), root.clone()))
            .await
            .unwrap();
        relay
            .wait(|s| s.thread.state == "snapshot" && s.thread.rows.len() == 1)
            .await;
        let (threads, heads) = (relay.threads(), relay.heads(0));
        assert_eq!(threads, 1);
        relay.send(json!([
            "EVENT",
            sub,
            relay.message(0, "reply", vec![tag(&["e", &root, "", "reply"])])
        ]));
        relay
            .until("thread and head refetch", |r| {
                r.threads() == threads + 1 && r.heads(0) == heads + 1
            })
            .await;
        relay.send(json!(["EVENT", sub, relay.message(0, "top level", vec![])]));
        relay
            .until("head refetch", |r| r.heads(0) == heads + 2)
            .await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            relay.threads(),
            threads + 1,
            "top-level event refetched the thread"
        );
        // A reaction without `h` naming a held row is a trigger too.
        let reaction = event(
            &relay.user,
            7,
            "+",
            vec![tag(&["e", &root])],
            nostr::Timestamp::now().as_secs(),
        );
        relay.send(json!(["EVENT", sub, reaction]));
        relay
            .until("reaction refetch", |r| {
                r.heads(0) == heads + 3 && r.threads() == threads + 2
            })
            .await;
        // While live the helper refreshes the open thread itself (3 s here).
        relay
            .until("periodic thread refresh", |r| r.threads() == threads + 3)
            .await;
        assert_eq!(relay.status.borrow().thread.state, "snapshot");
    })
    .await
    .expect("live thread fixture deadline");
}
