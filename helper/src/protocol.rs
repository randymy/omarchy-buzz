use serde::{Deserialize, Serialize};
pub const LIMIT: usize = 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "roomId")]
    pub room_id: Option<String>,
    pub text: Option<String>,
    pub mentions: Option<Vec<String>>,
    pub generation: Option<u64>,
    #[serde(rename = "instanceId")]
    pub instance_id: Option<String>,
}
pub fn request(bytes: &[u8]) -> Result<Request, &'static str> {
    if bytes.len() > LIMIT {
        return Err("oversized_request");
    }
    let r: Request = serde_json::from_slice(bytes).map_err(|_| "invalid_request")?;
    if r.version != 1 {
        return Err("incompatible_protocol");
    }
    if r.id.is_empty()
        || r.id.len() > 128
        || !r
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("invalid_request");
    }
    if !matches!(
        r.kind.as_str(),
        "get_snapshot"
            | "retry_connection"
            | "subscribe"
            | "fetch_recent"
            | "fetch_recipients"
            | "send_message"
    ) {
        return Err("unsupported_request");
    }
    if matches!(
        r.kind.as_str(),
        "fetch_recent" | "fetch_recipients" | "send_message"
    ) {
        let room = r.room_id.as_deref().ok_or("invalid_request")?;
        let parsed = uuid::Uuid::parse_str(room).map_err(|_| "invalid_request")?;
        if parsed.to_string() != room {
            return Err("invalid_request");
        }
    } else if r.room_id.is_some() {
        return Err("invalid_request");
    }
    if r.kind == "send_message" {
        let id = uuid::Uuid::parse_str(&r.id).map_err(|_| "invalid_request")?;
        if id.to_string() != r.id {
            return Err("invalid_request");
        }
        let text = r.text.as_deref().ok_or("invalid_request")?;
        if text.trim().is_empty() || text.len() > 4096 || text.contains('\0') {
            return Err("invalid_request");
        }
        let mentions = r.mentions.as_ref().ok_or("invalid_request")?;
        if mentions.len() > 20 {
            return Err("invalid_request");
        }
        let mut seen = std::collections::BTreeSet::new();
        for value in mentions {
            let key = nostr::PublicKey::from_hex(value).map_err(|_| "invalid_request")?;
            if key.to_hex() != *value || !seen.insert(value) {
                return Err("invalid_request");
            }
        }
        if !r.generation.is_some_and(|g| (1..=2147483647).contains(&g)) {
            return Err("invalid_request");
        }
        let instance = r.instance_id.as_deref().ok_or("invalid_request")?;
        if instance.is_empty()
            || instance.len() > 128
            || !instance
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err("invalid_request");
        }
    } else if r.text.is_some()
        || r.mentions.is_some()
        || r.generation.is_some()
        || r.instance_id.is_some()
    {
        return Err("invalid_request");
    }
    Ok(r)
}
#[derive(Clone)]
pub struct SendIntent {
    pub request_id: String,
    pub room: String,
    pub text: String,
    pub mentions: Vec<String>,
    pub generation: u64,
}
pub enum Command {
    Retry,
    FetchRecent(String),
    FetchRecipients(String),
    // Trusted fixture path; external IPC always uses the checked reply boundary.
    #[allow(dead_code)]
    Send(SendIntent),
    SendChecked(
        SendIntent,
        tokio::sync::oneshot::Sender<Option<&'static str>>,
    ),
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub request_id: Option<String>,
    pub room_id: Option<String>,
    pub event_id: Option<String>,
    pub state: String,
    pub category: Option<String>,
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            request_id: None,
            room_id: None,
            event_id: None,
            state: "idle".into(),
            category: None,
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Recipient {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientsView {
    pub state: String,
    pub room_id: Option<String>,
    pub entries: Vec<Recipient>,
    pub partial: bool,
    pub category: Option<String>,
}
impl RecipientsView {
    pub fn unavailable(room: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            entries: Vec::new(),
            partial: true,
            category: category.map(str::to_owned),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct HistoryRow {
    pub id: String,
    pub author: String,
    pub time: u64,
    pub text: String,
    pub edited: bool,
    pub truncated: bool,
    pub unavailable: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub state: String,
    pub room_id: Option<String>,
    pub rows: Vec<HistoryRow>,
    pub has_more: Option<bool>,
    pub category: Option<String>,
}
impl History {
    pub fn unavailable(room: Option<String>, category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            room_id: room,
            rows: Vec::new(),
            has_more: None,
            category: category.map(str::to_owned),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
}
#[derive(Clone, Serialize)]
pub struct Catalog {
    pub state: String,
    pub rooms: Vec<Room>,
    pub category: Option<String>,
}
impl Catalog {
    pub fn unavailable(category: Option<&str>) -> Self {
        Self {
            state: "unavailable".into(),
            rooms: Vec::new(),
            category: category.map(str::to_owned),
        }
    }
    pub fn loading() -> Self {
        Self {
            state: "loading".into(),
            ..Self::unavailable(None)
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connection: String,
    pub identity: Option<String>,
    pub relay: Option<String>,
    pub generation: u64,
    pub category: Option<String>,
    pub catalog: Catalog,
    pub history: History,
    pub delivery: Delivery,
    pub recipients: RecipientsView,
}
impl Status {
    pub fn new(c: &crate::config::Config) -> Self {
        Self {
            connection: "unconfigured".into(),
            identity: c.identity.clone(),
            relay: c.relay.clone(),
            generation: 1,
            category: None,
            catalog: Catalog::unavailable(None),
            history: History::unavailable(None, None),
            delivery: Delivery::default(),
            recipients: RecipientsView::unavailable(None, None),
        }
    }
}
pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"generation":s.generation,"capabilities":["connection_status","room_catalog","room_history","message_send","room_recipients","history_auto_refresh"],"backendRevision":crate::compatibility::BUZZ_REVISION,"status":s})
}
pub async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
) -> Result<Option<Vec<u8>>, &'static str> {
    read_line_buffered(r, &mut Vec::new()).await
}
// Retain partial input across select! cancellation; a future-local buffer
// would discard a split request whenever a status update wins the selection.
pub async fn read_line_buffered<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    line: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>, &'static str> {
    use tokio::io::AsyncBufReadExt;
    loop {
        let b = r.fill_buf().await.map_err(|_| "io_unavailable")?;
        if b.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("incomplete_frame")
            };
        }
        let n = b
            .iter()
            .position(|x| *x == b'\n')
            .map(|p| p + 1)
            .unwrap_or(b.len());
        if line.len() + n > LIMIT {
            return Err("oversized_request");
        }
        let end = b[n - 1] == b'\n';
        line.extend_from_slice(&b[..n]);
        r.consume(n);
        if end {
            line.pop();
            return Ok(Some(std::mem::take(line)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_authority() {
        assert!(request(br#"{"version":1,"id":"a","type":"send_message"}"#).is_err());
        assert!(request(
            br#"{"version":1,"id":"a","type":"get_snapshot","private_key":"sentinel"}"#
        )
        .is_err());
        assert!(request(&vec![b'a'; LIMIT + 1]).is_err());
    }
    #[tokio::test]
    async fn oversized_frame() {
        let data = vec![b'a'; LIMIT + 1];
        let mut r = tokio::io::BufReader::new(data.as_slice());
        assert!(read_line(&mut r).await.is_err());
    }
}
#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn status_does_not_claim_sync_or_authority() {
        let status = Status::new(&crate::config::Config::default());
        let v = envelope("hello", None, "fixture-instance", &status);
        assert_eq!(v["status"]["connection"], "unconfigured");
        assert_eq!(
            v["capabilities"],
            serde_json::json!([
                "connection_status",
                "room_catalog",
                "room_history",
                "message_send",
                "room_recipients",
                "history_auto_refresh"
            ])
        );
        assert_eq!(v["status"]["catalog"]["state"], "unavailable");
        assert_eq!(v["status"]["catalog"]["rooms"], serde_json::json!([]));
        assert!(v.get("privateKey").is_none());
    }
    #[test]
    fn versions_and_identifiers_are_fenced() {
        assert!(request(br#"{"version":2,"id":"a","type":"subscribe"}"#).is_err());
        assert!(request(br#"{"version":1,"id":"../a","type":"subscribe"}"#).is_err());
        assert!(request(br#"{"version":1,"id":"a","type":"subscribe"}"#).is_ok());
    }
    #[test]
    fn history_requests_require_canonical_room_scope() {
        let room = "00000000-0000-4000-8000-000000000001";
        let valid =
            serde_json::json!({"version":1,"id":"history-1","type":"fetch_recent","roomId":room});
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for value in [
            serde_json::json!({"version":1,"id":"a","type":"fetch_recent"}),
            serde_json::json!({"version":1,"id":"a","type":"fetch_recent","roomId":"../room"}),
            serde_json::json!({"version":1,"id":"a","type":"get_snapshot","roomId":room}),
        ] {
            assert!(request(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn sender_contract_rejects_unscoped_or_oversized_intents() {
        let valid = serde_json::json!({
            "version":1,"id":"00000000-0000-4000-8000-000000000001","type":"send_message",
            "roomId":"00000000-0000-4000-8000-000000000002", "text":"hello", "mentions":[],
            "instanceId":"test-instance", "generation":1
        });
        assert!(request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for field in ["roomId", "text", "mentions", "instanceId", "generation"] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("id", serde_json::json!("ui-1")),
            ("text", serde_json::json!(" ")),
            ("text", serde_json::json!("é".repeat(2049))),
            ("generation", serde_json::json!(0)),
            ("instanceId", serde_json::json!("../instance")),
            ("mentions", serde_json::json!(["@codex"])),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(
                request(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "invalid {field}"
            );
        }
        let mut signing = valid.clone();
        signing["kind"] = serde_json::json!(9);
        assert!(request(&serde_json::to_vec(&signing).unwrap()).is_err());
        let mut read = valid;
        read["type"] = serde_json::json!("fetch_recent");
        assert!(request(&serde_json::to_vec(&read).unwrap()).is_err());
    }
    #[test]
    fn maximum_projected_snapshot_fits_ipc_frame() {
        let mut status = Status::new(&crate::config::Config::default());
        status.relay = Some("x".repeat(2048));
        status.identity = Some("a".repeat(64));
        // Quotes/backslashes expand on JSON encoding. Projection replaces controls.
        status.catalog.rooms = (0..20)
            .map(|_| Room {
                id: "00000000-0000-4000-8000-000000000001".into(),
                name: "\\".repeat(128),
                description: "\\".repeat(256),
            })
            .collect();
        status.history.rows = (0..20)
            .map(|_| HistoryRow {
                id: "a".repeat(64),
                author: "b".repeat(64),
                time: u64::MAX,
                text: "\\".repeat(768),
                edited: true,
                truncated: true,
                unavailable: false,
            })
            .collect();
        status.recipients.entries = (0..20)
            .map(|_| Recipient {
                key: "c".repeat(64),
                name: "\\".repeat(64),
            })
            .collect();
        status.delivery = Delivery {
            request_id: Some("00000000-0000-4000-8000-000000000001".into()),
            room_id: Some("00000000-0000-4000-8000-000000000002".into()),
            event_id: Some("a".repeat(64)),
            state: "acknowledged".into(),
            category: None,
        };
        let encoded = serde_json::to_vec(&envelope(
            "status",
            Some(&"a".repeat(128)),
            &"i".repeat(128),
            &status,
        ))
        .unwrap();
        assert!(encoded.len() + 1 <= LIMIT, "{} byte frame", encoded.len());
    }
    #[tokio::test]
    async fn requires_complete_frames() {
        let mut r = tokio::io::BufReader::new(&b"{}"[..]);
        assert_eq!(read_line(&mut r).await.unwrap_err(), "incomplete_frame");
        let mut r = tokio::io::BufReader::new(&b"{}\n{}\n"[..]);
        assert_eq!(read_line(&mut r).await.unwrap().unwrap(), b"{}");
        assert_eq!(read_line(&mut r).await.unwrap().unwrap(), b"{}");
        assert!(read_line(&mut r).await.unwrap().is_none());
    }
}
#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn partial_request_survives_status_update() {
        let (mut sender, receiver) = tokio::io::duplex(256);
        let mut receiver = tokio::io::BufReader::new(receiver);
        let mut partial = Vec::new();
        sender.write_all(b"{\"version\":1,").await.unwrap();
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(20),
            read_line_buffered(&mut receiver, &mut partial)
        )
        .await
        .is_err());
        sender
            .write_all(b"\"id\":\"a\",\"type\":\"subscribe\"}\n")
            .await
            .unwrap();
        let frame = read_line_buffered(&mut receiver, &mut partial)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request(&frame).unwrap().kind, "subscribe");
        assert!(partial.is_empty());
    }
}
