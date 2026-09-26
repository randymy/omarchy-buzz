use super::*;
use nostr::{EventBuilder, Kind, Tag};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn event(author: &Keys, kind: u16, tags: Vec<Vec<String>>, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), "")
        .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn tags(values: &[&[&str]]) -> Vec<Vec<String>> {
    values
        .iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect()
}
const ID: &str = "11111111-1111-4111-8111-111111111111";
fn membership(author: &Keys, member: &Keys) -> Event {
    event(
        author,
        39002,
        tags(&[
            &["d", ID],
            &["p", &member.public_key().to_hex(), "", "member"],
        ]),
        100,
    )
}
fn metadata(author: &Keys, kind: &str) -> Event {
    event(
        author,
        39000,
        tags(&[
            &["d", ID],
            &["name", "Lobby"],
            &["t", kind],
            &["about", "Welcome"],
        ]),
        100,
    )
}

#[test]
fn nip11_self_is_authority_and_rotation_is_refused() {
    let relay = key(1).public_key();
    let contact = key(2).public_key();
    let document =
        serde_json::to_vec(&serde_json::json!({"self":relay.to_hex(),"pubkey":contact.to_hex()}))
            .unwrap();
    assert_eq!(info_signer(&document, Some(relay)).unwrap(), relay);
    assert_eq!(
        info_signer(&document, Some(contact)).unwrap_err(),
        "relay_identity_changed"
    );
    assert_eq!(
        info_signer(br#"{"pubkey":"ignored"}"#, None).unwrap_err(),
        "discovery_signer_unavailable"
    );
    assert_eq!(
        info_signer(&vec![b' '; INFO_BYTES + 1], None).unwrap_err(),
        "discovery_oversized"
    );
}
#[test]
fn valid_snapshot_partial_missing_metadata_and_unsupported_are_honest() {
    let relay = key(1);
    let member = key(2);
    let m = membership(&relay, &member);
    let c = reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[metadata(&relay, "stream")],
        1000,
    )
    .unwrap();
    assert_eq!(c.state, "partial");
    assert_eq!(c.category, "room_catalog_partial");
    assert_eq!(c.rooms.len(), 1);
    assert_eq!(c.rooms[0].name, "Lobby");
    assert_eq!(c.rooms[0].description, "Welcome");
    assert!(reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[],
        1000
    )
    .unwrap()
    .rooms
    .is_empty());
    assert!(reconcile(
        member.public_key(),
        relay.public_key(),
        &[m.clone()],
        &[metadata(&relay, "forum")],
        1000
    )
    .unwrap()
    .rooms
    .is_empty());
    let empty = reconcile(member.public_key(), relay.public_key(), &[], &[], 1000).unwrap();
    assert_eq!(empty.state, "partial");
}
#[test]
fn valid_signature_does_not_grant_membership_authority() {
    let relay = key(1);
    let member = key(2);
    let attacker = key(3);
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&attacker, &member)],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_untrusted_author"
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&relay, &attacker)],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_membership"
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[membership(&relay, &member)],
            &[metadata(&attacker, "stream")],
            1000
        )
        .unwrap_err(),
        "catalog_untrusted_author"
    );
}
#[test]
fn conflicts_duplicate_identifier_future_and_bad_signature_fail_closed() {
    let relay = key(1);
    let member = key(2);
    let good = membership(&relay, &member);
    let duplicate = event(
        &relay,
        39002,
        tags(&[
            &["d", ID],
            &["d", ID],
            &["p", &member.public_key().to_hex()],
        ]),
        100,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[duplicate],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_shape"
    );
    let later = event(
        &relay,
        39002,
        tags(&[&["d", ID], &["p", &member.public_key().to_hex()]]),
        101,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[good.clone(), later],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_conflicting_snapshot"
    );
    let future = event(
        &relay,
        39002,
        tags(&[&["d", ID], &["p", &member.public_key().to_hex()]]),
        1061,
    );
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[future],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_shape"
    );
    let mut tampered = good;
    tampered.content = "tampered".into();
    assert_eq!(
        reconcile(
            member.public_key(),
            relay.public_key(),
            &[tampered],
            &[],
            1000
        )
        .unwrap_err(),
        "catalog_invalid_signature"
    );
}
#[test]
fn presentation_text_is_byte_bounded_and_controls_are_removed() {
    let relay = key(1);
    let member = key(2);
    let long = "é\n".repeat(600);
    let data = event(
        &relay,
        39000,
        tags(&[
            &["d", ID],
            &["name", &long],
            &["t", "stream"],
            &["about", &long],
        ]),
        100,
    );
    let c = reconcile(
        member.public_key(),
        relay.public_key(),
        &[membership(&relay, &member)],
        &[data],
        1000,
    )
    .unwrap();
    assert!(c.rooms[0].name.len() <= 128);
    assert!(c.rooms[0].description.len() <= 512);
    assert!(!c.rooms[0].description.chars().any(char::is_control));
}
#[tokio::test]
async fn loopback_info_uses_info_without_credentials_and_rejects_redirect() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let relay = key(1).public_key();
    for redirect in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let payload = serde_json::json!({"self":relay.to_hex()}).to_string();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            loop {
                let mut byte = [0; 1];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                head.push(byte[0]);
                assert!(head.len() < 8192);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            assert!(head.starts_with("get /info "));
            assert!(head.contains("accept: application/nostr+json"));
            assert!(!head.contains("authorization:"));
            let response = if redirect {
                "HTTP/1.1 302 Found\r\nLocation: https://example.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let result = relay_signer(&origin, Some(relay)).await;
        if redirect {
            assert_eq!(result.unwrap_err(), "discovery_redirect_rejected");
        } else {
            assert_eq!(result.unwrap(), relay);
        }
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn complete_loopback_discovery_and_trust_failures() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    let relay = key(1);
    let member = key(2);
    for mode in ["valid", "rotation", "contact_only"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let info = if mode == "contact_only" {
            serde_json::json!({"pubkey":relay.public_key().to_hex()})
        } else {
            serde_json::json!({"self":relay.public_key().to_hex()})
        };
        let responses = vec![
            info.to_string(),
            serde_json::to_string(&vec![membership(&relay, &member)]).unwrap(),
            serde_json::to_string(&vec![metadata(&relay, "stream")]).unwrap(),
        ];
        let requests = if mode == "valid" { 3 } else { 1 };
        let task = tokio::spawn(async move {
            for (index, payload) in responses.into_iter().take(requests).enumerate() {
                let (mut stream, _) = listener.accept().await.unwrap();
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
                if index == 0 {
                    assert!(head.starts_with("get /info "));
                } else {
                    assert!(head.starts_with("post /query "));
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    assert!(length < 8192);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let filter: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        filter[0]["kinds"][0],
                        if index == 1 { 39002 } else { 39000 }
                    );
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            if requests == 1 {
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err(),
                    "trust failure still queried private data"
                );
            }
        });
        let pin = Some(if mode == "rotation" {
            key(3).public_key()
        } else {
            relay.public_key()
        });
        let result = discover(&origin, &member, pin).await;
        match mode {
            "valid" => {
                let catalog = result.unwrap();
                assert_eq!(catalog.signer, relay.public_key());
                assert_eq!(catalog.trust, "loopback_development");
                assert_eq!(catalog.state, "partial");
                assert_eq!(catalog.rooms.len(), 1);
                assert_eq!(catalog.rooms[0].id, ID);
            }
            "rotation" => assert_eq!(result.unwrap_err(), "relay_identity_changed"),
            _ => assert_eq!(result.unwrap_err(), "discovery_signer_unavailable"),
        }
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
    }
}
