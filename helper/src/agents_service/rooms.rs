//! The helper's verified joined-room list, read from its own control socket.
//! Only stream rooms of an authenticated, currently verified catalog for the
//! same relay and owner identity count; DMs are never agent rooms.
use std::{future::Future, pin::Pin, sync::Mutex};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::UnixStream,
    time::{timeout, Duration, Instant},
};

pub type RoomsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<String>, &'static str>> + Send + 'a>>;

pub trait RoomSource: Send + Sync {
    fn joined_rooms<'a>(&'a self, relay: &'a str, owner: &'a str) -> RoomsFuture<'a>;
}

const DEADLINE: Duration = Duration::from_secs(30);

/// Stream-room ids from one helper status frame, if it is a verified catalog
/// for this relay and owner.
pub fn verified_rooms(frame: &serde_json::Value, relay: &str, owner: &str) -> Option<Vec<String>> {
    let status = frame.get("status")?;
    if frame.get("version")? != 1
        || status.get("relay")?.as_str()? != relay
        || status.get("identity")?.as_str()? != owner
        || status.get("connection")?.as_str()? != "authenticated"
        || !matches!(
            status.get("catalog")?.get("state")?.as_str()?,
            "partial" | "ready"
        )
    {
        return None;
    }
    Some(
        status
            .get("catalog")?
            .get("rooms")?
            .as_array()?
            .iter()
            .filter(|room| room.get("kind").and_then(|k| k.as_str()) == Some("stream"))
            .filter_map(|room| room.get("id")?.as_str().map(str::to_owned))
            .filter(|id| super::store::canonical_uuid(id))
            .collect(),
    )
}

/// Reads the helper daemon's status over `control.sock` (activating it if
/// needed) until its catalog is verified, within 30 seconds.
pub struct HelperCatalog;
impl RoomSource for HelperCatalog {
    fn joined_rooms<'a>(&'a self, relay: &'a str, owner: &'a str) -> RoomsFuture<'a> {
        Box::pin(async move {
            let path = crate::ipc::socket_path()?;
            crate::ipc::socket_permissions(&path).map_err(|_| "relay_unavailable")?;
            let until = Instant::now() + DEADLINE;
            let stream = timeout(DEADLINE, UnixStream::connect(&path))
                .await
                .map_err(|_| "relay_unavailable")?
                .map_err(|_| "relay_unavailable")?;
            crate::ipc::peer_allowed(&stream).map_err(|_| "relay_unavailable")?;
            let (read, mut write) = stream.into_split();
            let subscribe = serde_json::json!({"version":1,"id":uuid::Uuid::new_v4().to_string(),"type":"subscribe"});
            let mut line = serde_json::to_vec(&subscribe).map_err(|_| "relay_unavailable")?;
            line.push(b'\n');
            timeout(DEADLINE, write.write_all(&line))
                .await
                .map_err(|_| "relay_unavailable")?
                .map_err(|_| "relay_unavailable")?;
            let mut read = BufReader::new(read);
            loop {
                let frame = timeout(
                    until.saturating_duration_since(Instant::now()),
                    crate::protocol::read_response_line(&mut read),
                )
                .await
                .map_err(|_| "relay_unavailable")?
                .map_err(|_| "relay_unavailable")?
                .ok_or("relay_unavailable")?;
                let value: serde_json::Value =
                    serde_json::from_slice(&frame).map_err(|_| "relay_unavailable")?;
                if let Some(rooms) = verified_rooms(&value, relay, owner) {
                    return Ok(rooms);
                }
            }
        })
    }
}

/// Synthetic room list for tests and fake-control mode. The relays asked
/// for are recorded in the second field.
pub struct FixedRooms(
    pub Mutex<Result<Vec<String>, &'static str>>,
    pub Mutex<Vec<String>>,
);
impl FixedRooms {
    pub fn new(rooms: Vec<String>) -> Self {
        Self(Mutex::new(Ok(rooms)), Mutex::new(Vec::new()))
    }
}
impl RoomSource for FixedRooms {
    fn joined_rooms<'a>(&'a self, relay: &'a str, _owner: &'a str) -> RoomsFuture<'a> {
        self.1.lock().unwrap().push(relay.to_owned());
        let rooms = self.0.lock().unwrap().clone();
        Box::pin(async move { rooms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RELAY: &str = "wss://relay.example/";
    const OWNER: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const ROOM: &str = "00000000-0000-4000-8000-0000000000b1";
    const DM: &str = "00000000-0000-4000-8000-0000000000d1";

    fn frame(connection: &str, state: &str, relay: &str) -> serde_json::Value {
        serde_json::json!({"version":1,"type":"status","status":{
            "connection":connection,"relay":relay,"identity":OWNER,
            "catalog":{"state":state,"rooms":[
                {"id":ROOM,"kind":"stream"},{"id":DM,"kind":"dm"},{"id":"bad","kind":"stream"}]}}})
    }

    #[test]
    fn only_a_verified_catalog_for_this_scope_yields_stream_rooms() {
        assert_eq!(
            verified_rooms(&frame("authenticated", "ready", RELAY), RELAY, OWNER),
            Some(vec![ROOM.to_string()])
        );
        assert_eq!(
            verified_rooms(&frame("authenticated", "partial", RELAY), RELAY, OWNER),
            Some(vec![ROOM.to_string()])
        );
        assert!(verified_rooms(&frame("connecting", "ready", RELAY), RELAY, OWNER).is_none());
        assert!(verified_rooms(&frame("authenticated", "loading", RELAY), RELAY, OWNER).is_none());
        assert!(verified_rooms(
            &frame("authenticated", "ready", "wss://other.example/"),
            RELAY,
            OWNER
        )
        .is_none());
        let other = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
        assert!(verified_rooms(&frame("authenticated", "ready", RELAY), RELAY, other).is_none());
    }
}
