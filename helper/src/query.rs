//! Bounded, unwired HTTP transport for initial room discovery. No UI signing API.
use base64::{engine::general_purpose::STANDARD, Engine};
use nostr::{
    hashes::{sha256, Hash},
    nips::nip98::{HttpData, HttpMethod},
    Event, EventBuilder, EventId, JsonUtil, Keys, Tag,
};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    redirect::Policy,
    Client,
};
use std::time::Duration;
use uuid::Uuid;

const RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_BYTES: usize = 8192;
const MAX_EVENTS: usize = 200;
/// One legacy thread page: 50 reply rows plus their auxiliary closure.
pub const THREAD_PAGE_EVENTS: usize = 400;
pub const THREAD_PAGE_BYTES: usize = 1024 * 1024;
pub const THREAD_PAGE_ROWS: u16 = 50;
pub const THREAD_DEPTH: u16 = 64;
static IN_FLIGHT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Initial discovery reads only; member is always the enrolled signing identity.
pub enum QueryRequest {
    JoinedRooms {
        limit: u16,
    },
    RoomMetadata {
        rooms: Vec<Uuid>,
    },
    /// NIP-CW channel window. `before` is the previous page's signed
    /// `next_cursor` echoed verbatim as `until` + `before_id`; `None` is the head.
    RoomHistory {
        room: Uuid,
        limit: u16,
        before: Option<(u64, EventId)>,
    },
    /// Desktop's legacy oldest-first thread read (NIP-CW "Legacy Oldest-first
    /// Threads"); `after` is the last loaded reply's `(created_at, id)`.
    ThreadReplies {
        room: Uuid,
        root: EventId,
        after: Option<(u64, EventId)>,
    },
    RoomMembers {
        room: Uuid,
    },
    Profiles {
        authors: Vec<nostr::PublicKey>,
    },
    AgentProfiles {
        authors: Vec<nostr::PublicKey>,
    },
    /// `user_status`: kind 30315 on the `d:general` coordinate (Desktop's
    /// `fetchUserStatusLookup` filter, `hooks.ts:198-250`).
    UserStatuses {
        authors: Vec<nostr::PublicKey>,
    },
    /// `presence`: kind 20001 for `authors`, answered by the relay's
    /// `synthesize_presence` with relay-signed snapshot events whose `p` tag
    /// names the subject (`presence::verify` checks the signer and subjects).
    Presence {
        authors: Vec<nostr::PublicKey>,
    },
    DmVisibility,
    /// Unscoped channel metadata: member channels plus every open channel
    /// (`channel_members.rs` `get_accessible_channel_ids`), for open rooms.
    OpenRooms,
}
impl QueryRequest {
    fn body(&self, keys: &Keys) -> Result<Vec<u8>, &'static str> {
        let filter = match self {
            Self::RoomMembers { room } => {
                serde_json::json!({"kinds":[39002],"#d":[room.to_string()],"limit":1})
            }
            Self::Profiles { authors } if !authors.is_empty() && authors.len() <= 20 => {
                serde_json::json!({"kinds":[0],"authors":authors.iter().map(|p|p.to_hex()).collect::<Vec<_>>(),"limit":authors.len()})
            }
            Self::AgentProfiles { authors } if !authors.is_empty() && authors.len() <= 20 => {
                serde_json::json!({"kinds":[10100],"authors":authors.iter().map(|p|p.to_hex()).collect::<Vec<_>>(),"limit":authors.len()})
            }
            Self::UserStatuses { authors } if !authors.is_empty() && authors.len() <= 20 => {
                serde_json::json!({"kinds":[30315],"authors":authors.iter().map(|p|p.to_hex()).collect::<Vec<_>>(),"#d":["general"],"limit":authors.len()})
            }
            Self::Presence { authors } if !authors.is_empty() && authors.len() <= 20 => {
                serde_json::json!({"kinds":[20001],"authors":authors.iter().map(|p|p.to_hex()).collect::<Vec<_>>(),"limit":authors.len()})
            }
            // NIP-DV per-viewer hidden-DM snapshot (docs/nips/NIP-DV.md:104-110).
            Self::DmVisibility => {
                serde_json::json!({"kinds":[30622],"#p":[keys.public_key().to_hex()],"limit":1})
            }
            Self::OpenRooms => serde_json::json!({"kinds":[39000],"limit":MAX_EVENTS}),
            Self::JoinedRooms { limit } if (1..=50).contains(limit) => {
                serde_json::json!({"kinds":[39002],"#p":[keys.public_key().to_hex()],"limit":limit})
            }
            Self::RoomMetadata { rooms } if !rooms.is_empty() && rooms.len() <= 20 => {
                serde_json::json!({"kinds":[39000],"#d":rooms.iter().map(Uuid::to_string).collect::<Vec<_>>(),"limit":rooms.len()})
            }
            Self::RoomHistory {
                room,
                limit,
                before,
            } if (1..=20).contains(limit) => {
                let mut filter = serde_json::json!({"kinds":[9,40002],"#h":[room.to_string()],"limit":limit,"top_level":true,"include_aux":true,"include_summaries":true});
                // Both or neither (NIP-CW); a continuation is never demoted to a head read.
                if let Some((until, id)) = before {
                    filter["until"] = serde_json::json!(until);
                    filter["before_id"] = serde_json::json!(id.to_hex());
                }
                filter
            }
            Self::ThreadReplies { room, root, after } => {
                let mut filter = serde_json::json!({"#h":[room.to_string()],"#e":[root.to_hex()],"kinds":[9,40002],"depth_limit":THREAD_DEPTH,"limit":THREAD_PAGE_ROWS,"include_aux":true});
                if let Some((created_at, id)) = after {
                    filter["thread_cursor"] = serde_json::json!(created_at);
                    filter["thread_cursor_id"] = serde_json::json!(id.to_hex());
                }
                filter
            }
            _ => return Err("invalid_query"),
        };
        let bytes = serde_json::to_vec(&vec![filter]).map_err(|_| "invalid_query")?;
        if bytes.len() > REQUEST_BYTES {
            return Err("invalid_query");
        }
        Ok(bytes)
    }
    fn budget(&self) -> (usize, usize) {
        match self {
            Self::ThreadReplies { .. } => (THREAD_PAGE_EVENTS, THREAD_PAGE_BYTES),
            _ => (MAX_EVENTS, RESPONSE_BYTES),
        }
    }
    fn matches(&self, event: &Event, keys: &Keys) -> bool {
        match self {
            Self::RoomMembers { room } => {
                event.kind.as_u16() == 39002
                    && event.tags.iter().any(|t| {
                        t.as_slice().first().map(String::as_str) == Some("d")
                            && t.as_slice()
                                .get(1)
                                .is_some_and(|id| *id == room.to_string())
                    })
            }
            Self::Profiles { authors } => {
                event.kind.as_u16() == 0 && authors.contains(&event.pubkey)
            }
            Self::AgentProfiles { authors } => {
                event.kind.as_u16() == 10100 && authors.contains(&event.pubkey)
            }
            Self::UserStatuses { authors } => {
                event.kind.as_u16() == 30315
                    && authors.contains(&event.pubkey)
                    && event.tags.iter().any(|t| {
                        t.as_slice().first().map(String::as_str) == Some("d")
                            && t.as_slice().get(1).map(String::as_str) == Some("general")
                    })
            }
            // The signer is the relay, not an author; `presence::verify` checks it.
            Self::Presence { .. } => event.kind.as_u16() == 20001,
            Self::RoomHistory { room, .. } => {
                let kind = event.kind.as_u16();
                // The pinned SDK's NIP-25 reaction builder emits an e target
                // without h; reducers verify its signature and target shape.
                let has_h = event
                    .tags
                    .iter()
                    .any(|t| t.as_slice().first().map(String::as_str) == Some("h"));
                matches!(kind, 9 | 40002 | 40003 | 5 | 9005 | 7 | 39005 | 39006)
                    && (matches!(kind, 5 | 9005)
                        || kind == 7 && !has_h
                        || event.tags.iter().any(|t| {
                            t.as_slice().first().map(String::as_str) == Some("h")
                                && t.as_slice()
                                    .get(1)
                                    .is_some_and(|id| *id == room.to_string())
                        }))
            }
            Self::ThreadReplies { room, .. } => {
                let kind = event.kind.as_u16();
                // Keep explicit cross-room h tags out even for reactions.
                let has_h = event
                    .tags
                    .iter()
                    .any(|t| t.as_slice().first().map(String::as_str) == Some("h"));
                matches!(kind, 9 | 40002 | 40003 | 5 | 9005 | 7)
                    && (matches!(kind, 5 | 9005)
                        || kind == 7 && !has_h
                        || event.tags.iter().any(|t| {
                            t.as_slice().first().map(String::as_str) == Some("h")
                                && t.as_slice()
                                    .get(1)
                                    .is_some_and(|id| *id == room.to_string())
                        }))
            }
            Self::DmVisibility => {
                event.kind.as_u16() == 30622
                    && event.tags.iter().any(|t| {
                        t.as_slice().first().map(String::as_str) == Some("p")
                            && t.as_slice().get(1) == Some(&keys.public_key().to_hex())
                    })
            }
            Self::OpenRooms => event.kind.as_u16() == 39000,
            Self::JoinedRooms { .. } => {
                event.kind.as_u16() == 39002
                    && event.tags.iter().any(|t| {
                        t.as_slice().first().map(String::as_str) == Some("p")
                            && t.as_slice().get(1) == Some(&keys.public_key().to_hex())
                    })
            }
            Self::RoomMetadata { rooms } => {
                event.kind.as_u16() == 39000
                    && event.tags.iter().any(|t| {
                        t.as_slice().first().map(String::as_str) == Some("d")
                            && t.as_slice()
                                .get(1)
                                .is_some_and(|id| rooms.iter().any(|room| room.to_string() == *id))
                    })
            }
        }
    }
}

fn endpoint(relay: &str) -> Result<url::Url, &'static str> {
    let canonical = crate::config::canonical_relay(relay).map_err(|_| "invalid_query_origin")?;
    let mut u = url::Url::parse(&canonical).map_err(|_| "invalid_query_origin")?;
    let scheme = if u.scheme() == "wss" { "https" } else { "http" };
    u.set_scheme(scheme).map_err(|_| "invalid_query_origin")?;
    u.set_path("/query");
    Ok(u)
}
pub(crate) fn authorization(
    keys: &Keys,
    url: &url::Url,
    body: &[u8],
) -> Result<HeaderValue, &'static str> {
    let data = HttpData::new(
        nostr::Url::parse(url.as_str()).map_err(|_| "invalid_query_origin")?,
        HttpMethod::POST,
    )
    .payload(sha256::Hash::hash(body));
    let nonce = Tag::parse(["nonce", &Uuid::new_v4().to_string()])
        .map_err(|_| "query_signing_unavailable")?;
    let event = EventBuilder::http_auth(data)
        .tags([nonce])
        .sign_with_keys(keys)
        .map_err(|_| "query_signing_unavailable")?;
    let value = format!("Nostr {}", STANDARD.encode(event.as_json().as_bytes()));
    let mut header = HeaderValue::from_str(&value).map_err(|_| "query_signing_unavailable")?;
    header.set_sensitive(true);
    Ok(header)
}

/// One read-only query against a fixed endpoint, with no retry or redirect.
/// Returned events have valid signatures and requested scope. Relay-author trust,
/// room membership decisions and catalog completeness remain caller obligations.
/// At most two reads run concurrently; extra calls fail without queueing.
pub async fn query(
    relay: &str,
    keys: &Keys,
    request: &QueryRequest,
) -> Result<Vec<Event>, &'static str> {
    let _permit = IN_FLIGHT.try_acquire().map_err(|_| "query_busy")?;
    let url = endpoint(relay)?;
    let body = request.body(keys)?;
    let (max_events, max_bytes) = request.budget();
    let auth = authorization(keys, &url, &body)?;
    let client = Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "query_transport_unavailable")?;
    let mut response = client
        .post(url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "query_timeout"
            } else {
                "query_transport_unavailable"
            }
        })?;
    if response.status().is_redirection() {
        return Err("query_redirect_rejected");
    }
    match response.status().as_u16() {
        200 => {}
        401 => return Err("query_auth_rejected"),
        403 => return Err("query_access_denied"),
        429 => return Err("query_rate_limited"),
        _ => return Err("query_unavailable"),
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err("query_oversized");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        if e.is_timeout() {
            "query_timeout"
        } else {
            "query_transport_unavailable"
        }
    })? {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err("query_oversized");
        }
        bytes.extend_from_slice(&chunk);
    }
    let events: Vec<Event> =
        serde_json::from_slice(&bytes).map_err(|_| "query_invalid_response")?;
    if events.len() > max_events {
        return Err("query_oversized");
    }
    for event in &events {
        event.verify().map_err(|_| "query_invalid_signature")?;
        if !request.matches(event, keys) {
            #[cfg(test)]
            if let QueryRequest::ThreadReplies { room, .. } = request {
                let kind = event.kind.as_u16();
                let has_h = event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice().first().map(String::as_str) == Some("h"));
                let room_h = event.tags.iter().any(|tag| {
                    tag.as_slice().first().map(String::as_str) == Some("h")
                        && tag.as_slice().get(1) == Some(&room.to_string())
                });
                eprintln!(
                    "OMARCHY_THREAD_SCOPE kind={} has_h={} room_h={} allowed_kind={}",
                    kind,
                    u8::from(has_h),
                    u8::from(room_h),
                    u8::from(matches!(kind, 9 | 40002 | 40003 | 5 | 9005 | 7))
                );
            }
            return Err("query_invalid_scope");
        }
    }
    Ok(events)
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
