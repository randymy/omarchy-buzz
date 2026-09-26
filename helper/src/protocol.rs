use serde::{Deserialize, Serialize};
pub const LIMIT: usize = 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
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
        "get_snapshot" | "retry_connection" | "subscribe"
    ) {
        return Err("unsupported_request");
    }
    Ok(r)
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
        }
    }
}
pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"generation":s.generation,"capabilities":["connection_status","room_catalog"],"backendRevision":"781d39510cf23cfe224e8f521ae06a23377e06de","status":s})
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
            serde_json::json!(["connection_status", "room_catalog"])
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
