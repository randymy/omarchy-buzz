//! Agent-service requests (`docs/AGENTS_SERVICE.md`, "IPC"). Parsing is
//! structural and strict: unknown keys, `null` values, fields a request type
//! does not take and missing required fields are all refused.
use super::store;
use serde::Deserialize;

/// Persona fields of `create_agent` (all present) and `update_agent` (only the
/// changed ones; never `startAtLogin`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fields {
    pub name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub rooms: Option<Vec<String>>,
    pub respond_to: Option<String>,
    pub workspace: Option<String>,
    pub start_at_login: Option<bool>,
    pub acp_command: Option<String>,
    pub answers_dms: Option<bool>,
}
const FIELD_KEYS: [&str; 11] = [
    "name",
    "description",
    "instructions",
    "harness",
    "model",
    "rooms",
    "respondTo",
    "workspace",
    "startAtLogin",
    "acpCommand",
    "answersDms",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub instance_id: String,
    pub agent_id: Option<String>,
    pub fields: Option<Fields>,
    pub forget: Option<bool>,
    pub enabled: Option<bool>,
    pub harness: Option<String>,
}

fn keys(value: &serde_json::Value) -> Vec<&str> {
    value
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default()
}
fn has_null(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .is_some_and(|o| o.values().any(serde_json::Value::is_null))
}

/// `Ok` for a well-formed request. A frame that is not a JSON request object
/// with a UUID `id` is `Err(None)` (the connection is closed); any other
/// refusal is `Err(Some(id))`, answered with an `agent_invalid` error.
pub fn parse(bytes: &[u8]) -> Result<Request, Option<String>> {
    if bytes.len() > crate::protocol::LIMIT {
        return Err(None);
    }
    let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| None)?;
    let id = raw
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| store::canonical_uuid(id))
        .map(str::to_owned)
        .ok_or(None)?;
    let refuse = || Some(id.clone());
    if has_null(&raw) {
        return Err(refuse());
    }
    let r: Request = serde_json::from_value(raw.clone()).map_err(|_| refuse())?;
    if r.version != 1
        || r.instance_id.is_empty()
        || r.instance_id.len() > 128
        || !r
            .instance_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err(refuse());
    }
    let allowed: &[&str] = match r.kind.as_str() {
        "subscribe" => &[],
        "create_agent" => &["fields"],
        "update_agent" => &["agentId", "fields"],
        "delete_agent" => &["agentId", "forget"],
        "enroll_agent" | "start_agent" | "stop_agent" => &["agentId"],
        "set_start_at_login" => &["agentId", "enabled"],
        "sign_in" | "refresh_bundle" => &["harness"],
        _ => return Err(refuse()),
    };
    let present: Vec<&str> = keys(&raw)
        .into_iter()
        .filter(|k| !matches!(*k, "version" | "id" | "type" | "instanceId"))
        .collect();
    // Every permitted key is required, except `forget` (defaults to false).
    let required = allowed.iter().filter(|k| **k != "forget");
    if present.iter().any(|k| !allowed.contains(k))
        || required.clone().any(|k| !present.contains(k))
    {
        return Err(refuse());
    }
    if r.agent_id
        .as_deref()
        .is_some_and(|a| !store::canonical_uuid(a))
    {
        return Err(refuse());
    }
    if let Some(fields) = raw.get("fields") {
        if has_null(fields) {
            return Err(refuse());
        }
        let given = keys(fields);
        match r.kind.as_str() {
            // The panel sends every persona field on create.
            "create_agent" if FIELD_KEYS.iter().any(|k| !given.contains(k)) => return Err(refuse()),
            "update_agent" if given.is_empty() || given.contains(&"startAtLogin") => {
                return Err(refuse())
            }
            _ => {}
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    const AGENT: &str = "00000000-0000-4000-8000-0000000000a1";
    fn frame(extra: serde_json::Value) -> Vec<u8> {
        let mut v = serde_json::json!({"version":1,"id":ID,"instanceId":"1-2"});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::to_vec(&v).unwrap()
    }
    fn full_fields() -> serde_json::Value {
        serde_json::json!({"name":"Scout","description":"","instructions":"Be brief.","harness":"codex",
            "model":"","rooms":["00000000-0000-4000-8000-0000000000b1"],"respondTo":"owner-only",
            "workspace":"","startAtLogin":false,"acpCommand":"buzz-acp","answersDms":false})
    }

    #[test]
    fn accepts_each_documented_shape() {
        for extra in [
            serde_json::json!({"type":"subscribe"}),
            serde_json::json!({"type":"create_agent","fields":full_fields()}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{"name":"New"}}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{"answersDms":true}}),
            serde_json::json!({"type":"delete_agent","agentId":AGENT}),
            serde_json::json!({"type":"delete_agent","agentId":AGENT,"forget":true}),
            serde_json::json!({"type":"enroll_agent","agentId":AGENT}),
            serde_json::json!({"type":"start_agent","agentId":AGENT}),
            serde_json::json!({"type":"stop_agent","agentId":AGENT}),
            serde_json::json!({"type":"set_start_at_login","agentId":AGENT,"enabled":true}),
            serde_json::json!({"type":"sign_in","harness":"codex"}),
            serde_json::json!({"type":"refresh_bundle","harness":"claude-code"}),
        ] {
            let r = parse(&frame(extra.clone())).unwrap_or_else(|_| panic!("{extra}"));
            assert_eq!(r.id, ID);
        }
    }

    #[test]
    fn refuses_authority_and_malformed_requests() {
        // Not a request with a UUID id: the connection is closed.
        assert_eq!(parse(b"not-json").unwrap_err(), None);
        assert_eq!(
            parse(br#"{"version":1,"id":"a","type":"subscribe","instanceId":"x"}"#).unwrap_err(),
            None
        );
        assert_eq!(
            parse(&vec![b' '; crate::protocol::LIMIT + 1]).unwrap_err(),
            None
        );
        let refused = Some(ID.to_string());
        let mut create_missing = full_fields();
        create_missing.as_object_mut().unwrap().remove("workspace");
        let mut create_without_dms = full_fields();
        create_without_dms
            .as_object_mut()
            .unwrap()
            .remove("answersDms");
        for extra in [
            serde_json::json!({"type":"get_snapshot"}),
            serde_json::json!({"type":"subscribe","agentId":AGENT}),
            serde_json::json!({"type":"subscribe","command":"/bin/sh"}),
            serde_json::json!({"type":"create_agent"}),
            serde_json::json!({"type":"create_agent","fields":create_missing}),
            serde_json::json!({"type":"create_agent","fields":create_without_dms}),
            serde_json::json!({"type":"create_agent","fields":{"name":"x","privateKey":"sentinel"}}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{}}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{"startAtLogin":true}}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{"name":null}}),
            serde_json::json!({"type":"update_agent","agentId":AGENT,"fields":{"answersDms":"yes"}}),
            serde_json::json!({"type":"update_agent","agentId":"not-a-uuid","fields":{"name":"x"}}),
            serde_json::json!({"type":"enroll_agent"}),
            serde_json::json!({"type":"enroll_agent","agentId":AGENT,"secret":"nsec"}),
            serde_json::json!({"type":"set_start_at_login","agentId":AGENT}),
            serde_json::json!({"type":"sign_in"}),
            serde_json::json!({"type":"sign_in","harness":"codex","command":"login"}),
            serde_json::json!({"type":"refresh_bundle"}),
            serde_json::json!({"type":"refresh_bundle","harness":"codex","scripts":"/tmp"}),
            serde_json::json!({"type":"refresh_bundle","agentId":AGENT}),
            serde_json::json!({"type":"stop_agent","agentId":AGENT,"forget":null}),
        ] {
            assert_eq!(
                parse(&frame(extra.clone())).unwrap_err(),
                refused,
                "{extra}"
            );
        }
        assert_eq!(
            parse(br#"{"version":2,"id":"00000000-0000-4000-8000-000000000001","type":"subscribe","instanceId":"x"}"#)
                .unwrap_err(),
            refused
        );
        assert_eq!(
            parse(
                br#"{"version":1,"id":"00000000-0000-4000-8000-000000000001","type":"subscribe"}"#
            )
            .unwrap_err(),
            refused
        );
        assert_eq!(
            parse(br#"{"version":1,"id":"00000000-0000-4000-8000-000000000001","type":"subscribe","instanceId":"a b"}"#)
                .unwrap_err(),
            refused
        );
    }
}
