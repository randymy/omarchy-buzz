//! Open rooms, join (kind 9021) and leave (kind 9022) through the production
//! observer, on the synthetic joined-room fixture.
use super::*;

const OPEN: &str = "55555555-5555-4555-8555-555555555555";
const JOIN: &str = "00000000-0000-4000-8000-0000000000e1";
const LEAVE_REFUSED: &str = "00000000-0000-4000-8000-0000000000e2";
const LEAVE: &str = "00000000-0000-4000-8000-0000000000e3";
const SILENT: &str = "00000000-0000-4000-8000-0000000000e4";

async fn act(
    f: &Recheck,
    action: crate::join::Action,
    id: &str,
    room: &str,
) -> Option<&'static str> {
    let (reply, answer) = oneshot::channel();
    f.commands
        .send(Command::RoomAction(action, id.into(), room.into(), reply))
        .await
        .unwrap();
    answer.await.unwrap()
}
fn tags(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}

#[tokio::test]
async fn open_rooms_are_joined_and_left_with_acknowledged_events() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    timeout(Duration::from_secs(30), async {
        // Joined rooms would not be re-checked for a minute on their own.
        let mut f = recheck_fixture_every(None, Duration::from_secs(60)).await;
        f.script.lock().unwrap().open = vec![OPEN.into()];
        wait_status(&mut f.status, |s| {
            s.catalog.state == "partial" && s.catalog.rooms.len() == 2
        })
        .await;
        // Nothing is open to join before a verified snapshot is shown.
        assert_eq!(
            act(&f, crate::join::Action::Join, JOIN, OPEN).await,
            Some("room_not_open")
        );
        f.commands.send(Command::FetchOpenRooms).await.unwrap();
        let s = snapshot(&mut f.status, |s| s.open_rooms.state == "snapshot").await;
        // Joined rooms and the private room are not offered.
        assert_eq!(
            s.open_rooms.rooms,
            vec![crate::protocol::OpenRoom {
                id: OPEN.into(),
                name: "Open fixture".into(),
                description: String::new(),
                kind: "stream".into(),
            }]
        );
        assert_eq!(
            act(&f, crate::join::Action::Join, JOIN, &f.a).await,
            Some("room_not_open")
        );
        assert_eq!(
            act(&f, crate::join::Action::Leave, JOIN, OPEN).await,
            Some("leave_rejected")
        );
        assert!(f.script.lock().unwrap().room_events.is_empty());

        f.script.lock().unwrap().room_reply = DmReply::Open;
        let checks = *f.discoveries.borrow();
        assert_eq!(act(&f, crate::join::Action::Join, JOIN, OPEN).await, None);
        let s = snapshot(&mut f.status, |s| s.room_action.state == "acknowledged").await;
        assert_eq!(s.room_action.action.as_deref(), Some("join"));
        assert_eq!(s.room_action.request_id.as_deref(), Some(JOIN));
        assert_eq!(s.room_action.room_id.as_deref(), Some(OPEN));
        // The joined-room check runs at once and lists the room; open rooms follow.
        let s = snapshot(&mut f.status, |s| {
            s.catalog.rooms.iter().any(|r| r.id == OPEN)
                && s.open_rooms.state == "snapshot"
                && s.open_rooms.rooms.is_empty()
        })
        .await;
        assert!(*f.discoveries.borrow() > checks);
        assert_eq!(s.catalog.rooms.len(), 3);

        f.script.lock().unwrap().room_reply = DmReply::Reject;
        assert_eq!(
            act(&f, crate::join::Action::Leave, LEAVE_REFUSED, OPEN).await,
            None
        );
        let s = snapshot(&mut f.status, |s| {
            s.room_action.request_id.as_deref() == Some(LEAVE_REFUSED)
                && s.room_action.state != "sending"
        })
        .await;
        assert_eq!(s.room_action.state, "rejected");
        assert_eq!(s.room_action.category.as_deref(), Some("leave_rejected"));
        assert!(s.catalog.rooms.iter().any(|r| r.id == OPEN));

        f.script.lock().unwrap().room_reply = DmReply::Open;
        assert_eq!(act(&f, crate::join::Action::Leave, LEAVE, OPEN).await, None);
        let s = snapshot(&mut f.status, |s| {
            s.room_action.request_id.as_deref() == Some(LEAVE)
                && s.room_action.state == "acknowledged"
                && !s.catalog.rooms.iter().any(|r| r.id == OPEN)
                && s.open_rooms.state == "snapshot"
                && s.open_rooms.rooms.len() == 1
        })
        .await;
        assert_eq!(s.room_action.action.as_deref(), Some("leave"));
        {
            let script = f.script.lock().unwrap();
            let kinds: Vec<u16> = script.room_events.iter().map(|e| e.kind.as_u16()).collect();
            assert_eq!(kinds, [9021, 9022, 9022]);
            for event in &script.room_events {
                // Exactly the pinned SDK builders: one `h` tag, empty content.
                assert_eq!(event.content, "");
                assert_eq!(tags(event), vec![vec!["h".to_string(), OPEN.to_string()]]);
            }
        }
        // A reported request ID is refused; an unanswered action becomes unknown.
        assert_eq!(
            act(&f, crate::join::Action::Join, LEAVE, OPEN).await,
            Some("setup_busy")
        );
        f.script.lock().unwrap().room_reply = DmReply::Silent;
        assert_eq!(act(&f, crate::join::Action::Join, SILENT, OPEN).await, None);
        assert_eq!(
            act(&f, crate::join::Action::Join, JOIN, OPEN).await,
            Some("setup_busy")
        );
        while f.status.borrow().room_action.state == "sending" {
            f.status.changed().await.unwrap();
        }
        let s = f.status.borrow().clone();
        assert_eq!(s.room_action.state, "unknown");
        assert_eq!(s.room_action.category.as_deref(), Some("relay_unavailable"));
        f.finish().await;
    })
    .await
    .expect("open room fixture deadline");
}
