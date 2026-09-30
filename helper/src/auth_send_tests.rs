//! Actual upstream NIP-42 transport plus SDK sender, with isolated loopback peer.
use super::*;
use crate::{
    ledger::Ledger,
    protocol::{Room, SendIntent},
    sending::Sender,
};
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn scenario(accept: Option<bool>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}/", listener.local_addr().unwrap());
    let keys = Keys::generate();
    let author = keys.public_key();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        ws.send(Message::Text(
            json!(["AUTH", "synthetic-send"]).to_string().into(),
        ))
        .await
        .unwrap();
        let Message::Text(raw) = ws.next().await.unwrap().unwrap() else {
            panic!()
        };
        let frame: Value = serde_json::from_str(&raw).unwrap();
        let auth: Event = serde_json::from_value(frame[1].clone()).unwrap();
        auth.verify().unwrap();
        assert_eq!(auth.pubkey, author);
        ws.send(Message::Text(
            json!(["OK", auth.id.to_hex(), true, ""]).to_string().into(),
        ))
        .await
        .unwrap();
        let Message::Text(raw) = ws.next().await.unwrap().unwrap() else {
            panic!()
        };
        let frame: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(frame[0], "EVENT");
        let event: Event = serde_json::from_value(frame[1].clone()).unwrap();
        event.verify().unwrap();
        assert_eq!(event.pubkey, author);
        assert_eq!(event.kind.as_u16(), 9);
        ws.send(Message::Text(
            json!(["OK", "0".repeat(64), true, ""]).to_string().into(),
        ))
        .await
        .unwrap();
        // A probe remains independently routable while acknowledgement is pending.
        let Message::Text(raw) = ws.next().await.unwrap().unwrap() else {
            panic!()
        };
        let frame: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(frame[0], "COUNT");
        ws.send(Message::Text(
            json!(["COUNT",frame[1],{"count":0}]).to_string().into(),
        ))
        .await
        .unwrap();
        if let Some(accepted) = accept {
            ws.send(Message::Text(
                json!(["OK", event.id.to_hex(), accepted, "fixture"])
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        } else {
            ws.close(None).await.unwrap();
        }
    });
    let mut conn = connect_identity(&origin, &keys).await.unwrap();
    let path = std::env::temp_dir().join(format!("buzz-send-wire-{}", uuid::Uuid::new_v4()));
    let mut sender = Sender::new(Some(Ledger::open(path.join("ledger.json")).unwrap()));
    let intent = SendIntent {
        request_id: uuid::Uuid::new_v4().to_string(),
        room: uuid::Uuid::new_v4().to_string(),
        root_id: None,
        text: "synthetic only".into(),
        mentions: vec![],
        generation: 1,
    };
    let mut status = Status::new(&config::Config::default());
    status.connection = "authenticated".into();
    status.catalog.rooms.push(Room {
        id: intent.room.clone(),
        name: "fixture".into(),
        description: String::new(),
        kind: "stream".into(),
        participants: Vec::new(),
        hidden: false,
    });
    let (_, event) = sender.prepare(intent, &origin, &keys, &status, true, true);
    let event = event.unwrap();
    conn.send_raw(&json!(["EVENT", event])).await.unwrap();
    let RelayMessage::Ok(wrong) = conn.next_event(Duration::from_secs(2)).await.unwrap() else {
        panic!()
    };
    assert!(sender
        .acknowledge(&wrong.event_id, wrong.accepted)
        .is_none());
    conn.send_raw(&json!(["COUNT","pending-send-probe",{"kinds":[0],"limit":1}]))
        .await
        .unwrap();
    let RelayMessage::Count {
        subscription_id, ..
    } = conn.next_event(Duration::from_secs(2)).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(subscription_id, "pending-send-probe");
    assert!(sender.is_pending());
    if let Some(accepted) = accept {
        let RelayMessage::Ok(ok) = conn.next_event(Duration::from_secs(2)).await.unwrap() else {
            panic!()
        };
        assert_eq!(
            sender.acknowledge(&ok.event_id, ok.accepted).unwrap().state,
            if accepted { "acknowledged" } else { "rejected" }
        );
    } else {
        assert!(conn.next_event(Duration::from_secs(2)).await.is_err());
        assert_eq!(sender.unknown().unwrap().state, "unknown");
    }
    timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn exact_ack_with_interleaved_count() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(8), scenario(Some(true)))
        .await
        .unwrap();
}
#[tokio::test]
async fn negative_ack_is_rejected() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(8), scenario(Some(false)))
        .await
        .unwrap();
}
#[tokio::test]
async fn lost_ack_is_unknown() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(8), scenario(None))
        .await
        .unwrap();
}
