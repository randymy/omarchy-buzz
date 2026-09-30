//! DM open through the production observer, on the synthetic joined-room fixture.
use super::*;

async fn open(f: &Recheck, id: &str, participants: Vec<String>) -> Option<&'static str> {
    let generation = f.status.borrow().generation;
    open_in(f, id, participants, generation).await
}
async fn open_in(
    f: &Recheck,
    id: &str,
    participants: Vec<String>,
    generation: u64,
) -> Option<&'static str> {
    let (reply, answer) = oneshot::channel();
    f.commands
        .send(Command::OpenDm(
            crate::protocol::DmOpenIntent {
                request_id: id.into(),
                participants,
                generation,
            },
            reply,
        ))
        .await
        .unwrap();
    answer.await.unwrap()
}
// The fixture's other member of both rooms, from the verified roster of A.
async fn roster_member(f: &mut Recheck) -> String {
    let a = f.a.clone();
    wait_status(&mut f.status, |s| {
        s.catalog.state == "partial" && s.catalog.rooms.len() == 2
    })
    .await;
    f.commands
        .send(Command::FetchRecipients(a.clone()))
        .await
        .unwrap();
    let own = f.status.borrow().identity.clone().unwrap();
    snapshot(&mut f.status, |s| {
        s.recipients.state == "snapshot" && s.recipients.room_id.as_deref() == Some(a.as_str())
    })
    .await
    .recipients
    .entries
    .into_iter()
    .map(|entry| entry.key)
    .find(|key| *key != own)
    .unwrap()
}
const FIRST: &str = "00000000-0000-4000-8000-0000000000d1";
const SECOND: &str = "00000000-0000-4000-8000-0000000000d2";
const THIRD: &str = "00000000-0000-4000-8000-0000000000d3";

#[tokio::test]
async fn acknowledged_open_is_parsed_and_rechecks_rooms_at_once() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(20), async {
        // Joined rooms would not be re-checked for a minute on their own.
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        let other = roster_member(&mut f).await;
        let own = f.status.borrow().identity.clone().unwrap();
        let stranger = Keys::generate().public_key().to_hex();
        assert_eq!(
            open(&f, FIRST, vec![stranger.clone()]).await,
            Some("dm_open_access_denied")
        );
        assert_eq!(
            open(&f, FIRST, vec![other.clone(), stranger]).await,
            Some("dm_open_access_denied")
        );
        assert_eq!(
            open(&f, FIRST, vec![own.clone()]).await,
            Some("dm_open_invalid")
        );
        let generation = f.status.borrow().generation;
        assert_eq!(
            open_in(&f, FIRST, vec![other.clone()], generation + 1).await,
            Some("dm_open_scope_changed")
        );
        assert!(
            f.script.lock().unwrap().dm_events.is_empty(),
            "a refused open was published"
        );
        assert_eq!(f.status.borrow().dm_open.state, "idle");

        f.script.lock().unwrap().dm = DmReply::Open;
        let checks = *f.discoveries.borrow();
        assert_eq!(open(&f, FIRST, vec![other.clone()]).await, None);
        let s = snapshot(&mut f.status, |s| s.dm_open.state == "acknowledged").await;
        assert_eq!(s.dm_open.request_id.as_deref(), Some(FIRST));
        assert_eq!(s.dm_open.channel_id.as_deref(), Some(DM_ROOM));
        assert_eq!(s.dm_open.created, Some(true));
        assert!(s.dm_open.category.is_none());
        {
            let script = f.script.lock().unwrap();
            let [event] = script.dm_events.as_slice() else {
                panic!("one DM open expected")
            };
            assert_eq!(event.kind.as_u16(), 41010);
            assert_eq!(event.content, "");
            let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
            assert_eq!(tags, vec![vec!["p".to_string(), other.clone()]]);
        }
        // The re-check runs now, not at the one-minute cadence, and lists the DM.
        let s = snapshot(&mut f.status, |s| {
            s.catalog.rooms.iter().any(|r| r.id == DM_ROOM)
        })
        .await;
        assert!(*f.discoveries.borrow() > checks);
        let dm = s.catalog.rooms.iter().find(|r| r.id == DM_ROOM).unwrap();
        assert_eq!(dm.kind, "dm");
        assert!(!dm.hidden);
        let mut expected = vec![own.clone(), other.clone()];
        expected.sort();
        assert_eq!(dm.participants, expected);
        // A reported request ID is never signed again.
        assert_eq!(
            open(&f, FIRST, vec![other.clone()]).await,
            Some("dm_open_request_reused")
        );
        // An existing DM's participants may be opened without any roster on screen.
        f.injector.send_modify(|s| {
            s.recipients = crate::protocol::RecipientsView::unavailable(None, None)
        });
        assert_eq!(open(&f, SECOND, vec![other.clone()]).await, None);
        let s = snapshot(&mut f.status, |s| {
            s.dm_open.request_id.as_deref() == Some(SECOND) && s.dm_open.state == "acknowledged"
        })
        .await;
        assert_eq!(s.dm_open.channel_id.as_deref(), Some(DM_ROOM));
        assert_eq!(f.script.lock().unwrap().dm_events.len(), 2);

        f.script.lock().unwrap().dm = DmReply::Reject;
        assert_eq!(open(&f, THIRD, vec![other]).await, None);
        let s = snapshot(&mut f.status, |s| {
            s.dm_open.request_id.as_deref() == Some(THIRD) && s.dm_open.state != "sending"
        })
        .await;
        assert_eq!(s.dm_open.state, "rejected");
        assert_eq!(s.dm_open.category.as_deref(), Some("dm_open_rejected"));
        assert!(s.dm_open.channel_id.is_none() && s.dm_open.created.is_none());
        f.finish().await;
    })
    .await
    .expect("DM open fixture deadline");
}

#[tokio::test]
async fn unanswered_open_is_busy_then_unknown() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        let other = roster_member(&mut f).await;
        let sent_at = tokio::time::Instant::now();
        assert_eq!(open(&f, FIRST, vec![other.clone()]).await, None);
        assert_eq!(f.status.borrow().dm_open.state, "sending");
        // One open at a time; an identical replay keeps its receipt and is not re-signed.
        assert_eq!(
            open(&f, SECOND, vec![other.clone()]).await,
            Some("dm_open_busy")
        );
        assert_eq!(open(&f, FIRST, vec![other.clone()]).await, None);
        let own = f.status.borrow().identity.clone().unwrap();
        assert_eq!(
            open(&f, FIRST, vec![own]).await,
            Some("dm_open_request_reused")
        );
        assert_eq!(f.status.borrow().dm_open.request_id.as_deref(), Some(FIRST));
        // Longer than `wait_status` allows: the open expires after its own deadline.
        while f.status.borrow().dm_open.state == "sending" {
            f.status.changed().await.unwrap();
        }
        let s = f.status.borrow().clone();
        assert!(sent_at.elapsed() >= crate::dm_open::TIMEOUT - Duration::from_millis(100));
        assert_eq!(s.dm_open.state, "unknown");
        assert_eq!(s.dm_open.category.as_deref(), Some("dm_open_unknown"));
        assert!(s.dm_open.channel_id.is_none());
        assert_eq!(s.connection, "authenticated");
        assert_eq!(
            f.script.lock().unwrap().dm_events.len(),
            1,
            "replay was published"
        );
        f.finish().await;
    })
    .await
    .expect("unanswered DM open fixture deadline");
}
