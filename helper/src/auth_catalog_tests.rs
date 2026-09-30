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
        let config=config::Config{relay:Some(origin.clone()),identity:Some(public.to_hex())};
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
