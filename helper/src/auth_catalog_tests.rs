//! Production observer integration, entirely synthetic on one loopback origin.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Keys, Kind, Tag};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct AbortTask<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn signed(author: &Keys, kind: u16, tags: Vec<Tag>) -> Event {
    EventBuilder::new(Kind::Custom(kind), "")
        .tags(tags)
        .sign_with_keys(author)
        .unwrap()
}
async fn wait_status(rx: &mut watch::Receiver<Status>, predicate: impl Fn(&Status) -> bool) {
    timeout(Duration::from_secs(5), async {
        loop {
            if predicate(&rx.borrow()) {
                break;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("status transition deadline");
}

#[tokio::test]
async fn authenticated_catalog_is_partial_and_reauthentication_clears_it() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(12), async {
        let user=Keys::generate(); let relay_keys=Keys::generate();
        let signer=relay_keys.public_key(); let public=user.public_key();
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin=format!("ws://{}/",listener.local_addr().unwrap());
        let room=uuid::Uuid::new_v4().to_string();
        let membership=signed(&relay_keys,39002,vec![Tag::parse(["d",&room]).unwrap(),Tag::parse(["p",&public.to_hex(),"","member"]).unwrap()]);
        let metadata=signed(&relay_keys,39000,vec![Tag::parse(["d",&room]).unwrap(),Tag::parse(["name","Synthetic Lobby"]).unwrap(),Tag::parse(["t","stream"]).unwrap(),Tag::parse(["about","Loopback fixture"]).unwrap()]);
        let server_origin=origin.clone();
        let (challenge_send,challenge_wait)=oneshot::channel();
        let (ack_send,ack_wait)=oneshot::channel();
        let (finish_send,finish_wait)=oneshot::channel();
        let (ws_done_send,ws_done_wait)=oneshot::channel();
        let server=AbortTask(tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.unwrap();
            let mut ws=accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH","initial-fixture"]).to_string().into())).await.unwrap();
            let Message::Text(text)=ws.next().await.unwrap().unwrap() else {panic!("AUTH expected")};
            let auth:Value=serde_json::from_str(&text).unwrap();assert_eq!(auth[0],"AUTH");
            let event:Event=serde_json::from_value(auth[1].clone()).unwrap();event.verify().unwrap();
            assert_eq!(event.pubkey,public);assert_eq!(event.kind,Kind::Authentication);
            assert!(event.tags.iter().any(|t|t.as_slice()==["relay",server_origin.as_str()]));
            ws.send(Message::Text(json!(["OK",event.id.to_hex(),true,""]).to_string().into())).await.unwrap();
            let websocket=AbortTask(tokio::spawn(async move {
                let mut challenge_wait=challenge_wait;let mut finish_wait=finish_wait;
                let mut ack_wait=Some(ack_wait);let mut challenged=false;
                loop {
                    tokio::select! {
                        _=&mut finish_wait=>break,
                        _=&mut challenge_wait,if !challenged=>{
                            challenged=true;
                            ws.send(Message::Text(json!(["AUTH","reauth-fixture"]).to_string().into())).await.unwrap();
                        },
                        frame=ws.next()=>{
                            let Some(Ok(Message::Text(text)))=frame else {break;};
                            let value:Value=serde_json::from_str(&text).unwrap();
                            match value[0].as_str().unwrap() {
                                "COUNT"=>{
                                    assert_eq!(value[2],json!({"kinds":[0],"authors":[public.to_hex()],"limit":1}));
                                    // An unrelated ID must never satisfy the pending probe.
                                    ws.send(Message::Text(json!(["COUNT","unrelated",{"count":0}]).to_string().into())).await.unwrap();
                                    ws.send(Message::Text(json!(["COUNT",value[1],{"count":0}]).to_string().into())).await.unwrap();
                                },
                                "AUTH"=>{
                                    assert!(challenged);
                                    let event:Event=serde_json::from_value(value[1].clone()).unwrap();event.verify().unwrap();
                                    assert_eq!(event.pubkey,public);
                                    assert!(event.tags.iter().any(|t|t.as_slice()==["challenge","reauth-fixture"]));
                                    timeout(Duration::from_secs(3),ack_wait.take().unwrap()).await.unwrap().unwrap();
                                    ws.send(Message::Text(json!(["OK",event.id.to_hex(),true,""]).to_string().into())).await.unwrap();
                                },
                                other=>panic!("observer emitted unsupported request {other}"),
                            }
                        }
                    }
                }
                ws_done_send.send(()).unwrap();
            }));
            let responses=[json!({"self":signer.to_hex()}).to_string(),serde_json::to_string(&vec![membership]).unwrap(),serde_json::to_string(&vec![metadata]).unwrap()];
            for (index,payload) in responses.into_iter().enumerate() {
                let (mut stream,_)=listener.accept().await.unwrap();
                let mut head=Vec::new();
                loop {
                    let mut byte=[0;1];assert_eq!(stream.read(&mut byte).await.unwrap(),1);
                    head.push(byte[0]);assert!(head.len()<16384);
                    if head.ends_with(b"\r\n\r\n") {break;}
                }
                let head=String::from_utf8(head).unwrap().to_ascii_lowercase();
                if index==0 {assert!(head.starts_with("get /info "));assert!(!head.contains("authorization:"));}
                else {
                    assert!(head.starts_with("post /query "));
                    assert!(head.contains("authorization: nostr "));
                    let length=head.lines().find_map(|l|l.strip_prefix("content-length: ")).unwrap().trim().parse::<usize>().unwrap();
                    assert!(length<8192);let mut body=vec![0;length];stream.read_exact(&mut body).await.unwrap();
                    let filters:Value=serde_json::from_slice(&body).unwrap();
                    assert_eq!(filters[0]["kinds"][0],if index==1 {39002}else{39000});
                }
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload).as_bytes()).await.unwrap();
            }
            // Keep the same-origin listener/socket alive until deliberate Retry.
            timeout(Duration::from_secs(5),ws_done_wait).await.unwrap().unwrap();
            drop(websocket);
        }));
        let config=config::Config{relay:Some(origin.clone()),identity:Some(public.to_hex()),communities:Vec::new()};
        let (tx,mut status)=watch::channel(Status::new(&config));
        let (send_retry,mut retry)=mpsc::channel(1);
        let mut connection=connect_identity(&origin,&user).await.unwrap();
        let observer=AbortTask(tokio::spawn(async move {
            let mut backoff=Backoff::default();let mut pin=None;
            let result=observe_connection(&mut connection,&user,&origin,&mut pin,&tx,&mut retry,&mut backoff,
                FreshnessPolicy{interval:Duration::from_secs(2),response:Duration::from_secs(1),..FRESHNESS}).await;
            assert_eq!(pin,Some(signer));
            result
        }));
        wait_status(&mut status,|s|s.catalog.state=="partial" && s.catalog.rooms.len()==1).await;
        {
            let state=status.borrow();assert_eq!(state.connection,"authenticated");
            assert_eq!(state.catalog.category.as_deref(),Some("room_catalog_partial"));
            assert_eq!(state.catalog.rooms[0].id,room);assert_eq!(state.catalog.rooms[0].name,"Synthetic Lobby");
            assert_eq!(state.catalog.rooms[0].description,"Loopback fixture");
        }
        challenge_send.send(()).unwrap();
        wait_status(&mut status,|s|s.connection=="connecting" && s.catalog.rooms.is_empty()).await;
        assert_ne!(status.borrow().catalog.state,"partial");
        ack_send.send(()).unwrap();
        send_retry.send(Command::Retry).await.unwrap();
        // Await through mutable handle, retaining abort-on-panic cleanup.
        let mut observer=observer;
        let exit=timeout(Duration::from_secs(3),&mut observer.0).await.unwrap().unwrap();
        assert!(matches!(exit,ConnectionExit::Retry));
        let _=finish_send.send(());
        let mut server=server;timeout(Duration::from_secs(3),&mut server.0).await.unwrap().unwrap();
    }).await.expect("end-to-end fixture deadline");
}

// Periodic joined-room check fixture: two synthetic rooms on one loopback
// origin, with each discovery optionally held open, failed, or narrowed.
struct Discovery {
    // None answers the relay information request with an error.
    rooms: Option<Vec<String>>,
    gate: Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>,
}
struct Script {
    next: std::collections::VecDeque<Discovery>,
    joined: Vec<String>,
    // How the fixture answers a kind 41010 DM open, and what it received.
    dm: DmReply,
    dm_events: Vec<Event>,
    // Open rooms offered by an unscoped kind 39000 read, how a kind 9021/9022
    // is answered, and what was received.
    open: Vec<String>,
    room_reply: DmReply,
    room_events: Vec<Event>,
    // How a kind 30315 status is answered, every one received, and the
    // accepted ones the relay now serves to status reads.
    status_reply: DmReply,
    status_events: Vec<Event>,
    statuses: Vec<Event>,
    // How a kind 20001 heartbeat is answered, every one received, the state
    // the relay keeps per author, how many presence reads were served, and
    // whether those reads are signed by a key other than the relay's.
    presence_reply: DmReply,
    presence_events: Vec<Event>,
    presence: std::collections::BTreeMap<String, String>,
    presence_reads: usize,
    presence_forged: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum DmReply {
    Silent,
    Open,
    Reject,
}
// The DM the fixture opens: the viewer and the other roster member.
const DM_ROOM: &str = "33333333-3333-4333-8333-333333333333";
struct Recheck {
    a: String,
    b: String,
    script: std::sync::Arc<std::sync::Mutex<Script>>,
    injector: watch::Sender<Status>,
    status: watch::Receiver<Status>,
    commands: mpsc::Sender<Command>,
    discoveries: watch::Receiver<usize>,
    ledger: std::path::PathBuf,
    observer: AbortTask<ConnectionExit>,
    _server: AbortTask<()>,
}
fn note(author: &Keys, kind: u16, body: &str, tags: Vec<Tag>) -> Event {
    EventBuilder::new(Kind::Custom(kind), body)
        .tags(tags)
        .sign_with_keys(author)
        .unwrap()
}
async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, Option<Value>) {
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
        .find_map(|line| line.strip_prefix("content-length: "))
        .map(|value| value.trim().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(length <= 8192);
    if length == 0 {
        return (head, None);
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    (head, Some(serde_json::from_slice(&body).unwrap()))
}
fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}
const ROOM_A: &str = "11111111-1111-4111-8111-111111111111";
const ROOM_B: &str = "22222222-2222-4222-8222-222222222222";
const FORBIDDEN: &str = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

async fn recheck_fixture(first: Option<Discovery>) -> Recheck {
    recheck_fixture_every(first, Duration::from_millis(400)).await
}
async fn recheck_fixture_every(first: Option<Discovery>, catalog: Duration) -> Recheck {
    let user = Keys::generate();
    let other = Keys::generate();
    let relay_keys = Keys::generate();
    let public = user.public_key();
    let signer = relay_keys.public_key();
    let (a, b) = (ROOM_A.to_string(), ROOM_B.to_string());
    let script = std::sync::Arc::new(std::sync::Mutex::new(Script {
        next: first.into_iter().collect(),
        joined: vec![a.clone(), b.clone()],
        dm: DmReply::Silent,
        dm_events: Vec::new(),
        open: Vec::new(),
        room_reply: DmReply::Silent,
        room_events: Vec::new(),
        status_reply: DmReply::Silent,
        status_events: Vec::new(),
        statuses: Vec::new(),
        presence_reply: DmReply::Silent,
        presence_events: Vec::new(),
        presence: std::collections::BTreeMap::new(),
        presence_reads: 0,
        presence_forged: false,
    }));
    let (count_tx, discoveries) = watch::channel(0_usize);
    let count_tx = std::sync::Arc::new(count_tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let served = script.clone();
    let server = AbortTask(tokio::spawn(async move {
        let mut handlers = tokio::task::JoinSet::new();
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        ws.send(Message::Text(
            json!(["AUTH", "recheck-fixture"]).to_string().into(),
        ))
        .await
        .unwrap();
        let Some(Ok(Message::Text(text))) = ws.next().await else {
            panic!("AUTH expected")
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
        auth.verify().unwrap();
        assert_eq!(auth.pubkey, public);
        ws.send(Message::Text(
            json!(["OK", auth.id.to_hex(), true, ""]).to_string().into(),
        ))
        .await
        .unwrap();
        // Liveness probes are answered; kind-9 EVENTs are never acknowledged,
        // so a submitted message stays pending for the whole fixture. A DM open
        // is answered as the script says; opening joins the viewer to DM_ROOM.
        let dm_script = served.clone();
        handlers.spawn(async move {
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let frame: Value = serde_json::from_str(&text).unwrap();
                match frame[0].as_str().unwrap() {
                    "COUNT" => ws
                        .send(Message::Text(
                            json!(["COUNT", frame[1], {"count":0}]).to_string().into(),
                        ))
                        .await
                        .unwrap(),
                    "EVENT" => {
                        let event: Event = serde_json::from_value(frame[1].clone()).unwrap();
                        if matches!(event.kind.as_u16(), 9021 | 9022) {
                            event.verify().unwrap();
                            assert_eq!(event.pubkey, public);
                            let id = event.id.to_hex();
                            let room = event
                                .tags
                                .iter()
                                .find(|t| t.as_slice()[0] == "h")
                                .unwrap()
                                .as_slice()[1]
                                .clone();
                            let reply = {
                                let mut script = dm_script.lock().unwrap();
                                let join = event.kind.as_u16() == 9021;
                                script.room_events.push(event);
                                match script.room_reply {
                                    DmReply::Silent => None,
                                    DmReply::Reject => Some(json!([
                                        "OK",
                                        id,
                                        false,
                                        "invalid: cannot remove the last owner"
                                    ])),
                                    DmReply::Open => {
                                        if join {
                                            script.joined.push(room.clone());
                                            script.open.retain(|r| *r != room);
                                        } else {
                                            script.joined.retain(|r| *r != room);
                                            script.open.push(room);
                                        }
                                        Some(json!(["OK", id, true, ""]))
                                    }
                                }
                            };
                            if let Some(reply) = reply {
                                ws.send(Message::Text(reply.to_string().into()))
                                    .await
                                    .unwrap();
                            }
                            continue;
                        }
                        if event.kind.as_u16() == 20001 {
                            event.verify().unwrap();
                            assert_eq!(event.pubkey, public);
                            let id = event.id.to_hex();
                            let reply = {
                                let mut script = dm_script.lock().unwrap();
                                script.presence_events.push(event.clone());
                                match script.presence_reply {
                                    DmReply::Silent => None,
                                    DmReply::Reject => Some(json!([
                                        "OK",
                                        id,
                                        false,
                                        "rate-limited: fixture refusal"
                                    ])),
                                    DmReply::Open => {
                                        // As `handle_ephemeral_event`: offline clears the entry.
                                        if event.content == "offline" {
                                            script.presence.remove(&event.pubkey.to_hex());
                                        } else {
                                            script.presence.insert(
                                                event.pubkey.to_hex(),
                                                event.content.clone(),
                                            );
                                        }
                                        Some(json!(["OK", id, true, ""]))
                                    }
                                }
                            };
                            if let Some(reply) = reply {
                                ws.send(Message::Text(reply.to_string().into()))
                                    .await
                                    .unwrap();
                            }
                            continue;
                        }
                        if event.kind.as_u16() == 30315 {
                            event.verify().unwrap();
                            assert_eq!(event.pubkey, public);
                            let id = event.id.to_hex();
                            let reply = {
                                let mut script = dm_script.lock().unwrap();
                                script.status_events.push(event.clone());
                                match script.status_reply {
                                    DmReply::Silent => None,
                                    DmReply::Reject => {
                                        Some(json!(["OK", id, false, "blocked: fixture refusal"]))
                                    }
                                    DmReply::Open => {
                                        // Parameterized replaceable: the relay keeps the latest.
                                        script.statuses.retain(|e| e.pubkey != event.pubkey);
                                        script.statuses.push(event);
                                        Some(json!(["OK", id, true, ""]))
                                    }
                                }
                            };
                            if let Some(reply) = reply {
                                ws.send(Message::Text(reply.to_string().into()))
                                    .await
                                    .unwrap();
                            }
                            continue;
                        }
                        if event.kind.as_u16() != 41010 {
                            continue;
                        }
                        event.verify().unwrap();
                        assert_eq!(event.pubkey, public);
                        let id = event.id.to_hex();
                        let reply = {
                            let mut script = dm_script.lock().unwrap();
                            script.dm_events.push(event);
                            match script.dm {
                                DmReply::Silent => None,
                                DmReply::Reject => {
                                    Some(json!(["OK", id, false, "restricted: fixture refusal"]))
                                }
                                DmReply::Open => {
                                    if !script.joined.iter().any(|room| room == DM_ROOM) {
                                        script.joined.push(DM_ROOM.into());
                                    }
                                    let answer = json!({"channel_id": DM_ROOM, "created": true});
                                    Some(json!(["OK", id, true, format!("response:{answer}")]))
                                }
                            }
                        };
                        if let Some(reply) = reply {
                            ws.send(Message::Text(reply.to_string().into()))
                                .await
                                .unwrap();
                        }
                    }
                    // The live subscription is never primed here: polling is under test.
                    "REQ" | "CLOSE" => {}
                    other => panic!("observer emitted unsupported request {other}"),
                }
            }
        });
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let script = served.clone();
            let count_tx = count_tx.clone();
            let relay_keys = relay_keys.clone();
            let other = other.clone();
            handlers.spawn(async move {
                let (head, body) = read_request(&mut stream).await;
                let reply = if head.starts_with("get /info ") {
                    count_tx.send_modify(|count| *count += 1);
                    let step = script.lock().unwrap().next.pop_front();
                    match step {
                        None => ok(&json!({"self": signer.to_hex()}).to_string()),
                        Some(Discovery { rooms, gate }) => {
                            if let Some((started, release)) = gate {
                                let _ = started.send(());
                                let _ = release.await;
                            }
                            match rooms {
                                Some(rooms) => {
                                    script.lock().unwrap().joined = rooms;
                                    ok(&json!({"self": signer.to_hex()}).to_string())
                                }
                                None => "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                            }
                        }
                    }
                } else {
                    let request = body.unwrap();
                    let filter = &request[0];
                    let joined = script.lock().unwrap().joined.clone();
                    let tagged = |name: &str| -> Vec<String> {
                        filter[name].as_array().map(|v| v.iter().map(|s| s.as_str().unwrap().to_owned()).collect()).unwrap_or_default()
                    };
                    let membership = |room: &str| note(&relay_keys, 39002, "", vec![
                        Tag::parse(["d", room]).unwrap(),
                        Tag::parse(["p", &public.to_hex(), "", "member"]).unwrap(),
                        Tag::parse(["p", &other.public_key().to_hex(), "", "member"]).unwrap(),
                    ]);
                    match filter["kinds"][0].as_u64().unwrap() {
                        39002 if filter.get("#p").is_some() => ok(&serde_json::to_string(&joined.iter().map(|room| membership(room)).collect::<Vec<_>>()).unwrap()),
                        39002 => {
                            let room = tagged("#d").remove(0);
                            if joined.contains(&room) { ok(&serde_json::to_string(&vec![membership(&room)]).unwrap()) } else { FORBIDDEN.into() }
                        }
                        // Unscoped: member and open channels, each with its visibility tag,
                        // plus one private channel the viewer could not see in pinned Buzz.
                        39000 if filter.get("#d").is_none() => {
                            assert_eq!(filter, &json!({"kinds":[39000],"limit":200}));
                            let open = script.lock().unwrap().open.clone();
                            let meta = |room: &str, name: &str, visibility: &str| note(&relay_keys, 39000, "", vec![
                                Tag::parse(["d", room]).unwrap(), Tag::parse(["name", name]).unwrap(), Tag::parse([visibility]).unwrap(),
                                Tag::parse(["closed"]).unwrap(), Tag::parse(["t", "stream"]).unwrap()]);
                            let mut events: Vec<Event> = joined.iter().filter(|r| *r != DM_ROOM).map(|room| meta(room, "Fixture", "public")).collect();
                            events.extend(open.iter().map(|room| meta(room, "Open fixture", "public")));
                            events.push(meta("44444444-4444-4444-8444-444444444444", "Private", "private"));
                            ok(&serde_json::to_string(&events).unwrap())
                        }
                        39000 => ok(&serde_json::to_string(&tagged("#d").iter().filter(|room| joined.contains(room)).map(|room| note(&relay_keys, 39000, "", if room == DM_ROOM {
                            vec![Tag::parse(["d", room.as_str()]).unwrap(), Tag::parse(["name", "DM"]).unwrap(), Tag::parse(["t", "dm"]).unwrap(), Tag::parse(["hidden"]).unwrap(),
                                Tag::parse(["p", &public.to_hex()]).unwrap(), Tag::parse(["p", &other.public_key().to_hex()]).unwrap()]
                        } else {
                            vec![Tag::parse(["d", room.as_str()]).unwrap(), Tag::parse(["name", "Fixture"]).unwrap(), Tag::parse(["t", "stream"]).unwrap()]
                        })).collect::<Vec<_>>()).unwrap()),
                        // No NIP-DV snapshot: nothing is hidden.
                        0 | 10100 | 30622 => ok("[]"),
                        // Statuses: the other member's fixed one, plus every accepted
                        // publication by the viewer (the reader picks the newest).
                        30315 => {
                            assert_eq!(tagged("#d"), vec!["general".to_string()]);
                            let authors = tagged("authors");
                            let mut events = script.lock().unwrap().statuses.clone();
                            events.push(note(&other, 30315, "Out sick", vec![Tag::parse(["d", "general"]).unwrap(), Tag::parse(["emoji", "🤒"]).unwrap()]));
                            events.retain(|e| authors.contains(&e.pubkey.to_hex()));
                            ok(&serde_json::to_string(&events).unwrap())
                        }
                        // `synthesize_presence`: relay-signed snapshots with a p tag
                        // for each author the relay holds; the other member is away.
                        20001 => {
                            assert_eq!(filter.as_object().unwrap().keys().cloned().collect::<Vec<_>>(), vec!["authors".to_string(), "kinds".into(), "limit".into()]);
                            let authors = tagged("authors");
                            let (held, forged) = {
                                let mut script = script.lock().unwrap();
                                script.presence_reads += 1;
                                let mut held = script.presence.clone();
                                held.insert(other.public_key().to_hex(), "away".into());
                                (held, script.presence_forged)
                            };
                            let signer = if forged { Keys::generate() } else { relay_keys.clone() };
                            let events: Vec<Event> = held.iter().filter(|(k, _)| authors.contains(k))
                                .map(|(k, state)| note(&signer, 20001, state, vec![Tag::parse(["p", k.as_str()]).unwrap()])).collect();
                            ok(&serde_json::to_string(&events).unwrap())
                        }
                        9 => {
                            let room = tagged("#h").remove(0);
                            if joined.contains(&room) {
                                let row = note(&other, 40002, "synthetic room text", vec![Tag::parse(["h", room.as_str()]).unwrap()]);
                                let bounds = note(&relay_keys, 39006, r#"{"has_more":false,"next_cursor":null}"#,
                                    vec![Tag::parse(["h", room.as_str()]).unwrap(), Tag::parse(["d", &format!("{room}:head")]).unwrap()]);
                                ok(&serde_json::to_string(&vec![row, bounds]).unwrap())
                            } else { FORBIDDEN.into() }
                        }
                        kind => panic!("unexpected query kind {kind}"),
                    }
                };
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    }));
    let config = config::Config {
        relay: Some(origin.clone()),
        identity: Some(public.to_hex()),
        communities: Vec::new(),
    };
    let (tx, status) = watch::channel(Status::new(&config));
    let injector = tx.clone();
    let (commands, mut requests) = mpsc::channel(4);
    let ledger = std::env::temp_dir().join(format!("buzz-room-recheck-{}", uuid::Uuid::new_v4()));
    let journal = crate::ledger::Ledger::open(ledger.join("ledger.json")).unwrap();
    let mut connection = connect_identity(&origin, &user).await.unwrap();
    let observer = AbortTask(tokio::spawn(async move {
        let mut sender = crate::sending::Sender::new(Some(journal));
        let mut backoff = Backoff::default();
        let mut pin = None;
        observe_sending(
            &mut connection,
            &user,
            &origin,
            &mut pin,
            &tx,
            &mut requests,
            &mut backoff,
            FreshnessPolicy {
                interval: Duration::from_secs(2),
                response: Duration::from_secs(1),
                catalog,
                status_gap: Duration::from_millis(300),
                presence_gap: Duration::from_millis(300),
                presence_heartbeat: Duration::from_millis(1500),
                ..FRESHNESS
            },
            &mut sender,
            &crate::setup::Setup::unavailable(),
        )
        .await
    }));
    Recheck {
        a,
        b,
        script,
        injector,
        status,
        commands,
        discoveries,
        ledger,
        observer,
        _server: server,
    }
}
fn held(rooms: Option<Vec<String>>) -> (Discovery, oneshot::Receiver<()>, oneshot::Sender<()>) {
    let (started_tx, started) = oneshot::channel();
    let (release, release_rx) = oneshot::channel();
    (
        Discovery {
            rooms,
            gate: Some((started_tx, release_rx)),
        },
        started,
        release,
    )
}
async fn snapshot(rx: &mut watch::Receiver<Status>, predicate: impl Fn(&Status) -> bool) -> Status {
    wait_status(rx, &predicate).await;
    let status = rx.borrow().clone();
    assert!(predicate(&status));
    status
}
impl Recheck {
    // Selected history and an open thread in A, the roster of B.
    async fn establish(&mut self) -> String {
        let (a, b) = (self.a.clone(), self.b.clone());
        wait_status(&mut self.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        self.commands
            .send(Command::FetchRecent(a.clone()))
            .await
            .unwrap();
        let root = snapshot(&mut self.status, |s| {
            s.history.state == "snapshot"
                && s.history.room_id.as_deref() == Some(a.as_str())
                && !s.history.rows.is_empty()
        })
        .await
        .history
        .rows[0]
            .id
            .clone();
        self.injector.send_modify(|s| {
            s.thread = Thread {
                state: "snapshot".into(),
                room_id: Some(a.clone()),
                root_id: Some(root.clone()),
                rows: Vec::new(),
                has_more: Some(false),
                category: Some("thread_completeness_unknown".into()),
            }
        });
        self.commands
            .send(Command::FetchRecipients(b.clone()))
            .await
            .unwrap();
        wait_status(&mut self.status, |s| {
            s.recipients.state == "snapshot" && s.recipients.room_id.as_deref() == Some(b.as_str())
        })
        .await;
        root
    }
    async fn finish(self) {
        self.commands.send(Command::Retry).await.unwrap();
        let mut observer = self.observer;
        assert!(matches!(
            timeout(Duration::from_secs(3), &mut observer.0)
                .await
                .unwrap()
                .unwrap(),
            ConnectionExit::Retry
        ));
        let _ = std::fs::remove_dir_all(&self.ledger);
    }
}
fn retained(s: &Status, a: &str, b: &str, root: &str) -> bool {
    s.catalog.state == "partial"
        && s.catalog.rooms.len() == 2
        && s.history.state == "snapshot"
        && s.history.room_id.as_deref() == Some(a)
        && s.thread.state == "snapshot"
        && s.thread.room_id.as_deref() == Some(a)
        && s.thread.root_id.as_deref() == Some(root)
        && s.recipients.state == "snapshot"
        && s.recipients.room_id.as_deref() == Some(b)
}

#[derive(PartialEq)]
enum Outcome {
    Same,
    RemoveSelected,
    RemoveOther,
    Fail,
}
async fn background_check(outcome: Outcome) {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(20), async {
        let mut f = recheck_fixture(None).await;
        let (a, b) = (f.a.clone(), f.b.clone());
        let root = f.establish().await;
        let after = match outcome {
            Outcome::Same => Some(vec![a.clone(), b.clone()]),
            Outcome::RemoveSelected => Some(vec![b.clone()]),
            Outcome::RemoveOther => Some(vec![a.clone()]),
            Outcome::Fail => None,
        };
        let (step, started, release) = held(after);
        f.script.lock().unwrap().next.push_back(step);
        timeout(Duration::from_secs(5), started)
            .await
            .unwrap()
            .unwrap();
        let seen = *f.discoveries.borrow();
        // The check is in flight: every published view stays, and the
        // helper still accepts a message for a listed room.
        assert!(
            retained(&f.status.borrow(), &a, &b, &root),
            "background room check blanked a view"
        );
        let generation = f.status.borrow().generation;
        let intent = crate::protocol::SendIntent {
            request_id: uuid::Uuid::new_v4().to_string(),
            room: a.clone(),
            root_id: None,
            text: "recheck fixture".into(),
            mentions: vec![],
            generation,
        };
        let request_id = intent.request_id.clone();
        let (reply, accepted) = oneshot::channel();
        f.commands
            .send(Command::SendChecked(intent, reply))
            .await
            .unwrap();
        assert_eq!(accepted.await.unwrap(), None);
        assert_eq!(f.status.borrow().delivery.state, "sending");
        assert!(retained(&f.status.borrow(), &a, &b, &root));
        release.send(()).unwrap();
        match outcome {
            Outcome::Same => {
                // The next check has started, so the held one was published.
                timeout(
                    Duration::from_secs(5),
                    f.discoveries.wait_for(|count| *count > seen),
                )
                .await
                .unwrap()
                .unwrap();
                let s = f.status.borrow().clone();
                assert!(retained(&s, &a, &b, &root));
                assert_eq!(s.delivery.state, "sending");
            }
            Outcome::RemoveSelected => {
                let s = snapshot(&mut f.status, |s| s.catalog.rooms.len() == 1).await;
                assert_eq!(s.catalog.rooms[0].id, b);
                assert_eq!(s.history.state, "unavailable");
                assert_eq!(s.history.room_id.as_deref(), Some(a.as_str()));
                assert_eq!(s.history.category.as_deref(), Some("history_access_denied"));
                assert!(s.history.rows.is_empty(), "removed room retained history");
                assert_eq!(s.thread.state, "unavailable");
                assert!(s.thread.room_id.is_none() && s.thread.root_id.is_none());
                assert_eq!(s.thread.category.as_deref(), Some("thread_access_denied"));
                assert_eq!(s.recipients.state, "snapshot");
                assert_eq!(s.recipients.room_id.as_deref(), Some(b.as_str()));
                assert_eq!(s.delivery.state, "unknown");
                assert_eq!(s.delivery.request_id.as_deref(), Some(request_id.as_str()));
                assert_eq!(s.delivery.category.as_deref(), Some("delivery_unknown"));
                assert!(s.activity.iter().all(|entry| entry.room_id == b));
                // A later message for the removed room is refused.
                let (reply, refused) = oneshot::channel();
                f.commands
                    .send(Command::SendChecked(
                        crate::protocol::SendIntent {
                            request_id: uuid::Uuid::new_v4().to_string(),
                            room: a.clone(),
                            root_id: None,
                            text: "late".into(),
                            mentions: vec![],
                            generation,
                        },
                        reply,
                    ))
                    .await
                    .unwrap();
                assert_eq!(refused.await.unwrap(), Some("send_access_denied"));
            }
            Outcome::RemoveOther => {
                let s = snapshot(&mut f.status, |s| s.catalog.rooms.len() == 1).await;
                assert_eq!(s.catalog.rooms[0].id, a);
                assert_eq!(s.history.state, "snapshot");
                assert_eq!(s.history.room_id.as_deref(), Some(a.as_str()));
                assert_eq!(s.thread.state, "snapshot");
                assert_eq!(s.thread.root_id.as_deref(), Some(root.as_str()));
                assert_eq!(s.recipients.state, "unavailable");
                assert_eq!(s.recipients.room_id.as_deref(), Some(b.as_str()));
                assert_eq!(
                    s.recipients.category.as_deref(),
                    Some("recipients_access_denied")
                );
                assert_eq!(
                    s.delivery.state, "sending",
                    "delivery in a remaining room was revoked"
                );
                assert!(s.activity.iter().all(|entry| entry.room_id == a));
            }
            Outcome::Fail => {
                let s = snapshot(&mut f.status, |s| s.catalog.state == "unavailable").await;
                assert!(s.catalog.rooms.is_empty());
                assert_eq!(
                    s.catalog.category.as_deref(),
                    Some("room_catalog_unavailable")
                );
                assert_eq!(s.history.state, "unavailable");
                assert!(s.history.room_id.is_none() && s.history.rows.is_empty());
                assert_eq!(s.recipients.state, "unavailable");
                assert!(s.recipients.room_id.is_none() && s.recipients.entries.is_empty());
                assert_eq!(s.thread.state, "unavailable");
                assert!(s.thread.room_id.is_none());
                assert!(s.activity.is_empty());
                // Unchanged: a failed check leaves delivery evidence alone.
                assert_eq!(s.delivery.state, "sending");
            }
        }
        f.finish().await;
    })
    .await
    .expect("room check fixture deadline");
}

#[tokio::test]
async fn background_room_check_keeps_published_views() {
    background_check(Outcome::Same).await;
}

#[tokio::test]
async fn removed_selected_room_clears_its_views_and_revokes_its_delivery() {
    background_check(Outcome::RemoveSelected).await;
}

#[tokio::test]
async fn removed_other_room_clears_only_its_roster() {
    background_check(Outcome::RemoveOther).await;
}

#[tokio::test]
async fn failed_background_room_check_clears_dependent_views() {
    background_check(Outcome::Fail).await;
}

#[tokio::test]
async fn first_room_check_after_authentication_publishes_loading() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(12), async {
        let (step, started, release) = held(Some(vec![ROOM_A.into(), ROOM_B.into()]));
        let mut f = recheck_fixture(Some(step)).await;
        timeout(Duration::from_secs(5), started)
            .await
            .unwrap()
            .unwrap();
        {
            let s = f.status.borrow();
            assert_eq!(s.connection, "authenticated");
            assert_eq!(s.catalog.state, "loading");
            assert!(s.catalog.rooms.is_empty());
        }
        release.send(()).unwrap();
        wait_status(&mut f.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        f.finish().await;
    })
    .await
    .expect("first room check fixture deadline");
}

#[cfg(test)]
#[path = "auth_dm_open_tests.rs"]
mod dm_open_integration;
#[cfg(test)]
#[path = "auth_join_tests.rs"]
mod join_integration;
#[cfg(test)]
#[path = "auth_presence_tests.rs"]
mod presence_integration;
#[cfg(test)]
#[path = "auth_status_tests.rs"]
mod status_integration;
