//! Bounded projection of self-authored agent profiles for verified room members.
//! The caller must supply keys from a verified, current kind-39002 room roster.
//! This module neither queries the relay nor infers process state or authority.
use nostr::{Event, PublicKey};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_MEMBERS: usize = 20;
const MAX_EVENTS: usize = 200;
const MAX_CONTENT_BYTES: usize = 4096;
const MAX_NAME_BYTES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHint {
    /// Signed event author, also present in the verified room roster.
    pub key: String,
    /// Untrusted, sanitized label from the agent's latest kind-10100 event.
    pub name: String,
    pub profile_event_id: String,
    /// A profile advertises an identity; it does not prove a running process.
    pub execution_state: &'static str,
}

/// Project only latest, unambiguous, valid kind-10100 profiles for roster keys.
/// An invalid event batch fails closed. A malformed latest content document or
/// conflicting latest timestamp omits that identity rather than reviving an
/// older advertisement. Absence of a profile yields no agent classification.
pub fn project(
    roster_keys: &[String],
    events: &[Event],
    now: u64,
) -> Result<Vec<AgentHint>, &'static str> {
    if roster_keys.len() > MAX_MEMBERS || events.len() > MAX_EVENTS {
        return Err("agents_oversized");
    }
    let mut members = BTreeSet::new();
    for key in roster_keys {
        let parsed = PublicKey::from_hex(key).map_err(|_| "agents_invalid_roster")?;
        if parsed.to_hex() != *key || !members.insert(key.clone()) {
            return Err("agents_invalid_roster");
        }
    }
    let mut latest: BTreeMap<String, (&Event, bool)> = BTreeMap::new();
    for event in events {
        if event.verify().is_err()
            || event.kind.as_u16() != 10100
            || event.created_at.as_secs() > now.saturating_add(60)
            || event.content.len() > MAX_CONTENT_BYTES
        {
            return Err("agents_invalid_profile");
        }
        let key = event.pubkey.to_hex();
        if !members.contains(&key) {
            return Err("agents_invalid_scope");
        }
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
    Ok(latest
        .into_iter()
        .filter_map(|(key, (event, conflict))| {
            if conflict {
                return None;
            }
            let value: serde_json::Value = serde_json::from_str(&event.content).ok()?;
            let object = value.as_object()?;
            // A content "pubkey", owner, permissions, channels, and status are
            // self-assertions; only the signed author is used as identity.
            let raw = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    object
                        .get("display_name")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or("");
            // Empty name asks the eventual UI to display the full key. It does
            // not erase otherwise valid self-authored agent evidence.
            let name = clean_name(raw);
            Some(AgentHint {
                key,
                name,
                profile_event_id: event.id.to_hex(),
                execution_state: "unknown",
            })
        })
        // Keep the complete status envelope within its fixed 64 KiB limit.
        .take(10)
        .collect())
}

fn clean_name(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        let ch = if ch.is_control()
            || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            ' '
        } else {
            ch
        };
        if out.len() + ch.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Timestamp};

    fn keys(n: u8) -> Keys {
        Keys::parse(&format!("{n:064x}")).unwrap()
    }
    fn profile(author: &Keys, at: u64, content: &str) -> Event {
        EventBuilder::new(Kind::Custom(10100), content)
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(author)
            .unwrap()
    }

    #[test]
    fn signed_member_profile_is_only_an_advertisement() {
        let agent = keys(1);
        let event = profile(
            &agent,
            100,
            r#"{"name":"Scout","pubkey":"forged","status":"online","owner_pubkey":"forged"}"#,
        );
        let hints = project(&[agent.public_key().to_hex()], &[event.clone()], 100).unwrap();
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].key, agent.public_key().to_hex());
        assert_eq!(hints[0].name, "Scout");
        assert_eq!(hints[0].profile_event_id, event.id.to_hex());
        assert_eq!(hints[0].execution_state, "unknown");
    }

    #[test]
    fn bad_signature_and_unrostered_author_fail_closed() {
        let agent = keys(1);
        let stranger = keys(2);
        let member = vec![agent.public_key().to_hex()];
        let mut tampered = profile(&agent, 100, r#"{"name":"Scout"}"#);
        tampered.content = r#"{"name":"Impostor"}"#.into();
        assert_eq!(
            project(&member, &[tampered], 100).unwrap_err(),
            "agents_invalid_profile"
        );
        assert_eq!(
            project(
                &member,
                &[profile(&stranger, 100, r#"{"name":"Other"}"#)],
                100
            )
            .unwrap_err(),
            "agents_invalid_scope"
        );
    }

    #[test]
    fn latest_invalid_or_conflicting_profile_does_not_revive_old_name() {
        let agent = keys(1);
        let roster = vec![agent.public_key().to_hex()];
        let old = profile(&agent, 100, r#"{"name":"Old"}"#);
        let malformed = profile(&agent, 101, "not-json");
        assert!(project(&roster, &[old.clone(), malformed], 101)
            .unwrap()
            .is_empty());
        let a = profile(&agent, 101, r#"{"name":"A"}"#);
        let b = profile(&agent, 101, r#"{"name":"B"}"#);
        assert!(project(&roster, &[old, a, b], 101).unwrap().is_empty());
    }

    #[test]
    fn future_and_bidi_payloads_are_bounded() {
        let agent = keys(1);
        let roster = vec![agent.public_key().to_hex()];
        assert_eq!(
            project(
                &roster,
                &[profile(&agent, 161, r#"{"name":"Future"}"#)],
                100
            )
            .unwrap_err(),
            "agents_invalid_profile"
        );
        let long = format!("{{\"name\":\"{}\u{202e}tail\"}}", "x".repeat(100));
        let hints = project(&roster, &[profile(&agent, 100, &long)], 100).unwrap();
        assert!(hints[0].name.len() <= MAX_NAME_BYTES);
        assert!(!hints[0].name.contains('\u{202e}'));
    }
}
