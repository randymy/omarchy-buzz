//! Communities (`communities`): join, switch, rename and leave, like Buzz
//! Desktop's account menu (`AddCommunityDialog.tsx`, `CommunitySwitcher.tsx`).
//!
//! A community is a relay; the human identity is one for all of them (Desktop's
//! `Community.pubkey` is display only, `types.ts:6-10`). Joining reuses the
//! invite grammar and HTTP contract of `join.rs` against the named relay with
//! the same key; switching saves the active relay, after which the connection
//! actor applies it with the usual generation bump and full status reset
//! (`auth::apply_loaded_config`). Leaving publishes Desktop's NIP-43 leave
//! request (`leaveCommunity.ts:55-88`: kind 28936, empty content, tags
//! `[["-"]]`) when the relay lists NIP 43, then removes the entry. The identity
//! and its Secret Service entries are never deleted, and the last community
//! cannot be left.
use crate::{
    config::{self, Community, Config},
    protocol::{CommunitiesView, CommunityEntry, JoinPolicy},
    setup::{blocking, Setup},
};
use buzz_ws_client::{NostrWsConnection, WsClientError};
use nostr::Keys;
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::time::timeout;
use zeroize::Zeroizing;

/// Fixed categories returned to the panel for community requests.
#[cfg(test)]
pub const CATEGORIES: [&str; 14] = [
    "join_invalid",
    "join_rejected",
    "join_rate_limited",
    "join_busy",
    "join_last",
    "join_full",
    "policy_required",
    "relay_unavailable",
    "identity_unavailable",
    "config_unavailable",
    "community_unknown",
    "name_invalid",
    "leave_rejected",
    "leave_owner",
];
/// One join at a time, and at least this long between two.
pub const JOIN_GAP: Duration = Duration::from_secs(5);
/// Desktop's kind for a NIP-43 leave request (`KIND_NIP43_LEAVE_REQUEST`).
pub const LEAVE_KIND: u16 = 28936;
const INPUT_BYTES: usize = 4096;
const CONNECT: Duration = Duration::from_secs(20);
const ANSWER: Duration = Duration::from_secs(15);
const HINTS: usize = 64;

/// What the pasted text names.
#[derive(Debug, PartialEq)]
pub enum Target {
    /// A community URL (canonical relay).
    Relay(String),
    /// An invite: its relay (canonical) when the link names one, and its code.
    Invite { relay: Option<String>, code: String },
}

/// `host[:port]` typed without a scheme (Desktop's `normalizeRelayUrl` assumes
/// `wss://`). A v2 invite code (`v2.<base64url>`) is never a host.
fn looks_like_host(value: &str) -> bool {
    if value.starts_with("v2.") {
        return false;
    }
    let (host, port) = match value.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (value, None),
    };
    if port.is_some_and(|p| p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    if host == "localhost" {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2
        && host.len() <= 253
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// Desktop's add-community parsing (`InviteRedeemForm.tsx:94-101`): an invite
/// link (`https://<relay>/invite/<code>`, `buzz://join?relay=…&code=…`) names
/// its relay; otherwise a community URL (`wss://`, `https://` → `wss://`,
/// `http://` → `ws://`, a bare host → `wss://`; trailing slashes dropped) is
/// the relay itself; anything else that is a well-formed code is a bare code.
/// Every relay is canonical (`config::canonical_relay`): no login, path,
/// query or fragment, `ws://` only for this computer.
pub fn parse_input(input: &str) -> Result<Target, &'static str> {
    const INVALID: &str = "join_invalid";
    let text = input.trim();
    if text.is_empty() || text.len() > INPUT_BYTES || text.chars().any(char::is_control) {
        return Err(INVALID);
    }
    if let Ok(invite) = crate::join::parse_invite(text) {
        match invite.relay {
            Some(relay) => {
                let relay = config::canonical_relay(&relay).map_err(|_| INVALID)?;
                return Ok(Target::Invite {
                    relay: Some(relay),
                    code: invite.code,
                });
            }
            None if !looks_like_host(text) => {
                return Ok(Target::Invite {
                    relay: None,
                    code: invite.code,
                })
            }
            None => {}
        }
    }
    let lower = text.to_ascii_lowercase();
    let candidate = if lower.starts_with("wss://") || lower.starts_with("ws://") {
        text.to_owned()
    } else if lower.starts_with("https://") {
        format!("wss://{}", &text[8..])
    } else if lower.starts_with("http://") {
        format!("ws://{}", &text[7..])
    } else if !text.contains("://") && looks_like_host(text) {
        format!("wss://{text}")
    } else {
        return Err(INVALID);
    };
    config::canonical_relay(candidate.trim_end_matches('/'))
        .map(Target::Relay)
        .map_err(|_| INVALID)
}

// NIP-11 `name` hints: untrusted, sanitized, bounded, display only; cached for
// this helper process (never written to the configuration, never a label).
static HINT_CACHE: Mutex<BTreeMap<String, Option<String>>> = Mutex::new(BTreeMap::new());
pub fn remember_hint(relay: &str, name: Option<String>) {
    if let Ok(mut cache) = HINT_CACHE.lock() {
        if cache.len() >= HINTS && !cache.contains_key(relay) {
            return;
        }
        cache.insert(relay.to_owned(), name);
    }
}
fn hint(relay: &str) -> Option<String> {
    HINT_CACHE.lock().ok()?.get(relay).cloned().flatten()
}
/// Relays of `c` whose hint was never read in this process.
pub fn unhinted(c: &Config) -> Vec<String> {
    let cache = HINT_CACHE.lock().map(|c| c.clone()).unwrap_or_default();
    c.communities
        .iter()
        .map(|e| e.relay.clone())
        .filter(|r| !cache.contains_key(r))
        .collect()
}
/// Reads the NIP-11 name of each relay (one at a time, bounded), caching even
/// a failure so an unreachable relay is not asked again.
pub async fn fetch_hints(relays: Vec<String>) {
    for relay in relays.into_iter().take(config::COMMUNITIES) {
        let name = timeout(
            Duration::from_secs(12),
            crate::catalog::relay_profile(&relay),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|p| p.name);
        remember_hint(&relay, name);
    }
}
/// Fills cached hints into a published view.
pub fn refresh_hints(view: &mut CommunitiesView) {
    for entry in view.entries.iter_mut() {
        entry.hint = hint(&entry.relay);
    }
}

/// The `status.communities` view of a configuration.
pub fn view(c: &Config) -> CommunitiesView {
    let c = c.clone().normalized();
    CommunitiesView {
        state: "ready".into(),
        active: c.relay.clone(),
        entries: c
            .communities
            .iter()
            .map(|e| CommunityEntry {
                relay: e.relay.clone(),
                name: e.name.clone(),
                host: config::host(&e.relay),
                active: c.relay.as_deref() == Some(e.relay.as_str()),
                hint: hint(&e.relay),
            })
            .collect(),
        category: None,
        pending_invite: false,
        notice: None,
    }
}
pub fn busy(view: &CommunitiesView) -> bool {
    matches!(
        view.state.as_str(),
        "joining" | "switching" | "renaming" | "leaving"
    )
}

/// A request handed to `perform`.
pub enum Request {
    Join(String),
    Switch(String),
    Rename(String, String),
    Leave(String),
}
impl Request {
    pub fn state(&self) -> &'static str {
        match self {
            Request::Join(_) => "joining",
            Request::Switch(_) => "switching",
            Request::Rename(..) => "renaming",
            Request::Leave(_) => "leaving",
        }
    }
}
/// A completed request. `switched`: the active relay changed (reconnect with a
/// new generation). `claimed`: membership was granted on the unchanged active
/// relay (re-check its rooms, or reconnect if disconnected).
#[derive(Debug, Default)]
pub struct Done {
    pub switched: bool,
    pub claimed: bool,
    /// An invite whose relay has terms: published as `status.setup` (`policy`)
    /// after the switch, for `accept_invite`.
    pub policy: Option<(String, JoinPolicy)>,
    /// First setup (no identity yet): the input carried an invite code that
    /// the panel redeems once the identity exists.
    pub pending_invite: bool,
    /// `already_absent` when the relay said this identity was no longer a member.
    pub notice: Option<&'static str>,
}

pub async fn perform(
    setup: &Setup,
    request: Request,
    keys: Option<&Keys>,
) -> Result<Done, &'static str> {
    match request {
        Request::Join(input) => {
            gate(setup)?;
            join(setup, &input, keys).await
        }
        Request::Switch(relay) => switch(setup, &relay, keys).await,
        Request::Rename(relay, name) => rename(setup, &relay, &name).await,
        Request::Leave(relay) => {
            let keys = keys.ok_or("relay_unavailable")?;
            leave(setup, &relay, keys).await
        }
    }
}

/// One join at a time (the caller holds it), at least `JOIN_GAP` apart.
fn gate(setup: &Setup) -> Result<(), &'static str> {
    let mut last = setup.joins.lock().map_err(|_| "join_busy")?;
    let now = Instant::now();
    if last.is_some_and(|at| now.duration_since(at) < JOIN_GAP) {
        return Err("join_rate_limited");
    }
    *last = Some(now);
    Ok(())
}

fn load(setup: &Setup) -> Result<Config, &'static str> {
    setup.load().map_err(|_| "config_unavailable")
}

/// The claim's categories in community wording.
fn claim_category(error: &'static str) -> &'static str {
    match error {
        "invite_invalid" => "join_invalid",
        "invite_rejected" | "invite_relay_mismatch" => "join_rejected",
        "invite_rate_limited" => "join_rate_limited",
        "policy_required" => "policy_required",
        _ => "relay_unavailable",
    }
}

async fn join(setup: &Setup, input: &str, keys: Option<&Keys>) -> Result<Done, &'static str> {
    let target = parse_input(input)?;
    let original = load(setup)?;
    let (relay, code) = match target {
        Target::Relay(relay) => (relay, None),
        Target::Invite {
            relay: Some(relay),
            code,
        } => (relay, Some(code)),
        // A bare code is for the active community.
        Target::Invite { relay: None, code } => {
            (original.relay.clone().ok_or("join_invalid")?, Some(code))
        }
    };
    // A community already listed, without an invite, is a switch to it
    // (Desktop dedups by relay URL).
    if code.is_none() && keys.is_some() && original.community(&relay).is_some() {
        return switch(setup, &relay, keys).await;
    }
    let profile = crate::catalog::relay_profile(&relay)
        .await
        .map_err(|_| "relay_unavailable")?;
    remember_hint(&relay, profile.name.clone());
    let Some(keys) = keys else {
        // First setup: no identity exists yet, so nothing can be claimed. The
        // relay becomes the only community; the panel creates the identity
        // next and redeems the invite (if any) after that.
        if original.identity.is_some() {
            return Err("identity_unavailable");
        }
        let dir = setup.dir()?;
        let chosen = relay.clone();
        let switched = blocking(move || {
            let current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
            if current != original {
                return Err("join_busy");
            }
            config::save_to(&dir, &config::with_relay(current.clone(), chosen.clone()))
                .map_err(|_| "config_unavailable")?;
            Ok(current.relay.as_deref() != Some(chosen.as_str()))
        })
        .await?;
        return Ok(Done {
            switched,
            pending_invite: code.is_some(),
            ..Done::default()
        });
    };
    if original.identity.as_deref() != Some(keys.public_key().to_hex().as_str()) {
        return Err("join_busy");
    }
    // The relay's own signing key can never be a member's identity.
    if profile.signer == keys.public_key() {
        return Err("identity_unavailable");
    }
    let mut done = Done::default();
    match code {
        None => {
            if original.community(&relay).is_none() {
                probe(&relay, keys).await?;
            }
        }
        Some(code) => match crate::join::fetch_policy(&relay)
            .await
            .map_err(claim_category)?
        {
            None => {
                crate::join::claim(&relay, keys, &code, None)
                    .await
                    .map_err(claim_category)?;
                done.claimed = true;
            }
            // Terms are shown and accepted in the community itself.
            Some(policy) => done.policy = Some((code, policy)),
        },
    }
    done.switched = activate(setup, original, &relay, keys).await?;
    if done.switched {
        done.claimed = false;
    }
    Ok(done)
}

/// A community URL without an invite: the relay must let this identity in
/// (NIP-42). A relay requiring membership refuses a non-member
/// (`handlers/auth.rs:272-297`); that refusal is `join_rejected`.
async fn probe(relay: &str, keys: &Keys) -> Result<(), &'static str> {
    match timeout(
        CONNECT,
        NostrWsConnection::connect_authenticated_with_options(
            relay,
            keys,
            None,
            crate::auth::connection_options(),
        ),
    )
    .await
    {
        Ok(Ok(connection)) => {
            let _ = timeout(Duration::from_secs(3), connection.disconnect()).await;
            Ok(())
        }
        Ok(Err(WsClientError::AuthFailed(_))) => Err("join_rejected"),
        _ => Err("relay_unavailable"),
    }
}

/// Adds `relay` (if new) and makes it active, after storing the identity's
/// secret under its `relay|identity` account. Store before saving: a failed
/// Secret Service write never leaves a community without its key.
async fn activate(
    setup: &Setup,
    original: Config,
    relay: &str,
    keys: &Keys,
) -> Result<bool, &'static str> {
    let dir = setup.dir()?;
    let secrets = setup.secrets.clone();
    let secret = Zeroizing::new(keys.secret_key().to_secret_hex());
    let relay = relay.to_owned();
    blocking(move || {
        let current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
        if current != original {
            return Err("join_busy");
        }
        let mut next = current.clone();
        if next.community(&relay).is_none() {
            if next.communities.len() >= config::COMMUNITIES {
                return Err("join_full");
            }
            next.communities.push(Community {
                relay: relay.clone(),
                name: config::derive_name(&relay),
                joined_at: config::now(),
            });
        }
        next.relay = Some(relay);
        let account = config::account(&next).map_err(|_| "config_unavailable")?;
        secrets
            .store(&account, &secret)
            .map_err(|_| "identity_unavailable")?;
        config::save_to(&dir, &next).map_err(|_| "config_unavailable")?;
        Ok(current.relay != next.relay)
    })
    .await
}

fn known(c: &Config, relay: &str) -> Result<String, &'static str> {
    let relay = config::canonical_relay(relay).map_err(|_| "community_unknown")?;
    c.community(&relay).ok_or("community_unknown")?;
    Ok(relay)
}

async fn switch(setup: &Setup, relay: &str, keys: Option<&Keys>) -> Result<Done, &'static str> {
    let original = load(setup)?;
    let relay = known(&original, relay)?;
    if original.relay.as_deref() == Some(relay.as_str()) {
        return Ok(Done::default());
    }
    if let Some(keys) = keys {
        if original.identity.as_deref() != Some(keys.public_key().to_hex().as_str()) {
            return Err("join_busy");
        }
        return Ok(Done {
            switched: activate(setup, original, &relay, keys).await?,
            ..Done::default()
        });
    }
    // No key loaded (locked or missing): only the choice is saved; the key is
    // read from that community's own Secret Service entry when connecting.
    let dir = setup.dir()?;
    blocking(move || {
        let current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
        if current != original {
            return Err("join_busy");
        }
        let mut next = current;
        next.relay = Some(relay);
        config::save_to(&dir, &next).map_err(|_| "config_unavailable")
    })
    .await?;
    Ok(Done {
        switched: true,
        ..Done::default()
    })
}

async fn rename(setup: &Setup, relay: &str, name: &str) -> Result<Done, &'static str> {
    let original = load(setup)?;
    let relay = known(&original, relay)?;
    let name = config::label(name);
    if name.is_empty() {
        return Err("name_invalid");
    }
    let dir = setup.dir()?;
    blocking(move || {
        let mut current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
        if current != original {
            return Err("join_busy");
        }
        for entry in current.communities.iter_mut() {
            if entry.relay == relay {
                entry.name = name.clone();
            }
        }
        config::save_to(&dir, &current).map_err(|_| "config_unavailable")
    })
    .await?;
    Ok(Done::default())
}

/// The relay's refusal texts Desktop recognizes (`leaveCommunity.ts:31-36`)
/// and the pinned relay's owner refusal (`ingest.rs` NIP-43 leave).
fn absent(message: &str) -> bool {
    message.to_ascii_lowercase().contains("not a relay member")
}

/// Kind 28936, empty content, tags `[["-"]]` (NIP-70 protected), signed by
/// this identity: Desktop's `leaveCommunity` event.
pub fn leave_event(keys: &Keys) -> Result<nostr::Event, &'static str> {
    nostr::EventBuilder::new(nostr::Kind::Custom(LEAVE_KIND), "")
        .tag(nostr::Tag::protected())
        .sign_with_keys(keys)
        .map_err(|_| "relay_unavailable")
}

/// Publishes the leave request on its own authenticated connection and waits
/// for the relay's `OK` for that exact event. `Ok(None)`: left;
/// `Ok(Some("already_absent"))`: the relay no longer had this member.
async fn publish_leave(relay: &str, keys: &Keys) -> Result<Option<&'static str>, &'static str> {
    let event = leave_event(keys)?;
    let mut connection = match timeout(
        CONNECT,
        NostrWsConnection::connect_authenticated_with_options(
            relay,
            keys,
            None,
            crate::auth::connection_options(),
        ),
    )
    .await
    {
        Ok(Ok(connection)) => connection,
        // A relay requiring membership refuses a non-member's NIP-42 with
        // "restricted: not a relay member" (`handlers/auth.rs:272-297`).
        Ok(Err(WsClientError::AuthFailed(message))) if absent(&message) => {
            return Ok(Some("already_absent"))
        }
        Ok(Err(WsClientError::AuthFailed(_))) => return Err("leave_rejected"),
        _ => return Err("relay_unavailable"),
    };
    let answer = timeout(ANSWER, connection.send_event(event)).await;
    let _ = timeout(Duration::from_secs(3), connection.disconnect()).await;
    match answer {
        Ok(Ok(ok)) if ok.accepted => Ok(None),
        Ok(Ok(ok)) if absent(&ok.message) => Ok(Some("already_absent")),
        Ok(Ok(ok))
            if ok
                .message
                .to_ascii_lowercase()
                .contains("owner cannot leave") =>
        {
            Err("leave_owner")
        }
        Ok(Ok(_)) => Err("leave_rejected"),
        // No answer: the relay may have applied it. The entry stays; leaving
        // again reads "not a relay member" if it did.
        _ => Err("relay_unavailable"),
    }
}

async fn leave(setup: &Setup, relay: &str, keys: &Keys) -> Result<Done, &'static str> {
    let original = load(setup)?;
    let relay = known(&original, relay)?;
    if original.communities.len() <= 1 {
        return Err("join_last");
    }
    if original.identity.as_deref() != Some(keys.public_key().to_hex().as_str()) {
        return Err("join_busy");
    }
    // Desktop asks only a relay that requires membership (NIP 43 listed).
    let profile = crate::catalog::relay_profile(&relay)
        .await
        .map_err(|_| "relay_unavailable")?;
    remember_hint(&relay, profile.name.clone());
    let notice = if profile.membership {
        publish_leave(&relay, keys).await?
    } else {
        None
    };
    let dir = setup.dir()?;
    let secrets = setup.secrets.clone();
    let secret = Zeroizing::new(keys.secret_key().to_secret_hex());
    let switched = blocking(move || {
        let current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
        if current != original {
            return Err("join_busy");
        }
        let mut next = current.clone();
        let index = next
            .communities
            .iter()
            .position(|c| c.relay == relay)
            .ok_or("community_unknown")?;
        next.communities.remove(index);
        if next.relay.as_deref() == Some(relay.as_str()) {
            // The next one in the list, else the one before it.
            let chosen = next.communities[index.min(next.communities.len() - 1)]
                .relay
                .clone();
            next.relay = Some(chosen);
            let account = config::account(&next).map_err(|_| "config_unavailable")?;
            secrets
                .store(&account, &secret)
                .map_err(|_| "identity_unavailable")?;
        }
        config::save_to(&dir, &next).map_err(|_| "config_unavailable")?;
        Ok(current.relay != next.relay)
    })
    .await?;
    Ok(Done {
        switched,
        notice,
        ..Done::default()
    })
}

#[cfg(test)]
#[path = "communities_tests.rs"]
mod tests;
