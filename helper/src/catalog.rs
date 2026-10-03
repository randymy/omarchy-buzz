//! Origin-bound NIP-11 discovery and relay-author-validated room snapshots.
//! A bounded #p query cannot prove a complete roster or current membership forever.
use crate::query::{query, QueryRequest};
use nostr::{Event, EventId, Keys, PublicKey, Timestamp};
use reqwest::{redirect::Policy, Client};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use uuid::Uuid;

// Hosted Buzz can include inline community icons larger than 32 KiB.
// Bound the entire document; only its signer is returned to the caller.
const INFO_BYTES: usize = 128 * 1024;
/// Joined rooms (DMs included) are read in pages of `PAGE` kind 39002 events,
/// newest first, continued with Desktop's composite `(until, before_id)` cursor
/// (`fetch.rs` `advance_directory_cursor`). The panel holds at most
/// `MAX_PAGES` pages so a status frame stays bounded (`MAX_ROOMS` rows).
pub const PAGE: u16 = 50;
pub const MAX_PAGES: usize = 4;
pub const MAX_ROOMS: usize = PAGE as usize * MAX_PAGES;
/// A relay-signed membership snapshot's position: `(created_at, event id)`.
pub type Cursor = (u64, EventId);
static DISCOVERY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Clone, Debug, Serialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `"stream"` or `"dm"`.
    pub kind: &'static str,
    /// DM participant keys (sorted lowercase hex, self included); empty for streams.
    pub participants: Vec<String>,
    /// Viewer-hidden DM per a relay-signed NIP-DV snapshot. Never set for streams.
    pub hidden: bool,
}
#[derive(Clone, Debug)]
pub struct Catalog {
    pub signer: PublicKey,
    pub trust: &'static str,
    pub state: &'static str,
    pub category: &'static str,
    pub rooms: Vec<Room>,
    /// `relay − local` seconds from this discovery's NIP-11 `Date` header.
    pub clock_skew: Option<i64>,
    /// The last membership page was full: more joined rooms may exist.
    pub has_more: bool,
    /// Where the next page continues; present only with `has_more`.
    pub next: Option<Cursor>,
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

/// The relay's NIP-11 location: the configured origin over HTTP(S), `/info`.
pub(crate) fn info_url(relay: &str) -> Result<url::Url, &'static str> {
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
    Ok(url)
}

/// No proxy, compression, retries or redirects; short deadlines.
pub(crate) fn info_client() -> Result<Client, &'static str> {
    Client::builder()
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
        .map_err(|_| "discovery_unavailable")
}

/// TLS verifies the configured origin, not an independent identity assertion.
/// Persisting the returned signer constitutes TOFU; rotation must be explicit.
/// Unencrypted transport is accepted only for config's loopback fixture origins.
pub async fn relay_signer(relay: &str, pin: Option<PublicKey>) -> Result<PublicKey, &'static str> {
    relay_info(relay, pin).await.0
}

/// `relay_signer`, with the clock offset read from the same response's `Date`
/// header (`clock::from_headers`) whenever a response arrived.
pub async fn relay_info(
    relay: &str,
    pin: Option<PublicKey>,
) -> (Result<PublicKey, &'static str>, Option<i64>) {
    let (url, client) = match info_url(relay).and_then(|url| Ok((url, info_client()?))) {
        Ok(pair) => pair,
        Err(error) => return (Err(error), None),
    };
    let response = match client
        .get(url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return (Err("discovery_unavailable"), None),
    };
    let skew = crate::clock::from_headers(response.headers());
    (info_body(response, pin).await, skew)
}

/// What `communities` reads from a relay's NIP-11 document besides its
/// signer: the self-asserted `name` (an untrusted label, sanitized and bounded
/// like a profile name; display hint only) and whether `supported_nips` lists
/// 43 (relay membership, so leaving needs a NIP-43 leave request). Icons are
/// never read.
#[derive(Clone, Debug, PartialEq)]
pub struct RelayProfile {
    pub signer: PublicKey,
    pub name: Option<String>,
    pub membership: bool,
}
pub(crate) fn info_profile(
    bytes: &[u8],
    pin: Option<PublicKey>,
) -> Result<RelayProfile, &'static str> {
    let signer = info_signer(bytes, pin)?;
    let doc: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "discovery_invalid_info")?;
    let name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .map(crate::config::label)
        .filter(|n| !n.is_empty());
    let membership = doc
        .get("supported_nips")
        .and_then(|v| v.as_array())
        .is_some_and(|nips| nips.iter().any(|n| n.as_u64() == Some(43)));
    Ok(RelayProfile {
        signer,
        name,
        membership,
    })
}
/// `relay_signer` with the profile fields `communities` shows and needs.
pub async fn relay_profile(relay: &str) -> Result<RelayProfile, &'static str> {
    let url = info_url(relay)?;
    let response = info_client()?
        .get(url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|_| "discovery_unavailable")?;
    let bytes = info_bytes(response).await?;
    info_profile(&bytes, None)
}

async fn info_body(
    response: reqwest::Response,
    pin: Option<PublicKey>,
) -> Result<PublicKey, &'static str> {
    info_signer(&info_bytes(response).await?, pin)
}

async fn info_bytes(mut response: reqwest::Response) -> Result<Vec<u8>, &'static str> {
    if response.status().is_redirection() {
        return Err("discovery_redirect_rejected");
    }
    // A refusal is an answer about access, not an unreachable relay: a
    // background check must not keep a catalog after it.
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err("discovery_denied");
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
    Ok(bytes)
}

pub(crate) fn one_tag<'a>(event: &'a Event, name: &str) -> Result<Option<&'a str>, &'static str> {
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
pub(crate) fn clean(text: &str, cap: usize) -> String {
    let mut value = String::new();
    for ch in text.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if value.len() + ch.len_utf8() > cap {
            break;
        }
        value.push(ch);
    }
    value
}
fn room_id(event: &Event) -> Result<Uuid, &'static str> {
    let value = one_tag(event, "d")?.ok_or("catalog_invalid_shape")?;
    let id = Uuid::parse_str(value).map_err(|_| "catalog_invalid_shape")?;
    if id.to_string() != value {
        return Err("catalog_invalid_shape");
    }
    Ok(id)
}
pub(crate) fn validated(
    event: &Event,
    signer: PublicKey,
    kind: u16,
    now: u64,
) -> Result<Uuid, &'static str> {
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

/// DM participants from kind 39000 `p` tags (side_effects.rs:1220-1225).
/// Upstream bounds a DM to 2-9 participants including its creator (buzz-db
/// store/dm.rs:109-118; relay open command command_executor.rs:315-319,491-495).
/// A DM outside those bounds, with a non-canonical or repeated key, or without the
/// viewer is not something pinned Buzz produces, so it rejects the whole catalog.
fn participants(event: &Event, member: PublicKey) -> Result<Vec<String>, &'static str> {
    let mut keys = BTreeSet::new();
    for tag in event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "p"))
    {
        let value = tag.as_slice().get(1).ok_or("catalog_invalid_shape")?;
        let key = PublicKey::from_hex(value).map_err(|_| "catalog_invalid_shape")?;
        if key.to_hex() != *value || !keys.insert(value.clone()) {
            return Err("catalog_invalid_shape");
        }
    }
    if !(2..=9).contains(&keys.len()) || !keys.contains(&member.to_hex()) {
        return Err("catalog_invalid_shape");
    }
    Ok(keys.into_iter().collect())
}

/// Display name from the other participants' profile names, in key order.
/// Missing or blank names fall back to a 12-hex key prefix; at most three names
/// are shown, then `+N`. Profile names are self-asserted hints, never identity.
fn dm_name(member: PublicKey, participants: &[String], names: &BTreeMap<String, String>) -> String {
    let own = member.to_hex();
    let others: Vec<&String> = participants.iter().filter(|p| **p != own).collect();
    let mut parts: Vec<String> = others
        .iter()
        .take(3)
        .map(
            |key| match names.get(*key).filter(|n| !n.trim().is_empty()) {
                Some(name) => name.clone(),
                None => format!("{}…", &key[..12]),
            },
        )
        .collect();
    if others.len() > 3 {
        parts.push(format!("+{}", others.len() - 3));
    }
    // Profile names are already sanitized (recipients.rs); this bounds the total.
    clean(&parts.join(", "), 128)
}

/// Other participants of every DM, the keys whose profiles name DMs. At most
/// `MAX_ROOMS` x 8 others, since `reconcile` bounds both.
fn dm_others(catalog: &Catalog, member: PublicKey) -> BTreeSet<String> {
    let own = member.to_hex();
    catalog
        .rooms
        .iter()
        .filter(|r| r.kind == "dm")
        .flat_map(|r| r.participants.iter().filter(|p| **p != own).cloned())
        .collect()
}

/// Names from one author-scoped kind 0 response. Any bad signature, kind,
/// unrequested author or future timestamp discards the whole batch, and a
/// same-second conflict discards that author, matching recipients.rs.
pub fn profile_names(
    wanted: &BTreeSet<String>,
    profiles: &[Event],
    now: u64,
) -> BTreeMap<String, String> {
    if profiles.len() > 200
        || profiles.iter().any(|e| {
            e.verify().is_err()
                || e.kind.as_u16() != 0
                || e.created_at.as_secs() > now.saturating_add(60)
                || !wanted.contains(&e.pubkey.to_hex())
        })
    {
        return BTreeMap::new();
    }
    let mut latest: BTreeMap<String, (&Event, bool)> = BTreeMap::new();
    for event in profiles {
        let key = event.pubkey.to_hex();
        match latest.get_mut(&key) {
            Some((old, conflict)) if old.created_at == event.created_at => {
                if old.id != event.id {
                    *conflict = true;
                }
            }
            Some((old, _)) if old.created_at > event.created_at => {}
            _ => {
                latest.insert(key, (event, false));
            }
        }
    }
    latest
        .into_iter()
        .filter(|(_, (_, conflict))| !conflict)
        .map(|(key, (event, _))| (key, crate::recipients::name(event)))
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

/// Renames every DM from participant profile names (missing ones use key prefixes).
pub fn apply_names(catalog: &mut Catalog, member: PublicKey, names: &BTreeMap<String, String>) {
    for room in catalog.rooms.iter_mut().filter(|r| r.kind == "dm") {
        room.name = dm_name(member, &room.participants, names);
    }
}

/// Marks viewer-hidden DMs from the NIP-DV snapshot response (`kinds:[30622],
/// #p:[self], limit:1`). No snapshot means nothing is hidden (NIP-DV.md:110).
/// A present snapshot must be signed by the relay identity (NIP-DV.md:43,120) and
/// have exactly the shape of NIP-DV.md:64-85 as published by pinned Buzz
/// (side_effects.rs:3585-3643): empty content, one `["d", self]`, one `["p", self]`,
/// and only `["h", <channel uuid>]` otherwise. Anything else rejects the catalog
/// rather than guessing which DMs the viewer hid. Streams are never affected.
pub fn apply_visibility(
    catalog: &mut Catalog,
    member: PublicKey,
    snapshots: &[Event],
    now: u64,
) -> Result<(), &'static str> {
    let Some(event) = snapshots.first() else {
        return Ok(());
    };
    if snapshots.len() > 1
        || event.verify().is_err()
        || event.pubkey != catalog.signer
        || event.kind.as_u16() != 30622
        || !event.content.is_empty()
        || event.created_at.as_secs() > now.saturating_add(60)
    {
        return Err("catalog_invalid_shape");
    }
    let own = member.to_hex();
    let (mut d, mut p) = (0, 0);
    let mut hidden = BTreeSet::new();
    for tag in event.tags.iter().map(|t| t.as_slice()) {
        let [name, value] = tag else {
            return Err("catalog_invalid_shape");
        };
        match name.as_str() {
            "d" if *value == own => d += 1,
            "p" if *value == own => p += 1,
            "h" => {
                let id = Uuid::parse_str(value).map_err(|_| "catalog_invalid_shape")?;
                if id.to_string() != *value {
                    return Err("catalog_invalid_shape");
                }
                // A set: repeated ids are harmless (NIP-DV.md:85).
                hidden.insert(value.clone());
            }
            _ => return Err("catalog_invalid_shape"),
        }
    }
    if d != 1 || p != 1 {
        return Err("catalog_invalid_shape");
    }
    for room in catalog.rooms.iter_mut().filter(|r| r.kind == "dm") {
        room.hidden = hidden.contains(&room.id);
    }
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
    if memberships.len() > MAX_ROOMS || metadata.len() > MAX_ROOMS {
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
        // Pinned Buzz emits the NIP-29 `hidden` tag only for DMs, as a hint not to
        // list them in public group lists (buzz-relay handlers/side_effects.rs:1215-1219).
        // It is always present on DMs and says nothing about the viewer's hide state
        // (that is NIP-DV, applied in `apply_visibility`). A stream carrying it is not
        // produced upstream and stays excluded, as before.
        let dm = match kind {
            "stream" if !flag("hidden") => false,
            "dm" => true,
            _ => continue,
        };
        if flag("archived") {
            continue;
        }
        let participants = if dm {
            participants(event, member)?
        } else {
            Vec::new()
        };
        // Upstream DM names are literally "DM"/"Group DM (N)" (buzz-db store/dm.rs:160-164);
        // they are never projected. Until profiles are applied, DMs use key prefixes.
        let name = if dm {
            dm_name(member, &participants, &BTreeMap::new())
        } else {
            clean(name, 128)
        };
        if name.trim().is_empty() {
            return Err("catalog_invalid_shape");
        }
        let description = clean(one_tag(event, "about")?.unwrap_or(""), 256);
        rooms.push(Room {
            id: id.to_string(),
            name,
            description,
            kind: if dm { "dm" } else { "stream" },
            participants,
            hidden: false,
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
        clock_skew: None,
        has_more: false,
        next: None,
    })
}

/// Open rooms listed at most (`status.openRooms`).
pub const OPEN_ROOMS: usize = 50;

/// Open stream rooms from an unscoped kind 39000 response
/// (`channel_members.rs` `get_accessible_channel_ids`: member channels plus
/// every `visibility = open` channel). Pinned Buzz tags each 39000 with exactly
/// one of `["public"]` (visibility open) or `["private"]`
/// (`side_effects.rs` `emit_group_discovery_events`; the `closed` tag is on
/// every channel and says nothing about joining). Every event is validated as
/// in `reconcile` against the pinned signer; any bad event, repeated room with
/// another event, or a visibility tag that is missing, doubled or carries a
/// value rejects the whole response. Kept: `public`, `t` = `stream`, not
/// `hidden`, not `archived`, and not in `joined`; by name, at most 50.
pub fn open_rooms(
    signer: PublicKey,
    events: &[Event],
    joined: &BTreeSet<String>,
    now: u64,
) -> Result<Vec<crate::protocol::OpenRoom>, &'static str> {
    if events.len() > 200 {
        return Err("catalog_oversized");
    }
    let mut seen = BTreeMap::new();
    for event in events {
        let id = validated(event, signer, 39000, now)?;
        insert(&mut seen, id, event)?;
    }
    let mut rooms = Vec::new();
    for (id, event) in seen {
        let flags = |name: &str| {
            event
                .tags
                .iter()
                .filter(|t| t.as_slice().first().is_some_and(|v| v == name))
                .map(|t| t.as_slice().len())
                .collect::<Vec<_>>()
        };
        let (public, private) = (flags("public"), flags("private"));
        let open = match (public.as_slice(), private.as_slice()) {
            ([1], []) => true,
            ([], [1]) => false,
            _ => return Err("catalog_invalid_shape"),
        };
        let name = one_tag(event, "name")?.ok_or("catalog_invalid_shape")?;
        let kind = one_tag(event, "t")?.ok_or("catalog_invalid_shape")?;
        if kind.is_empty()
            || kind.len() > 32
            || !kind.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err("catalog_invalid_shape");
        }
        let about = one_tag(event, "about")?.unwrap_or("");
        if !open
            || kind != "stream"
            || !flags("hidden").is_empty()
            || !flags("archived").is_empty()
            || joined.contains(&id.to_string())
        {
            continue;
        }
        let name = clean(name, 128);
        if name.trim().is_empty() {
            return Err("catalog_invalid_shape");
        }
        rooms.push(crate::protocol::OpenRoom {
            id: id.to_string(),
            name,
            description: clean(about, 256),
            kind: "stream".into(),
        });
    }
    rooms.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    rooms.truncate(OPEN_ROOMS);
    Ok(rooms)
}

/// One authenticated unscoped 39000 read for open rooms, checked against the
/// pinned relay signer. `joined` is the published catalog's room IDs.
pub async fn discover_open(
    relay: &str,
    keys: &Keys,
    pin: PublicKey,
    joined: BTreeSet<String>,
) -> Result<Vec<crate::protocol::OpenRoom>, &'static str> {
    let events = query(relay, keys, &QueryRequest::OpenRooms).await?;
    open_rooms(pin, &events, &joined, Timestamp::now().as_secs())
}

/// The first page only, with no earlier rooms to confirm (fixtures).
#[cfg(test)]
pub async fn discover(
    relay: &str,
    keys: &Keys,
    pin: Option<PublicKey>,
) -> Result<Catalog, &'static str> {
    discover_pages(relay, keys, pin, 1, &[]).await
}

/// A page must hold only snapshots at or before the cursor it continues from.
fn check_page(page: &[Event], before: Option<Cursor>) -> Result<(), &'static str> {
    if page.len() > PAGE as usize
        || before.is_some_and(|(at, _)| page.iter().any(|e| e.created_at.as_secs() > at))
    {
        return Err("catalog_invalid_shape");
    }
    Ok(())
}

/// One snapshot per room: the newer of two (a room's roster changed between
/// pages). Events without a readable room id stay for `reconcile` to reject.
fn newest(events: Vec<Event>) -> Vec<Event> {
    let mut by_room: BTreeMap<Uuid, Event> = BTreeMap::new();
    let mut rest = Vec::new();
    for event in events {
        match room_id(&event) {
            Ok(id) => match by_room.get(&id) {
                Some(old) if (old.created_at, old.id) >= (event.created_at, event.id) => {}
                _ => {
                    by_room.insert(id, event);
                }
            },
            Err(_) => rest.push(event),
        }
    }
    by_room.into_values().chain(rest).collect()
}

/// Reads per request; one page of memberships needs one metadata read.
const METADATA_BATCH: usize = PAGE as usize;

/// Metadata, DM visibility and names for already authorized memberships.
async fn build(
    relay: &str,
    keys: &Keys,
    signer: PublicKey,
    memberships: &[Event],
) -> Result<Catalog, &'static str> {
    // Validate authorship before letting returned IDs shape further requests.
    reconcile(
        keys.public_key(),
        signer,
        memberships,
        &[],
        Timestamp::now().as_secs(),
    )?;
    let ids: Vec<Uuid> = memberships.iter().map(room_id).collect::<Result<_, _>>()?;
    let mut metadata = Vec::new();
    for batch in ids.chunks(METADATA_BATCH) {
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
        memberships,
        &metadata,
        Timestamp::now().as_secs(),
    )?;
    if result.rooms.iter().any(|r| r.kind == "dm") {
        let snapshots = query(relay, keys, &QueryRequest::DmVisibility).await?;
        apply_visibility(
            &mut result,
            keys.public_key(),
            &snapshots,
            Timestamp::now().as_secs(),
        )?;
        let names = dm_profile_names(relay, keys, dm_others(&result, keys.public_key())).await;
        apply_names(&mut result, keys.public_key(), &names);
    }
    result.trust = if relay.starts_with("ws:") {
        "loopback_development"
    } else {
        "tls_origin"
    };
    Ok(result)
}

/// The relay-signed rosters of rooms this identity may no longer be in. A
/// refusal ("not a member") is an answer: a batch that is refused is asked
/// again room by room, and a refused room is simply not confirmed.
async fn confirm_rosters(
    relay: &str,
    keys: &Keys,
    rooms: &[Uuid],
) -> Result<Vec<Event>, &'static str> {
    let ask = |rooms: Vec<Uuid>| async move {
        query(relay, keys, &QueryRequest::RoomRosters { rooms }).await
    };
    match ask(rooms.to_vec()).await {
        Err("query_access_denied") if rooms.len() > 1 => {
            let mut events = Vec::new();
            for room in rooms {
                match ask(vec![*room]).await {
                    Ok(found) => events.extend(found),
                    Err("query_access_denied") => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(events)
        }
        Err("query_access_denied") => Ok(Vec::new()),
        other => other,
    }
}

/// The first `pages` pages of joined rooms. `known` are rooms the panel already
/// lists: a membership snapshot moves to the front when its roster changes, so
/// a listed room can fall past the last page read without having been left.
/// Each one missing from the pages is confirmed by its own roster read, and
/// kept only when that relay-signed roster still names this identity.
pub async fn discover_pages(
    relay: &str,
    keys: &Keys,
    pin: Option<PublicKey>,
    pages: usize,
    known: &[String],
) -> Result<Catalog, &'static str> {
    let _permit = DISCOVERY.try_acquire().map_err(|_| "discovery_busy")?;
    let (signer, clock_skew) = relay_info(relay, pin).await;
    let signer = signer?;
    let mut memberships = Vec::new();
    let (mut before, mut next, mut has_more) = (None, None, false);
    for _ in 0..pages.clamp(1, MAX_PAGES) {
        let page = query(
            relay,
            keys,
            &QueryRequest::JoinedRooms {
                limit: PAGE,
                before,
            },
        )
        .await?;
        check_page(&page, before)?;
        has_more = page.len() == PAGE as usize;
        next = if has_more {
            page.last().map(|e| (e.created_at.as_secs(), e.id))
        } else {
            None
        };
        memberships.extend(page);
        if !has_more {
            break;
        }
        before = next;
    }
    let mut memberships = newest(memberships);
    let held: BTreeSet<String> = memberships
        .iter()
        .filter_map(|e| room_id(e).ok())
        .map(|id| id.to_string())
        .collect();
    let missing: Vec<Uuid> = known
        .iter()
        .filter(|id| !held.contains(*id))
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();
    let me = keys.public_key().to_hex();
    for batch in missing.chunks(METADATA_BATCH) {
        let rosters = confirm_rosters(relay, keys, batch).await?;
        memberships.extend(rosters.into_iter().filter(|e| {
            e.tags.iter().any(|t| {
                t.as_slice().first().is_some_and(|v| v == "p")
                    && t.as_slice().get(1).is_some_and(|v| *v == me)
            })
        }));
    }
    let mut result = build(relay, keys, signer, &newest(memberships)).await?;
    result.clock_skew = clock_skew;
    result.has_more = has_more;
    result.next = next;
    Ok(result)
}

/// The page after `before`, for "Load more": its rooms only; the caller merges.
pub async fn discover_more(
    relay: &str,
    keys: &Keys,
    pin: PublicKey,
    before: Cursor,
) -> Result<Catalog, &'static str> {
    let _permit = DISCOVERY.try_acquire().map_err(|_| "discovery_busy")?;
    let signer = relay_info(relay, Some(pin)).await.0?;
    let page = query(
        relay,
        keys,
        &QueryRequest::JoinedRooms {
            limit: PAGE,
            before: Some(before),
        },
    )
    .await?;
    check_page(&page, Some(before))?;
    let has_more = page.len() == PAGE as usize;
    let next = if has_more {
        page.last().map(|e| (e.created_at.as_secs(), e.id))
    } else {
        None
    };
    let mut result = build(relay, keys, signer, &newest(page)).await?;
    result.has_more = has_more;
    result.next = next;
    Ok(result)
}

/// Best-effort profile lookup for DM names, in author batches of 20 within a
/// short budget so the catalog refresh stays inside its caller's timeout. A busy
/// read slot is retried briefly; any other failure leaves key prefixes.
pub(crate) async fn dm_profile_names(
    relay: &str,
    keys: &Keys,
    wanted: BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    let authors: Vec<PublicKey> = wanted
        .iter()
        .filter_map(|k| PublicKey::from_hex(k).ok())
        .collect();
    let lookup = async {
        for batch in authors.chunks(20) {
            let request = QueryRequest::Profiles {
                authors: batch.to_vec(),
            };
            let mut attempts = 0;
            let events = loop {
                match query(relay, keys, &request).await {
                    Err("query_busy") if attempts < 3 => {
                        attempts += 1;
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    other => break other,
                }
            };
            if let Ok(events) = events {
                let scope = batch.iter().map(PublicKey::to_hex).collect();
                names.extend(profile_names(&scope, &events, Timestamp::now().as_secs()));
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(4), lookup).await;
    names
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
