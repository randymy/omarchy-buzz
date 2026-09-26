use super::*;
fn fixture() -> (Sender, SendIntent, Keys, Status, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("buzz-send-test-{}", uuid::Uuid::new_v4()));
    let ledger = Ledger::open(path.join("ledger.json")).unwrap();
    let keys = Keys::generate();
    let intent = SendIntent {
        request_id: uuid::Uuid::new_v4().to_string(),
        room: uuid::Uuid::new_v4().to_string(),
        text: "synthetic message".into(),
        mentions: vec![],
        generation: 1,
    };
    let mut status = Status::new(&crate::config::Config::default());
    status.connection = "authenticated".into();
    status.catalog.rooms.push(crate::protocol::Room {
        id: intent.room.clone(),
        name: "synthetic".into(),
        description: String::new(),
    });
    (Sender::new(Some(ledger)), intent, keys, status, path)
}
#[test]
fn signed_once_exact_ack_and_conflicting_request() {
    let (mut sender, intent, keys, status, path) = fixture();
    let (delivery, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    let event = event.unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 9);
    assert_eq!(event.content, intent.text);
    assert_eq!(delivery.state, "sending");
    assert!(sender.acknowledge(&"0".repeat(64), true).is_none());
    assert_eq!(
        sender.acknowledge(&event.id.to_hex(), true).unwrap().state,
        "acknowledged"
    );
    let (replayed, new) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    assert!(new.is_none());
    assert_eq!(replayed.event_id, Some(event.id.to_hex()));
    assert_eq!(replayed.state, "acknowledged");
    let mut changed = intent;
    changed.text = "different".into();
    assert_eq!(
        sender
            .prepare(changed, "ws://127.0.0.1/", &keys, &status, true, true)
            .0
            .category
            .as_deref(),
        Some("send_request_reused")
    );
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn unknown_survives_restart_without_resigning() {
    let (mut sender, intent, keys, status, path) = fixture();
    let (_, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    let id = event.unwrap().id.to_hex();
    assert_eq!(sender.unknown().unwrap().state, "unknown");
    assert!(sender.last.is_none());
    drop(sender);
    let mut sender = Sender::new(Some(Ledger::open(path.join("ledger.json")).unwrap()));
    let (delivery, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert_eq!(delivery.event_id, Some(id));
    assert_eq!(delivery.state, "failed");
    assert_eq!(delivery.category.as_deref(), Some("send_request_reused"));
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn failed_preflight_cannot_produce_event() {
    let (mut sender, intent, keys, status, path) = fixture();
    assert!(sender
        .prepare(
            intent.clone(),
            "ws://127.0.0.1/",
            &keys,
            &status,
            false,
            true
        )
        .1
        .is_none());
    let mut missing = Sender::new(None);
    assert_eq!(
        missing
            .prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true)
            .0
            .category
            .as_deref(),
        Some("send_ledger_unavailable")
    );
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn persistence_failure_prevents_publication() {
    let (mut sender, intent, keys, status, path) = fixture();
    let file = path.join("ledger.json");
    if file.exists() {
        std::fs::remove_file(&file).unwrap();
    }
    std::fs::create_dir(&file).unwrap();
    let (delivery, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert!(!sender.is_pending());
    assert_eq!(
        delivery.category.as_deref(),
        Some("send_ledger_unavailable")
    );
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn text_mentions_and_generation_are_bounded() {
    let (mut sender, intent, keys, status, path) = fixture();
    for text in [" ".into(), "x".repeat(4097), "nul\0text".into()] {
        let mut changed = intent.clone();
        changed.text = text;
        let (delivery, event) =
            sender.prepare(changed, "ws://127.0.0.1/", &keys, &status, true, true);
        assert!(event.is_none());
        assert_eq!(delivery.category.as_deref(), Some("send_invalid"));
    }
    let mut changed = intent.clone();
    changed.mentions = vec![keys.public_key().to_hex(); 2];
    assert!(sender
        .prepare(changed, "ws://127.0.0.1/", &keys, &status, true, true)
        .1
        .is_none());
    let mut changed = intent;
    changed.generation += 1;
    assert_eq!(
        sender
            .prepare(changed, "ws://127.0.0.1/", &keys, &status, true, true)
            .0
            .category
            .as_deref(),
        Some("send_scope_changed")
    );
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn exact_ack_evidence_survives_failed_outcome_journal_in_memory() {
    let (mut sender, intent, keys, status, path) = fixture();
    let (_, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    let id = event.unwrap().id.to_hex();
    let file = path.join("ledger.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    let delivery = sender.acknowledge(&id, true).unwrap();
    assert_eq!(delivery.state, "acknowledged");
    assert_eq!(
        delivery.category.as_deref(),
        Some("send_ledger_unavailable")
    );
    let (delivery, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert_eq!(delivery.state, "acknowledged");
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn selected_mentions_require_current_room_roster_and_use_exact_key() {
    let (mut sender, mut intent, keys, mut status, path) = fixture();
    let recipient = Keys::generate().public_key().to_hex();
    intent.mentions = vec![recipient.clone()];
    assert_eq!(
        sender
            .prepare(
                intent.clone(),
                "ws://127.0.0.1/",
                &keys,
                &status,
                true,
                true
            )
            .0
            .category
            .as_deref(),
        Some("send_access_denied")
    );
    status.recipients = crate::protocol::RecipientsView {
        state: "snapshot".into(),
        room_id: Some(uuid::Uuid::new_v4().to_string()),
        partial: false,
        category: None,
        entries: vec![crate::protocol::Recipient {
            key: recipient.clone(),
            name: "Untrusted label".into(),
        }],
    };
    assert!(sender
        .prepare(
            intent.clone(),
            "ws://127.0.0.1/",
            &keys,
            &status,
            true,
            true
        )
        .1
        .is_none());
    status.recipients.room_id = Some(intent.room.clone());
    status.recipients.entries.clear();
    assert!(sender
        .prepare(
            intent.clone(),
            "ws://127.0.0.1/",
            &keys,
            &status,
            true,
            true
        )
        .1
        .is_none());
    status.recipients.entries.push(crate::protocol::Recipient {
        key: recipient.clone(),
        name: "Duplicate display name".into(),
    });
    let (_, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    let event = event.unwrap();
    event.verify().unwrap();
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|t| t.as_slice()[0] == "p")
        .collect();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_slice(), ["p", recipient.as_str()]);
    sender.unknown();
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn known_roster_revocation_denies_plain_sends_even_with_stale_catalog() {
    let (mut sender, intent, keys, mut status, path) = fixture();
    status.recipients = crate::protocol::RecipientsView::unavailable(
        Some(intent.room.clone()),
        Some("recipients_access_denied"),
    );
    let (delivery, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert!(!sender.is_pending());
    assert_eq!(delivery.category.as_deref(), Some("send_access_denied"));
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn malformed_recipient_result_does_not_invent_membership_revocation() {
    let (mut sender,intent,keys,mut status,path)=fixture();
    status.recipients=crate::protocol::RecipientsView::unavailable(Some(intent.room.clone()),Some("recipients_invalid"));
    let (_,event)=sender.prepare(intent,"ws://127.0.0.1/",&keys,&status,true,true);
    assert!(event.is_some());sender.unknown();drop(sender);std::fs::remove_dir_all(path).unwrap();
}
