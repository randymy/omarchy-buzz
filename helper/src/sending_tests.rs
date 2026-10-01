use super::*;
fn fixture() -> (Sender, SendIntent, Keys, Status, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("buzz-send-test-{}", uuid::Uuid::new_v4()));
    let ledger = Ledger::open(path.join("ledger.json")).unwrap();
    let keys = Keys::generate();
    let intent = SendIntent {
        request_id: uuid::Uuid::new_v4().to_string(),
        room: uuid::Uuid::new_v4().to_string(),
        root_id: None,
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
        kind: "stream".into(),
        participants: Vec::new(),
        hidden: false,
    });
    (Sender::new(Some(ledger)), intent, keys, status, path)
}
fn selected_root(status: &mut Status, room: &str, root: &str) {
    status.history = crate::protocol::History {
        state: "snapshot".into(),
        room_id: Some(room.into()),
        rows: vec![crate::protocol::HistoryRow {
            reactions: None,
            thread: None,
            id: root.into(),
            author: "synthetic".into(),
            time: 1,
            text: "root".into(),
            edited: false,
            truncated: false,
            unavailable: false,
            attachments: vec![],
            attachments_unavailable: false,
        }],
        has_more: Some(false),
        category: None,
        next_cursor: None,
        older_state: "idle".into(),
        live: false,
    };
    status.thread = crate::protocol::Thread {
        state: "snapshot".into(),
        room_id: Some(room.into()),
        root_id: Some(root.into()),
        rows: vec![],
        has_more: Some(false),
        category: None,
    };
}
#[test]
fn reply_requires_current_verified_root_and_signs_sdk_direct_reply_tags() {
    let (mut sender, mut intent, keys, mut status, path) = fixture();
    let root = "a".repeat(64);
    intent.root_id = Some(root.clone());
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
    selected_root(&mut status, &intent.room, &root);
    status.thread.state = "unavailable".into();
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
    status.thread.state = "snapshot".into();
    status.history.room_id = Some(uuid::Uuid::new_v4().to_string());
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
    status.history.room_id = Some(intent.room.clone());
    status.history.rows[0].unavailable = true;
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
    status.history.rows[0].unavailable = false;
    let (delivery, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    assert_eq!(delivery.state, "sending");
    let event = event.unwrap();
    event.verify().unwrap();
    let id = event.id.to_hex();
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice()[0] == "e")
        .collect();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_slice(), ["e", root.as_str(), "", "reply"]);
    let mut changed = intent.clone();
    changed.root_id = Some("b".repeat(64));
    assert_eq!(sender.pending_error(&changed), Some("send_request_reused"));
    sender.acknowledge(&id, true).unwrap();
    let (replay, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    assert!(event.is_none());
    assert_eq!(replay.event_id, Some(id));
    assert_eq!(replay.state, "acknowledged");
    drop(sender);
    let mut sender = Sender::new(Some(Ledger::open(path.join("ledger.json")).unwrap()));
    let mut changed = intent;
    let other = "b".repeat(64);
    selected_root(&mut status, &changed.room, &other);
    changed.root_id = Some(other);
    let (denied, event) = sender.prepare(changed, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert_eq!(denied.category.as_deref(), Some("send_request_reused"));
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
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
        agents: Vec::new(),
        state: "snapshot".into(),
        room_id: Some(uuid::Uuid::new_v4().to_string()),
        partial: false,
        category: None,
        entries: vec![crate::protocol::Recipient {
            key: recipient.clone(),
            name: "Untrusted label".into(),
            status: None,
            presence: None,
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
        status: None,
        presence: None,
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
    let (mut sender, intent, keys, mut status, path) = fixture();
    status.recipients = crate::protocol::RecipientsView::unavailable(
        Some(intent.room.clone()),
        Some("recipients_invalid"),
    );
    let (_, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_some());
    sender.unknown();
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn in_flight_replay_and_other_request_preserve_original_receipt() {
    let (mut sender, intent, keys, status, path) = fixture();
    let (original, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    let id = event.unwrap().id.to_hex();
    let (replay, event) = sender.prepare(
        intent.clone(),
        "ws://127.0.0.1/",
        &keys,
        &status,
        true,
        true,
    );
    assert!(event.is_none());
    assert_eq!(replay.state, "sending");
    assert_eq!(replay.event_id, original.event_id);
    assert_eq!(replay.request_id, original.request_id);
    let mut other = intent;
    other.request_id = uuid::Uuid::new_v4().to_string();
    let (busy, event) = sender.prepare(other, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(event.is_none());
    assert_eq!(busy.state, "sending");
    assert_eq!(busy.request_id, original.request_id);
    assert_eq!(sender.acknowledge(&id, true).unwrap().state, "acknowledged");
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn distinct_requests_same_second_sign_distinct_events() {
    let (_sender, mut intent, keys, _status, path) = fixture();
    let stamp = nostr::Timestamp::from(1700000000);
    let first = build_event(&intent, &keys, &[], &[], Some(stamp)).unwrap();
    first.verify().unwrap();
    assert!(first
        .tags
        .iter()
        .any(|t| t.as_slice() == ["omarchy-buzz-request", intent.request_id.as_str()]));
    assert_eq!(
        build_event(&intent, &keys, &[], &[], Some(stamp))
            .unwrap()
            .id,
        first.id
    );
    intent.request_id = uuid::Uuid::new_v4().to_string();
    let second = build_event(&intent, &keys, &[], &[], Some(stamp)).unwrap();
    second.verify().unwrap();
    assert_eq!(first.created_at, second.created_at);
    assert_ne!(first.id, second.id);
    drop(_sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn pending_uuid_binding_rejects_changed_text_mentions_or_scope() {
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
    assert!(sender.pending_error(&intent).is_none());
    let mut changed = intent.clone();
    changed.text.push('!');
    assert_eq!(sender.pending_error(&changed), Some("send_request_reused"));
    let mut changed = intent.clone();
    changed.mentions = vec![Keys::generate().public_key().to_hex()];
    assert_eq!(sender.pending_error(&changed), Some("send_request_reused"));
    let mut changed = intent.clone();
    changed.room = uuid::Uuid::new_v4().to_string();
    assert_eq!(sender.pending_error(&changed), Some("send_request_reused"));
    let mut changed = intent;
    changed.request_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(sender.pending_error(&changed), Some("send_busy"));
    assert_eq!(sender.acknowledge(&id, true).unwrap().state, "acknowledged");
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn direct_messages_tag_every_other_participant_like_desktop() {
    let (mut sender, mut intent, keys, mut status, path) = fixture();
    let own = keys.public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let friend = Keys::generate().public_key().to_hex();
    let mut participants = vec![own.clone(), agent.clone(), friend.clone()];
    participants.sort();
    status.catalog.rooms[0].kind = "dm".into();
    status.catalog.rooms[0].participants = participants;
    // An explicit mention of a participant is not repeated.
    status.recipients = crate::protocol::RecipientsView {
        state: "snapshot".into(),
        room_id: Some(intent.room.clone()),
        entries: vec![crate::protocol::Recipient {
            key: friend.clone(),
            name: "friend".into(),
            status: None,
            presence: None,
        }],
        agents: Vec::new(),
        partial: false,
        category: None,
    };
    intent.mentions = vec![friend.clone()];
    let (_, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    let event = event.unwrap();
    event.verify().unwrap();
    let tagged: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice()[0] == "p")
        .map(|t| t.as_slice()[1].clone())
        .collect();
    let mut others = vec![agent, friend.clone()];
    others.sort();
    let expected: Vec<String> = std::iter::once(friend.clone())
        .chain(others.into_iter().filter(|k| *k != friend))
        .collect();
    assert_eq!(tagged, expected);
    assert!(!tagged.contains(&own));
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn stream_messages_tag_only_explicit_mentions() {
    let (mut sender, intent, keys, status, path) = fixture();
    let (_, event) = sender.prepare(intent, "ws://127.0.0.1/", &keys, &status, true, true);
    assert!(!event.unwrap().tags.iter().any(|t| t.as_slice()[0] == "p"));
    drop(sender);
    std::fs::remove_dir_all(path).unwrap();
}
fn pending(scope: &str, n: u8, mime: &str, name: &str, dim: Option<&str>) -> PendingAttachment {
    let hash = format!("{n:02x}").repeat(32);
    let ext = if mime == "image/png" { "png" } else { "pdf" };
    PendingAttachment {
        scope: scope.into(),
        name: name.into(),
        mime: mime.into(),
        size: 1000 + u64::from(n),
        url: format!("https://relay.example/media/{hash}.{ext}"),
        hash,
        dim: dim.map(str::to_owned),
    }
}
#[test]
fn pending_attachments_travel_as_imeta_tags_and_markdown_lines() {
    let (mut sender, intent, keys, mut status, path) = fixture();
    let image = pending(&intent.room, 1, "image/png", "shot.png", Some("640x480"));
    let file = pending(&intent.room, 2, "application/pdf", "Q3 [final].pdf", None);
    // Another draft's attachment stays out of this message.
    let other = pending(
        &format!("{}:{}", intent.room, "a".repeat(64)),
        3,
        "image/png",
        "x.png",
        None,
    );
    status.pending_attachments = vec![image.clone(), other.clone(), file.clone()];
    let (delivery, event) = sender.prepare(
        intent.clone(),
        "wss://relay.example/",
        &keys,
        &status,
        true,
        true,
    );
    assert_eq!(delivery.state, "sending");
    let event = event.unwrap();
    assert_eq!(
        event.content,
        format!(
            "synthetic message\n![image]({})\n[Q3 \\[final\\].pdf]({})",
            image.url, file.url
        )
    );
    let imeta: Vec<Vec<String>> = event
        .tags
        .iter()
        .filter(|t| t.as_slice()[0] == "imeta")
        .map(|t| t.as_slice().to_vec())
        .collect();
    assert_eq!(
        imeta,
        vec![
            vec![
                "imeta".to_string(),
                format!("url {}", image.url),
                "m image/png".into(),
                format!("x {}", image.hash),
                "size 1001".into(),
                "dim 640x480".into(),
                "filename shot.png".into(),
            ],
            vec![
                "imeta".to_string(),
                format!("url {}", file.url),
                "m application/pdf".into(),
                format!("x {}", file.hash),
                "size 1002".into(),
                "filename Q3 [final].pdf".into(),
            ],
        ]
    );
    // The panel's own parser reads the message back as sent, without the lines.
    let (list, broken, text) = crate::attachments::project(&event, "https://relay.example");
    assert!(!broken);
    assert_eq!(text, "synthetic message");
    assert_eq!(
        list.iter().map(|a| a.hash.as_str()).collect::<Vec<_>>(),
        [image.hash.as_str(), file.hash.as_str()]
    );
    // Accepted: exactly this draft's attachments are released.
    let id = event.id.to_hex();
    assert_eq!(sender.acknowledge(&id, true).unwrap().state, "acknowledged");
    assert_eq!(
        sender.take_sent_media(),
        Some((
            intent.room.clone(),
            vec![image.hash.clone(), file.hash.clone()]
        ))
    );
    assert_eq!(sender.take_sent_media(), None);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn a_blank_text_is_sent_only_with_attachments() {
    let (mut sender, mut intent, keys, mut status, path) = fixture();
    intent.text = " ".into();
    let (delivery, event) = sender.prepare(
        intent.clone(),
        "wss://relay.example/",
        &keys,
        &status,
        true,
        true,
    );
    assert_eq!(delivery.category.as_deref(), Some("send_invalid"));
    assert!(event.is_none());
    intent.text = String::new();
    status.pending_attachments = vec![pending(&intent.room, 1, "image/png", "a.png", None)];
    let (delivery, event) = sender.prepare(
        intent.clone(),
        "wss://relay.example/",
        &keys,
        &status,
        true,
        true,
    );
    assert_eq!(delivery.state, "sending");
    let event = event.unwrap();
    assert_eq!(
        event.content,
        format!("\n![image]({})", status.pending_attachments[0].url)
    );
    // A rejected event keeps its attachments in the draft.
    sender.acknowledge(&event.id.to_hex(), false);
    assert_eq!(sender.take_sent_media(), None);
    std::fs::remove_dir_all(path).unwrap();
}
