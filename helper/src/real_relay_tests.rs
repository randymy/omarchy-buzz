//! Opt-in component conformance against a disposable pinned Buzz server.
//! The runner bootstraps the public key of the explicitly synthetic owner below.
//! No production config, keyring, external host, or SQL connection is used here.
use crate::{
    catalog, config, history, ledger::Ledger, protocol, recipients, sending::Sender, thread,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use buzz_sdk::builders::{build_add_member, build_create_channel, build_remove_member};
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use nostr::{
    hashes::{sha256, Hash},
    nips::nip98::{HttpData, HttpMethod},
    Event, EventBuilder, JsonUtil, Keys, Tag,
};
use reqwest::{header::AUTHORIZATION, redirect::Policy, Client, StatusCode};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

struct FixtureDirectory(PathBuf);
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture_origin() -> String {
    let raw = std::env::var("OMARCHY_BUZZ_TEST_RELAY_URL")
        .expect("explicit disposable OMARCHY_BUZZ_TEST_RELAY_URL required");
    let url = url::Url::parse(&raw).expect("fixture URL");
    assert_eq!(url.scheme(), "ws");
    assert!(matches!(url.host_str(), Some("127.0.0.1" | "[::1]")));
    assert!(url.port().is_some(), "explicit disposable port required");
    assert!(url.username().is_empty() && url.password().is_none());
    assert!(url.query().is_none() && url.fragment().is_none());
    assert!(url.path().is_empty() || url.path() == "/");
    config::canonical_relay(&raw).expect("canonical fixture origin")
}
async fn publish(conn: &mut NostrWsConnection, event: Event) {
    let id = event.id.to_hex();
    let ok = timeout(Duration::from_secs(10), conn.send_event(event))
        .await
        .expect("publish deadline")
        .expect("publish transport");
    assert_eq!(ok.event_id, id);
    assert!(ok.accepted, "fixture event rejected: {}", ok.message);
}

// Exercise the ordinary NIP-98 bridge with a fresh authorization for each
// request. The caller has already restricted relay to a disposable loopback URL.
async fn post_event(relay: &str, keys: &Keys, event: &Event) -> (StatusCode, serde_json::Value) {
    let mut url = url::Url::parse(relay).unwrap();
    url.set_scheme("http").unwrap();
    url.set_path("/events");
    let body = event.as_json();
    let auth = EventBuilder::http_auth(
        HttpData::new(nostr::Url::parse(url.as_str()).unwrap(), HttpMethod::POST)
            .payload(sha256::Hash::hash(body.as_bytes())),
    )
    .tags([Tag::parse(["nonce", &Uuid::new_v4().to_string()]).unwrap()])
    .sign_with_keys(keys)
    .unwrap();
    let client = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let response = client
        .post(url)
        .header(
            AUTHORIZATION,
            format!("Nostr {}", STANDARD.encode(auth.as_json())),
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .expect("ordinary event POST transport");
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .expect("ordinary event POST response body");
    assert!(
        bytes.len() <= 8192,
        "ordinary event POST response too large"
    );
    let result = serde_json::from_slice(&bytes).expect("ordinary event POST JSON response");
    (status, result)
}

#[tokio::test]
#[ignore = "requires explicitly started disposable upstream relay and synthetic owner seed"]
async fn real_relay_messaging_conformance() {
    let _serial = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(90), async {
        let relay = fixture_origin();
        // Deliberately public fixture scalar; never enroll it in any credential store.
        let owner = Keys::parse("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let user = Keys::generate();
        eprintln!("OMARCHY_CONFORMANCE_STAGE=owner_auth");
        let mut admin = NostrWsConnection::connect_authenticated(&relay, &owner, None)
            .await
            .expect("owner NIP-42");
        eprintln!("OMARCHY_CONFORMANCE_STAGE=member_auth");
        let mut client = NostrWsConnection::connect_authenticated(&relay, &user, None)
            .await
            .expect("synthetic member NIP-42");
        eprintln!("OMARCHY_CONFORMANCE_STAGE=create_room");
        let room = Uuid::new_v4();
        publish(
            &mut admin,
            build_create_channel(
                room,
                &format!("omarchy-conformance-{room}"),
                Some(buzz_sdk::Visibility::Private),
                Some(buzz_sdk::ChannelKind::Stream),
                None,
                None,
            )
            .unwrap()
            .sign_with_keys(&owner)
            .unwrap(),
        )
        .await;
        publish(
            &mut admin,
            build_add_member(room, &user.public_key().to_hex(), None)
                .unwrap()
                .sign_with_keys(&owner)
                .unwrap(),
        )
        .await;
        eprintln!("OMARCHY_CONFORMANCE_STAGE=catalog");
        let mut discovery_error = "room_not_observed";
        let discovered = timeout(Duration::from_secs(12), async {
            loop {
                match catalog::discover(&relay, &user, None).await {
                    Ok(c) if c.rooms.iter().any(|r| r.id == room.to_string()) => break c,
                    Ok(_) => discovery_error = "room_not_observed",
                    Err(category) => discovery_error = category,
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("joined-room discovery deadline: {discovery_error}"));
        eprintln!("OMARCHY_CONFORMANCE_STAGE=roster");
        let roster = recipients::fetch(&relay, &user, discovered.signer, room)
            .await
            .expect("relay-signed roster");
        let mentioned = owner.public_key().to_hex();
        assert!(roster.entries.iter().any(|r| r.key == mentioned));
        assert!(roster
            .entries
            .iter()
            .any(|r| r.key == user.public_key().to_hex()));
        let mut status = protocol::Status::new(&config::Config {
            relay: Some(relay.clone()),
            identity: Some(user.public_key().to_hex()),
        });
        status.connection = "authenticated".into();
        status.catalog.rooms = discovered
            .rooms
            .iter()
            .map(|r| protocol::Room {
                id: r.id.clone(),
                name: r.name.clone(),
                description: r.description.clone(),
            })
            .collect();
        status.recipients = protocol::RecipientsView {
            agents: roster.agents,
            state: "snapshot".into(),
            room_id: Some(roster.room),
            entries: roster
                .entries
                .into_iter()
                .map(|r| protocol::Recipient {
                    key: r.key,
                    name: r.name,
                })
                .collect(),
            partial: roster.partial,
            category: None,
        };
        let dir = FixtureDirectory(
            std::env::temp_dir().join(format!("omarchy-real-relay-{}", Uuid::new_v4())),
        );
        let mut sender = Sender::new(Some(Ledger::open(dir.0.join("ledger.json")).unwrap()));
        // Match production freshness: an exact-ID authorized own-profile COUNT,
        // rather than treating TCP or the NIP-42 acknowledgement as liveness.
        eprintln!("OMARCHY_CONFORMANCE_STAGE=freshness_count");
        let probe = Uuid::new_v4().to_string();
        client
            .send_raw(&json!(["COUNT",probe,{"kinds":[0],"authors":[user.public_key().to_hex()]}]))
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                if let RelayMessage::Count {
                    subscription_id, ..
                } = client.next_event(Duration::from_secs(5)).await.unwrap()
                {
                    if subscription_id == probe {
                        break;
                    }
                }
            }
        })
        .await
        .expect("exact-ID authorized COUNT freshness");
        let text = format!("synthetic conformance {}", Uuid::new_v4());
        let intent = protocol::SendIntent {
            request_id: Uuid::new_v4().to_string(),
            room: room.to_string(),
            root_id: None,
            text: text.clone(),
            mentions: vec![mentioned.clone()],
            generation: status.generation,
        };
        eprintln!("OMARCHY_CONFORMANCE_STAGE=prepare_send");
        let (receipt, event) = sender.prepare(intent, &relay, &user, &status, true, true);
        assert_eq!(receipt.state, "sending");
        let event = event.expect("durably reserved SDK message");
        event.verify().unwrap();
        let event_id = event.id.to_hex();
        assert!(event
            .tags
            .iter()
            .any(|t| t.as_slice() == ["p", mentioned.as_str()]));
        eprintln!("OMARCHY_CONFORMANCE_STAGE=send_ack");
        client.send_raw(&json!(["EVENT", event])).await.unwrap();
        let ack = timeout(Duration::from_secs(15), async {
            loop {
                if let RelayMessage::Ok(ok) =
                    client.next_event(Duration::from_secs(5)).await.unwrap()
                {
                    if ok.event_id == event_id {
                        break ok;
                    }
                }
            }
        })
        .await
        .expect("exact-ID real relay acknowledgement");
        assert!(ack.accepted, "real message rejection: {}", ack.message);
        assert_eq!(
            sender
                .acknowledge(&ack.event_id, ack.accepted)
                .unwrap()
                .state,
            "acknowledged"
        );
        eprintln!("OMARCHY_CONFORMANCE_STAGE=history");
        let mut history_error = "message_not_observed";
        let current_history = timeout(Duration::from_secs(12), async {
            loop {
                match history::fetch(&relay, &user, discovered.signer, room).await {
                    Ok(h)
                        if h.rows.iter().any(|r| {
                            r.id == event_id
                                && r.text == text
                                && r.author_pubkey == user.public_key().to_hex()
                        }) =>
                    {
                        break h
                    }
                    Ok(_) => history_error = "message_not_observed",
                    Err(category) => history_error = category,
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("channel-window history deadline: {history_error}"));
        // Observe exact mention tags from persisted server data, not merely local construction.
        eprintln!("OMARCHY_CONFORMANCE_STAGE=persisted_mention");
        let sid = Uuid::new_v4().to_string();
        client
            .send_raw(&json!(["REQ",sid,{"ids":[event_id]}]))
            .await
            .unwrap();
        timeout(Duration::from_secs(10), async {
            loop {
                match client.next_event(Duration::from_secs(5)).await.unwrap() {
                    RelayMessage::Event {
                        subscription_id,
                        event,
                    } if subscription_id == sid => {
                        event.verify().unwrap();
                        assert_eq!(event.id.to_hex(), event_id);
                        assert_eq!(event.content, text);
                        assert!(event
                            .tags
                            .iter()
                            .any(|t| t.as_slice() == ["p", mentioned.as_str()]));
                        break;
                    }
                    RelayMessage::Eose { subscription_id } if subscription_id == sid => {
                        panic!("accepted event missing from persisted subscription")
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("persisted mention query deadline");
        client.send_raw(&json!(["CLOSE", sid])).await.unwrap();
        // A reply can only use the root selected from a verified room history
        // and its matching relay-signed thread snapshot.
        status.history = protocol::History {
            state: "snapshot".into(),
            room_id: Some(current_history.room),
            rows: current_history
                .rows
                .into_iter()
                .map(|row| protocol::HistoryRow {
                    id: row.id,
                    author: row.author_pubkey,
                    time: row.timestamp,
                    text: row.text,
                    edited: row.edited,
                    truncated: row.truncated,
                    unavailable: row.unavailable,
                })
                .collect(),
            has_more: Some(current_history.has_more),
            category: Some(current_history.category.into()),
        };
        let initial_thread = thread::fetch(&relay, &user, discovered.signer, room, &event_id)
            .await
            .expect("verified initial thread snapshot");
        assert_eq!(initial_thread.root, event_id);
        assert_eq!(initial_thread.room, room.to_string());
        status.thread = protocol::Thread {
            state: "snapshot".into(),
            room_id: Some(initial_thread.room),
            root_id: Some(initial_thread.root),
            rows: vec![],
            has_more: Some(initial_thread.has_more),
            category: Some(initial_thread.category.into()),
        };
        let reply_text = format!("synthetic thread reply {}", Uuid::new_v4());
        let reply_intent = protocol::SendIntent {
            request_id: Uuid::new_v4().to_string(),
            room: room.to_string(),
            root_id: Some(event_id.clone()),
            text: reply_text.clone(),
            mentions: vec![],
            generation: status.generation,
        };
        let (reply_receipt, reply_event) =
            sender.prepare(reply_intent.clone(), &relay, &user, &status, true, true);
        assert_eq!(reply_receipt.state, "sending");
        let reply_event = reply_event.expect("durably reserved SDK thread reply");
        reply_event.verify().unwrap();
        let reply_id = reply_event.id.to_hex();
        assert!(reply_event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["e", event_id.as_str(), "", "reply"]));
        client
            .send_raw(&json!(["EVENT", reply_event]))
            .await
            .unwrap();
        let reply_ack = timeout(Duration::from_secs(15), async {
            loop {
                if let RelayMessage::Ok(ok) =
                    client.next_event(Duration::from_secs(5)).await.unwrap()
                {
                    if ok.event_id == reply_id {
                        break ok;
                    }
                }
            }
        })
        .await
        .expect("exact-ID thread reply acknowledgement");
        assert!(
            reply_ack.accepted,
            "real thread reply rejected: {}",
            reply_ack.message
        );
        assert_eq!(
            sender.acknowledge(&reply_ack.event_id, true).unwrap().state,
            "acknowledged"
        );
        let (replayed, duplicate) =
            sender.prepare(reply_intent, &relay, &user, &status, true, true);
        assert!(duplicate.is_none());
        assert_eq!(replayed.event_id.as_deref(), Some(reply_id.as_str()));
        let mut thread_error = "reply_not_observed";
        timeout(Duration::from_secs(12), async {
            loop {
                match thread::fetch(&relay, &user, discovered.signer, room, &event_id).await {
                    Ok(snapshot)
                        if snapshot.rows.iter().any(|row| {
                            row.id == reply_id
                                && row.text == reply_text
                                && row.author_pubkey == user.public_key().to_hex()
                        }) =>
                    {
                        break
                    }
                    Ok(_) => thread_error = "reply_not_observed",
                    Err(category) => thread_error = category,
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("thread reply persistence deadline: {thread_error}"));
        eprintln!("OMARCHY_CONFORMANCE_STAGE=http_member_write");
        let active = buzz_sdk::builders::build_message(
            room,
            "active synthetic HTTP write",
            None,
            &[],
            false,
            &[],
            &[],
        )
        .unwrap()
        .sign_with_keys(&user)
        .unwrap();
        let (status_code, receipt) = post_event(&relay, &user, &active).await;
        assert_eq!(status_code, StatusCode::OK);
        assert_eq!(receipt["event_id"], active.id.to_hex());
        assert_eq!(receipt["accepted"], true);
        eprintln!("OMARCHY_CONFORMANCE_STAGE=remove_member");
        publish(
            &mut admin,
            build_remove_member(room, &user.public_key().to_hex())
                .unwrap()
                .sign_with_keys(&owner)
                .unwrap(),
        )
        .await;
        eprintln!("OMARCHY_CONFORMANCE_STAGE=revoked_roster");
        timeout(Duration::from_secs(12), async {
            loop {
                let events = crate::query::query(
                    &relay,
                    &owner,
                    &crate::query::QueryRequest::RoomMembers { room },
                )
                .await
                .expect("owner roster transport after removal");
                let current = recipients::roster(
                    room,
                    owner.public_key(),
                    discovered.signer,
                    &events,
                    nostr::Timestamp::now().as_secs(),
                )
                .expect("fresh relay-signed owner-visible roster after removal");
                if !current
                    .entries
                    .iter()
                    .any(|r| r.key == user.public_key().to_hex())
                {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("owner-visible signed roster must exclude removed member");
        // Buzz skips inaccessible private-channel results rather than promising
        // HTTP 403. An empty successful read is unavailable evidence, never a
        // signed membership-denial claim. Transport/malformed failures fail this
        // test instead of being accepted as evidence of removal.
        timeout(Duration::from_secs(12), async {
            loop {
                match crate::query::query(
                    &relay,
                    &user,
                    &crate::query::QueryRequest::RoomMembers { room },
                )
                .await
                {
                    Ok(events) if events.is_empty() => break,
                    Ok(events) => match recipients::roster(
                        room,
                        user.public_key(),
                        discovered.signer,
                        &events,
                        nostr::Timestamp::now().as_secs(),
                    ) {
                        Err("recipients_access_denied") => break,
                        Ok(_) => sleep(Duration::from_millis(100)).await,
                        Err(_) => panic!("malformed member-visible roster after removal"),
                    },
                    Err("query_access_denied") => break,
                    Err(category) => panic!("revoked member roster transport: {category}"),
                }
            }
        })
        .await
        .expect("removed member must receive no usable roster");
        // This is a Sender component assertion using explicitly injected status
        // from the independently owner-verified revocation above. It does not
        // claim the daemon infers authoritative denial from an empty query.
        status.recipients = protocol::RecipientsView::unavailable(
            Some(room.to_string()),
            Some("recipients_access_denied"),
        );
        eprintln!("OMARCHY_CONFORMANCE_STAGE=local_revoked_write");
        let (denied, outgoing) = sender.prepare(
            protocol::SendIntent {
                request_id: Uuid::new_v4().to_string(),
                room: room.to_string(),
                root_id: None,
                text: "locally blocked revoked write".into(),
                mentions: vec![],
                generation: status.generation,
            },
            &relay,
            &user,
            &status,
            true,
            true,
        );
        assert_eq!(denied.category.as_deref(), Some("send_access_denied"));
        assert!(outgoing.is_none());
        // Even a previously authenticated socket must lose write authority.
        eprintln!("OMARCHY_CONFORMANCE_STAGE=relay_revoked_write");
        let rejected = buzz_sdk::builders::build_message(
            room,
            "revoked synthetic write",
            None,
            &[],
            false,
            &[],
            &[],
        )
        .unwrap()
        .sign_with_keys(&user)
        .unwrap();
        let id = rejected.id.to_hex();
        let ok = timeout(Duration::from_secs(10), client.send_event(rejected))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ok.event_id, id);
        assert!(!ok.accepted, "revoked member write accepted");
        eprintln!("OMARCHY_CONFORMANCE_STAGE=http_revoked_write");
        let revoked = buzz_sdk::builders::build_message(
            room,
            "revoked synthetic HTTP write",
            None,
            &[],
            false,
            &[],
            &[],
        )
        .unwrap()
        .sign_with_keys(&user)
        .unwrap();
        let (status_code, rejection) = post_event(&relay, &user, &revoked).await;
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert_eq!(rejection["error"], "restricted: not a channel member");
        eprintln!("OMARCHY_CONFORMANCE_STAGE=complete");
        client.disconnect().await.unwrap();
        admin.disconnect().await.unwrap();
    })
    .await
    .expect("bounded real-relay component conformance");
}
