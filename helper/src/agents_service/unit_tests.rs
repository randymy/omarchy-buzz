use super::*;
use crate::agents_service::test_support::{persona, ROOM_A, ROOM_B};

const ID: &str = "3f2b8c1e-5d4a-4b6e-9c7d-0a1b2c3d4e5f";
// Public keys of the NIP-OA test-vector secrets 1 (owner) and 2 (agent).
const OWNER: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const AGENT: &str = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
const RELAY: &str = "wss://relay.example/";

fn example_paths() -> Paths {
    let home = PathBuf::from("/home/example");
    Paths {
        state: home.join(".local/state"),
        config: home.join(".config"),
        data: home.join(".local/share"),
        home,
    }
}
// NIP-OA specification vector (conditions `kind=1&created_at<1713957000`):
// the owner's attestation for AGENT.
const AUTH_TAG: &str = r#"["auth","79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798","kind=1&created_at<1713957000","8b7df2575caf0a108374f8471722b233c53f9ff827a8b0f91861966c3b9dd5cb2e189eae9f49d72187674c2f5bd244145e10ff86c9f257ffe65a1ee5f108b369"]"#;

fn enrolled() -> Persona {
    let mut p = persona(ID, "/home/example/work/scout");
    p.identity = Some(AGENT.into());
    p.auth_tag = Some(AUTH_TAG.into());
    p.rooms = vec![ROOM_A.into(), ROOM_B.into()];
    p.model = "gpt-5.1-codex".into();
    p
}

#[test]
fn renders_the_reviewed_template_exactly() {
    let rendered = render(&example_paths(), &enrolled(), RELAY, OWNER).unwrap();
    assert_eq!(rendered, include_str!("testdata/agent.service"));
}

#[test]
fn exec_start_is_the_contract_argv() {
    let paths = example_paths();
    let mut p = enrolled();
    assert_eq!(
        exec_argv(&paths, &p, RELAY, OWNER).unwrap(),
        [
            "/home/example/.local/share/omarchy-buzz/agent-codex/launcher/room-agent",
            "--harness",
            "codex",
            "--profile",
            "/home/example/.local/state/omarchy-buzz-agent-preview/codex",
            "--workspace",
            "/home/example/work/scout",
            "--bundle",
            "/home/example/.local/share/omarchy-buzz/agent-codex",
            "--relay",
            RELAY,
            "--room",
            ROOM_A,
            "--room",
            ROOM_B,
            "--owner",
            OWNER,
            "--identity",
            AGENT,
            "--respond-to",
            "owner-only",
            "--auth-tag",
            &format!("/home/example/.local/state/omarchy-buzz/agents/{ID}/auth-tag.json"),
            "--instructions",
            &format!("/home/example/.local/state/omarchy-buzz/agents/{ID}/instructions.md"),
            "--model",
            "gpt-5.1-codex",
        ]
    );
    // `mentions` is passed literally; empty instructions and model are omitted.
    p.harness = "claude-code".into();
    p.respond_to = "mentions".into();
    p.instructions.clear();
    p.model.clear();
    let argv = exec_argv(&paths, &p, RELAY, OWNER).unwrap();
    assert_eq!(
        argv[0],
        "/home/example/.local/share/omarchy-buzz/agent-claude-code/launcher/room-agent"
    );
    assert_eq!(
        argv[4],
        "/home/example/.local/state/omarchy-buzz-agent-preview/claude-code"
    );
    assert_eq!(
        &argv[argv.len() - 4..],
        [
            "--respond-to",
            "mentions",
            "--auth-tag",
            &format!("/home/example/.local/state/omarchy-buzz/agents/{ID}/auth-tag.json")
        ]
    );
    assert!(
        !argv.contains(&"--instructions".to_string()) && !argv.contains(&"--model".to_string())
    );
}

#[test]
fn refuses_unrenderable_scopes() {
    let paths = example_paths();
    let p = enrolled();
    let mut unenrolled = p.clone();
    unenrolled.identity = None;
    assert!(exec_argv(&paths, &unenrolled, RELAY, OWNER).is_err());
    let mut unattested = p.clone();
    unattested.auth_tag = None;
    assert!(
        exec_argv(&paths, &unattested, RELAY, OWNER).is_err(),
        "an enrolled agent always carries its attestation"
    );
    let other_owner = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    assert!(
        exec_argv(&paths, &p, RELAY, other_owner).is_err(),
        "the attestation must be the current owner's"
    );
    assert!(
        exec_argv(&paths, &p, RELAY, AGENT).is_err(),
        "owner equals agent"
    );
    assert!(exec_argv(&paths, &p, "wss://relay.example/path", OWNER).is_err());
    assert!(exec_argv(&paths, &p, "http://relay.example/", OWNER).is_err());
    assert!(exec_argv(&paths, &p, RELAY, "not-a-key").is_err());
}

#[test]
fn every_word_is_quoted_against_systemd_expansion() {
    let paths = example_paths();
    let mut p = enrolled();
    p.workspace = "/home/example/a b/%h/$HOME/\"q\"/back\\slash".into();
    let text = render(&paths, &p, RELAY, OWNER).unwrap();
    let exec = text.lines().find(|l| l.starts_with("ExecStart=")).unwrap();
    assert!(
        exec.contains(r#""/home/example/a b/%%h/$$HOME/\"q\"/back\\slash""#),
        "{exec}"
    );
    assert_eq!(
        text.lines().filter(|l| l.starts_with("ExecStart=")).count(),
        1
    );
    assert!(quote("a\nb").is_err());
}

#[test]
fn unit_control_argv_is_fixed_and_names_are_checked() {
    let unit = store::unit_name(ID);
    assert_eq!(
        op_argv(Op::Start, &unit).unwrap(),
        ["/usr/bin/systemctl", "--user", "start", unit.as_str()]
    );
    assert_eq!(op_argv(Op::Stop, &unit).unwrap()[2], "stop");
    assert_eq!(op_argv(Op::Enable, &unit).unwrap()[2], "enable");
    assert_eq!(op_argv(Op::Disable, &unit).unwrap()[2], "disable");
    assert_eq!(
        reload_argv(),
        ["/usr/bin/systemctl", "--user", "daemon-reload"]
    );
    assert_eq!(
        show_argv(&unit).unwrap(),
        [
            "/usr/bin/systemctl",
            "--user",
            "show",
            "--property=ActiveState",
            unit.as_str()
        ]
    );
    for bad in [
        "omarchy-buzz.service",
        "omarchy-buzz-agent-x.service",
        "omarchy-buzz-agent-3f2b8c1e-5d4a-4b6e-9c7d-0a1b2c3d4e5f.service; rm -rf ~",
        "--now",
    ] {
        assert!(op_argv(Op::Start, bad).is_err(), "{bad}");
        assert!(show_argv(bad).is_err(), "{bad}");
    }
}

#[test]
fn show_output_maps_to_contract_states() {
    assert_eq!(unit_state("ActiveState=active\n"), "active");
    assert_eq!(unit_state("ActiveState=inactive\n"), "inactive");
    assert_eq!(unit_state("ActiveState=failed\n"), "failed");
    for other in [
        "ActiveState=activating\n",
        "ActiveState=deactivating\n",
        "ActiveState=reloading\n",
        "",
        "garbage",
    ] {
        assert_eq!(unit_state(other), "unknown", "{other}");
    }
}

#[test]
fn fake_control_records_real_argv_and_tracks_state() {
    let fake = FakeControl::default();
    let unit = store::unit_name(ID);
    assert_eq!(fake.state(&unit), "inactive");
    fake.run(Op::Enable, &unit).unwrap();
    fake.run(Op::Start, &unit).unwrap();
    assert_eq!(fake.state(&unit), "active");
    fake.run(Op::Stop, &unit).unwrap();
    assert_eq!(fake.state(&unit), "inactive");
    fake.units
        .lock()
        .unwrap()
        .insert(unit.clone(), ("failed", true));
    assert_eq!(fake.state(&unit), "failed");
    fake.failing.lock().unwrap().push(Op::Start);
    assert_eq!(fake.run(Op::Start, &unit), Err("unit_failed"));
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls[1], op_argv(Op::Enable, &unit).unwrap());
    assert_eq!(calls[2], op_argv(Op::Start, &unit).unwrap());
    assert!(fake.run(Op::Start, "../../etc").is_err());
}

#[test]
fn bounded_runs_never_use_a_shell_and_time_out() {
    let (code, out) = run_bounded(
        &["/usr/bin/printf".into(), "%s".into(), "a;b $(x)".into()],
        Duration::from_secs(5),
        true,
    )
    .unwrap();
    assert_eq!((code, out.as_slice()), (Some(0), b"a;b $(x)".as_slice()));
    let started = Instant::now();
    let (code, _) = run_bounded(
        &["/usr/bin/sleep".into(), "5".into()],
        Duration::from_millis(100),
        false,
    )
    .unwrap();
    assert_eq!(code, None);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(run_bounded(
        &["/nonexistent/program".into()],
        Duration::from_secs(1),
        false
    )
    .is_err());
}
