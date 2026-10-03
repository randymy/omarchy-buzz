//! `presence` through the production observer, on the synthetic joined-room
//! fixture: one signed kind 20001 per change (and per heartbeat), outcomes only
//! from the exact `OK`, the verified relay-signed reads, and offline on
//! shutdown.
use super::*;
use crate::presence::Mode;

async fn set(f: &Recheck, mode: Mode, active: bool) -> Option<&'static str> {
    let (reply, answer) = oneshot::channel();
    f.commands
        .send(Command::SetPresence(mode, active, reply))
        .await
        .unwrap();
    answer.await.unwrap()
}
fn sent(f: &Recheck) -> Vec<String> {
    f.script
        .lock()
        .unwrap()
        .presence_events
        .iter()
        .map(|e| e.content.clone())
        .collect()
}
async fn wait_sent(f: &Recheck, count: usize) {
    timeout(Duration::from_secs(5), async {
        while sent(f).len() < count {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("presence publication deadline");
}

#[tokio::test]
async fn presence_is_signed_once_acknowledged_exactly_and_read_from_the_relay() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(40), async {
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        let own = f.status.borrow().identity.clone().unwrap();
        wait_status(&mut f.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        let a = f.a.clone();
        f.commands
            .send(Command::FetchRecipients(a.clone()))
            .await
            .unwrap();
        let s = snapshot(&mut f.status, |s| s.recipients.state == "snapshot").await;
        // Before the panel asks, nothing is published or read.
        assert_eq!(s.presence, crate::protocol::PresenceView::unavailable());
        assert!(s.recipients.entries.iter().all(|e| e.presence.is_none()));
        assert!(sent(&f).is_empty());
        assert_eq!(f.script.lock().unwrap().presence_reads, 0);
        let other = s
            .recipients
            .entries
            .iter()
            .find(|e| e.key != own)
            .unwrap()
            .key
            .clone();

        // One signed kind 20001, acknowledged on its exact OK; the read follows.
        f.script.lock().unwrap().presence_reply = DmReply::Open;
        assert_eq!(set(&f, Mode::Auto, true).await, None);
        let s = snapshot(&mut f.status, |s| {
            s.presence.state == "ready" && !s.presence.peers.is_empty()
        })
        .await;
        assert_eq!(s.presence.mode.as_deref(), Some("auto"));
        assert_eq!(s.presence.published.as_deref(), Some("online"));
        assert!(s.presence.last_published_at.is_some());
        assert_eq!(
            s.presence.peers,
            vec![crate::protocol::PresencePeer {
                key: other.clone(),
                presence: "away".into()
            }]
        );
        for entry in &s.recipients.entries {
            let expected = if entry.key == own { "online" } else { "away" };
            assert_eq!(entry.presence.as_deref(), Some(expected));
        }
        {
            let script = f.script.lock().unwrap();
            let [event] = script.presence_events.as_slice() else {
                panic!("one presence event expected")
            };
            event.verify().unwrap();
            assert_eq!(event.pubkey.to_hex(), own);
            assert_eq!(event.kind.as_u16(), 20001);
            assert_eq!(event.content, "online");
            let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
            assert_eq!(tags, vec![vec!["status".to_string(), "online".into()]]);
            // The read asked only for the other member (never this identity).
            assert!(script.presence_reads >= 1);
        }

        // The same preference and hint: nothing new is signed.
        assert_eq!(set(&f, Mode::Auto, true).await, None);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(sent(&f), vec!["online"]);

        // Idle: away, after the gap; then the heartbeat repeats it.
        assert_eq!(set(&f, Mode::Auto, false).await, None);
        snapshot(&mut f.status, |s| {
            s.presence.published.as_deref() == Some("away")
        })
        .await;
        wait_sent(&f, 3).await;
        assert_eq!(sent(&f), vec!["online", "away", "away"]);

        // A refusal is a category; the connection is unaffected.
        f.script.lock().unwrap().presence_reply = DmReply::Reject;
        assert_eq!(set(&f, Mode::Offline, true).await, None);
        let s = snapshot(&mut f.status, |s| s.presence.state == "failed").await;
        assert_eq!(s.presence.category.as_deref(), Some("presence_rejected"));
        assert_eq!(s.presence.published.as_deref(), Some("away"));
        assert_eq!(s.connection, "authenticated");
        assert_eq!(sent(&f).last().map(String::as_str), Some("offline"));
        // Offline is not repeated by the heartbeat.
        let count = sent(&f).len();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        assert_eq!(sent(&f).len(), count);

        // A read signed by anyone but the pinned relay key is never shown.
        f.script.lock().unwrap().presence_forged = true;
        f.script.lock().unwrap().presence_reply = DmReply::Open;
        assert_eq!(set(&f, Mode::Auto, true).await, None);
        f.injector.send_modify(|s| {
            s.recipients = crate::protocol::RecipientsView::unavailable(None, None)
        });
        let reads = f.script.lock().unwrap().presence_reads;
        f.commands
            .send(Command::FetchRecipients(a.clone()))
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while f.script.lock().unwrap().presence_reads == reads {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let s = f.status.borrow().clone();
        // The earlier verified state may still show; the forged read changed nothing.
        let shown = s
            .presence
            .peers
            .iter()
            .find(|p| p.key == other)
            .map(|p| p.presence.as_str());
        assert!(shown.is_none() || shown == Some("away"));

        // Shutdown: offline once, answered once the relay acknowledged it.
        let s = snapshot(&mut f.status, |s| {
            s.presence.published.as_deref() == Some("online")
        })
        .await;
        assert_eq!(s.presence.state, "ready");
        // Past the gap, and well before the next heartbeat (1.5 s).
        tokio::time::sleep(Duration::from_millis(400)).await;
        let before = sent(&f).len();
        let (reply, done) = oneshot::channel();
        let started = tokio::time::Instant::now();
        f.commands
            .send(Command::PresenceShutdown(reply))
            .await
            .unwrap();
        timeout(crate::ipc::PRESENCE_SHUTDOWN, done)
            .await
            .expect("shutdown answered within its bound")
            .unwrap();
        assert!(started.elapsed() < crate::ipc::PRESENCE_SHUTDOWN);
        // A heartbeat may have left just before; offline follows it once the
        // gate allows, still inside the bound.
        let after = sent(&f);
        assert_eq!(after.last().map(String::as_str), Some("offline"));
        let tail = &after[before..];
        assert!(
            tail.len() <= 2 && tail[..tail.len() - 1].iter().all(|s| s == "online"),
            "{tail:?}"
        );
        let before = after.len() - 1;
        assert_eq!(
            f.status.borrow().presence.published.as_deref(),
            Some("offline")
        );
        // Nothing more after shutdown.
        tokio::time::sleep(Duration::from_millis(2000)).await;
        assert_eq!(sent(&f).len(), before + 1);
        f.finish().await;
    })
    .await
    .expect("presence fixture deadline");
}

#[tokio::test]
async fn the_last_panel_leaving_publishes_offline_once() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(20), async {
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        wait_status(&mut f.status, |s| s.catalog.state == "partial").await;
        f.script.lock().unwrap().presence_reply = DmReply::Open;
        // Detaching before anything was published owes nothing.
        f.commands.send(Command::PresenceDetach).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(sent(&f).is_empty());
        assert_eq!(set(&f, Mode::Away, true).await, None);
        snapshot(&mut f.status, |s| {
            s.presence.published.as_deref() == Some("away")
        })
        .await;
        f.commands.send(Command::PresenceDetach).await.unwrap();
        let s = snapshot(&mut f.status, |s| {
            s.presence.published.as_deref() == Some("offline")
        })
        .await;
        assert!(s.presence.mode.is_none());
        tokio::time::sleep(Duration::from_millis(2000)).await;
        assert_eq!(sent(&f), vec!["away", "offline"]);
        // Shutdown while a heartbeat is unanswered: it is abandoned and offline
        // follows as soon as the gate allows, inside the shutdown bound.
        f.script.lock().unwrap().presence_reply = DmReply::Silent;
        assert_eq!(set(&f, Mode::Auto, true).await, None);
        wait_sent(&f, 3).await;
        f.script.lock().unwrap().presence_reply = DmReply::Open;
        let (reply, done) = oneshot::channel();
        f.commands
            .send(Command::PresenceShutdown(reply))
            .await
            .unwrap();
        timeout(crate::ipc::PRESENCE_SHUTDOWN, done)
            .await
            .expect("shutdown answered within its bound")
            .unwrap();
        assert_eq!(sent(&f), vec!["away", "offline", "online", "offline"]);
        f.finish().await;
    })
    .await
    .expect("presence detach fixture deadline");
}

#[tokio::test]
async fn statuses_and_presence_age_out_while_room_checks_keep_failing() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(40), async {
        // The other member's status expires in a few seconds; presence read
        // more than two seconds ago is too old to show.
        let expires = nostr::Timestamp::now().as_secs() + 4;
        let mut f = recheck_fixture_aging(
            None,
            Duration::from_millis(400),
            Duration::from_secs(2),
            Some(expires),
        )
        .await;
        let own = f.status.borrow().identity.clone().unwrap();
        wait_status(&mut f.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        let a = f.a.clone();
        f.commands
            .send(Command::FetchRecipients(a.clone()))
            .await
            .unwrap();
        f.script.lock().unwrap().presence_reply = DmReply::Open;
        assert_eq!(set(&f, Mode::Auto, true).await, None);
        let other = |s: &Status| s.recipients.entries.iter().find(|e| e.key != own).cloned();
        snapshot(&mut f.status, |s| {
            s.recipients.state == "snapshot"
                && other(s)
                    .is_some_and(|e| e.status.is_some() && e.presence.as_deref() == Some("away"))
        })
        .await;
        // From now on every joined-room check fails as an unavailable relay.
        let checks = {
            let mut script = f.script.lock().unwrap();
            for _ in 0..60 {
                script.next.push_back(Discovery {
                    rooms: None,
                    gate: None,
                    changed: false,
                });
            }
            *f.discoveries.borrow()
        };
        // The kept views still age: the expired status and the old presence go.
        timeout(Duration::from_secs(15), async {
            loop {
                {
                    let s = f.status.borrow();
                    if other(&s).is_some_and(|e| e.status.is_none() && e.presence.is_none())
                        && s.presence.peers.is_empty()
                    {
                        break;
                    }
                }
                f.status.changed().await.unwrap();
            }
        })
        .await
        .expect("expired status and old presence were still shown");
        let s = f.status.borrow().clone();
        assert_eq!(s.connection, "authenticated");
        assert!(s.catalog.state == "partial" && s.catalog.rooms.len() == 2);
        assert_eq!(s.recipients.state, "snapshot");
        // Only failed checks ran meanwhile: none of them read presence again.
        assert!(*f.discoveries.borrow() > checks + 2);
        assert!(!f.script.lock().unwrap().next.is_empty());
        f.finish().await;
    })
    .await
    .expect("aging fixture deadline");
}
