//! The agent-service socket protocol end to end over a Unix socket pair.
use super::*;
use crate::agents_service::{
    harness::FakeSpawner,
    keys::FakeKeyring,
    rooms::FixedRooms,
    test_support::{TempHome, ROOM_A},
    unit::FakeControl,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const INSTANCE: &str = "4242-1";

struct Session {
    _home: TempHome,
    service: Arc<Service>,
    write: tokio::net::unix::OwnedWriteHalf,
    read: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
}
async fn session() -> Session {
    let home = TempHome::new();
    let owner = nostr::Keys::generate().public_key().to_hex();
    let service = Service::open(
        home.paths.clone(),
        Deps {
            control: Arc::new(FakeControl::default()),
            keyring: Arc::new(FakeKeyring::default()),
            spawner: Arc::new(FakeSpawner::default()),
            rooms: Arc::new(FixedRooms::new(vec![ROOM_A.into()])),
        },
        Box::new(move || {
            Ok(crate::config::Config {
                relay: Some("ws://127.0.0.1:9/".into()),
                identity: Some(owner.clone()),
            })
        }),
    )
    .unwrap();
    let (server, client_side) = UnixStream::pair().unwrap();
    tokio::spawn(client(server, service.clone(), INSTANCE.into()));
    let (read, write) = client_side.into_split();
    Session {
        _home: home,
        service,
        write,
        read: BufReader::new(read).lines(),
    }
}
impl Session {
    async fn send(&mut self, value: serde_json::Value) {
        let mut line = serde_json::to_vec(&value).unwrap();
        line.push(b'\n');
        self.write.write_all(&line).await.unwrap();
    }
    async fn frame(&mut self) -> Option<serde_json::Value> {
        tokio::time::timeout(Duration::from_secs(5), self.read.next_line())
            .await
            .expect("frame deadline")
            .unwrap()
            .map(|l| serde_json::from_str(&l).unwrap())
    }
    /// The frame answering `id` (status updates may arrive first).
    async fn answer(&mut self, id: &str) -> serde_json::Value {
        loop {
            let frame = self.frame().await.expect("connection open");
            if frame["id"] == id {
                return frame;
            }
        }
    }
}
fn request(id: &str, value: serde_json::Value) -> serde_json::Value {
    let mut frame = serde_json::json!({"version":1,"id":id,"instanceId":INSTANCE});
    frame
        .as_object_mut()
        .unwrap()
        .extend(value.as_object().unwrap().clone());
    frame
}
const R1: &str = "00000000-0000-4000-8000-00000000c001";
const R2: &str = "00000000-0000-4000-8000-00000000c002";
const R3: &str = "00000000-0000-4000-8000-00000000c003";
const R4: &str = "00000000-0000-4000-8000-00000000c004";

#[tokio::test]
async fn hello_subscribe_create_and_errors() {
    let mut s = session().await;
    let hello = s.frame().await.unwrap();
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["id"], serde_json::Value::Null);
    assert_eq!(hello["instanceId"], INSTANCE);
    assert_eq!(hello["capabilities"], serde_json::json!(["agent_manager"]));
    assert_eq!(hello["status"]["agents"], serde_json::json!([]));
    assert_eq!(hello["status"]["pending"], serde_json::Value::Null);
    assert!(hello.get("generation").is_none());

    s.send(request(R1, serde_json::json!({"type":"subscribe"})))
        .await;
    let status = s.answer(R1).await;
    assert_eq!(status["type"], "status");

    let fields = serde_json::json!({"name":"Scout","description":"","instructions":"","harness":"codex",
        "model":"","rooms":[ROOM_A],"respondTo":"owner-only","workspace":"","startAtLogin":false,"acpCommand":"buzz-acp"});
    s.send(request(
        R2,
        serde_json::json!({"type":"create_agent","fields":fields}),
    ))
    .await;
    let done = s.answer(R2).await;
    assert_eq!(done["type"], "status");
    assert_eq!(
        done["status"]["pending"],
        serde_json::json!({"requestId":R2,"type":"create_agent","state":"done","category":null})
    );
    assert_eq!(done["status"]["agents"][0]["name"], "Scout");

    // A structurally valid request with a refused field: error frame, session continues.
    s.send(request(
        R3,
        serde_json::json!({"type":"create_agent","fields":{"name":"x"}}),
    ))
    .await;
    let error = s.answer(R3).await;
    assert_eq!(
        error,
        serde_json::json!({"version":1,"type":"error","id":R3,"instanceId":INSTANCE,"category":"agent_invalid"})
    );
    // Another daemon instance's request is refused.
    let mut stale = request(R3, serde_json::json!({"type":"subscribe"}));
    stale["instanceId"] = "other".into();
    s.send(stale).await;
    assert_eq!(s.answer(R3).await["category"], "agent_invalid");
    // One mutation at a time.
    let held = s.service.begin().unwrap();
    s.send(request(
        R4,
        serde_json::json!({"type":"sign_in","harness":"codex"}),
    ))
    .await;
    assert_eq!(s.answer(R4).await["category"], "agent_busy");
    drop(held);
    // A frame that is not a request ends the session.
    s.write.write_all(b"not-json\n").await.unwrap();
    loop {
        match s.frame().await {
            None => break,
            Some(frame) => assert_ne!(frame["type"], "error"),
        }
    }
}

#[tokio::test]
async fn subscribed_clients_receive_every_change() {
    let mut s = session().await;
    s.frame().await.unwrap();
    s.send(request(R1, serde_json::json!({"type":"subscribe"})))
        .await;
    s.answer(R1).await;
    // A change made by another client (here directly) is pushed unsolicited.
    let service = s.service.clone();
    tokio::spawn(async move { service.inspect_harnesses().await });
    let pushed = s.frame().await.unwrap();
    assert_eq!(pushed["type"], "status");
    assert_eq!(pushed["id"], serde_json::Value::Null);
}

#[test]
fn bridge_forwards_requests_and_refuses_non_requests() {
    assert!(bridge_check(br#"{"version":1,"id":"00000000-0000-4000-8000-00000000c001","instanceId":"x","type":"create_agent"}"#).is_ok());
    assert!(bridge_check(b"not-json").is_err());
    assert!(bridge_check(br#"{"version":1,"id":"nope","type":"subscribe"}"#).is_err());
}
