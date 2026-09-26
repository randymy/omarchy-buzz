//! Opt-in component conformance against a disposable pinned Buzz server.
//! The runner bootstraps the public key of the explicitly synthetic owner below.
//! No production config, keyring, external host, or SQL connection is used here.
use crate::{catalog, config, history, ledger::Ledger, protocol, recipients, sending::Sender};
use buzz_sdk::builders::{build_add_member, build_create_channel, build_remove_member};
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use nostr::{Event, Keys};
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
        timeout(Duration::from_secs(12), async {
            loop {
                match history::fetch(&relay, &user, discovered.signer, room).await {
                    Ok(h)
                        if h.rows.iter().any(|r| {
                            r.id == event_id
                                && r.text == text
                                && r.author_pubkey == user.public_key().to_hex()
                        }) =>
                    {
                        break
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
        eprintln!("OMARCHY_CONFORMANCE_STAGE=complete");
        client.disconnect().await.unwrap();
        admin.disconnect().await.unwrap();
    })
    .await
    .expect("bounded real-relay component conformance");
}
