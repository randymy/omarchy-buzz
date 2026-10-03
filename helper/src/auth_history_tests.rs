//! Selected-room history through the production observer; synthetic same-origin HTTP/WS.
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

struct AbortTask<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn event(key: &Keys, kind: u16, content: &str, tags: Vec<Tag>) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags)
        .sign_with_keys(key)
        .unwrap()
}
async fn wait_status(rx: &mut watch::Receiver<Status>, test: impl Fn(&Status) -> bool) {
    timeout(Duration::from_secs(4), async {
        loop {
            if test(&rx.borrow()) {
                break;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("status deadline");
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
async fn scenario(in_flight: bool, automatic: bool, automatic_failure: bool) {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(15),async {
        let user=Keys::generate();let relay=Keys::generate();let signer=relay.public_key();let public=user.public_key();
        let room=uuid::Uuid::new_v4().to_string();let unauthorized=uuid::Uuid::new_v4().to_string();
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let origin=format!("ws://{}/",listener.local_addr().unwrap());
        let membership=event(&relay,39002,"",vec![Tag::parse(["d",&room]).unwrap(),Tag::parse(["p",&public.to_hex()]).unwrap()]);
        let metadata=event(&relay,39000,"",vec![Tag::parse(["d",&room]).unwrap(),Tag::parse(["name","Synthetic Room"]).unwrap(),Tag::parse(["t","stream"]).unwrap()]);
        let row=event(&user,40002,"synthetic recent body",vec![Tag::parse(["h",&room]).unwrap()]);let row_id=row.id.to_hex();
        let bounds=event(&relay,39006,r#"{"has_more":false,"next_cursor":null}"#,vec![Tag::parse(["h",&room]).unwrap(),Tag::parse(["d",&format!("{room}:head")]).unwrap()]);
        let page=serde_json::to_string(&vec![row,bounds.clone()]).unwrap();
        let newer=event(&user,40002,"new synthetic body",vec![Tag::parse(["h",&room]).unwrap()]);
        let newer_id=newer.id.to_hex();
        let newer_page=serde_json::to_string(&vec![newer,bounds]).unwrap();
        let requests=Arc::new(AtomicUsize::new(0));let count=requests.clone();
        let server_origin=origin.clone();let expected_room=room.clone();
        let (challenge_send,challenge_wait)=oneshot::channel();let (ack_send,ack_wait)=oneshot::channel();
        let (finish_send,finish_wait)=oneshot::channel();let (ws_done_send,ws_done_wait)=oneshot::channel();
        let (held_send,held_wait)=oneshot::channel();let (release_send,release_wait)=oneshot::channel();
        let (released_send,released_wait)=oneshot::channel();
        let (idle_send,idle_wait)=oneshot::channel();
        let mut held_wait=Some(held_wait);let mut release_send=Some(release_send);let mut released_wait=Some(released_wait);
        let server=AbortTask(tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.unwrap();let mut ws=accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH","initial"]).to_string().into())).await.unwrap();
            let Some(Ok(Message::Text(text)))=ws.next().await else {panic!("AUTH expected")};
            let frame:Value=serde_json::from_str(&text).unwrap();assert_eq!(frame[0],"AUTH");
            let auth:Event=serde_json::from_value(frame[1].clone()).unwrap();auth.verify().unwrap();assert_eq!(auth.pubkey,public);assert_eq!(auth.kind,Kind::Authentication);
            assert!(auth.tags.iter().any(|t|t.as_slice()==["relay",server_origin.as_str()]));
            ws.send(Message::Text(json!(["OK",auth.id.to_hex(),true,""]).to_string().into())).await.unwrap();
            let websocket=AbortTask(tokio::spawn(async move {
                let mut challenge_wait=challenge_wait;let mut finish_wait=finish_wait;let mut ack_wait=Some(ack_wait);let mut challenged=false;
                loop {tokio::select! {
                    _=&mut finish_wait=>break,
                    _=&mut challenge_wait,if !challenged=>{challenged=true;ws.send(Message::Text(json!(["AUTH","again"]).to_string().into())).await.unwrap();},
                    frame=ws.next()=>{
                        let Some(Ok(Message::Text(text)))=frame else {break;};let value:Value=serde_json::from_str(&text).unwrap();
                        match value[0].as_str().unwrap() {
                            "COUNT"=>{assert_eq!(value[2],json!({"kinds":[0],"authors":[public.to_hex()],"limit":1}));ws.send(Message::Text(json!(["COUNT",value[1],{"count":0}]).to_string().into())).await.unwrap();},
                            "AUTH"=>{assert!(challenged);let event:Event=serde_json::from_value(value[1].clone()).unwrap();event.verify().unwrap();assert_eq!(event.pubkey,public);assert!(event.tags.iter().any(|t|t.as_slice()==["challenge","again"]));timeout(Duration::from_secs(4),ack_wait.take().unwrap()).await.unwrap().unwrap();ws.send(Message::Text(json!(["OK",event.id.to_hex(),true,""]).to_string().into())).await.unwrap();},
                            // Never primed here: the polling fallback is under test.
                            "REQ"|"CLOSE"=>{},
                            other=>panic!("unexpected observer request {other}"),
                        }
                    }
                }}
                let _=ws_done_send.send(());
            }));
            let mut responses=vec![json!({"self":signer.to_hex()}).to_string(),serde_json::to_string(&vec![membership]).unwrap(),serde_json::to_string(&vec![metadata]).unwrap(),page.clone()];
            if in_flight {responses.push(page);}
            if automatic {responses.push(newer_page);}
            let mut held_send=Some(held_send);let mut release_wait=Some(release_wait);let mut released_send=Some(released_send);
            for (index,payload) in responses.into_iter().enumerate() {
                // The activity poll's recent-replies read (no `top_level`) is not part of this sequence.
                let (mut stream,head,body)=loop {
                    let (mut stream,_)=listener.accept().await.unwrap();let (head,body)=request(&mut stream).await;
                    if body.as_ref().is_some_and(|b|b[0]["kinds"]==json!([9,40002])&&b[0].get("top_level").is_none()) {let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]").await;continue;}
                    break (stream,head,body);
                };
                count.fetch_add(1,Ordering::SeqCst);
                if index==0 {assert!(head.starts_with("get /info "));assert!(body.is_none());}
                else {
                    assert!(head.starts_with("post /query "));assert!(head.contains("authorization: nostr "));let body=body.unwrap();
                    if index==1 {assert_eq!(body[0]["kinds"],json!([39002]));}
                    else if index==2 {assert_eq!(body[0]["kinds"],json!([39000]));}
                    else {assert_eq!(body,json!([{"kinds":[9,40002],"#h":[expected_room],"limit":20,"top_level":true,"include_aux":true,"include_summaries":true}]));}
                }
                if index==4 {held_send.take().unwrap().send(()).unwrap();timeout(Duration::from_secs(8),release_wait.take().unwrap()).await.unwrap().unwrap();}
                // Aborted stale reads may close before this response is written.
                let response=if automatic_failure && index==4 {"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()} else {format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload)};
                let _=stream.write_all(response.as_bytes()).await;
                if index==4 {let _=released_send.take().unwrap().send(());}
            }
            if automatic_failure {
                assert!(timeout(Duration::from_millis(5500),listener.accept()).await.is_err(),"denied room was queried again");
                let _=idle_send.send(());
            }
            timeout(Duration::from_secs(5),ws_done_wait).await.unwrap().unwrap();drop(websocket);
        }));
        let config=config::Config{relay:Some(origin.clone()),identity:Some(public.to_hex()),communities:Vec::new()};let (tx,mut status)=watch::channel(Status::new(&config));
        let (commands,mut command_rx)=mpsc::channel(4);let mut conn=connect_identity(&origin,&user).await.unwrap();
        let observer=AbortTask(tokio::spawn(async move {
            let mut pin=None;let mut backoff=Backoff::default();
            let exit=observe_connection(&mut conn,&user,&origin,&mut pin,&tx,&mut command_rx,&mut backoff,FreshnessPolicy{interval:Duration::from_secs(2),response:Duration::from_secs(1),..FRESHNESS}).await;
            assert_eq!(pin,Some(signer));exit
        }));
        wait_status(&mut status,|s|s.catalog.rooms.len()==1).await;assert_eq!(requests.load(Ordering::SeqCst),3);
        commands.send(crate::protocol::Command::FetchRecent(unauthorized.clone())).await.unwrap();
        wait_status(&mut status,|s|s.history.category.as_deref()==Some("history_access_denied")).await;
        assert!(status.borrow().history.rows.is_empty());assert_eq!(requests.load(Ordering::SeqCst),3,"arbitrary room caused an HTTP read");
        commands.send(crate::protocol::Command::FetchRecent(room.clone())).await.unwrap();
        wait_status(&mut status,|s|s.history.rows.len()==1).await;
        {let state=status.borrow();assert_eq!(state.history.room_id.as_deref(),Some(room.as_str()));assert_eq!(state.history.state,"snapshot");assert_eq!(state.history.category.as_deref(),Some("history_completeness_unknown"));assert_eq!(state.history.rows[0].id,row_id);assert_eq!(state.history.rows[0].author,public.to_hex());assert_eq!(state.history.rows[0].text,"synthetic recent body");}
        if automatic {
            timeout(Duration::from_secs(8),held_wait.take().unwrap()).await.unwrap().unwrap();
            {let state=status.borrow();assert_eq!(state.history.state,"snapshot","background refresh hid a valid snapshot");assert_eq!(state.history.rows[0].id,row_id);}
            assert_eq!(requests.load(Ordering::SeqCst),5,"overlapping history reads");
            release_send.take().unwrap().send(()).unwrap();timeout(Duration::from_secs(3),released_wait.take().unwrap()).await.unwrap().unwrap();
            if automatic_failure {
                wait_status(&mut status,|s|s.history.state=="unavailable"&&s.history.rows.is_empty()).await;
                assert_eq!(status.borrow().history.category.as_deref(),Some("history_access_denied"));
                assert!(status.borrow().catalog.rooms.iter().all(|r|r.id!=room),"denied room remained selectable");
                let intent=crate::protocol::SendIntent {action: Default::default(), request_id:uuid::Uuid::new_v4().to_string(),room:room.clone(),root_id:None,text:"must not send".into(),mentions:vec![],generation:status.borrow().generation};
                let (reply,received)=oneshot::channel();
                commands.send(crate::protocol::Command::SendChecked(intent,reply)).await.unwrap();
                assert_eq!(received.await.unwrap(),Some("send_access_denied"),"revoked room prepared a send");
                assert_eq!(requests.load(Ordering::SeqCst),5);
                timeout(Duration::from_secs(7),idle_wait).await.unwrap().unwrap();
                assert_eq!(requests.load(Ordering::SeqCst),5,"denied room caused another HTTP read");
            } else {
                wait_status(&mut status,|s|s.history.rows.first().is_some_and(|r|r.id==newer_id)).await;
                assert_eq!(status.borrow().history.state,"snapshot");
            }
        }
        if in_flight {
            commands.send(crate::protocol::Command::FetchRecent(room.clone())).await.unwrap();
            timeout(Duration::from_secs(3),held_wait.take().unwrap()).await.unwrap().unwrap();
        }
        challenge_send.send(()).unwrap();
        wait_status(&mut status,|s|s.connection=="connecting"&&s.history.rows.is_empty()).await;
        assert_eq!(status.borrow().history.state,"unavailable");
        if in_flight {
            release_send.take().unwrap().send(()).unwrap();timeout(Duration::from_secs(3),released_wait.take().unwrap()).await.unwrap().unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(status.borrow().history.rows.is_empty(),"stale query resurrected history");assert_eq!(status.borrow().connection,"connecting");
        }
        commands.send(crate::protocol::Command::Retry).await.unwrap();ack_send.send(()).unwrap();
        let mut observer=observer;assert!(matches!(timeout(Duration::from_secs(3),&mut observer.0).await.unwrap().unwrap(),ConnectionExit::Retry));
        let _=finish_send.send(());let mut server=server;timeout(Duration::from_secs(3),&mut server.0).await.unwrap().unwrap();
    }).await.expect("history observer fixture deadline");
}
#[tokio::test]
async fn selected_room_only_and_completed_history_clears_on_reauthentication() {
    scenario(false, false, false).await;
}
#[tokio::test]
async fn in_flight_history_cannot_reappear_after_reauthentication() {
    scenario(true, false, false).await;
}
#[tokio::test]
async fn selected_history_refreshes_without_loading_flicker() {
    scenario(false, true, false).await;
}
#[tokio::test]
async fn failed_background_refresh_clears_stale_rows() {
    scenario(false, true, true).await;
}

/// Older pages through the production observer: the continuation request, rows
/// prepended, kept across the automatic head refresh, dropped on re-selection,
/// and never requested for a room that is not selected.
#[tokio::test]
async fn older_page_is_held_across_head_refresh_and_dropped_on_reselect() {
    let _fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(25), async {
        let user = Keys::generate();
        let relay = Keys::generate();
        let signer = relay.public_key();
        let public = user.public_key();
        let room = uuid::Uuid::new_v4().to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let now = nostr::Timestamp::now().as_secs();
        let membership = event(&relay, 39002, "", vec![Tag::parse(["d", &room]).unwrap(), Tag::parse(["p", &public.to_hex()]).unwrap()]);
        let metadata = event(&relay, 39000, "", vec![Tag::parse(["d", &room]).unwrap(), Tag::parse(["name", "Synthetic Room"]).unwrap(), Tag::parse(["t", "stream"]).unwrap()]);
        let stamp = |content: &str, at: u64| {
            EventBuilder::new(Kind::Custom(40002), content)
                .tags([Tag::parse(["h", &room]).unwrap()])
                .custom_created_at(nostr::Timestamp::from(at))
                .sign_with_keys(&user)
                .unwrap()
        };
        let recent = stamp("synthetic recent", now - 10);
        let older = stamp("synthetic older", now - 100);
        let head_cursor = json!({"created_at": now - 10, "id": recent.id.to_hex()});
        let heads = Arc::new(AtomicUsize::new(0));
        let continuations = Arc::new(AtomicUsize::new(0));
        let (head_count, older_count) = (heads.clone(), continuations.clone());
        let (server_room, server_relay, server_origin) = (room.clone(), relay.clone(), origin.clone());
        let (recent_page, older_page) = (recent.clone(), older.clone());
        let expected_cursor = head_cursor.clone();
        let (finish_send, mut finish_wait) = oneshot::channel::<()>();
        let server = AbortTask(tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH", "initial"]).to_string().into())).await.unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("AUTH expected") };
            let frame: Value = serde_json::from_str(&text).unwrap();
            let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
            auth.verify().unwrap();
            assert!(auth.tags.iter().any(|t| t.as_slice() == ["relay", server_origin.as_str()]));
            ws.send(Message::Text(json!(["OK", auth.id.to_hex(), true, ""]).to_string().into())).await.unwrap();
            let _websocket = AbortTask(tokio::spawn(async move {
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    // Never primed here: the polling fallback is under test.
                    if matches!(value[0].as_str(), Some("REQ" | "CLOSE")) {
                        continue;
                    }
                    assert_eq!(value[0], "COUNT");
                    ws.send(Message::Text(json!(["COUNT", value[1], {"count": 0}]).to_string().into())).await.unwrap();
                }
            }));
            let bounds = |d: String, content: Value| {
                event(&server_relay, 39006, &content.to_string(), vec![Tag::parse(["h", &server_room]).unwrap(), Tag::parse(["d", &d]).unwrap()])
            };
            loop {
                let (mut stream, _) = tokio::select! {
                    accepted = listener.accept() => accepted.unwrap(),
                    _ = &mut finish_wait => break,
                };
                let (head, body) = request(&mut stream).await;
                let payload = if head.starts_with("get /info ") {
                    json!({"self": signer.to_hex()}).to_string()
                } else {
                    let body = body.unwrap();
                    if body[0]["kinds"] == json!([9, 40002]) && body[0].get("top_level").is_none() {
                        "[]".to_string()
                    } else if body[0]["kinds"] == json!([39002]) {
                        serde_json::to_string(&vec![membership.clone()]).unwrap()
                    } else if body[0]["kinds"] == json!([39000]) {
                        serde_json::to_string(&vec![metadata.clone()]).unwrap()
                    } else if body[0].get("until").is_some() {
                        older_count.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(body, json!([{"kinds":[9,40002],"#h":[server_room],"limit":20,"top_level":true,"include_aux":true,"include_summaries":true,
                            "until": expected_cursor["created_at"], "before_id": expected_cursor["id"]}]));
                        let d = format!("{server_room}:{}:{}", expected_cursor["created_at"], expected_cursor["id"].as_str().unwrap());
                        serde_json::to_string(&vec![older_page.clone(), bounds(d, json!({"has_more": false, "next_cursor": null}))]).unwrap()
                    } else {
                        head_count.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(body, json!([{"kinds":[9,40002],"#h":[server_room],"limit":20,"top_level":true,"include_aux":true,"include_summaries":true}]));
                        serde_json::to_string(&vec![recent_page.clone(), bounds(format!("{server_room}:head"), json!({"has_more": true, "next_cursor": expected_cursor}))]).unwrap()
                    }
                };
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).as_bytes()).await;
            }
        }));
        let config = config::Config { relay: Some(origin.clone()), identity: Some(public.to_hex()), communities: Vec::new() };
        let (tx, mut status) = watch::channel(Status::new(&config));
        let (commands, mut command_rx) = mpsc::channel(4);
        let mut conn = connect_identity(&origin, &user).await.unwrap();
        let observer = AbortTask(tokio::spawn(async move {
            let mut pin = None;
            let mut backoff = Backoff::default();
            observe_connection(&mut conn, &user, &origin, &mut pin, &tx, &mut command_rx, &mut backoff,
                FreshnessPolicy { interval: Duration::from_secs(2), response: Duration::from_secs(1), ..FRESHNESS }).await
        }));
        wait_status(&mut status, |s| s.catalog.rooms.len() == 1).await;
        // Nothing is selected yet: an older read is ignored without any HTTP request.
        commands.send(crate::protocol::Command::FetchOlder(room.clone())).await.unwrap();
        commands.send(crate::protocol::Command::FetchRecent(room.clone())).await.unwrap();
        wait_status(&mut status, |s| s.history.state == "snapshot" && s.history.rows.len() == 1).await;
        assert_eq!(continuations.load(Ordering::SeqCst), 0);
        {
            let s = status.borrow();
            let cursor = s.history.next_cursor.as_ref().expect("signed continuation offered");
            assert_eq!((cursor.created_at, cursor.id.as_str()), (now - 10, recent.id.to_hex().as_str()));
            assert_eq!(s.history.has_more, Some(true));
        }
        commands.send(crate::protocol::Command::FetchOlder(room.clone())).await.unwrap();
        wait_status(&mut status, |s| s.history.rows.len() == 2 && s.history.older_state == "idle").await;
        {
            let s = status.borrow();
            assert_eq!(s.history.rows[0].id, older.id.to_hex(), "older row goes first");
            assert_eq!(s.history.rows[1].id, recent.id.to_hex());
            assert!(s.history.next_cursor.is_none());
            assert_eq!(s.history.has_more, Some(false));
            assert_eq!(s.history.category.as_deref(), Some("history_completeness_unknown"));
        }
        assert_eq!(continuations.load(Ordering::SeqCst), 1);
        // The automatic head refresh keeps the older page.
        let before = heads.load(Ordering::SeqCst);
        timeout(Duration::from_secs(8), async {
            while heads.load(Ordering::SeqCst) == before {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("automatic head refresh");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(status.borrow().history.rows.len(), 2, "head refresh dropped the older page");
        // Selecting the room again starts from its head.
        commands.send(crate::protocol::Command::FetchRecent(room.clone())).await.unwrap();
        wait_status(&mut status, |s| s.history.state == "snapshot" && s.history.rows.len() == 1).await;
        assert!(status.borrow().history.next_cursor.is_some());
        assert_eq!(continuations.load(Ordering::SeqCst), 1);
        // Another room's request never reads older pages.
        commands.send(crate::protocol::Command::FetchOlder(uuid::Uuid::new_v4().to_string())).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(continuations.load(Ordering::SeqCst), 1);
        drop(observer);
        let _ = finish_send.send(());
        drop(server);
    })
    .await
    .expect("older history observer fixture deadline");
}
