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
    // relay: the canonical relay of the agent's community.
    check(&|p| p.relay = "ws://127.0.0.1:4000/".into(), true);
    for bad in [
        "",
        "wss://Relay.example/",
        "wss://relay.example",
        "wss://relay.example/path",
        "https://relay.example/",
        "ws://relay.example/",
    ] {
        check(&|p| p.relay = bad.into(), false);
    }
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
    store.agents.push(Agent::from_persona(persona(ID, "/w")));
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
    assert!(text.contains("\"version\": 2"));
    // A version 1 store written before `answersDms` existed loads with it
    // off; a non-boolean value is refused. Version 2 requires the field.
    let v1 = String::from_utf8(v1_bytes(&[persona(ID, "/w")])).unwrap();
    assert!(v1.contains("\"answersDms\": false"));
    std::fs::write(&file, v1.replace("\"answersDms\": false,", "")).unwrap();
    assert_eq!(Store::open(&t.paths).unwrap().agents, store.agents);
    std::fs::write(
        &file,
        v1.replace("\"answersDms\": false", "\"answersDms\": true"),
    )
    .unwrap();
    assert!(Store::open(&t.paths).unwrap().agents[0].answers_dms);
    std::fs::write(
        &file,
        v1.replace("\"answersDms\": false", "\"answersDms\": \"yes\""),
    )
    .unwrap();
    assert_eq!(Store::open(&t.paths).err(), Some("store_invalid"));
    std::fs::write(&file, &good).unwrap();
    for bad in [
        "not json".to_string(),
        text.replace("\"version\": 2", "\"version\": 3"),
        text.replace("\"version\": 2", "\"version\": 0"),
        text.replace("\"answersDms\": false,", ""),
        // A version 2 record in a version 1 file, and the reverse.
        text.replace("\"version\": 2", "\"version\": 1"),
        v1.replace("\"version\": 1", "\"version\": 2"),
        text.replace("\"primary\": true", "\"primary\": 1"),
        text.replace("\"instances\": [", "\"instances\": [], \"x\": ["),
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
    dup.agents.push(Agent::from_persona(persona(ID, "/x")));
    assert!(dup.save().is_err());
    let mut many = Store::open(&t.paths).unwrap();
    many.agents = (0..17)
        .map(|n| Agent::from_persona(persona(&format!("00000000-0000-4000-8000-{n:012}"), "/w")))
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

/// A version 1 store file holding `personas`.
fn v1_bytes(personas: &[Persona]) -> Vec<u8> {
    serde_json::to_vec_pretty(&FileV1 {
        version: 1,
        agents: personas.to_vec(),
    })
    .unwrap()
}
fn write_v1(t: &TempHome, personas: &[Persona]) -> Vec<u8> {
    ensure_private_directory(&t.paths.store_dir()).unwrap();
    let bytes = v1_bytes(personas);
    write_private(&t.paths.store_file(), &bytes).unwrap();
    bytes
}
/// A store file as written before `relay` existed: the persona JSON without it.
fn write_old_store(t: &TempHome, personas: &[Persona]) -> Vec<u8> {
    write_v1(t, personas);
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(t.paths.store_file()).unwrap()).unwrap();
    for agent in value["agents"].as_array_mut().unwrap() {
        agent.as_object_mut().unwrap().remove("relay");
    }
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    std::fs::write(t.paths.store_file(), &bytes).unwrap();
    bytes
}
/// An enrolled persona and the unit file the service generated for it.
fn enrolled_with_unit(t: &TempHome, id: &str, relay: &str) -> Persona {
    let owner = nostr::Keys::generate();
    let agent = nostr::Keys::generate();
    let mut p = persona(id, "/w");
    p.relay = relay.into();
    p.identity = Some(agent.public_key().to_hex());
    p.auth_tag = Some(buzz_sdk::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap());
    let text = super::super::unit::render(&t.paths, &p, &owner.public_key().to_hex()).unwrap();
    std::fs::create_dir_all(t.paths.units_dir()).unwrap();
    std::fs::write(t.paths.unit_file(id), text).unwrap();
    p
}

#[test]
fn personas_without_a_relay_take_their_unit_files_relay() {
    let t = TempHome::new();
    // An agent already running against the first community: its unit names it.
    let unit_relay_url = "wss://first.example/";
    let with_unit = enrolled_with_unit(&t, ID, unit_relay_url);
    let unit_bytes = std::fs::read(t.paths.unit_file(ID)).unwrap();
    // Another agent never started: no unit file.
    let without_unit = persona(OTHER, "/x");
    let old = write_old_store(&t, &[with_unit.clone(), without_unit.clone()]);
    assert!(!String::from_utf8_lossy(&old).contains("\"relay\""));
    // The plain `open` refuses rather than guessing.
    assert_eq!(Store::open(&t.paths).err(), Some("store_invalid"));
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);

    let mut asked = 0;
    let store = Store::open_with(&t.paths, || {
        asked += 1;
        Some("wss://second.example/".into())
    })
    .unwrap();
    // The configuration is asked once, only for the persona without a unit.
    assert_eq!(asked, 1);
    assert_eq!(store.agents[0].first().relay, unit_relay_url);
    assert_eq!(store.agents[1].first().relay, "wss://second.example/");
    // Written back once, atomically, at 0600; the unit file is only read.
    assert_eq!(mode(&t.paths.store_file()), 0o600);
    let written = std::fs::read(t.paths.store_file()).unwrap();
    assert_eq!(Store::open(&t.paths).unwrap().agents, store.agents);
    assert_eq!(std::fs::read(t.paths.unit_file(ID)).unwrap(), unit_bytes);
    let names: Vec<_> = std::fs::read_dir(t.paths.store_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort();
    assert_eq!(
        names,
        ["personas.json", "personas.v1.json"].map(std::ffi::OsString::from)
    );
    assert_eq!(
        std::fs::read(t.paths.store_dir().join("personas.v1.json")).unwrap(),
        old
    );
    // Idempotent: a migrated store is not rewritten and never asks again.
    let again = Store::open_with(&t.paths, || panic!("asked again")).unwrap();
    assert_eq!(again.agents, store.agents);
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), written);
    // A persona that has a relay keeps it, whatever its unit file says.
    let t = TempHome::new();
    let mut kept = enrolled_with_unit(&t, ID, "wss://first.example/");
    kept.relay = "wss://other.example/".into();
    write_v1(&t, &[kept.clone()]);
    assert_eq!(
        Store::open_with(&t.paths, || panic!("asked"))
            .unwrap()
            .agents,
        vec![Agent::from_persona(kept)]
    );
}

#[test]
fn personas_without_a_relay_or_unit_take_the_first_community() {
    let t = TempHome::new();
    let old = write_old_store(&t, &[persona(ID, "/w"), persona(OTHER, "/x")]);
    // No community configured: refused, nothing written.
    assert_eq!(
        Store::open_with(&t.paths, || None).err(),
        Some("store_invalid")
    );
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);
    // A configured but non-canonical relay is refused as invalid, too.
    assert_eq!(
        Store::open_with(&t.paths, || Some("wss://First.example".into())).err(),
        Some("store_invalid")
    );
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);
    let store = Store::open_with(&t.paths, || Some("wss://first.example/".into())).unwrap();
    assert!(store
        .agents
        .iter()
        .all(|a| a.first().relay == "wss://first.example/"));
    assert_eq!(Store::open(&t.paths).unwrap().agents, store.agents);
}

#[test]
fn unreadable_or_ambiguous_unit_files_refuse_the_migration() {
    let t = TempHome::new();
    enrolled_with_unit(&t, ID, "wss://first.example/");
    let unit = t.paths.unit_file(ID);
    let good = std::fs::read_to_string(&unit).unwrap();
    let old = write_old_store(&t, &[persona(ID, "/w")]);
    let exec = good
        .lines()
        .find(|l| l.starts_with("ExecStart="))
        .unwrap()
        .to_owned();
    for bad in [
        good.replace(r#" "--relay" "wss://first.example/""#, ""),
        good.replace(
            r#""--relay" "wss://first.example/""#,
            r#""--relay" "wss://First.example""#,
        ),
        good.replace(
            r#""--relay" "wss://first.example/""#,
            r#"--relay wss://first.example/"#,
        ),
        good.replace(
            r#""--relay" "wss://first.example/""#,
            r#""--relay" "wss://first.example/" "--relay" "wss://second.example/""#,
        ),
        good.replace(r#""wss://first.example/""#, r#""wss://first.example/"#),
        format!("{good}{exec}\n"),
        good.replace("ExecStart=", "#ExecStart="),
        "x".repeat(70 * 1024),
    ] {
        std::fs::write(&unit, &bad).unwrap();
        assert_eq!(
            Store::open_with(&t.paths, || Some("wss://fallback.example/".into())).err(),
            Some("store_invalid"),
            "{bad}"
        );
        assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);
    }
    // A linked unit file is refused, not followed.
    let elsewhere = t.paths.home.join("elsewhere.service");
    std::fs::write(&elsewhere, &good).unwrap();
    std::fs::remove_file(&unit).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &unit).unwrap();
    assert_eq!(
        Store::open_with(&t.paths, || Some("wss://fallback.example/".into())).err(),
        Some("store_invalid")
    );
    std::fs::remove_file(&unit).unwrap();
    // The generated file itself parses, escapes included.
    std::fs::write(&unit, &good).unwrap();
    assert_eq!(
        unit_relay(&t.paths, ID).unwrap().as_deref(),
        Some("wss://first.example/")
    );
    assert_eq!(unit_relay(&t.paths, OTHER).unwrap(), None);
    assert_eq!(
        unquote_words(r#""a b" "c\"d" "e\\f" "%%h" "$$X""#).unwrap(),
        ["a b", "c\"d", "e\\f", "%h", "$X"]
    );
    for bad in [
        r#"a"#,
        r#""a"b"#,
        r#""a"  "b""#,
        r#""%h""#,
        r#""\n""#,
        r#""open"#,
    ] {
        assert!(unquote_words(bad).is_none(), "{bad}");
    }
}

#[test]
fn the_largest_valid_store_fits_its_bound() {
    let t = TempHome::new();
    let owner = nostr::Keys::generate();
    let room = |n: usize| format!("00000000-0000-4000-8000-{n:012}");
    let mut store = Store::open(&t.paths).unwrap();
    store.agents = (0..MAX_AGENTS)
        .map(|n| {
            let agent = nostr::Keys::generate();
            let mut p = persona(&format!("00000000-0000-4000-8000-{n:012}"), "/w");
            p.name = "\u{10FFFF}".repeat(64);
            p.description = "\u{10FFFF}".repeat(256);
            // Every character escaped in JSON.
            p.instructions = "\"".repeat(INSTRUCTIONS_BYTES);
            p.model = "m".repeat(64);
            p.identity = Some(agent.public_key().to_hex());
            p.auth_tag =
                Some(buzz_sdk::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap());
            p.last_error = Some("x".repeat(32));
            p.published_at = u64::MAX;
            let mut a = Agent::from_persona(p.clone());
            for i in 1..MAX_INSTANCES {
                let mut other = p.clone();
                other.relay = format!("wss://{i}{}/", "r".repeat(2030));
                other.primary = false;
                a.absorb(other);
            }
            for (i, instance) in a.instances.iter_mut().enumerate() {
                if i == 0 {
                    instance.relay = format!("wss://0{}/", "r".repeat(2030));
                }
                instance.rooms = (0..8).map(room).collect();
                instance.member_rooms = (0..MAX_MEMBER_ROOMS).map(room).collect();
                instance.workspace = format!("/{}", "w".repeat(PATH_BYTES - 1));
            }
            assert!(a.valid());
            a
        })
        .collect();
    store.save().unwrap();
    let size = std::fs::metadata(t.paths.store_file()).unwrap().len();
    assert!(size <= STORE_BYTES, "{size}");
    assert_eq!(Store::open(&t.paths).unwrap().agents, store.agents);
}

/// The maintainer's machine: one enrolled, published, running agent written
/// by 0.0.26 (version 1, with `relay`) or earlier (without), its generated
/// unit file and its default workspace. Wrapping must keep its unit name,
/// workspace, rooms and memberships, and render the same unit file.
#[test]
fn a_running_version_1_agent_keeps_its_unit_and_workspace() {
    for with_relay in [true, false] {
        let t = TempHome::new();
        let id = "5b2c8f4e-3d1a-4e6b-9c7d-0a1b2c3d4e5f";
        let relay_url = "wss://relay.example.org/";
        let owner = nostr::Keys::generate();
        let agent = nostr::Keys::generate();
        let workspace = create_default_workspace(&t.paths, id).unwrap();
        let workspace = workspace.to_str().unwrap().to_owned();
        assert!(workspace.ends_with(&format!("/omarchy-buzz-room-workspaces/{id}")));
        let mut legacy = persona(id, &workspace);
        legacy.name = "vClaude".into();
        legacy.harness = "claude-code".into();
        legacy.model = "opus".into();
        legacy.answers_dms = true;
        legacy.rooms = vec![ROOM_A.into(), ROOM_B.into()];
        legacy.member_rooms = vec![ROOM_A.into(), ROOM_B.into()];
        legacy.relay = relay_url.into();
        legacy.identity = Some(agent.public_key().to_hex());
        legacy.auth_tag =
            Some(buzz_sdk::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap());
        legacy.published = true;
        legacy.published_at = 1_790_000_000;
        legacy.start_at_login = true;
        let owner_hex = owner.public_key().to_hex();
        let unit_text = super::super::unit::render(&t.paths, &legacy, &owner_hex).unwrap();
        std::fs::create_dir_all(t.paths.units_dir()).unwrap();
        let unit_path = t
            .paths
            .units_dir()
            .join(format!("omarchy-buzz-agent-{id}.service"));
        std::fs::write(&unit_path, &unit_text).unwrap();
        let old = if with_relay {
            write_v1(&t, &[legacy.clone()])
        } else {
            write_old_store(&t, &[legacy.clone()])
        };

        let store = Store::open_with(&t.paths, || panic!("the unit file names the relay")).unwrap();
        assert_eq!(store.agents.len(), 1);
        let wrapped = &store.agents[0];
        assert_eq!(wrapped.instances.len(), 1);
        let instance = wrapped.first();
        assert!(instance.primary && wrapped.instances[0].primary);
        assert_eq!(instance.key(), id);
        assert_eq!(
            unit_name(&instance.key()),
            format!("omarchy-buzz-agent-{id}.service")
        );
        assert_eq!(t.paths.unit_file(&instance.key()), unit_path);
        assert_eq!(instance.workspace, workspace);
        assert_eq!(
            t.paths.default_workspace(&instance.key()).to_str(),
            Some(workspace.as_str())
        );
        // Every field the record had is kept as it was.
        assert_eq!(instance, legacy);
        // The unit the service would write for it is the running one, byte
        // for byte; the file itself was only read.
        assert_eq!(
            super::super::unit::render(&t.paths, &instance, &owner_hex).unwrap(),
            unit_text
        );
        assert_eq!(std::fs::read_to_string(&unit_path).unwrap(), unit_text);
        // Written as version 2 once, the version 1 bytes kept beside it.
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(t.paths.store_file()).unwrap()).unwrap();
        assert_eq!(written["version"], 2);
        assert_eq!(written["agents"][0]["instances"][0]["primary"], true);
        assert_eq!(
            std::fs::read(t.paths.store_dir().join("personas.v1.json")).unwrap(),
            old
        );
        let again = Store::open_with(&t.paths, || panic!("asked again")).unwrap();
        assert_eq!(again.agents, store.agents);
    }
}

#[test]
fn a_failed_wrap_changes_nothing() {
    let t = TempHome::new();
    let old = write_v1(&t, &[persona(ID, "/w")]);
    // The backup cannot be written (a directory is in the way): refused.
    std::fs::create_dir(t.paths.store_dir().join("personas.v1.json")).unwrap();
    assert_eq!(Store::open(&t.paths).err(), Some("store_unavailable"));
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);
    std::fs::remove_dir(t.paths.store_dir().join("personas.v1.json")).unwrap();
    // An invalid version 1 record is refused before anything is written.
    let mut bad = persona(ID, "/w");
    bad.rooms.clear();
    let old = write_v1(&t, &[bad]);
    assert_eq!(Store::open(&t.paths).err(), Some("store_invalid"));
    assert_eq!(std::fs::read(t.paths.store_file()).unwrap(), old);
    assert!(!t.paths.store_dir().join("personas.v1.json").exists());
}

#[test]
fn instances_have_their_own_unit_names_and_workspaces() {
    let t = TempHome::new();
    let owner = nostr::Keys::generate();
    let agent = nostr::Keys::generate();
    let mut first = persona(ID, "/w");
    first.identity = Some(agent.public_key().to_hex());
    first.auth_tag =
        Some(buzz_sdk::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap());
    let mut second = first.clone();
    second.relay = "wss://second.example/".into();
    second.primary = false;
    second.rooms = vec![ROOM_B.into()];
    second.workspace = "/v".into();
    // The suffix is the first 12 hex digits of SHA-256 of the canonical relay.
    let h = relay_hash("wss://second.example/");
    assert_eq!(h.len(), 12);
    assert!(h
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    // `printf 'wss://second.example/' | sha256sum`
    assert_eq!(h, "00b6bbbc333b");
    assert_eq!(second.key(), format!("{ID}-{h}"));
    assert_eq!(
        unit_name(&second.key()),
        format!("omarchy-buzz-agent-{ID}-{h}.service")
    );
    assert_eq!(
        t.paths.default_workspace(&second.key()),
        t.paths.workspaces().join(format!("{ID}-{h}"))
    );
    let mut a = Agent::from_persona(first.clone());
    a.absorb(second.clone());
    assert!(a.valid());
    assert_eq!(a.personas(), vec![first.clone(), second.clone()]);
    assert_eq!(a.persona("wss://second.example/"), Some(second.clone()));
    // Distinct communities, at most one primary, at most four instances; an
    // agent without an identity has only one.
    let mut twice = a.clone();
    twice.instances.push(twice.instances[1].clone());
    assert!(!twice.valid());
    let mut primaries = a.clone();
    primaries.instances[1].primary = true;
    assert!(!primaries.valid());
    let mut many = a.clone();
    for n in 2..=MAX_INSTANCES {
        let mut i = many.instances[1].clone();
        i.relay = format!("wss://c{n}.example/");
        many.instances.push(i);
    }
    assert!(!many.valid());
    many.instances.pop();
    assert!(many.valid());
    let mut anonymous = a.clone();
    anonymous.identity = None;
    anonymous.auth_tag = None;
    assert!(!anonymous.valid());
    anonymous.instances.truncate(1);
    assert!(anonymous.valid());
    let mut empty = a.clone();
    empty.instances.clear();
    assert!(!empty.valid());
    // Every instance passes the persona rules.
    let mut bad = a.clone();
    bad.instances[1].rooms.clear();
    assert!(!bad.valid());
    // A primary that was left: the remaining instance keeps its suffixed name.
    let mut left = a.clone();
    left.instances.remove(0);
    assert!(left.valid());
    assert_eq!(left.first().key(), format!("{ID}-{h}"));

    // Workspace rules treat instances as separate agents: an instance's own
    // default workspace is allowed, another instance's (of the same agent) is not.
    let own = create_default_workspace(&t.paths, &second.key()).unwrap();
    let own = own.to_str().unwrap();
    let primary_ws = create_default_workspace(&t.paths, ID).unwrap();
    let mut first_at_default = first.clone();
    first_at_default.workspace = primary_ws.to_str().unwrap().into();
    check_workspace(&t.paths, own, &second.key(), &[&first_at_default]).unwrap();
    assert_eq!(
        check_workspace(&t.paths, own, ID, &[]),
        Err("workspace_refused")
    );
    assert_eq!(
        check_workspace(
            &t.paths,
            primary_ws.to_str().unwrap(),
            &second.key(),
            &[&first_at_default]
        ),
        Err("workspace_refused")
    );
}
