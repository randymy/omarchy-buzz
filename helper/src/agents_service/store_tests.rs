use super::*;
use crate::agents_service::test_support::{mode, persona, TempHome, ROOM_A, ROOM_B};
use std::os::unix::fs::PermissionsExt;

const ID: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";

#[test]
fn every_field_rule_is_enforced() {
    let base = persona(ID, "/w");
    assert!(base.valid());
    let check = |change: &dyn Fn(&mut Persona), expected: bool| {
        let mut p = base.clone();
        change(&mut p);
        assert_eq!(p.valid(), expected, "{p:?}");
    };
    // id: canonical lowercase UUID v4 only.
    check(&|p| p.id = ID.to_uppercase(), false);
    check(
        &|p| p.id = "aaaaaaaa-1111-1111-8111-111111111111".into(),
        false,
    );
    check(&|p| p.id = "not-a-uuid".into(), false);
    // name: 1-64 characters, no control, bidi or invisible characters.
    check(&|p| p.name = String::new(), false);
    check(&|p| p.name = "   ".into(), false);
    check(&|p| p.name = "x".repeat(64), true);
    check(&|p| p.name = "é".repeat(64), true);
    check(&|p| p.name = "x".repeat(65), false);
    check(&|p| p.name = "a\nb".into(), false);
    check(&|p| p.name = "a\u{7f}b".into(), false);
    check(&|p| p.name = "evil\u{202e}txt".into(), false);
    check(&|p| p.name = "a\u{2066}b".into(), false);
    check(&|p| p.name = "a\u{200b}b".into(), false);
    check(&|p| p.name = "a\u{feff}b".into(), false);
    // description: at most 256 characters, same sanitizing, may be empty.
    check(&|p| p.description = String::new(), true);
    check(&|p| p.description = "d".repeat(256), true);
    check(&|p| p.description = "d".repeat(257), false);
    check(&|p| p.description = "a\u{200f}b".into(), false);
    check(&|p| p.description = "tab\there".into(), false);
    // instructions: at most 16 KiB of text; newline, CR and tab only.
    check(&|p| p.instructions = String::new(), true);
    check(&|p| p.instructions = "i".repeat(INSTRUCTIONS_BYTES), true);
    check(
        &|p| p.instructions = "i".repeat(INSTRUCTIONS_BYTES + 1),
        false,
    );
    check(
        &|p| p.instructions = "é".repeat(INSTRUCTIONS_BYTES / 2 + 1),
        false,
    );
    check(&|p| p.instructions = "a\r\n\tb".into(), true);
    check(&|p| p.instructions = "a\0b".into(), false);
    check(&|p| p.instructions = "a\u{1b}[2Jb".into(), false);
    // harness
    check(&|p| p.harness = "claude-code".into(), true);
    check(&|p| p.harness = "claude".into(), false);
    check(&|p| p.harness = "goose".into(), false);
    // model: empty or at most 64 of [A-Za-z0-9._:-].
    check(
        &|p| p.model = "gpt-5.1-codex:latest_x".replace('_', "."),
        true,
    );
    check(&|p| p.model = "m".repeat(64), true);
    check(&|p| p.model = "m".repeat(65), false);
    check(&|p| p.model = "a b".into(), false);
    check(&|p| p.model = "a/b".into(), false);
    check(&|p| p.model = "--flag".into(), true);
    // acpCommand: buzz-acp only.
    check(&|p| p.acp_command = "buzz-goose-acp".into(), false);
    check(&|p| p.acp_command = "/usr/bin/buzz-acp".into(), false);
    // rooms: 1-8 unique canonical UUIDs.
    check(&|p| p.rooms = Vec::new(), false);
    check(&|p| p.rooms = vec![ROOM_A.into(), ROOM_A.into()], false);
    check(&|p| p.rooms = vec![ROOM_A.to_uppercase()], false);
    check(&|p| p.rooms = vec!["general".into()], false);
    check(
        &|p| {
            p.rooms = (1..=8)
                .map(|n| format!("00000000-0000-4000-8000-00000000000{n}"))
                .collect()
        },
        true,
    );
    check(
        &|p| {
            p.rooms = (1..=9)
                .map(|n| format!("00000000-0000-4000-8000-00000000000{n}"))
                .collect()
        },
        false,
    );
    // respondTo
    check(&|p| p.respond_to = "mentions".into(), true);
    check(&|p| p.respond_to = "anyone".into(), false);
    check(&|p| p.respond_to = "allowlist".into(), false);
    // workspace: absolute and normalized.
    for bad in [
        "",
        "relative",
        "/a/../b",
        "/a/./b",
        "/a//b",
        "/a/",
        "/a\nb",
        "/a\u{202e}",
    ] {
        check(&|p| p.workspace = bad.into(), false);
    }
    // identity / attestation consistency.
    check(&|p| p.identity = Some("zz".into()), false);
    check(&|p| p.published = true, false);
    check(&|p| p.auth_tag = Some("[]".into()), false);
    check(&|p| p.member_rooms = vec![ROOM_B.into()], false);
    check(&|p| p.last_error = Some("relay said: no".into()), false);
}

#[test]
fn attestation_must_verify_for_the_stored_identity() {
    let owner = nostr::Keys::generate();
    let agent = nostr::Keys::generate();
    let mut p = persona(ID, "/w");
    p.identity = Some(agent.public_key().to_hex());
    p.auth_tag = Some(buzz_sdk::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap());
    assert!(p.valid());
    // Acknowledged memberships may include dropped rooms still to be left,
    // but stay unique canonical UUIDs and bounded.
    let enrolled = p.clone();
    p.member_rooms = vec![ROOM_A.into(), ROOM_B.into()];
    assert!(p.valid());
    p.member_rooms = vec![ROOM_B.into(), ROOM_B.into()];
    assert!(!p.valid());
    p.member_rooms = vec![ROOM_B.to_uppercase()];
    assert!(!p.valid());
    p.member_rooms = (0..=MAX_MEMBER_ROOMS)
        .map(|n| format!("00000000-0000-4000-8000-{n:012}"))
        .collect();
    assert!(!p.valid());
    p.member_rooms.pop();
    assert!(p.valid());
    let mut p = enrolled;
    p.identity = Some(nostr::Keys::generate().public_key().to_hex());
    assert!(!p.valid());
}

#[test]
fn store_round_trips_at_0600_and_refuses_invalid_files() {
    let t = TempHome::new();
    let mut store = Store::open(&t.paths).unwrap();
    assert!(store.agents.is_empty());
    assert_eq!(mode(&t.paths.store_dir()), 0o700);
    store.agents.push(persona(ID, "/w"));
    store.save().unwrap();
    assert_eq!(mode(&t.paths.store_file()), 0o600);
    assert_eq!(Store::open(&t.paths).unwrap().agents, store.agents);
    // No temporary files remain after an atomic replacement.
    let names: Vec<_> = std::fs::read_dir(t.paths.store_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("personas.json")]);

    let file = t.paths.store_file();
    let good = std::fs::read(&file).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(Store::open(&t.paths).err(), Some("store_invalid"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let text = String::from_utf8(good.clone()).unwrap();
    for bad in [
        "not json".to_string(),
        text.replace("\"version\": 1", "\"version\": 2"),
        text.replace(
            "\"name\": \"Scout\"",
            "\"name\": \"Scout\", \"privateKey\": \"x\"",
        ),
        text.replace("buzz-acp", "sh"),
    ] {
        std::fs::write(&file, bad.as_bytes()).unwrap();
        assert_eq!(Store::open(&t.paths).err(), Some("store_invalid"), "{bad}");
    }
    // Duplicate ids and more than 16 agents are refused on load and save.
    let mut dup = Store::open(&{
        std::fs::write(&file, &good).unwrap();
        t.paths.clone()
    })
    .unwrap();
    dup.agents.push(persona(ID, "/x"));
    assert!(dup.save().is_err());
    let mut many = Store::open(&t.paths).unwrap();
    many.agents = (0..17)
        .map(|n| persona(&format!("00000000-0000-4000-8000-{n:012}"), "/w"))
        .collect();
    assert!(many.save().is_err());
    many.agents.truncate(16);
    many.save().unwrap();
    assert_eq!(Store::open(&t.paths).unwrap().agents.len(), 16);
}

#[test]
fn workspace_rules_refuse_sensitive_shared_and_unsafe_directories() {
    let t = TempHome::new();
    let p = &t.paths;
    let s = |path: &std::path::Path| path.to_str().unwrap().to_owned();
    let ok = t.private_dir("work/scout");
    assert_eq!(check_workspace(p, &ok, ID, &[]), Ok(()));
    // Own default workspace is allowed, another agent's default is not.
    let own = create_default_workspace(p, ID).unwrap();
    assert_eq!(mode(&own), 0o700);
    assert_eq!(check_workspace(p, &s(&own), ID, &[]), Ok(()));
    let theirs = create_default_workspace(p, OTHER).unwrap();
    let refused = |w: &str, others: &[&Persona]| {
        assert_eq!(
            check_workspace(p, w, ID, others),
            Err("workspace_refused"),
            "{w}"
        );
    };
    refused(&s(&theirs), &[]);
    // $HOME itself and its ancestors.
    refused(&s(&p.home), &[]);
    refused(&s(p.home.parent().unwrap()), &[]);
    refused("/", &[]);
    // Sensitive trees, whether or not they exist.
    for sensitive in [".config", ".config/omarchy", ".ssh", ".gnupg/private"] {
        refused(&t.private_dir(sensitive), &[]);
    }
    refused(&t.private_dir(".local/state/omarchy-buzz/agents"), &[]);
    refused(
        &t.private_dir(".local/state/omarchy-buzz-agent-preview/codex"),
        &[],
    );
    refused(
        &t.private_dir(".local/state/omarchy-buzz-agent-preview/claude-code/x"),
        &[],
    );
    refused(
        &t.private_dir(".local/state/omarchy-buzz-room-workspaces/codex"),
        &[],
    );
    refused(&t.private_dir(".local/state/omarchy-buzzard"), &[]);
    refused(&s(&p.state), &[]);
    refused(&t.private_dir(".local/share/omarchy-buzz/agent-codex"), &[]);
    // Another agent's workspace: equal, inside or containing.
    let other = persona(OTHER, &t.private_dir("work/shared"));
    refused(&other.workspace, &[&other]);
    refused(&t.private_dir("work/shared/inner"), &[&other]);
    refused(&t.private_dir("work"), &[&other]);
    assert_eq!(check_workspace(p, &ok, ID, &[&other]), Ok(()));
    // Missing, relative, non-canonical, symlinked, or accessible to others.
    refused(&s(&p.home.join("missing")), &[]);
    refused("work/scout", &[]);
    refused(&format!("{ok}/"), &[]);
    refused(&format!("{ok}/../scout"), &[]);
    let link = p.home.join("linked");
    std::os::unix::fs::symlink(p.home.join("work"), &link).unwrap();
    refused(&s(&link.join("scout")), &[]);
    refused(&s(&link), &[]);
    let open = t.private_dir("work/open");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    refused(&open, &[]);
    let file = p.home.join("work/file");
    std::fs::write(&file, b"x").unwrap();
    refused(&s(&file), &[]);
}

#[test]
fn default_workspace_parent_must_be_safe_and_is_never_loosened() {
    let t = TempHome::new();
    let parent = t.paths.workspaces();
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        create_default_workspace(&t.paths, ID).err(),
        Some("workspace_refused")
    );
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    create_default_workspace(&t.paths, ID).unwrap();
    // An existing parent is left as it was.
    assert_eq!(mode(&parent), 0o755);
}
