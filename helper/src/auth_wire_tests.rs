//! Synthetic loopback conformance tests through the production connection seam.
use super::*;
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    (listener, url)
}
async fn send(ws: &mut WebSocketStream<TcpStream>, value: Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
async fn auth_event(
    ws: &mut WebSocketStream<TcpStream>,
    relay: &str,
    challenge: &str,
    expected: nostr::PublicKey,
) -> Event {
    let frame = timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Text(text) = frame else {
        panic!("expected AUTH text frame")
    };
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value[0], "AUTH");
    let event: Event = serde_json::from_value(value[1].clone()).unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind, nostr::Kind::Authentication);
    assert_eq!(event.pubkey, expected);
    assert!(event.content.is_empty());
    assert_eq!(event.tags.len(), 2);
    for (name, expected) in [("relay", relay), ("challenge", challenge)] {
        assert!(event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == [name, expected]));
    }
    event
}

#[tokio::test]
async fn auth_requires_matching_ack_and_preserves_unrelated_ok() {
    let (listener, url) = listener().await;
    let keys = Keys::generate();
    let public = keys.public_key();
    let server_url = url.clone();
    let (release, wait) = tokio::sync::oneshot::channel();
    let (sent_wrong, wrong_observed) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        send(&mut ws, json!(["AUTH", "synthetic-challenge"])).await;
        let event = auth_event(&mut ws, &server_url, "synthetic-challenge", public).await;
        send(&mut ws, json!(["OK", "00".repeat(32), true, ""])).await;
        sent_wrong.send(()).unwrap();
        timeout(Duration::from_secs(2), wait)
            .await
            .unwrap()
            .unwrap();
        send(&mut ws, json!(["OK", event.id.to_hex(), true, ""])).await;
    });
    let future = connect_identity(&url, &keys);
    tokio::pin!(future);
    // Drive connection alongside the relay; the unrelated accepted ID must not
    // satisfy authentication, even though its boolean is true.
    tokio::select! {
        _ = wrong_observed => {},
        _ = &mut future => panic!("authenticated before exact acknowledgment"),
        _ = tokio::time::sleep(Duration::from_secs(2)) => panic!("relay deadline"),
    }
    assert!(timeout(Duration::from_millis(30), &mut future)
        .await
        .is_err());
    release.send(()).unwrap();
    let mut connection = timeout(Duration::from_secs(2), &mut future)
        .await
        .unwrap()
        .unwrap();
    match connection.next_event(Duration::from_secs(1)).await.unwrap() {
        RelayMessage::Ok(ok) => assert_eq!(ok.event_id, "00".repeat(32)),
        _ => panic!("unrelated relay frame was lost"),
    }
    server.await.unwrap();
}

#[tokio::test]
async fn auth_rejection_returns_only_safe_category() {
    let (listener, url) = listener().await;
    let keys = Keys::generate();
    let public = keys.public_key();
    let server_url = url.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        send(&mut ws, json!(["AUTH", "reject-challenge"])).await;
        let event = auth_event(&mut ws, &server_url, "reject-challenge", public).await;
        send(
            &mut ws,
            json!(["OK", event.id.to_hex(), false, "synthetic-sensitive-reason"]),
        )
        .await;
    });
    let result = timeout(Duration::from_secs(2), connect_identity(&url, &keys))
        .await
        .unwrap();
    assert!(matches!(result, Err("auth_rejected")));
    server.await.unwrap();
}

#[tokio::test]
async fn consumed_auth_notification_is_available_for_reauthentication() {
    let (listener, url) = listener().await;
    let keys = Keys::generate();
    let public = keys.public_key();
    let server_url = url.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        for challenge in ["first-challenge", "second-challenge"] {
            send(&mut ws, json!(["AUTH", challenge])).await;
            let event = auth_event(&mut ws, &server_url, challenge, public).await;
            send(&mut ws, json!(["OK", event.id.to_hex(), true, ""])).await;
        }
    });
    let mut connection = timeout(Duration::from_secs(2), connect_identity(&url, &keys))
        .await
        .unwrap()
        .unwrap();
    match connection.next_event(Duration::from_secs(2)).await.unwrap() {
        RelayMessage::Auth { challenge } => assert_eq!(challenge, "second-challenge"),
        _ => panic!("expected second authentication challenge"),
    }
    timeout(Duration::from_secs(2), connection.authenticate(&keys, None))
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
}
