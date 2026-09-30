//! Inviting people (`invite_mint`): the relay owner or an admin mints an
//! invite code the panel turns into shareable links.
//!
//! Follows the pinned relay (`crates/buzz-relay/src/api/invites.rs`
//! `mint_invite`, `validate_mint_request`) and Desktop's client
//! (`desktop/src/shared/api/invites.ts` `mintInvite`):
//!
//! `POST /api/invites`, NIP-98 signed with a payload hash, body
//! `{"max_uses": n, "ttl_secs": s}` → `{code, expires_at, max_uses,
//! uses_remaining, url}`. Only the tenant's `owner` or `admin` may mint (403
//! otherwise). The request carries no role: every claim grants `member`
//! (`invites.rs` claim paths). The relay has no route to list or revoke
//! invites at this revision.
//!
//! The code is a bearer credential for joining: it is published to the panel
//! only, never logged.
use crate::join::{body, client, http_url};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use nostr::Keys;
use reqwest::StatusCode;

/// The panel offers 1, 5 or 25 uses; the helper accepts 1 to 100 (the relay
/// accepts up to 10 000).
pub const MAX_USES: std::ops::RangeInclusive<u32> = 1..=100;
/// One hour to 30 days (the relay's `MAX_INVITE_TTL_SECS`).
pub const HOURS: std::ops::RangeInclusive<u32> = 1..=720;
/// The role every claimed invite grants.
pub const ROLE: &str = "member";
const RESPONSE_BYTES: usize = 16 * 1024;
/// Allowed difference between the relay's expiry and the one requested, for
/// clock skew and request time.
const EXPIRY_SLACK_SECS: u64 = 15 * 60;
/// `buzz-core` `invite.rs`: `v2.` and 32 random bytes, unpadded base64url.
const V2_PREFIX: &str = "v2.";
const V2_SECRET_LEN: usize = 32;

/// Fixed categories for a mint.
#[cfg(test)]
pub const CATEGORIES: [&str; 5] = [
    "invite_forbidden",
    "invite_rejected",
    "invite_rate_limited",
    "relay_unavailable",
    "setup_busy",
];

/// What the relay confirmed, checked against what was asked for.
#[derive(Debug, PartialEq)]
pub struct Minted {
    pub code: String,
    pub expires_at: u64,
    pub max_uses: u32,
}

/// A canonical v2 code (`buzz-core` `validate_v2_code`): nothing else is ever
/// shown as an invite.
pub fn valid_v2_code(code: &str) -> bool {
    let Some(encoded) = code.strip_prefix(V2_PREFIX) else {
        return false;
    };
    URL_SAFE_NO_PAD.decode(encoded).is_ok_and(|secret| {
        secret.len() == V2_SECRET_LEN && URL_SAFE_NO_PAD.encode(&secret) == encoded
    })
}

/// The exact request body: both limits explicit, like Desktop's settings.
pub fn request_body(max_uses: u32, hours: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "max_uses": max_uses,
        "ttl_secs": u64::from(hours) * 3600,
    }))
    .expect("a JSON object of two integers serializes")
}

/// `http(s)://<configured relay authority>/invite/<code>`: the relay's `url`,
/// which names the tenant host the request was made to.
pub fn landing_url(relay: &str, code: &str) -> Result<String, &'static str> {
    Ok(http_url(relay, &format!("/invite/{code}"))?.to_string())
}

/// Exactly the relay's mint answer for this request. Unknown or missing keys,
/// a non-canonical code, other limits, an expiry far from the one requested
/// or a link to another host refuse the response.
pub fn parse_minted(
    bytes: &[u8],
    relay: &str,
    max_uses: u32,
    hours: u32,
    now: u64,
) -> Result<Minted, &'static str> {
    const INVALID: &str = "relay_unavailable";
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let object = value.as_object().ok_or(INVALID)?;
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["code", "expires_at", "max_uses", "url", "uses_remaining"] {
        return Err(INVALID);
    }
    let code = object
        .get("code")
        .and_then(serde_json::Value::as_str)
        .filter(|code| valid_v2_code(code))
        .ok_or(INVALID)?;
    let expires_at = object
        .get("expires_at")
        .and_then(serde_json::Value::as_u64)
        .ok_or(INVALID)?;
    let expected = now + u64::from(hours) * 3600;
    if expires_at.abs_diff(expected) > EXPIRY_SLACK_SECS {
        return Err(INVALID);
    }
    for name in ["max_uses", "uses_remaining"] {
        if object.get(name).and_then(serde_json::Value::as_u64) != Some(u64::from(max_uses)) {
            return Err(INVALID);
        }
    }
    let url = object.get("url").and_then(serde_json::Value::as_str);
    if url != Some(landing_url(relay, code)?.as_str()) {
        return Err(INVALID);
    }
    Ok(Minted {
        code: code.to_owned(),
        expires_at,
        max_uses,
    })
}

fn now() -> Result<u64, &'static str> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "relay_unavailable")
}

/// Mints one invite as `keys` (NIP-98, payload-bound, `u` = the exact URL).
pub async fn mint(
    relay: &str,
    keys: &Keys,
    max_uses: u32,
    hours: u32,
) -> Result<Minted, &'static str> {
    if !MAX_USES.contains(&max_uses) || !HOURS.contains(&hours) {
        return Err("invite_rejected");
    }
    let url = http_url(relay, "/api/invites")?;
    let bytes = request_body(max_uses, hours);
    let auth = crate::query::authorization(keys, &url, &bytes).map_err(|_| "relay_unavailable")?;
    let response = client()?
        .post(url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .header("Content-Type", "application/json")
        .body(bytes)
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        StatusCode::OK => parse_minted(
            &body(response, RESPONSE_BYTES).await?,
            relay,
            max_uses,
            hours,
            now()?,
        ),
        // "only relay owners and admins can create invites"; also a membership
        // or federated-identity refusal of this key.
        StatusCode::FORBIDDEN => Err("invite_forbidden"),
        StatusCode::TOO_MANY_REQUESTS => Err("invite_rate_limited"),
        // Refused limits or JSON, or a refused NIP-98 proof.
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => Err("invite_rejected"),
        _ => Err("relay_unavailable"),
    }
}

/// `status.invites` views.
pub fn minting() -> crate::protocol::Invites {
    crate::protocol::Invites {
        state: "minting".into(),
        ..Default::default()
    }
}
pub fn minted(minted: Minted) -> crate::protocol::Invites {
    crate::protocol::Invites {
        state: "minted".into(),
        code: Some(minted.code),
        expires_at: Some(minted.expires_at),
        max_uses: Some(minted.max_uses),
        role: Some(ROLE.into()),
        category: None,
    }
}
pub fn failed(category: &'static str) -> crate::protocol::Invites {
    crate::protocol::Invites {
        state: "failed".into(),
        category: Some(category.into()),
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
