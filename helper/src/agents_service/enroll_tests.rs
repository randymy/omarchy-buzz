//! Enrollment against a synthetic loopback relay (the `auth_*_tests.rs`
//! pattern): exact events, OK/rejection/timeout outcomes, no secret material.
use super::*;
use crate::agents_service::test_support::{persona, relay, Answer, ROOM_A, ROOM_B};
use tokio::net::TcpListener;

const ID: &str = "3f2b8c1e-5d4a-4b6e-9c7d-0a1b2c3d4e5f";

fn secret(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn enrolled(owner: &Keys, agent: &Keys) -> Persona {
    let mut p = persona(ID, "/home/example/work");
    p.rooms = vec![ROOM_A.into(), ROOM_B.into()];
    p.identity = Some(agent.public_key().to_hex());
    p.auth_tag = Some(attestation(owner, &agent.public_key()).unwrap());
    p
}
fn tags(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}

#[test]
fn attestation_is_upstream_nip_oa_with_empty_conditions() {
    let (owner, agent) = (secret(1), secret(2));
    let tag = attestation(&owner, &agent.public_key()).unwrap();
    let parsed: Vec<String> = serde_json::from_str(&tag).unwrap();
    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[0], "auth");
    assert_eq!(parsed[1], owner.public_key().to_hex());
    assert_eq!(parsed[2], "");
    assert_eq!(
        buzz_sdk::nip_oa::verify_auth_tag(&tag, &agent.public_key()).unwrap(),
        owner.public_key()
    );
    assert!(buzz_sdk::nip_oa::verify_auth_tag(&tag, &owner.public_key()).is_err());
    assert!(
        attestation(&owner, &owner.public_key()).is_err(),
        "self-attestation"
    );
    // The same SDK verifier accepts the NIP-OA specification vector.
    let vector = r#"["auth","79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798","kind=1&created_at<1713957000","8b7df2575caf0a108374f8471722b233c53f9ff827a8b0f91861966c3b9dd5cb2e189eae9f49d72187674c2f5bd244145e10ff86c9f257ffe65a1ee5f108b369"]"#;
    assert_eq!(
        buzz_sdk::nip_oa::verify_auth_tag(vector, &agent.public_key()).unwrap(),
        owner.public_key()
    );
}

#[tokio::test]
async fn publishes_exact_records_membership_and_profile_after_ok() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let p = enrolled(&owner, &agent);
    let report = publish_all(&relay.url, &owner, &agent, &p).await;
    assert_eq!(report.error, None);
    assert_eq!(
        report.member_rooms,
        vec![ROOM_A.to_string(), ROOM_B.to_string()]
    );
    let seen = relay.seen.lock().unwrap().clone();
    let kinds: Vec<(usize, u16)> = seen
        .iter()
        .map(|s| (s.connection, s.event.kind.as_u16()))
        .collect();
    assert_eq!(
        kinds,
        [(0, 30175), (0, 30177), (0, 9000), (0, 9000), (1, 0)]
    );
    let agent_hex = agent.public_key().to_hex();

    let persona_event = &seen[0].event;
    assert_eq!(seen[0].author, owner.public_key());
    assert_eq!(persona_event.pubkey, owner.public_key());
    assert_eq!(tags(persona_event), [["d", ID]]);
    assert_eq!(
        persona_event.content,
        r#"{"display_name":"Scout","system_prompt":"Answer briefly.\nCite files.","acp_command":"buzz-acp","runtime":"codex","respond_to":"owner-only","description":"Reads the logs."}"#
    );
    let managed = &seen[1].event;
    assert_eq!(managed.pubkey, owner.public_key());
    assert_eq!(tags(managed), [["d", agent_hex.as_str()]]);
    assert_eq!(
        managed.content,
        format!(
            r#"{{"name":"Scout","persona_id":"{ID}","parallelism":1,"respond_to":"owner-only"}}"#
        )
    );
    assert_eq!(
        report.published_at,
        Some(persona_event.created_at.as_secs())
    );
    assert_eq!(managed.created_at, persona_event.created_at);
    for (s, room) in seen[2..4].iter().zip([ROOM_A, ROOM_B]) {
        assert_eq!(s.event.pubkey, owner.public_key());
        assert_eq!(s.event.content, "");
        assert_eq!(
            tags(&s.event),
            [["h", room], ["p", agent_hex.as_str()], ["role", "bot"]]
        );
    }
    // The agent authenticates with its own key and the owner's attestation.
    let profile = &seen[4];
    assert_eq!(profile.author, agent.public_key());
    let auth_tag: Vec<String> = serde_json::from_str(p.auth_tag.as_deref().unwrap()).unwrap();
    assert!(profile.auth_tags.contains(&auth_tag));
    assert_eq!(profile.event.pubkey, agent.public_key());
    assert_eq!(
        profile.event.content,
        r#"{"about":"Reads the logs.","display_name":"Scout"}"#
    );
    assert_eq!(tags(&profile.event), [auth_tag.clone()]);
    // The owner connection carries no attestation.
    assert!(!seen[0].auth_tags.iter().any(|t| t[0] == "auth"));
    // No published event carries the agent's secret key or (except the
    // agent-signed profile) the attestation.
    let secret_hex = agent.secret_key().to_secret_hex();
    let nsec = nostr::ToBech32::to_bech32(agent.secret_key()).unwrap();
    for s in &seen {
        let wire = serde_json::to_string(&s.event).unwrap();
        assert!(!wire.contains(&secret_hex) && !wire.contains(&nsec));
    }
    for s in &seen[..4] {
        assert!(
            !s.event.content.contains(&auth_tag[3]),
            "attestation leaked into {}",
            s.event.kind
        );
        assert!(!tags(&s.event).iter().any(|t| t[0] == "auth"));
    }
}

#[tokio::test]
async fn acknowledged_rooms_are_not_added_again_and_stamps_are_monotonic() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let mut p = enrolled(&owner, &agent);
    p.member_rooms = vec![ROOM_A.into()];
    p.published_at = Timestamp::now().as_secs() + 100;
    let report = publish_all(&relay.url, &owner, &agent, &p).await;
    assert_eq!(report.error, None);
    assert_eq!(report.member_rooms, vec![ROOM_B.to_string()]);
    assert_eq!(report.published_at, Some(p.published_at + 1));
    let seen = relay.seen.lock().unwrap().clone();
    let adds: Vec<_> = seen
        .iter()
        .filter(|s| s.event.kind.as_u16() == 9000)
        .collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(tags(&adds[0].event)[0], ["h", ROOM_B]);
}

#[tokio::test]
async fn rejection_stops_publication_with_a_fixed_category() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(
        |kind| {
            if kind == 30177 {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let report = publish_all(&relay.url, &owner, &agent, &enrolled(&owner, &agent)).await;
    assert_eq!(
        report,
        Report {
            member_rooms: vec![],
            removed_rooms: vec![],
            published_at: None,
            error: Some("enroll_failed")
        }
    );
    let kinds: Vec<u16> = relay
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.event.kind.as_u16())
        .collect();
    assert_eq!(kinds, [30175, 30177]);

    let relay = self::relay(
        |kind| {
            if kind == 0 {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let report = publish_all(&relay.url, &owner, &agent, &enrolled(&owner, &agent)).await;
    // Records and memberships were acknowledged; the profile was not.
    assert_eq!(report.member_rooms.len(), 2);
    assert!(report.published_at.is_some());
    assert_eq!(report.error, Some("enroll_failed"));
}

#[tokio::test]
async fn closed_or_unreachable_relays_and_rejected_auth_are_categorized() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let p = enrolled(&owner, &agent);
    let closed = relay(
        |kind| {
            if kind == 9000 {
                Answer::Close
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let report = publish_all(&closed.url, &owner, &agent, &p).await;
    assert_eq!(report.error, Some("relay_unavailable"));
    assert!(report.member_rooms.is_empty() && report.published_at.is_some());

    let refused = relay(|_| Answer::Accept, true).await;
    let report = publish_all(&refused.url, &owner, &agent, &p).await;
    assert_eq!(report.error, Some("enroll_failed"));
    assert!(refused.seen.lock().unwrap().is_empty());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    drop(listener);
    assert_eq!(
        publish_all(&url, &owner, &agent, &p).await.error,
        Some("relay_unavailable")
    );

    // A persona whose attestation is not for this agent is never published.
    let other = Keys::generate();
    let mut forged = p.clone();
    forged.auth_tag = Some(attestation(&owner, &other.public_key()).unwrap());
    let fresh = relay(|_| Answer::Accept, false).await;
    assert_eq!(
        publish_all(&fresh.url, &owner, &agent, &forged).await.error,
        Some("enroll_failed")
    );
    assert!(fresh.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unanswered_event_is_unknown_after_the_ok_deadline() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(
        |kind| {
            if kind == 9000 {
                Answer::Silent
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let started = Instant::now();
    let report = publish_all(&relay.url, &owner, &agent, &enrolled(&owner, &agent)).await;
    assert!(started.elapsed() >= OK_TIMEOUT - Duration::from_millis(100));
    assert_eq!(report.error, Some("relay_unavailable"));
    assert!(report.member_rooms.is_empty());
    // The unanswered add-member was written once and never retransmitted.
    let adds = relay
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.event.kind.as_u16() == 9000)
        .count();
    assert_eq!(adds, 1);
}

fn removals(seen: &[crate::agents_service::test_support::Seen]) -> Vec<Vec<Vec<String>>> {
    seen.iter()
        .filter(|s| s.event.kind.as_u16() == 9001)
        .map(|s| tags(&s.event))
        .collect()
}

#[tokio::test]
async fn dropped_rooms_are_left_with_exact_remove_member_events() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let mut p = enrolled(&owner, &agent);
    // ROOM_A stays, ROOM_B was dropped after its membership was acknowledged.
    p.rooms = vec![ROOM_A.into()];
    p.member_rooms = vec![ROOM_A.into(), ROOM_B.into()];
    let report = publish_all(&relay.url, &owner, &agent, &p).await;
    assert_eq!(report.error, None);
    assert!(report.member_rooms.is_empty(), "nothing is added again");
    assert_eq!(report.removed_rooms, vec![ROOM_B.to_string()]);
    let seen = relay.seen.lock().unwrap().clone();
    let kinds: Vec<(usize, u16)> = seen
        .iter()
        .map(|s| (s.connection, s.event.kind.as_u16()))
        .collect();
    assert_eq!(kinds, [(0, 30175), (0, 30177), (0, 9001), (1, 0)]);
    let remove = &seen[2];
    assert_eq!(remove.author, owner.public_key());
    assert_eq!(remove.event.pubkey, owner.public_key());
    assert_eq!(remove.event.content, "");
    assert_eq!(
        tags(&remove.event),
        [["h", ROOM_B], ["p", agent.public_key().to_hex().as_str()]]
    );
    assert!(!remove.auth_tags.iter().any(|t| t[0] == "auth"));
    assert_eq!(remove.event.created_at, seen[0].event.created_at);
}

#[tokio::test]
async fn a_rejected_or_unanswered_removal_is_not_recorded() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let mut p = enrolled(&owner, &agent);
    p.rooms = vec![ROOM_A.into()];
    p.member_rooms = vec![ROOM_A.into(), ROOM_B.into()];
    let rejecting = relay(
        |kind| {
            if kind == 9001 {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let report = publish_all(&rejecting.url, &owner, &agent, &p).await;
    assert_eq!(report.error, Some("enroll_failed"));
    assert!(report.removed_rooms.is_empty());
    let kinds: Vec<u16> = rejecting
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.event.kind.as_u16())
        .collect();
    assert_eq!(kinds, [30175, 30177, 9001], "the profile waits for a retry");

    let closing = relay(
        |kind| {
            if kind == 9001 {
                Answer::Close
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let report = publish_all(&closing.url, &owner, &agent, &p).await;
    assert_eq!(report.error, Some("relay_unavailable"));
    assert!(report.removed_rooms.is_empty());
}

#[tokio::test]
async fn leaving_all_rooms_publishes_only_removals() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let (owner, agent) = (Keys::generate(), Keys::generate());
    let agent_hex = agent.public_key().to_hex();
    let rooms = vec![ROOM_A.to_string(), ROOM_B.to_string()];
    let at = Timestamp::from(1_800_000_000);
    let accepting = relay(|_| Answer::Accept, false).await;
    let report = leave_rooms(&accepting.url, &owner, &agent.public_key(), &rooms, at).await;
    assert_eq!(
        report,
        Report {
            removed_rooms: rooms.clone(),
            ..Report::default()
        }
    );
    let seen = accepting.seen.lock().unwrap().clone();
    assert_eq!(
        removals(&seen),
        [
            [["h", ROOM_A], ["p", agent_hex.as_str()]],
            [["h", ROOM_B], ["p", agent_hex.as_str()]]
        ]
    );
    assert_eq!(seen.len(), 2, "no other kind is published");
    for s in &seen {
        assert_eq!((s.connection, s.author), (0, owner.public_key()));
        assert_eq!(s.event.created_at, at);
    }
    // Nothing to leave: no connection at all.
    let idle = relay(|_| Answer::Accept, false).await;
    assert_eq!(
        leave_rooms(&idle.url, &owner, &agent.public_key(), &[], at).await,
        Report::default()
    );
    assert!(idle.seen.lock().unwrap().is_empty());
    // A rejection stops at that room.
    let rejecting = relay(|_| Answer::Reject, false).await;
    let report = leave_rooms(&rejecting.url, &owner, &agent.public_key(), &rooms, at).await;
    assert_eq!(report.error, Some("enroll_failed"));
    assert!(report.removed_rooms.is_empty());
    assert_eq!(rejecting.seen.lock().unwrap().len(), 1);
    // Refused AUTH and an unreachable relay.
    let refused = relay(|_| Answer::Accept, true).await;
    let report = leave_rooms(&refused.url, &owner, &agent.public_key(), &rooms, at).await;
    assert_eq!(report.error, Some("enroll_failed"));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    drop(listener);
    let report = leave_rooms(&url, &owner, &agent.public_key(), &rooms, at).await;
    assert_eq!(report.error, Some("relay_unavailable"));
    // An unanswered removal is unknown after the OK deadline, never resent.
    let silent = relay(|_| Answer::Silent, false).await;
    let started = Instant::now();
    let report = leave_rooms(&silent.url, &owner, &agent.public_key(), &rooms, at).await;
    assert!(started.elapsed() >= OK_TIMEOUT - Duration::from_millis(100));
    assert_eq!(report.error, Some("relay_unavailable"));
    assert!(report.removed_rooms.is_empty());
    assert_eq!(silent.seen.lock().unwrap().len(), 1);
}
