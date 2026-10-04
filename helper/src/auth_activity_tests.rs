//! Two-room observer fixture: a denied background poll revokes only that room.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Keys, Kind, Tag};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

fn event(author: &Keys, kind: u16, body: &str, tags: Vec<Tag>) -> Event {
    EventBuilder::new(Kind::Custom(kind), body)
        .tags(tags)
        .sign_with_keys(author)
        .unwrap()
}
async fn read_request(stream: &mut TcpStream) -> (String, Option<Value>) {
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

async fn scenario(select_background_before_denial: bool) {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(25), async {
        let user = Keys::generate();
        let other = Keys::generate();
        let relay_key = Keys::generate();
        let public = user.public_key();
        // Lexical order determines the first background candidate. The first
        // tick skips selected A; the second tick reaches B.
        let a = "11111111-1111-4111-8111-111111111111".to_string();
        let b = "22222222-2222-4222-8222-222222222222".to_string();
        let members: Vec<Event> = [&a, &b].iter().map(|room| event(&relay_key, 39002, "",
            vec![Tag::parse(["d", room.as_str()]).unwrap(), Tag::parse(["p", &public.to_hex()]).unwrap()])).collect();
        let metadata: Vec<Event> = [&a, &b].iter().map(|room| event(&relay_key, 39000, "",
            vec![Tag::parse(["d", room.as_str()]).unwrap(), Tag::parse(["name", "Fixture"]).unwrap(), Tag::parse(["t", "stream"]).unwrap()])).collect();
        let row = event(&other, 40002, "selected room text", vec![Tag::parse(["h", &a]).unwrap()]);
        let selected_id = row.id.to_hex();
        let bounds = event(&relay_key, 39006, r#"{"has_more":false,"next_cursor":null}"#,
            vec![Tag::parse(["h", &a]).unwrap(), Tag::parse(["d", &format!("{a}:head")]).unwrap()]);
        let history = serde_json::to_string(&vec![row, bounds]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let signer = relay_key.public_key();
        let b_queries = Arc::new(AtomicUsize::new(0));
        let counted = b_queries.clone();
        let origin_for_server = origin.clone();
        let a_for_server = a.clone();
        let b_for_server = b.clone();
        let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
        let (started_tx, started_rx) = oneshot::channel::<()>();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let mut started_tx = Some(started_tx);
            let mut release_rx = Some(release_rx);
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH", "fixture"]).to_string().into())).await.unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("AUTH expected") };
            let frame: Value = serde_json::from_str(&text).unwrap();
            let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
            auth.verify().unwrap();
            assert_eq!(auth.pubkey, public);
            assert!(auth.tags.iter().any(|tag| tag.as_slice() == ["relay", origin_for_server.as_str()]));
            ws.send(Message::Text(json!(["OK", auth.id.to_hex(), true, ""]).to_string().into())).await.unwrap();
            let ws_task = tokio::spawn(async move {
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    // The live subscription is never primed here: polling is under test.
                    if matches!(frame[0].as_str(), Some("REQ" | "CLOSE")) {
                        continue;
                    }
                    assert_eq!(frame[0], "COUNT");
                    ws.send(Message::Text(json!(["COUNT", frame[1], {"count":0}]).to_string().into())).await.unwrap();
                }
            });
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        let (head, body) = read_request(&mut stream).await;
                        let reply = if head.starts_with("get /info ") {
                            ok(&json!({"self": signer.to_hex()}).to_string())
                        } else {
                            let request = body.unwrap();
                            match request[0]["kinds"].as_array().unwrap()[0].as_u64().unwrap() {
                                39002 => ok(&serde_json::to_string(&members).unwrap()),
                                39000 => ok(&serde_json::to_string(&metadata).unwrap()),
                                // Names of authors the roster does not list: none have a profile here.
                                0 => ok("[]"),
                                9 => {
                                    let room = request[0]["#h"][0].as_str().unwrap();
                                    if room == b_for_server {
                                        counted.fetch_add(1, Ordering::SeqCst);
                                        if select_background_before_denial && started_tx.is_some() {
                                            let _ = started_tx.take().unwrap().send(());
                                            let _ = release_rx.take().unwrap().await;
                                        }
                                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                                    } else {
                                        assert_eq!(room, a_for_server);
                                        ok(&history)
                                    }
                                },
                                kind => panic!("unexpected query kind {kind}"),
                            }
                        };
                        stream.write_all(reply.as_bytes()).await.unwrap();
                    }
                }
            }
            ws_task.abort();
        });
        let config = config::Config { relay: Some(origin.clone()), identity: Some(public.to_hex()), communities: Vec::new() };
        let (tx, mut status) = watch::channel(Status::new(&config));
        let (commands, mut command_rx) = mpsc::channel(4);
        let mut conn = connect_identity(&origin, &user).await.unwrap();
        let observer = tokio::spawn(async move {
            let mut pin = None;
            let mut backoff = Backoff::default();
            observe_connection(&mut conn, &user, &origin, &mut pin, &tx, &mut command_rx,
                &mut backoff, FreshnessPolicy { interval: Duration::from_secs(2), response: Duration::from_secs(1), ..FRESHNESS }).await
        });
        timeout(Duration::from_secs(4), async {
            while status.borrow().catalog.rooms.len() != 2 { status.changed().await.unwrap(); }
        }).await.unwrap();
        commands.send(Command::FetchRecent(a.clone())).await.unwrap();
        timeout(Duration::from_secs(4), async {
            while status.borrow().history.rows.first().is_none_or(|row| row.id != selected_id) {
                status.changed().await.unwrap();
            }
        }).await.unwrap();
        if select_background_before_denial {
            timeout(Duration::from_secs(14), started_rx).await.unwrap().unwrap();
            commands.send(Command::FetchRecent(b.clone())).await.unwrap();
            timeout(Duration::from_secs(3), async {
                while status.borrow().history.room_id.as_deref() != Some(b.as_str())
                    || status.borrow().history.state != "loading" {
                    status.changed().await.unwrap();
                }
            }).await.unwrap();
            release_tx.send(()).unwrap();
        }
        timeout(Duration::from_secs(14), async {
            while status.borrow().catalog.rooms.iter().any(|room| room.id == b) {
                status.changed().await.unwrap();
            }
        }).await.unwrap();
        if select_background_before_denial {
            assert_eq!(status.borrow().history.room_id.as_deref(), Some(b.as_str()));
            assert_eq!(status.borrow().history.state, "unavailable");
            assert_eq!(status.borrow().history.category.as_deref(), Some("history_access_denied"));
            assert!(status.borrow().history.rows.is_empty(), "revoked room retained history");
            assert_ne!(status.borrow().recipients.state, "snapshot");
        } else {
            assert_eq!(b_queries.load(Ordering::SeqCst), 1);
            assert_eq!(status.borrow().history.room_id.as_deref(), Some(a.as_str()));
            assert_eq!(status.borrow().history.rows[0].id, selected_id);
            tokio::time::sleep(Duration::from_secs(6)).await;
            assert_eq!(b_queries.load(Ordering::SeqCst), 1, "revoked room was polled again");
        }
        commands.send(Command::Retry).await.unwrap();
        assert!(matches!(timeout(Duration::from_secs(3), observer).await.unwrap().unwrap(), ConnectionExit::Retry));
        let _ = stop_tx.send(());
        timeout(Duration::from_secs(3), server).await.unwrap().unwrap();
    }).await.expect("activity fixture deadline");
}

#[tokio::test]
async fn denied_background_room_is_removed_without_changing_selected_history() {
    scenario(false).await;
}

#[tokio::test]
async fn background_denial_clears_room_selected_while_query_was_in_flight() {
    scenario(true).await;
}

/// A reply under my message reaches the tracker through the production polls:
/// the head page (`top_level`) never holds it, the ordinary recent read does.
#[tokio::test]
async fn thread_reply_reaches_notice_through_the_recent_read() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(40), async {
        let user = Keys::generate();
        let other = Keys::generate();
        let relay_key = Keys::generate();
        let public = user.public_key();
        let a = "11111111-1111-4111-8111-111111111111".to_string();
        let h = || Tag::parse(["h", "11111111-1111-4111-8111-111111111111"]).unwrap();
        let members = vec![event(&relay_key, 39002, "", vec![Tag::parse(["d", a.as_str()]).unwrap(), Tag::parse(["p", &public.to_hex()]).unwrap()])];
        let metadata = vec![event(&relay_key, 39000, "", vec![Tag::parse(["d", a.as_str()]).unwrap(), Tag::parse(["name", "Fixture"]).unwrap(), Tag::parse(["t", "stream"]).unwrap()])];
        let root = event(&user, 9, "my question", vec![h()]);
        let root_id = root.id.to_hex();
        // Signed when first served: it must be newer than the tracker's baseline.
        let reply_tags = vec![h(), Tag::parse(["e", &root_id, "", "root"]).unwrap(), Tag::parse(["e", &root_id, "", "reply"]).unwrap()];
        let reply_id = Arc::new(std::sync::Mutex::new(String::new()));
        let served_id = reply_id.clone();
        let bounds = event(&relay_key, 39006, r#"{"has_more":false,"next_cursor":null}"#,
            vec![h(), Tag::parse(["d", &format!("{a}:head")]).unwrap()]);
        let head = serde_json::to_string(&vec![root.clone(), bounds]).unwrap();
        let root_for_server = root.clone();
        let empty = serde_json::to_string::<Vec<Event>>(&vec![]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let signer = relay_key.public_key();
        let recent_reads = Arc::new(AtomicUsize::new(0));
        let counted = recent_reads.clone();
        let room_for_server = a.clone();
        let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH", "fixture"]).to_string().into())).await.unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("AUTH expected") };
            let frame: Value = serde_json::from_str(&text).unwrap();
            let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
            ws.send(Message::Text(json!(["OK", auth.id.to_hex(), true, ""]).to_string().into())).await.unwrap();
            let ws_task = tokio::spawn(async move {
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if matches!(frame[0].as_str(), Some("REQ" | "CLOSE")) {
                        continue;
                    }
                    ws.send(Message::Text(json!(["COUNT", frame[1], {"count":0}]).to_string().into())).await.unwrap();
                }
            });
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        let (head_text, body) = read_request(&mut stream).await;
                        let reply_body = if head_text.starts_with("get /info ") {
                            ok(&json!({"self": signer.to_hex()}).to_string())
                        } else {
                            let request = body.unwrap();
                            match request[0]["kinds"].as_array().unwrap()[0].as_u64().unwrap() {
                                39002 => ok(&serde_json::to_string(&members).unwrap()),
                                39000 => ok(&serde_json::to_string(&metadata).unwrap()),
                                // Names of authors the roster does not list: none have a profile here.
                                0 => ok("[]"),
                                9 if request[0]["top_level"] == true => ok(&head),
                                9 => {
                                    // The production shape of the recent read: no extension flags.
                                    assert_eq!(request, json!([{"kinds":[9,40002],"#h":[room_for_server.clone()],"limit":20}]));
                                    // The first poll is the baseline; the reply arrives afterwards.
                                    if counted.fetch_add(1, Ordering::SeqCst) == 0 { ok(&empty) } else {
                                        let reply = event(&other, 9, "an answer in the thread", reply_tags.clone());
                                        *served_id.lock().unwrap() = reply.id.to_hex();
                                        ok(&serde_json::to_string(&vec![reply, root_for_server.clone()]).unwrap())
                                    }
                                },
                                kind => panic!("unexpected query kind {kind}"),
                            }
                        };
                        stream.write_all(reply_body.as_bytes()).await.unwrap();
                    }
                }
            }
            ws_task.abort();
        });
        let config = config::Config { relay: Some(origin.clone()), identity: Some(public.to_hex()), communities: Vec::new() };
        let (tx, mut status) = watch::channel(Status::new(&config));
        let (commands, mut command_rx) = mpsc::channel(4);
        let mut conn = connect_identity(&origin, &user).await.unwrap();
        let observer = tokio::spawn(async move {
            let mut pin = None;
            let mut backoff = Backoff::default();
            observe_connection(&mut conn, &user, &origin, &mut pin, &tx, &mut command_rx,
                &mut backoff, FreshnessPolicy { interval: Duration::from_secs(2), response: Duration::from_secs(1), ..FRESHNESS }).await
        });
        // The polls are about 5 s apart and the room is never selected.
        timeout(Duration::from_secs(25), async {
            while status.borrow().activity.first().and_then(|s| s.notice.as_ref()).is_none() {
                status.changed().await.unwrap();
            }
        }).await.unwrap();
        let summary = status.borrow().activity[0].clone();
        let notice = summary.notice.unwrap();
        assert_eq!((notice.kind, notice.count, notice.seq), ("thread", 1, 1));
        assert_eq!(notice.event_id, *reply_id.lock().unwrap());
        assert_eq!(notice.thread_root.as_deref(), Some(root_id.as_str()));
        assert_eq!(notice.snippet, "an answer in the thread");
        assert_eq!(notice.room_name, "Fixture");
        assert!(summary.observed >= 1);
        commands.send(Command::Retry).await.unwrap();
        assert!(matches!(timeout(Duration::from_secs(3), observer).await.unwrap().unwrap(), ConnectionExit::Retry));
        let _ = stop_tx.send(());
        timeout(Duration::from_secs(3), server).await.unwrap().unwrap();
    }).await.expect("thread reply fixture deadline");
}
