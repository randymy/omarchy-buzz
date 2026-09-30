//! Agent enrollment publications, built as the pinned Buzz Desktop does:
//!
//! - NIP-OA attestation: `buzz_sdk::nip_oa::compute_auth_tag(owner, agent, "")`
//!   (Desktop `commands/agents.rs:432-443`, empty conditions).
//! - kind 30175 persona, owner-signed, `["d", <persona id>]`, no `shared` tag
//!   (author-only read), content fields in the NIP-AP order
//!   (`managed_agents/persona_events.rs`, `docs/nips/NIP-AP.md`).
//! - kind 30177 managed agent, owner-signed, `["d", <agent pubkey>]`, slim
//!   definition-linked projection: never the secret key, the NIP-OA tag, the
//!   environment or runtime fields (`managed_agents/agent_events.rs`,
//!   `buzz-core/src/kind.rs:284-291`).
//! - kind 9000 add-member per room, owner-signed, `h`, `p`, `role=bot`
//!   (`buzz_sdk::build_add_member`, as `buzz-cli` and Desktop's
//!   `attachManagedAgentToChannel` add agents).
//! - kind 0 agent profile, agent-signed with the `auth` tag
//!   (Desktop `relay.rs:504-536`), on a connection whose AUTH carries it.
//!
//! Each event is written on a short-lived authenticated connection and counts
//! only after the relay's `OK` for its exact id. No subscription is opened.
use super::store::Persona;
use buzz_ws_client::{NostrWsConnection, RelayMessage, WsClientError};
use nostr::{Event, EventBuilder, Keys, Kind, PublicKey, Tag, Timestamp};
use serde::Serialize;
use tokio::time::{timeout, Duration, Instant};

pub const KIND_PERSONA: u16 = 30175;
pub const KIND_MANAGED_AGENT: u16 = 30177;
pub const OK_TIMEOUT: Duration = Duration::from_secs(15);
/// Matches `scripts/room-agent` (`--agents 1`).
const PARALLELISM: u32 = 1;

pub fn attestation(owner: &Keys, agent: &PublicKey) -> Result<String, &'static str> {
    buzz_sdk::nip_oa::compute_auth_tag(owner, agent, "").map_err(|_| "enroll_failed")
}

/// The contract's `respondTo` as the NIP-AP / `buzz-acp` wire value.
pub fn wire_respond_to(value: &str) -> &'static str {
    match value {
        "mentions" => "anyone",
        _ => "owner-only",
    }
}

#[derive(Serialize)]
struct PersonaContent<'a> {
    display_name: &'a str,
    system_prompt: &'a str,
    acp_command: &'a str,
    runtime: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    respond_to: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

#[derive(Serialize)]
struct ManagedAgentContent<'a> {
    name: &'a str,
    persona_id: &'a str,
    parallelism: u32,
    respond_to: &'a str,
}

fn sign(builder: EventBuilder, keys: &Keys, at: Timestamp) -> Result<Event, &'static str> {
    builder
        .custom_created_at(at)
        .sign_with_keys(keys)
        .map_err(|_| "enroll_failed")
}

pub fn persona_event(
    persona: &Persona,
    owner: &Keys,
    at: Timestamp,
) -> Result<Event, &'static str> {
    let content = serde_json::to_string(&PersonaContent {
        display_name: &persona.name,
        system_prompt: &persona.instructions,
        acp_command: &persona.acp_command,
        runtime: &persona.harness,
        model: Some(persona.model.as_str()).filter(|m| !m.is_empty()),
        respond_to: wire_respond_to(&persona.respond_to),
        description: Some(persona.description.as_str()).filter(|d| !d.is_empty()),
    })
    .map_err(|_| "enroll_failed")?;
    let d = Tag::parse(["d", persona.id.as_str()]).map_err(|_| "enroll_failed")?;
    sign(
        EventBuilder::new(Kind::Custom(KIND_PERSONA), content).tags([d]),
        owner,
        at,
    )
}

pub fn managed_agent_event(
    persona: &Persona,
    agent: &PublicKey,
    owner: &Keys,
    at: Timestamp,
) -> Result<Event, &'static str> {
    let content = serde_json::to_string(&ManagedAgentContent {
        name: &persona.name,
        persona_id: &persona.id,
        parallelism: PARALLELISM,
        respond_to: wire_respond_to(&persona.respond_to),
    })
    .map_err(|_| "enroll_failed")?;
    let d = Tag::parse(["d", agent.to_hex().as_str()]).map_err(|_| "enroll_failed")?;
    sign(
        EventBuilder::new(Kind::Custom(KIND_MANAGED_AGENT), content).tags([d]),
        owner,
        at,
    )
}

pub fn add_member_event(
    room: &str,
    agent: &PublicKey,
    owner: &Keys,
    at: Timestamp,
) -> Result<Event, &'static str> {
    let room = uuid::Uuid::parse_str(room).map_err(|_| "enroll_failed")?;
    let builder =
        buzz_sdk::build_add_member(room, &agent.to_hex(), Some(buzz_sdk::MemberRole::Bot))
            .map_err(|_| "enroll_failed")?;
    sign(builder, owner, at)
}

pub fn profile_event(
    persona: &Persona,
    agent: &Keys,
    auth_tag: &Tag,
    at: Timestamp,
) -> Result<Event, &'static str> {
    let about = Some(persona.description.as_str()).filter(|d| !d.is_empty());
    let builder = buzz_sdk::build_profile(Some(&persona.name), None, None, about, None)
        .map_err(|_| "enroll_failed")?
        .tags([auth_tag.clone()]);
    sign(builder, agent, at)
}

/// Relay failures as fixed categories; relay text is never returned.
fn connect_category(category: &'static str) -> &'static str {
    match category {
        "auth_rejected" => "enroll_failed",
        _ => "relay_unavailable",
    }
}

/// Writes one EVENT and waits for the `OK` with its exact id. A rejection is
/// `enroll_failed`; no answer, a closed socket or a new AUTH challenge is
/// `relay_unavailable` (the outcome is unknown).
pub async fn publish(conn: &mut NostrWsConnection, event: &Event) -> Result<(), &'static str> {
    let id = event.id.to_hex();
    let until = Instant::now() + OK_TIMEOUT;
    timeout(
        OK_TIMEOUT,
        conn.send_raw(&serde_json::json!(["EVENT", event])),
    )
    .await
    .map_err(|_| "relay_unavailable")?
    .map_err(|_| "relay_unavailable")?;
    loop {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("relay_unavailable");
        }
        match conn.next_event(remaining).await {
            Ok(RelayMessage::Ok(ok)) if ok.event_id == id => {
                return if ok.accepted {
                    Ok(())
                } else {
                    Err("enroll_failed")
                };
            }
            Ok(RelayMessage::Auth { .. }) => return Err("relay_unavailable"),
            Ok(_) | Err(WsClientError::Timeout) => {}
            Err(_) => return Err("relay_unavailable"),
        }
    }
}

/// What reached the relay, reported even when a later step failed.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Rooms whose add-member command was acknowledged (in addition to
    /// those already acknowledged before).
    pub member_rooms: Vec<String>,
    /// `created_at` used for the replaceable records, once both were accepted.
    pub published_at: Option<u64>,
    pub error: Option<&'static str>,
}

/// NIP-AP monotonic `created_at`: `max(now, previous + 1)`.
pub fn next_timestamp(previous: u64) -> Timestamp {
    Timestamp::from(Timestamp::now().as_secs().max(previous.saturating_add(1)))
}

pub async fn publish_all(relay: &str, owner: &Keys, agent: &Keys, persona: &Persona) -> Report {
    let mut report = Report::default();
    if let Err(category) = publish_steps(relay, owner, agent, persona, &mut report).await {
        report.error = Some(category);
    }
    report
}

async fn publish_steps(
    relay: &str,
    owner: &Keys,
    agent: &Keys,
    persona: &Persona,
    report: &mut Report,
) -> Result<(), &'static str> {
    let tag_json = persona.auth_tag.as_deref().ok_or("enroll_failed")?;
    let agent_key = agent.public_key();
    if persona.identity.as_deref() != Some(agent_key.to_hex().as_str())
        || owner.public_key() == agent_key
    {
        return Err("enroll_failed");
    }
    buzz_sdk::nip_oa::verify_auth_tag(tag_json, &agent_key).map_err(|_| "enroll_failed")?;
    let auth_tag = Tag::parse(
        buzz_sdk::nip_oa::parse_auth_tag(tag_json)
            .map_err(|_| "enroll_failed")?
            .as_slice(),
    )
    .map_err(|_| "enroll_failed")?;
    let at = next_timestamp(persona.published_at);
    let mut conn = crate::auth::connect_identity(relay, owner)
        .await
        .map_err(connect_category)?;
    let owner_steps = async {
        publish(&mut conn, &persona_event(persona, owner, at)?).await?;
        publish(
            &mut conn,
            &managed_agent_event(persona, &agent_key, owner, at)?,
        )
        .await?;
        report.published_at = Some(at.as_secs());
        for room in persona
            .rooms
            .iter()
            .filter(|room| !persona.member_rooms.contains(room))
        {
            publish(&mut conn, &add_member_event(room, &agent_key, owner, at)?).await?;
            report.member_rooms.push(room.clone());
        }
        Ok::<(), &'static str>(())
    }
    .await;
    let _ = timeout(Duration::from_secs(2), conn.disconnect()).await;
    owner_steps?;
    let mut conn = crate::auth::connect_attested(relay, agent, Some(&auth_tag))
        .await
        .map_err(connect_category)?;
    let result = publish(&mut conn, &profile_event(persona, agent, &auth_tag, at)?).await;
    let _ = timeout(Duration::from_secs(2), conn.disconnect()).await;
    result
}

#[cfg(test)]
#[path = "enroll_tests.rs"]
mod tests;
