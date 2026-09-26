//! Origin-bound NIP-11 discovery and relay-author-validated room snapshots.
//! A bounded #p query cannot prove a complete roster or current membership forever.
use crate::query::{query, QueryRequest};
use nostr::{Event, Keys, PublicKey, Timestamp};
use reqwest::{redirect::Policy, Client};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

const INFO_BYTES: usize = 32 * 1024;
const LIMIT: u16 = 20;
static DISCOVERY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Clone, Debug, Serialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
}
#[derive(Clone, Debug)]
pub struct Catalog {
    pub signer: PublicKey,
    pub trust: &'static str,
    pub state: &'static str,
    pub category: &'static str,
    pub rooms: Vec<Room>,
}

/// `self` is the signing identity. The NIP-11 `pubkey` contact is NOT authority.
fn info_signer(bytes: &[u8], pin: Option<PublicKey>) -> Result<PublicKey, &'static str> {
    if bytes.len() > INFO_BYTES {
        return Err("discovery_oversized");
    }
    let doc: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "discovery_invalid_info")?;
    let signer = doc
        .get("self")
        .and_then(|v| v.as_str())
        .and_then(|s| PublicKey::from_hex(s).ok())
        .ok_or("discovery_signer_unavailable")?;
    if pin.is_some_and(|expected| expected != signer) {
        return Err("relay_identity_changed");
    }
    Ok(signer)
}

/// TLS verifies the configured origin, not an independent identity assertion.
/// Persisting the returned signer constitutes TOFU; rotation must be explicit.
/// Unencrypted transport is accepted only for config's loopback fixture origins.
pub async fn relay_signer(relay: &str, pin: Option<PublicKey>) -> Result<PublicKey, &'static str> {
    let canonical = crate::config::canonical_relay(relay).map_err(|_| "invalid_query_origin")?;
    let mut url = url::Url::parse(&canonical).map_err(|_| "invalid_query_origin")?;
    let scheme = if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    url.set_scheme(scheme).map_err(|_| "invalid_query_origin")?;
    // Pinned Buzz exposes the same NIP-11 document at /info and GET / with Accept.
    url.set_path("/info");
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
        .map_err(|_| "discovery_unavailable")?;
    let mut response = client
        .get(url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|_| "discovery_unavailable")?;
    if response.status().is_redirection() {
        return Err("discovery_redirect_rejected");
    }
    if response.status().as_u16() != 200 {
        return Err("discovery_unavailable");
    }
    if response
        .content_length()
        .is_some_and(|n| n > INFO_BYTES as u64)
    {
        return Err("discovery_oversized");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "discovery_unavailable")?
    {
        if bytes.len().saturating_add(chunk.len()) > INFO_BYTES {
            return Err("discovery_oversized");
        }
        bytes.extend_from_slice(&chunk);
    }
    info_signer(&bytes, pin)
}

fn one_tag<'a>(event: &'a Event, name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut matches = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == name));
    let value = matches
        .next()
        .map(|t| t.as_slice().get(1).map(String::as_str));
    if matches.next().is_some() || value == Some(None) {
        return Err("catalog_invalid_shape");
    }
    Ok(value.flatten())
}
fn room_id(event: &Event) -> Result<Uuid, &'static str> {
    let value = one_tag(event, "d")?.ok_or("catalog_invalid_shape")?;
    let id = Uuid::parse_str(value).map_err(|_| "catalog_invalid_shape")?;
    if id.to_string() != value {
        return Err("catalog_invalid_shape");
    }
    Ok(id)
}
fn validated(event: &Event, signer: PublicKey, kind: u16, now: u64) -> Result<Uuid, &'static str> {
    event.verify().map_err(|_| "catalog_invalid_signature")?;
    if event.pubkey != signer {
        return Err("catalog_untrusted_author");
    }
    if event.kind.as_u16() != kind
        || !event.content.is_empty()
        || event.created_at.as_secs() > now.saturating_add(60)
    {
        return Err("catalog_invalid_shape");
    }
    room_id(event)
}
fn insert<'a>(
    map: &mut BTreeMap<Uuid, &'a Event>,
    id: Uuid,
    event: &'a Event,
) -> Result<(), &'static str> {
    if let Some(old) = map.get(&id) {
        if old.id == event.id {
            return Ok(());
        }
        // Conflicting versions in a membership response cannot establish current authority.
        return Err("catalog_conflicting_snapshot");
    }
    map.insert(id, event);
    Ok(())
}

/// Revalidate even when events came from query.rs. Missing metadata stays unknown.
/// Timestamps of events describe changes, not read freshness; static rooms need not
/// change daily. On refresh failure the owner must mark any retained snapshot stale.
pub fn reconcile(
    member: PublicKey,
    signer: PublicKey,
    memberships: &[Event],
    metadata: &[Event],
    now: u64,
) -> Result<Catalog, &'static str> {
    if memberships.len() > LIMIT as usize || metadata.len() > LIMIT as usize {
        return Err("catalog_oversized");
    }
    let mut joined = BTreeMap::new();
    for event in memberships {
        let id = validated(event, signer, 39002, now)?;
        let mut contains = false;
        for tag in event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().is_some_and(|v| v == "p"))
        {
            let p = tag
                .as_slice()
                .get(1)
                .and_then(|v| PublicKey::from_hex(v).ok())
                .ok_or("catalog_invalid_shape")?;
            contains |= p == member;
        }
        if !contains {
            return Err("catalog_invalid_membership");
        }
        insert(&mut joined, id, event)?;
    }
    let mut details = BTreeMap::new();
    for event in metadata {
        let id = validated(event, signer, 39000, now)?;
        if !joined.contains_key(&id) {
            return Err("catalog_invalid_scope");
        }
        insert(&mut details, id, event)?;
    }
    let mut rooms = Vec::new();
    for (id, _membership) in joined {
        // No metadata means no display claim; partial keeps absence explicit.
        let Some(event) = details.get(&id) else {
            continue;
        };
        let name = one_tag(event, "name")?.ok_or("catalog_invalid_shape")?;
        let kind = one_tag(event, "t")?.ok_or("catalog_invalid_shape")?;
        if kind.is_empty()
            || kind.len() > 32
            || !kind.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err("catalog_invalid_shape");
        }
        let flag = |n: &str| {
            event
                .tags
                .iter()
                .any(|t| t.as_slice().first().is_some_and(|v| v == n))
        };
        if kind != "stream" || flag("hidden") || flag("archived") {
            continue;
        }
        let clean = |text: &str, cap: usize| {
            let mut value = String::new();
            for ch in text.chars() {
                let ch = if ch.is_control() { ' ' } else { ch };
                if value.len() + ch.len_utf8() > cap {
                    break;
                }
                value.push(ch);
            }
            value
        };
        let name = clean(name, 128);
        if name.trim().is_empty() {
            return Err("catalog_invalid_shape");
        }
        let description = clean(one_tag(event, "about")?.unwrap_or(""), 256);
        rooms.push(Room {
            id: id.to_string(),
            name,
            description,
        });
    }
    // Query limits and no pagination/completeness marker prevent a complete claim,
    // including for an empty result. Rows indicate last observed membership only.
    Ok(Catalog {
        signer,
        trust: "tls_origin",
        state: "partial",
        category: "room_catalog_partial",
        rooms,
    })
}

pub async fn discover(
    relay: &str,
    keys: &Keys,
    pin: Option<PublicKey>,
) -> Result<Catalog, &'static str> {
    let _permit = DISCOVERY.try_acquire().map_err(|_| "discovery_busy")?;
    let signer = relay_signer(relay, pin).await?;
    let memberships = query(relay, keys, &QueryRequest::JoinedRooms { limit: LIMIT }).await?;
    // Validate authorship before letting returned IDs shape further requests.
    reconcile(
        keys.public_key(),
        signer,
        &memberships,
        &[],
        Timestamp::now().as_secs(),
    )?;
    let ids: Vec<Uuid> = memberships.iter().map(room_id).collect::<Result<_, _>>()?;
    let mut metadata = Vec::new();
    for batch in ids.chunks(20) {
        metadata.extend(
            query(
                relay,
                keys,
                &QueryRequest::RoomMetadata {
                    rooms: batch.to_vec(),
                },
            )
            .await?,
        );
    }
    let mut result = reconcile(
        keys.public_key(),
        signer,
        &memberships,
        &metadata,
        Timestamp::now().as_secs(),
    )?;
    result.trust = if relay.starts_with("ws:") {
        "loopback_development"
    } else {
        "tls_origin"
    };
    Ok(result)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
