use super::*;
use crate::{ledger::Ledger, protocol::SendIntent, sending::Sender};
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use tokio::{io::AsyncReadExt, net::TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};

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

struct AbortOnDrop(tokio::task::AbortHandle);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn integrated(mode: u8) {
    let reauth = mode == 1;
    let withhold = mode == 2;
    use tokio::io::AsyncWriteExt;
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(if withhold {25} else {12}),async {
        let user=Keys::generate();let signer=Keys::generate();let public=user.public_key();let room=uuid::Uuid::new_v4().to_string();
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let origin=format!("ws://{}/",listener.local_addr().unwrap());
        let signed=|kind, tags|nostr::EventBuilder::new(nostr::Kind::Custom(kind),"").tags(tags).sign_with_keys(&signer).unwrap();
        let membership=signed(39002,vec![nostr::Tag::parse(["d",&room]).unwrap(),nostr::Tag::parse(["p",&public.to_hex()]).unwrap()]);
        let metadata=signed(39000,vec![nostr::Tag::parse(["d",&room]).unwrap(),nostr::Tag::parse(["name","Send Fixture"]).unwrap(),nostr::Tag::parse(["t","stream"]).unwrap()]);
        let payloads=vec![json!({"self":signer.public_key().to_hex()}).to_string(),serde_json::to_string(&vec![membership]).unwrap(),serde_json::to_string(&vec![metadata]).unwrap()];
        let expected_room=room.clone();
        let (done_send,mut done_wait)=tokio::sync::oneshot::channel();
        let server=tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.unwrap();let mut ws=accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!(["AUTH","fixture"]).to_string().into())).await.unwrap();
            let Message::Text(raw)=ws.next().await.unwrap().unwrap() else {panic!()};let frame:Value=serde_json::from_str(&raw).unwrap();let auth:Event=serde_json::from_value(frame[1].clone()).unwrap();auth.verify().unwrap();
            ws.send(Message::Text(json!(["OK",auth.id.to_hex(),true,""]).to_string().into())).await.unwrap();
            let websocket=tokio::spawn(async move {
                let mut event=None;let mut count_after_event=false;
                loop {tokio::select! {
                    _=&mut done_wait=>break,
                    frame=ws.next()=>{
                        let Some(Ok(Message::Text(raw)))=frame else {break};let frame:Value=serde_json::from_str(&raw).unwrap();
                        match frame[0].as_str().unwrap() {
                            "COUNT"=>{
                                ws.send(Message::Text(json!(["COUNT",frame[1],{"count":0}]).to_string().into())).await.unwrap();
                                if event.is_some(){count_after_event=true;if !withhold {let id=event.take().unwrap();ws.send(Message::Text(json!(["OK",id,true,""]).to_string().into())).await.unwrap();}}
                            },
                            "AUTH"=>{let auth:Event=serde_json::from_value(frame[1].clone()).unwrap();auth.verify().unwrap();assert_eq!(auth.pubkey,public);ws.send(Message::Text(json!(["OK",auth.id.to_hex(),true,""]).to_string().into())).await.unwrap();},
                            "EVENT"=>{let message:Event=serde_json::from_value(frame[1].clone()).unwrap();message.verify().unwrap();assert_eq!(message.pubkey,public);assert_eq!(message.kind.as_u16(),9);assert!(message.tags.iter().any(|t|t.as_slice()==["h",expected_room.as_str()]));assert_eq!(message.content,"observer fixture");event=Some(message.id.to_hex());ws.send(Message::Text(json!(["OK","0".repeat(64),true,""]).to_string().into())).await.unwrap();if reauth {ws.send(Message::Text(json!(["AUTH","reauth-send"]).to_string().into())).await.unwrap();}},
                            other=>panic!("unexpected {other}"),
                        }
                    }
                }}
                assert!(count_after_event);
            });
            let _websocket_guard=AbortOnDrop(websocket.abort_handle());
            for payload in payloads {let (mut stream,_)=listener.accept().await.unwrap();let _=request(&mut stream).await;stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload).as_bytes()).await.unwrap();}
            websocket.await.unwrap();
        });
        let _server_guard=AbortOnDrop(server.abort_handle());
        let path=std::env::temp_dir().join(format!("buzz-observer-send-{}",uuid::Uuid::new_v4()));let ledger=Ledger::open(path.join("ledger.json")).unwrap();
        let (tx,mut rx)=watch::channel(Status::new(&config::Config{relay:Some(origin.clone()),identity:Some(public.to_hex())}));let (commands,mut requests)=mpsc::channel(4);let mut conn=connect_identity(&origin,&user).await.unwrap();
        let observer=tokio::spawn(async move {let mut sender=Sender::new(Some(ledger));let mut backoff=Backoff::default();let mut pin=None;observe_sending(&mut conn,&user,&origin,&mut pin,&tx,&mut requests,&mut backoff,FreshnessPolicy{interval:Duration::from_millis(150),response:Duration::from_secs(1)},&mut sender).await});
        let _observer_guard=AbortOnDrop(observer.abort_handle());
        while rx.borrow().catalog.rooms.is_empty(){rx.changed().await.unwrap();}
        let sent_at=tokio::time::Instant::now();
        let generation=rx.borrow().generation;
        commands.send(Command::Send(SendIntent{request_id:uuid::Uuid::new_v4().to_string(),room,text:"observer fixture".into(),mentions:vec![],generation})).await.unwrap();
        while rx.borrow().delivery.state!=if reauth || withhold {"unknown"} else {"acknowledged"} {rx.changed().await.unwrap();}
        assert!(rx.borrow().delivery.event_id.is_some());
        if withhold {assert!(sent_at.elapsed()>=Duration::from_millis(14900));assert_eq!(rx.borrow().connection,"authenticated");assert_eq!(rx.borrow().delivery.category.as_deref(),Some("delivery_unknown"));}
        if reauth {while rx.borrow().connection!="authenticated" {rx.changed().await.unwrap();} tokio::time::sleep(Duration::from_millis(200)).await;assert_eq!(rx.borrow().delivery.state,"unknown");}
        commands.send(Command::Retry).await.unwrap();assert!(matches!(observer.await.unwrap(),ConnectionExit::Retry));let _=done_send.send(());server.await.unwrap();std::fs::remove_dir_all(path).unwrap();
    }).await.unwrap();
}

#[tokio::test]
async fn observer_signed_send_ack_keeps_count_live() {
    integrated(0).await;
}
#[tokio::test]
async fn observer_reauth_pending_send_remains_unknown() {
    integrated(1).await;
}

#[tokio::test]
async fn observer_missing_ack_expires_while_count_stays_fresh() {
    integrated(2).await;
}
