use super::*;
use nostr::{nips::nip98::verify_auth_header, Kind, Timestamp};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};

struct Capture {
    auth: String,
    body: Vec<u8>,
    head: String,
}
async fn capture(tcp: &mut TcpStream) -> Capture {
    let mut all = Vec::new();
    let header_end;
    loop {
        let mut b = [0; 1024];
        let n = tcp.read(&mut b).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&b[..n]);
        assert!(all.len() <= 16384);
        if let Some(p) = all.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = p + 4;
            break;
        }
    }
    let head = String::from_utf8(all[..header_end].to_vec()).unwrap();
    let get = |name: &str| {
        head.lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.trim().to_string())
            })
            .unwrap()
    };
    let auth = get("authorization");
    let length: usize = get("content-length").parse().unwrap();
    assert!(length <= 8192);
    let mut body = all[header_end..].to_vec();
    while body.len() < length {
        let mut b = [0; 1024];
        let n = tcp.read(&mut b).await.unwrap();
        assert!(n > 0);
        body.extend_from_slice(&b[..n]);
    }
    assert_eq!(body.len(), length);
    Capture { auth, body, head }
}
async fn server(response: Vec<u8>) -> (String, JoinHandle<Capture>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        timeout(Duration::from_secs(3), async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let request = capture(&mut tcp).await;
            let _ = tcp.write_all(&response).await;
            request
        })
        .await
        .unwrap()
    });
    (relay, task)
}
fn response(body: &[u8]) -> Vec<u8> {
    let mut wire=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
    wire.extend_from_slice(body);
    wire
}
fn membership(keys: &Keys) -> Event {
    EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", &Uuid::new_v4().to_string()]).unwrap(),
            Tag::public_key(keys.public_key()),
        ])
        .sign_with_keys(&Keys::generate())
        .unwrap()
}

#[tokio::test]
async fn sdk_reaction_without_h_is_valid_aux_but_wrong_scope_or_signature_fails() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let room = Uuid::new_v4();
    let root = EventBuilder::new(Kind::Custom(9), "root")
        .tags([Tag::parse(["h", &room.to_string()]).unwrap()])
        .sign_with_keys(&keys)
        .unwrap();
    let reaction = buzz_sdk::build_reaction(root.id, "+")
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
    assert!(!reaction
        .tags
        .iter()
        .any(|tag| tag.as_slice().first().map(String::as_str) == Some("h")));
    for request in [
        QueryRequest::RoomHistory {
            room,
            limit: 20,
            before: None,
        },
        QueryRequest::ThreadReplies {
            room,
            root: root.id,
            after: None,
        },
    ] {
        let (origin, server_task) = server(response(
            &serde_json::to_vec(&vec![reaction.clone()]).unwrap(),
        ))
        .await;
        assert_eq!(
            query(&origin, &keys, &request).await.unwrap()[0].id,
            reaction.id
        );
        server_task.await.unwrap();
        let wrong = EventBuilder::new(Kind::Custom(7), "+")
            .tags([
                Tag::parse(["h", &Uuid::new_v4().to_string()]).unwrap(),
                Tag::event(root.id),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let (origin, server_task) =
            server(response(&serde_json::to_vec(&vec![wrong]).unwrap())).await;
        assert!(matches!(
            query(&origin, &keys, &request).await,
            Err("query_invalid_scope")
        ));
        server_task.await.unwrap();
        let mut forged = reaction.clone();
        forged.content = "forged".into();
        let (origin, server_task) =
            server(response(&serde_json::to_vec(&vec![forged]).unwrap())).await;
        assert!(matches!(
            query(&origin, &keys, &request).await,
            Err("query_invalid_signature")
        ));
        server_task.await.unwrap();
    }
}
#[test]
fn endpoint_and_nonce_binding() {
    assert_eq!(
        endpoint("wss://EXAMPLE.com").unwrap().as_str(),
        "https://example.com/query"
    );
    assert!(endpoint("wss://example.com/path").is_err());
    assert!(endpoint("ws://external.example").is_err());
    let keys = Keys::generate();
    let url = endpoint("ws://localhost:3000").unwrap();
    let body = b"[]";
    let first = authorization(&keys, &url, body).unwrap();
    let second = authorization(&keys, &url, body).unwrap();
    assert_ne!(first, second);
    assert!(first.is_sensitive());
    assert_eq!(
        verify_auth_header(
            first.to_str().unwrap(),
            &nostr::Url::parse(url.as_str()).unwrap(),
            HttpMethod::POST,
            Timestamp::now(),
            Some(body)
        )
        .unwrap(),
        keys.public_key()
    );
    assert!(verify_auth_header(
        first.to_str().unwrap(),
        &nostr::Url::parse(url.as_str()).unwrap(),
        HttpMethod::POST,
        Timestamp::now(),
        Some(b"[{}]")
    )
    .is_err());
    let decoded = STANDARD
        .decode(first.to_str().unwrap().strip_prefix("Nostr ").unwrap())
        .unwrap();
    let event: Event = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(
        event
            .tags
            .iter()
            .filter(|t| t.as_slice()[0] == "nonce")
            .count(),
        1
    );
}
#[tokio::test]
async fn exact_bytes_and_auth_reach_fixed_query() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let event = membership(&keys);
    let body = serde_json::to_vec(&vec![event.clone()]).unwrap();
    let (relay, task) = server(response(&body)).await;
    let events = query(
        &relay,
        &keys,
        &QueryRequest::JoinedRooms {
            limit: 20,
            before: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event.id);
    let got = task.await.unwrap();
    assert!(got.head.starts_with("POST /query HTTP/1.1\r\n"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&got.body).unwrap(),
        serde_json::json!([{"kinds":[39002],"#p":[keys.public_key().to_hex()],"limit":20}])
    );
    assert_eq!(
        verify_auth_header(
            &got.auth,
            &nostr::Url::parse(endpoint(&relay).unwrap().as_str()).unwrap(),
            HttpMethod::POST,
            Timestamp::now(),
            Some(&got.body)
        )
        .unwrap(),
        keys.public_key()
    );
}
#[tokio::test]
async fn redirect_contacts_no_target() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let location = format!("http://{}/query", target.local_addr().unwrap());
    let wire=format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes();
    let (relay, task) = server(wire).await;
    assert!(matches!(
        query(
            &relay,
            &Keys::generate(),
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_redirect_rejected")
    ));
    assert!(timeout(Duration::from_millis(50), target.accept())
        .await
        .is_err());
    task.await.unwrap();
}
#[tokio::test]
async fn rejects_length_and_chunked_overflow() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let wire = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        RESPONSE_BYTES + 1
    )
    .into_bytes();
    let (relay, task) = server(wire).await;
    assert!(matches!(
        query(
            &relay,
            &Keys::generate(),
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_oversized")
    ));
    task.await.unwrap();
    let mut wire =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
    for length in [65536; 9] {
        wire.extend_from_slice(format!("{length:x}\r\n").as_bytes());
        wire.extend(std::iter::repeat_n(b' ', length));
        wire.extend_from_slice(b"\r\n");
    }
    wire.extend_from_slice(b"0\r\n\r\n");
    let (relay, task) = server(wire).await;
    assert!(matches!(
        query(
            &relay,
            &Keys::generate(),
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_oversized")
    ));
    task.await.unwrap();
}
#[tokio::test]
async fn rejects_malformed_signature_and_scope() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    for body in [b"{}".to_vec(), b"[broken".to_vec()] {
        let (relay, task) = server(response(&body)).await;
        assert!(matches!(
            query(
                &relay,
                &keys,
                &QueryRequest::JoinedRooms {
                    limit: 1,
                    before: None
                }
            )
            .await,
            Err("query_invalid_response")
        ));
        task.await.unwrap();
    }
    let mut event = membership(&keys);
    event.content = "changed after signing".into();
    let body = serde_json::to_vec(&vec![event]).unwrap();
    let (relay, task) = server(response(&body)).await;
    assert!(matches!(
        query(
            &relay,
            &keys,
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_invalid_signature")
    ));
    task.await.unwrap();
    let event = membership(&Keys::generate());
    let body = serde_json::to_vec(&vec![event]).unwrap();
    let (relay, task) = server(response(&body)).await;
    assert!(matches!(
        query(
            &relay,
            &keys,
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_invalid_scope")
    ));
    task.await.unwrap();
}
#[tokio::test]
async fn response_reason_does_not_escape_category() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let(relay,task)=server(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 18\r\nConnection: close\r\n\r\nsentinel-sensitive".to_vec()).await;
    assert!(matches!(
        query(
            &relay,
            &Keys::generate(),
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_auth_rejected")
    ));
    task.await.unwrap();
}

#[tokio::test]
async fn concurrent_capacity_rejects_without_network() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let _held = IN_FLIGHT.acquire_many(2).await.unwrap();
    assert!(matches!(
        query(
            "ws://localhost:3000",
            &Keys::generate(),
            &QueryRequest::JoinedRooms {
                limit: 1,
                before: None
            }
        )
        .await,
        Err("query_busy")
    ));
}
#[test]
fn metadata_request_is_typed_and_bounded() {
    let keys = Keys::generate();
    let room = Uuid::new_v4();
    let request = QueryRequest::RoomMetadata { rooms: vec![room] };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body(&keys).unwrap()).unwrap(),
        serde_json::json!([{"kinds":[39000],"#d":[room.to_string()],"limit":1}])
    );
    assert!(QueryRequest::RoomMetadata { rooms: vec![] }
        .body(&keys)
        .is_err());
    assert!(QueryRequest::RoomMetadata {
        rooms: vec![room; 51]
    }
    .body(&keys)
    .is_err());
    assert!(QueryRequest::JoinedRooms {
        limit: 0,
        before: None
    }
    .body(&keys)
    .is_err());
    assert!(QueryRequest::JoinedRooms {
        limit: 51,
        before: None
    }
    .body(&keys)
    .is_err());
}

#[test]
fn agent_profile_request_is_exact_author_and_bounded() {
    let viewer = Keys::generate();
    let agent = Keys::generate();
    let authors = vec![agent.public_key()];
    let request = QueryRequest::AgentProfiles {
        authors: authors.clone(),
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body(&viewer).unwrap()).unwrap(),
        serde_json::json!([{"kinds":[10100],"authors":[agent.public_key().to_hex()],"limit":1}])
    );
    let profile = EventBuilder::new(Kind::Custom(10100), r#"{"name":"Agent"}"#)
        .sign_with_keys(&agent)
        .unwrap();
    assert!(request.matches(&profile, &viewer));
    assert!(!QueryRequest::AgentProfiles {
        authors: vec![viewer.public_key()]
    }
    .matches(&profile, &viewer));
    assert!(!QueryRequest::Profiles { authors }.matches(&profile, &viewer));
    assert!(QueryRequest::AgentProfiles { authors: vec![] }
        .body(&viewer)
        .is_err());
    assert!(QueryRequest::AgentProfiles {
        authors: vec![agent.public_key(); 21]
    }
    .body(&viewer)
    .is_err());
}

#[test]
fn only_room_history_requests_and_admits_thread_summaries() {
    let keys = Keys::generate();
    let relay = Keys::generate();
    let room = Uuid::new_v4();
    let root = EventBuilder::new(Kind::Custom(9), "root")
        .sign_with_keys(&keys)
        .unwrap();
    let history = QueryRequest::RoomHistory {
        room,
        limit: 20,
        before: None,
    };
    let thread = QueryRequest::ThreadReplies {
        room,
        root: root.id,
        after: None,
    };
    let body = |request: &QueryRequest| {
        serde_json::from_slice::<serde_json::Value>(&request.body(&keys).unwrap()).unwrap()
    };
    assert_eq!(body(&history)[0]["include_summaries"], true);
    assert!(body(&thread)[0].get("include_summaries").is_none());
    let summary = |h: &str| {
        EventBuilder::new(Kind::Custom(39005), "{}")
            .tags([
                Tag::event(root.id),
                Tag::parse(["d", &root.id.to_hex()]).unwrap(),
                Tag::parse(["h", h]).unwrap(),
            ])
            .sign_with_keys(&relay)
            .unwrap()
    };
    assert!(history.matches(&summary(&room.to_string()), &keys));
    assert!(!history.matches(&summary(&Uuid::new_v4().to_string()), &keys));
    assert!(!thread.matches(&summary(&room.to_string()), &keys));
}

#[test]
fn dm_visibility_request_is_viewer_scoped() {
    let viewer = Keys::generate();
    let relay = Keys::generate();
    let request = QueryRequest::DmVisibility;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body(&viewer).unwrap()).unwrap(),
        serde_json::json!([{"kinds":[30622],"#p":[viewer.public_key().to_hex()],"limit":1}])
    );
    let own = viewer.public_key().to_hex();
    let mine = EventBuilder::new(Kind::Custom(30622), "")
        .tags([Tag::parse(["p", own.as_str()]).unwrap()])
        .sign_with_keys(&relay)
        .unwrap();
    assert!(request.matches(&mine, &viewer));
    let theirs = EventBuilder::new(Kind::Custom(30622), "")
        .tags([Tag::parse(["p", relay.public_key().to_hex().as_str()]).unwrap()])
        .sign_with_keys(&relay)
        .unwrap();
    assert!(!request.matches(&theirs, &viewer));
}

#[test]
fn user_status_request_is_author_scoped_general_and_bounded() {
    let viewer = Keys::generate();
    let member = Keys::generate();
    let authors = vec![viewer.public_key(), member.public_key()];
    let request = QueryRequest::UserStatuses {
        authors: authors.clone(),
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body(&viewer).unwrap()).unwrap(),
        serde_json::json!([{"kinds":[30315],"authors":[viewer.public_key().to_hex(),member.public_key().to_hex()],"#d":["general"],"limit":2}])
    );
    let status = |author: &Keys, d: &str| {
        EventBuilder::new(Kind::Custom(30315), "Busy")
            .tags([Tag::parse(["d", d]).unwrap()])
            .sign_with_keys(author)
            .unwrap()
    };
    assert!(request.matches(&status(&member, "general"), &viewer));
    assert!(!request.matches(&status(&member, "music"), &viewer));
    assert!(!request.matches(&status(&Keys::generate(), "general"), &viewer));
    assert!(!QueryRequest::Profiles { authors }.matches(&status(&member, "general"), &viewer));
    assert!(QueryRequest::UserStatuses { authors: vec![] }
        .body(&viewer)
        .is_err());
    assert!(QueryRequest::UserStatuses {
        authors: vec![member.public_key(); 21]
    }
    .body(&viewer)
    .is_err());
}

#[test]
fn people_requests_use_desktops_kind_zero_filters() {
    let viewer = Keys::generate();
    let json = |request: &QueryRequest| {
        serde_json::from_slice::<serde_json::Value>(&request.body(&viewer).unwrap()).unwrap()
    };
    let directory = QueryRequest::People {
        query: String::new(),
    };
    assert_eq!(
        json(&directory),
        serde_json::json!([{"kinds":[0],"limit":50,"page":1}])
    );
    let search = QueryRequest::People {
        query: "tyl".into(),
    };
    assert_eq!(
        json(&search),
        serde_json::json!([{"kinds":[0],"search":"tyl","search_mode":"prefix","limit":50,"page":1}])
    );
    assert!(QueryRequest::People {
        query: "x".repeat(65)
    }
    .body(&viewer)
    .is_err());
    assert_eq!(search.budget().0, 50);
    let profile = EventBuilder::new(Kind::Custom(0), "{}")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let note = EventBuilder::new(Kind::Custom(1), "x")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    assert!(search.matches(&profile, &viewer) && directory.matches(&profile, &viewer));
    assert!(!search.matches(&note, &viewer));
}
