//! `user_status` through the production observer, on the synthetic joined-room
//! fixture: one signed kind 30315 per accepted request, outcomes only from the
//! exact `OK`, the rate limit, and the verified reads that follow.
use super::*;

async fn request(
    f: &Recheck,
    set: Option<(&str, Option<&str>, Option<u32>)>,
) -> Option<&'static str> {
    let (reply, answer) = oneshot::channel();
    let intent = crate::protocol::StatusIntent {
        set: set.map(|(text, emoji, hours)| crate::protocol::StatusSet {
            text: text.into(),
            emoji: emoji.map(str::to_owned),
            hours,
        }),
    };
    f.commands
        .send(Command::SetStatus(intent, reply))
        .await
        .unwrap();
    answer.await.unwrap()
}
fn tag_list(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}

#[tokio::test]
async fn status_is_signed_once_acknowledged_exactly_and_read_back() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(40), async {
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        let own = f.status.borrow().identity.clone().unwrap();
        // Nothing is read until a roster read, which carries this identity's
        // status (none yet) and the other member's.
        wait_status(&mut f.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        assert_eq!(f.status.borrow().user_status.state, "unavailable");
        let a = f.a.clone();
        f.commands
            .send(Command::FetchRecipients(a.clone()))
            .await
            .unwrap();
        let s = snapshot(&mut f.status, |s| s.user_status.state == "ready").await;
        assert!(s.user_status.mine.is_none() && s.user_status.category.is_none());
        let own_entry = s.recipients.entries.iter().find(|e| e.key == own).unwrap();
        assert!(own_entry.status.is_none());
        assert!(s
            .recipients
            .entries
            .iter()
            .any(|e| e.key != own && e.status.is_some()));
        assert!(f.script.lock().unwrap().status_events.is_empty());

        // Refused before anything is signed.
        assert_eq!(
            request(&f, Some(("", None, None))).await,
            Some("status_invalid")
        );
        assert_eq!(
            request(&f, Some(("ok", Some("x"), None))).await,
            Some("status_invalid")
        );
        assert_eq!(
            request(&f, Some(("ok", None, Some(169)))).await,
            Some("status_invalid")
        );
        assert!(f.script.lock().unwrap().status_events.is_empty());

        f.script.lock().unwrap().status_reply = DmReply::Open;
        let before = nostr::Timestamp::now().as_secs();
        assert_eq!(
            request(&f, Some(("  In a meeting ", Some("🗣️"), Some(4)))).await,
            None
        );
        let s = snapshot(&mut f.status, |s| {
            s.user_status.state == "ready" && s.user_status.mine.is_some()
        })
        .await;
        let mine = s.user_status.mine.unwrap();
        assert_eq!(
            (mine.text.as_str(), mine.emoji.as_deref()),
            ("In a meeting", Some("🗣️"))
        );
        let expires = mine.expires_at.unwrap();
        assert!(
            expires >= before + 4 * 3600 && expires <= nostr::Timestamp::now().as_secs() + 4 * 3600
        );
        {
            let script = f.script.lock().unwrap();
            let [event] = script.status_events.as_slice() else {
                panic!("one status event expected")
            };
            event.verify().unwrap();
            assert_eq!(event.pubkey.to_hex(), own);
            assert_eq!(event.kind.as_u16(), 30315);
            assert_eq!(event.content, "In a meeting");
            assert_eq!(
                tag_list(event),
                vec![
                    vec!["d".to_string(), "general".into()],
                    vec!["emoji".into(), "🗣️".into()],
                    vec!["expiration".into(), expires.to_string()],
                ]
            );
        }
        // A second request inside the gap is refused and nothing is signed.
        assert_eq!(request(&f, None).await, Some("status_rate_limited"));
        assert_eq!(f.script.lock().unwrap().status_events.len(), 1);

        // The roster read carries both statuses; the other member's comes from
        // the relay, this identity's matches what was published. The shown
        // roster is dropped first so only the new read can satisfy the wait.
        f.injector.send_modify(|s| {
            s.recipients = crate::protocol::RecipientsView::unavailable(None, None)
        });
        f.commands
            .send(Command::FetchRecipients(a.clone()))
            .await
            .unwrap();
        let s = snapshot(&mut f.status, |s| {
            s.recipients.state == "snapshot" && s.recipients.room_id.as_deref() == Some(a.as_str())
        })
        .await;
        for entry in &s.recipients.entries {
            let status = entry.status.as_ref().expect("both members have a status");
            if entry.key == own {
                assert_eq!(
                    (status.text.as_str(), status.emoji.as_deref()),
                    ("In a meeting", Some("🗣️"))
                );
            } else {
                assert_eq!(
                    (status.text.as_str(), status.emoji.as_deref()),
                    ("Out sick", Some("🤒"))
                );
            }
        }
        assert_eq!(
            s.user_status.mine.as_ref().map(|m| m.text.as_str()),
            Some("In a meeting")
        );

        // After the gap: clear publishes the empty replacement.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(request(&f, None).await, None);
        let s = snapshot(&mut f.status, |s| {
            s.user_status.state == "ready" && s.user_status.mine.is_none()
        })
        .await;
        let own_entry = s.recipients.entries.iter().find(|e| e.key == own).unwrap();
        assert!(
            own_entry.status.is_none(),
            "the roster still shows a cleared status"
        );
        {
            let script = f.script.lock().unwrap();
            let clear = script.status_events.last().unwrap();
            assert_eq!(script.status_events.len(), 2);
            assert_eq!(clear.content, "");
            assert_eq!(
                tag_list(clear),
                vec![vec!["d".to_string(), "general".into()]]
            );
        }
        // The confirming read (one second later) agrees: still none.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(f.status.borrow().user_status.mine.is_none());

        // A refusal keeps the shown status and reports `status_rejected`.
        f.script.lock().unwrap().status_reply = DmReply::Reject;
        assert_eq!(request(&f, Some(("Lunch", None, None))).await, None);
        let s = snapshot(&mut f.status, |s| s.user_status.state == "failed").await;
        assert_eq!(s.user_status.category.as_deref(), Some("status_rejected"));
        assert!(s.user_status.mine.is_none());

        // No answer: unknown after the publication deadline, as relay_unavailable.
        tokio::time::sleep(Duration::from_millis(400)).await;
        f.script.lock().unwrap().status_reply = DmReply::Silent;
        let sent_at = tokio::time::Instant::now();
        assert_eq!(
            request(&f, Some(("Away", Some(":palm_tree:"), Some(1)))).await,
            None
        );
        snapshot(&mut f.status, |s| s.user_status.state == "sending").await;
        // One at a time.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(request(&f, None).await, Some("status_rate_limited"));
        while f.status.borrow().user_status.state == "sending" {
            f.status.changed().await.unwrap();
        }
        assert!(sent_at.elapsed() >= crate::user_status::TIMEOUT - Duration::from_millis(100));
        let s = f.status.borrow().clone();
        assert_eq!(s.user_status.state, "failed");
        assert_eq!(s.user_status.category.as_deref(), Some("relay_unavailable"));
        assert_eq!(s.connection, "authenticated");
        assert_eq!(f.script.lock().unwrap().status_events.len(), 4);
        f.finish().await;
    })
    .await
    .expect("status fixture deadline");
}
