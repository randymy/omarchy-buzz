//! The service through its request surface, with fake unit control, keyring,
//! spawner and room source, a temporary home and a loopback relay.
use super::*;
use crate::agents_service::{
    harness::{self, FakeSpawner},
    keys::FakeKeyring,
    request,
    rooms::FixedRooms,
    test_support::{mode, relay, Answer, TempHome, ROOM_A, ROOM_B, ROOM_C},
    unit::FakeControl,
};

struct Fixture {
    home: TempHome,
    control: Arc<FakeControl>,
    keyring: Arc<FakeKeyring>,
    spawner: Arc<FakeSpawner>,
    rooms: Arc<FixedRooms>,
    owner: nostr::Keys,
    /// What the configuration loader returns; tests switch communities here.
    config: Arc<Mutex<Config>>,
    service: Arc<Service>,
    serial: std::cell::Cell<u32>,
}

fn fixture(relay_url: &str) -> Fixture {
    let home = TempHome::new();
    let control = Arc::new(FakeControl::default());
    let keyring = Arc::new(FakeKeyring::default());
    let spawner = Arc::new(FakeSpawner::default());
    let rooms = Arc::new(FixedRooms::new(vec![
        ROOM_A.into(),
        ROOM_B.into(),
        ROOM_C.into(),
    ]));
    let owner = nostr::Keys::generate();
    *keyring.owner.lock().unwrap() = Some(owner.clone());
    let config = Arc::new(Mutex::new(
        Config {
            relay: Some(relay_url.into()),
            identity: Some(owner.public_key().to_hex()),
            communities: Vec::new(),
        }
        .normalized(),
    ));
    let loader = config.clone();
    let service = Service::open(
        home.paths.clone(),
        Deps {
            control: control.clone(),
            keyring: keyring.clone(),
            spawner: spawner.clone(),
            rooms: rooms.clone(),
        },
        Box::new(move || Ok(loader.lock().unwrap().clone())),
    )
    .unwrap();
    Fixture {
        home,
        control,
        keyring,
        spawner,
        rooms,
        owner,
        config,
        service,
        serial: std::cell::Cell::new(0),
    }
}

impl Fixture {
    fn request(&self, value: serde_json::Value) -> Request {
        self.serial.set(self.serial.get() + 1);
        let mut frame = serde_json::json!({"version":1,"instanceId":"test",
            "id":format!("00000000-0000-4000-8000-{:012}", self.serial.get())});
        frame
            .as_object_mut()
            .unwrap()
            .extend(value.as_object().unwrap().clone());
        request::parse(&serde_json::to_vec(&frame).unwrap()).unwrap()
    }
    async fn run(&self, value: serde_json::Value) -> Result<(), &'static str> {
        let r = self.request(value);
        let slot = self.service.begin().expect("mutation slot");
        let result = self.service.execute(&r, slot).await;
        let pending = self.service.snapshot().pending.unwrap();
        assert_eq!(pending.request_id, r.id);
        assert_eq!(pending.kind, r.kind);
        match result {
            Ok(()) => assert_eq!((pending.state, pending.category), ("done", None)),
            Err(category) => assert_eq!(
                (pending.state, pending.category),
                ("failed", Some(category))
            ),
        }
        result
    }
    async fn create(&self, fields: serde_json::Value) -> Result<String, &'static str> {
        let mut all = fields_json();
        all.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        self.run(serde_json::json!({"type":"create_agent","fields":all}))
            .await?;
        Ok(self.service.snapshot().agents.last().unwrap().id.clone())
    }
    fn agent(&self, id: &str) -> AgentView {
        self.service
            .snapshot()
            .agents
            .into_iter()
            .find(|a| a.id == id)
            .unwrap()
    }
    fn stored(&self, id: &str) -> Persona {
        self.service.store.lock().unwrap().get(id).unwrap().first()
    }
    fn ready(&self, harness: &str) {
        *self.spawner.present.lock().unwrap() = true;
        let p = &self.home.paths;
        let mut outputs = self.spawner.outputs.lock().unwrap();
        outputs.insert(
            super::harness::check_argv(p, harness).join(" "),
            "ready\n".into(),
        );
        outputs.insert(
            super::harness::status_argv(p, harness).join(" "),
            "signed-in\n".into(),
        );
    }
}

fn fields_json() -> serde_json::Value {
    serde_json::json!({"name":"Scout","description":"Reads the logs.","instructions":"Answer briefly.",
        "harness":"codex","model":"","rooms":[ROOM_A],"respondTo":"owner-only","workspace":"",
        "startAtLogin":false,"acpCommand":"buzz-acp","answersDms":false})
}
const UNREACHABLE: &str = "ws://127.0.0.1:9/";

#[tokio::test]
async fn create_validates_defaults_and_persists() {
    let f = fixture(UNREACHABLE);
    let id = f.create(serde_json::json!({})).await.unwrap();
    let agent = f.agent(&id);
    assert!(uuid::Uuid::parse_str(&id).unwrap().get_version_num() == 4);
    let default = f.home.paths.default_workspace(&id);
    assert_eq!(agent.workspace, default.to_str().unwrap());
    assert_eq!(mode(&default), 0o700);
    assert_eq!(
        (
            agent.identity.clone(),
            agent.enrolled,
            agent.unit,
            agent.published,
            agent.last_error.clone()
        ),
        (None, false, "inactive", false, None)
    );
    assert_eq!(mode(&f.home.paths.store_file()), 0o600);
    // Persisted: a new service instance reads the same persona back.
    assert_eq!(
        store::Store::open(&f.home.paths).unwrap().agents,
        f.service.store.lock().unwrap().agents
    );
    // A chosen private workspace is accepted as given.
    let chosen = f.home.private_dir("work/two");
    let second = f
        .create(serde_json::json!({"workspace":chosen}))
        .await
        .unwrap();
    assert_eq!(f.agent(&second).workspace, chosen);
    // ... but never another agent's.
    assert_eq!(
        f.create(serde_json::json!({"workspace":chosen})).await,
        Err("workspace_refused")
    );
    assert_eq!(
        f.create(serde_json::json!({"workspace":f.home.paths.home.to_str().unwrap()}))
            .await,
        Err("workspace_refused")
    );
    for bad in [
        serde_json::json!({"name":""}),
        serde_json::json!({"name":"x\u{202e}"}),
        serde_json::json!({"harness":"goose"}),
        serde_json::json!({"acpCommand":"sh"}),
        serde_json::json!({"model":"a b"}),
        serde_json::json!({"rooms":[]}),
        serde_json::json!({"respondTo":"anyone"}),
        serde_json::json!({"workspace":"relative/path"}),
        serde_json::json!({"instructions":"x".repeat(16385)}),
    ] {
        assert_eq!(f.create(bad.clone()).await, Err("agent_invalid"), "{bad}");
    }
    // Rooms must be in the helper's verified joined-room list.
    assert_eq!(
        f.create(serde_json::json!({"rooms":["00000000-0000-4000-8000-0000000000ff"]}))
            .await,
        Err("agent_invalid")
    );
    *f.rooms.0.lock().unwrap() = Err("relay_unavailable");
    assert_eq!(
        f.create(serde_json::json!({})).await,
        Err("relay_unavailable")
    );
    assert_eq!(f.service.snapshot().agents.len(), 2);
}

#[tokio::test]
async fn at_most_sixteen_agents() {
    let f = fixture(UNREACHABLE);
    for _ in 0..16 {
        f.create(serde_json::json!({})).await.unwrap();
    }
    assert_eq!(f.create(serde_json::json!({})).await, Err("agent_limit"));
    assert_eq!(f.service.snapshot().agents.len(), 16);
}

#[tokio::test]
async fn unconfigured_helper_cannot_verify_rooms() {
    let f = fixture(UNREACHABLE);
    let service = Service::open(
        f.home.paths.clone(),
        Deps {
            control: f.control.clone(),
            keyring: f.keyring.clone(),
            spawner: f.spawner.clone(),
            rooms: f.rooms.clone(),
        },
        Box::new(|| Ok(Config::default())),
    )
    .unwrap();
    let r = f.request(serde_json::json!({"type":"create_agent","fields":fields_json()}));
    assert_eq!(
        service.execute(&r, service.begin().unwrap()).await,
        Err("relay_unavailable")
    );
}

#[tokio::test]
async fn one_mutation_at_a_time() {
    let f = fixture(UNREACHABLE);
    let held = f.service.begin().unwrap();
    assert!(f.service.begin().is_none());
    assert!(!f.service.idle());
    drop(held);
    assert!(f.service.begin().is_some());
}

#[tokio::test]
async fn enroll_start_stop_login_and_delete_through_fakes() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let f = fixture(&relay.url);
    let id = f
        .create(serde_json::json!({"rooms":[ROOM_A, ROOM_B]}))
        .await
        .unwrap();
    // Not enrolled yet: nothing can start.
    assert_eq!(
        f.run(serde_json::json!({"type":"start_agent","agentId":id}))
            .await,
        Err("agent_invalid")
    );
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    let agent = f.agent(&id);
    let identity = agent.identity.clone().unwrap();
    assert!(agent.enrolled && agent.published && agent.last_error.is_none());
    assert_eq!(agent.unit, "unknown");
    // The secret lives only in the (fake) keyring, as 64 lowercase hex.
    let secret = f
        .keyring
        .agents
        .lock()
        .unwrap()
        .get(&identity)
        .unwrap()
        .clone();
    assert!(
        secret.len() == 64
            && secret
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let stored_file = std::fs::read_to_string(f.home.paths.store_file()).unwrap();
    assert!(!stored_file.contains(secret.as_str()));
    let persona = f.stored(&id);
    assert_eq!(
        persona.member_rooms,
        vec![ROOM_A.to_string(), ROOM_B.to_string()]
    );
    assert_eq!(
        buzz_sdk::nip_oa::verify_auth_tag(
            persona.auth_tag.as_deref().unwrap(),
            &nostr::PublicKey::from_hex(&identity).unwrap()
        )
        .unwrap(),
        f.owner.public_key()
    );
    assert_eq!(mode(&f.home.paths.agent_dir(&id)), 0o700);
    assert_eq!(mode(&f.home.paths.auth_tag_file(&id)), 0o600);
    assert_eq!(mode(&f.home.paths.instructions_file(&id)), 0o600);
    assert_eq!(
        std::fs::read_to_string(f.home.paths.instructions_file(&id)).unwrap(),
        "Answer briefly."
    );
    let published: Vec<u16> = relay
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.event.kind.as_u16())
        .collect();
    assert_eq!(published, [30175, 30177, 9000, 9000, 0]);
    // Enrolling again keeps the identity and does not re-add rooms.
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(f.agent(&id).identity.as_deref(), Some(identity.as_str()));
    assert_eq!(
        relay
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.event.kind.as_u16() == 9000)
            .count(),
        2
    );

    // Harness readiness gates start.
    assert_eq!(
        f.run(serde_json::json!({"type":"start_agent","agentId":id}))
            .await,
        Err("harness_missing")
    );
    f.ready("codex");
    f.spawner.outputs.lock().unwrap().insert(
        super::harness::status_argv(&f.home.paths, "codex").join(" "),
        "signed-out\n".into(),
    );
    assert_eq!(
        f.run(serde_json::json!({"type":"start_agent","agentId":id}))
            .await,
        Err("not_signed_in")
    );
    let codex = f
        .service
        .snapshot()
        .harnesses
        .into_iter()
        .find(|h| h.id == "codex")
        .unwrap();
    assert_eq!((codex.bundle, codex.signed_in), ("ready", Some(false)));
    f.ready("codex");
    f.control.calls.lock().unwrap().clear();
    f.run(serde_json::json!({"type":"start_agent","agentId":id}))
        .await
        .unwrap();
    let name = store::unit_name(&id);
    let unit_file = f.home.paths.unit_file(&id);
    assert_eq!(mode(&unit_file), 0o600);
    assert_eq!(
        std::fs::read_to_string(&unit_file).unwrap(),
        unit::render(
            &f.home.paths,
            &f.stored(&id),
            &f.owner.public_key().to_hex()
        )
        .unwrap()
    );
    let calls = f.control.calls.lock().unwrap().clone();
    assert_eq!(calls[0], unit::reload_argv());
    assert_eq!(calls[1], unit::op_argv(Op::Start, &name).unwrap());
    assert_eq!(calls[2], unit::show_argv(&name).unwrap());
    assert_eq!(f.agent(&id).unit, "active");

    // A failed unit is reported, then stop maps back to inactive.
    f.control
        .units
        .lock()
        .unwrap()
        .insert(name.clone(), ("failed", false));
    f.service.inspect_units().await;
    assert_eq!(f.agent(&id).unit, "failed");
    f.run(serde_json::json!({"type":"stop_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(f.agent(&id).unit, "inactive");
    f.control.failing.lock().unwrap().push(Op::Stop);
    assert_eq!(
        f.run(serde_json::json!({"type":"stop_agent","agentId":id}))
            .await,
        Err("unit_failed")
    );
    f.control.failing.lock().unwrap().clear();

    // Start at login maps to enable/disable.
    f.control.calls.lock().unwrap().clear();
    f.run(serde_json::json!({"type":"set_start_at_login","agentId":id,"enabled":true}))
        .await
        .unwrap();
    assert!(f.agent(&id).start_at_login);
    assert!(f.control.units.lock().unwrap()[&name].1);
    assert!(f
        .control
        .calls
        .lock()
        .unwrap()
        .contains(&unit::op_argv(Op::Enable, &name).unwrap()));
    f.run(serde_json::json!({"type":"set_start_at_login","agentId":id,"enabled":false}))
        .await
        .unwrap();
    assert!(!f.agent(&id).start_at_login && !f.control.units.lock().unwrap()[&name].1);

    // Delete stops, disables, removes the unit and private files; the identity
    // stays in the keyring unless `forget` is set.
    f.control.calls.lock().unwrap().clear();
    f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(
        *f.control.calls.lock().unwrap(),
        [
            unit::op_argv(Op::Stop, &name).unwrap(),
            unit::op_argv(Op::Disable, &name).unwrap(),
            unit::reload_argv()
        ]
    );
    assert!(!unit_file.exists() && !f.home.paths.agent_dir(&id).exists());
    assert!(f.service.snapshot().agents.is_empty());
    assert!(f.keyring.agents.lock().unwrap().contains_key(&identity));
    // The user's default workspace is kept.
    assert!(f.home.paths.default_workspace(&id).is_dir());
}

#[tokio::test]
async fn delete_with_forget_removes_the_identity() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let f = fixture(&relay.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    let identity = f.agent(&id).identity.unwrap();
    f.run(serde_json::json!({"type":"delete_agent","agentId":id,"forget":true}))
        .await
        .unwrap();
    assert!(!f.keyring.agents.lock().unwrap().contains_key(&identity));
    assert_eq!(
        f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
            .await,
        Err("agent_invalid")
    );
}

#[tokio::test]
async fn enrollment_failures_are_recorded_without_relay_text() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(
        |kind| {
            if kind == 30175 {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let f = fixture(&relay.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    assert_eq!(
        f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
            .await,
        Err("enroll_failed")
    );
    let agent = f.agent(&id);
    // The identity exists (so a retry reuses it) but nothing counts as published.
    assert!(agent.enrolled && !agent.published);
    assert_eq!(agent.last_error.as_deref(), Some("enroll_failed"));
    let frame = serde_json::to_string(&f.service.snapshot()).unwrap();
    assert!(!frame.contains("synthetic-sensitive-reason"));
    // No owner key: enrollment fails before anything is published.
    let g = fixture(&relay.url);
    let other = g.create(serde_json::json!({})).await.unwrap();
    *g.keyring.owner.lock().unwrap() = None;
    let before = relay.seen.lock().unwrap().len();
    assert_eq!(
        g.run(serde_json::json!({"type":"enroll_agent","agentId":other}))
            .await,
        Err("enroll_failed")
    );
    assert!(g.agent(&other).identity.is_none());
    assert_eq!(relay.seen.lock().unwrap().len(), before);
    // A keyring that cannot store the secret leaves the agent unenrolled.
    *g.keyring.owner.lock().unwrap() = Some(g.owner.clone());
    *g.keyring.fail_store.lock().unwrap() = true;
    assert_eq!(
        g.run(serde_json::json!({"type":"enroll_agent","agentId":other}))
            .await,
        Err("enroll_failed")
    );
    assert!(g.agent(&other).identity.is_none());
}

#[tokio::test]
async fn updates_stop_only_for_launch_fields_and_republish_enrolled_agents() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let f = fixture(&relay.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    // Unenrolled: saved without any relay traffic.
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"name":"Renamed"}}))
        .await
        .unwrap();
    assert_eq!(f.agent(&id).name, "Renamed");
    assert!(relay.seen.lock().unwrap().is_empty());
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    let name = store::unit_name(&id);
    f.control
        .units
        .lock()
        .unwrap()
        .insert(name.clone(), ("active", false));
    // A description edit republishes but does not stop the running agent.
    let before = relay.seen.lock().unwrap().len();
    f.run(
        serde_json::json!({"type":"update_agent","agentId":id,"fields":{"description":"New text"}}),
    )
    .await
    .unwrap();
    assert_eq!(f.control.units.lock().unwrap()[&name].0, "active");
    let kinds: Vec<u16> = relay.seen.lock().unwrap()[before..]
        .iter()
        .map(|s| s.event.kind.as_u16())
        .collect();
    assert_eq!(kinds, [30175, 30177, 0]);
    assert!(f.agent(&id).published);
    // A rooms edit stops it first and adds only the new room.
    let before = relay.seen.lock().unwrap().len();
    f.run(
        serde_json::json!({"type":"update_agent","agentId":id,"fields":{"rooms":[ROOM_A, ROOM_C]}}),
    )
    .await
    .unwrap();
    assert_eq!(f.control.units.lock().unwrap()[&name].0, "inactive");
    let seen = relay.seen.lock().unwrap()[before..].to_vec();
    let adds: Vec<_> = seen
        .iter()
        .filter(|s| s.event.kind.as_u16() == 9000)
        .collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(
        adds[0].event.tags.iter().next().unwrap().as_slice(),
        ["h", ROOM_C]
    );
    assert!(!seen.iter().any(|s| s.event.kind.as_u16() == 9001));
    assert_eq!(
        f.stored(&id).member_rooms,
        vec![ROOM_A.to_string(), ROOM_C.to_string()]
    );
    // A workspace edit alone stops a running agent but publishes nothing.
    f.control
        .units
        .lock()
        .unwrap()
        .insert(name.clone(), ("active", false));
    let before = relay.seen.lock().unwrap().len();
    let chosen = f.home.private_dir("work/moved");
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"workspace":chosen}}))
        .await
        .unwrap();
    assert_eq!(f.control.units.lock().unwrap()[&name].0, "inactive");
    assert_eq!(relay.seen.lock().unwrap().len(), before);
    // Answering direct messages changes the launch argv: it stops the agent
    // like a rooms edit, and publishes nothing.
    assert!(!f.agent(&id).answers_dms);
    f.control
        .units
        .lock()
        .unwrap()
        .insert(name.clone(), ("active", false));
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"answersDms":true}}))
        .await
        .unwrap();
    assert_eq!(f.control.units.lock().unwrap()[&name].0, "inactive");
    assert_eq!(relay.seen.lock().unwrap().len(), before);
    assert!(f.agent(&id).answers_dms && f.stored(&id).answers_dms);
    // Invalid edits change nothing.
    for bad in [
        serde_json::json!({"name":""}),
        serde_json::json!({"rooms":["00000000-0000-4000-8000-0000000000ff"]}),
        serde_json::json!({"acpCommand":"other"}),
    ] {
        assert_eq!(
            f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":bad}))
                .await,
            Err("agent_invalid")
        );
    }
    assert_eq!(f.agent(&id).workspace, chosen);
    // A failed republish leaves the edit saved and unpublished.
    drop(relay);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"model":"gpt-5"}}))
        .await
        .unwrap_err();
    let agent = f.agent(&id);
    assert_eq!(agent.model, "gpt-5");
    assert!(!agent.published);
    assert_eq!(agent.last_error.as_deref(), Some("relay_unavailable"));
}

static REJECT_REMOVALS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn new_kinds(
    relay: &crate::agents_service::test_support::Relay,
    before: usize,
) -> Vec<(u16, String)> {
    relay.seen.lock().unwrap()[before..]
        .iter()
        .map(|s| {
            let h = s
                .event
                .tags
                .iter()
                .find(|t| t.as_slice()[0] == "h")
                .map(|t| t.as_slice()[1].clone())
                .unwrap_or_default();
            (s.event.kind.as_u16(), h)
        })
        .collect()
}

#[tokio::test]
async fn dropped_rooms_and_deleted_agents_leave_the_relay_rooms() {
    use std::sync::atomic::Ordering::SeqCst;
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    REJECT_REMOVALS.store(false, SeqCst);
    let relay = relay(
        |kind| {
            if kind == 9001 && REJECT_REMOVALS.load(std::sync::atomic::Ordering::SeqCst) {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let f = fixture(&relay.url);
    let id = f
        .create(serde_json::json!({"rooms":[ROOM_A, ROOM_B]}))
        .await
        .unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    let both = vec![ROOM_A.to_string(), ROOM_B.to_string()];
    assert_eq!(f.stored(&id).member_rooms, both);
    let (a, b) = (ROOM_A.to_string(), ROOM_B.to_string());
    let rooms = |value: &[&str]| serde_json::json!({"type":"update_agent","agentId":id,"fields":{"rooms":value}});

    // A rejected removal keeps the membership recorded, to be left later.
    REJECT_REMOVALS.store(true, SeqCst);
    let before = relay.seen.lock().unwrap().len();
    assert_eq!(f.run(rooms(&[ROOM_A])).await, Err("enroll_failed"));
    assert_eq!(
        new_kinds(&relay, before),
        [
            (30175, String::new()),
            (30177, String::new()),
            (9001, b.clone())
        ]
    );
    let stored = f.stored(&id);
    assert_eq!(
        (stored.rooms.clone(), stored.member_rooms.clone()),
        (vec![a.clone()], both.clone())
    );
    assert_eq!(f.agent(&id).last_error.as_deref(), Some("enroll_failed"));
    // Taking the room back while still a member neither adds nor removes it.
    let before = relay.seen.lock().unwrap().len();
    f.run(rooms(&[ROOM_A, ROOM_B])).await.unwrap();
    assert_eq!(
        new_kinds(&relay, before),
        [
            (30175, String::new()),
            (30177, String::new()),
            (0, String::new())
        ]
    );
    // Dropping it again with a cooperative relay leaves it.
    REJECT_REMOVALS.store(false, SeqCst);
    let before = relay.seen.lock().unwrap().len();
    f.run(rooms(&[ROOM_A])).await.unwrap();
    assert_eq!(
        new_kinds(&relay, before),
        [
            (30175, String::new()),
            (30177, String::new()),
            (9001, b.clone()),
            (0, String::new())
        ]
    );
    assert_eq!(f.stored(&id).member_rooms, vec![a.clone()]);
    // Adding it back is one add, and ROOM_A is never added again.
    let before = relay.seen.lock().unwrap().len();
    f.run(rooms(&[ROOM_A, ROOM_B])).await.unwrap();
    assert_eq!(
        new_kinds(&relay, before),
        [
            (30175, String::new()),
            (30177, String::new()),
            (9000, b.clone()),
            (0, String::new())
        ]
    );
    assert_eq!(f.stored(&id).member_rooms, both);

    // A delete whose removal is rejected keeps the agent, its unit state and
    // the memberships not yet left, so it can be retried.
    REJECT_REMOVALS.store(true, SeqCst);
    f.control.calls.lock().unwrap().clear();
    let before = relay.seen.lock().unwrap().len();
    assert_eq!(
        f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
            .await,
        Err("enroll_failed")
    );
    assert_eq!(new_kinds(&relay, before), [(9001, a.clone())]);
    assert!(f.control.calls.lock().unwrap().is_empty());
    assert_eq!(f.stored(&id).member_rooms, both);
    assert!(f.home.paths.agent_dir(&id).exists());
    // The retry leaves every room, publishes nothing else and deletes.
    REJECT_REMOVALS.store(false, SeqCst);
    let before = relay.seen.lock().unwrap().len();
    f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(new_kinds(&relay, before), [(9001, a), (9001, b)]);
    assert!(f.service.snapshot().agents.is_empty());
    assert_eq!(
        relay
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.event.kind.as_u16() == 9000)
            .count(),
        3,
        "two at enrollment, one when ROOM_B was taken back"
    );
}

#[tokio::test]
async fn delete_skips_memberships_of_a_previous_owner() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let f = fixture(&relay.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    // The helper's owner identity changed: it did not attest or add the agent.
    let owner = nostr::Keys::generate();
    let keyring = Arc::new(FakeKeyring::default());
    *keyring.owner.lock().unwrap() = Some(owner.clone());
    let config = Config {
        relay: Some(relay.url.clone()),
        identity: Some(owner.public_key().to_hex()),
        communities: Vec::new(),
    }
    .normalized();
    let service = Service::open(
        f.home.paths.clone(),
        Deps {
            control: f.control.clone(),
            keyring,
            spawner: f.spawner.clone(),
            rooms: f.rooms.clone(),
        },
        Box::new(move || Ok(config.clone())),
    )
    .unwrap();
    let before = relay.seen.lock().unwrap().len();
    let r = f.request(serde_json::json!({"type":"delete_agent","agentId":id}));
    service.execute(&r, service.begin().unwrap()).await.unwrap();
    assert_eq!(relay.seen.lock().unwrap().len(), before);
    assert!(service.snapshot().agents.is_empty());
}

#[tokio::test]
async fn sign_in_only_starts_the_reviewed_script_for_known_harnesses() {
    let f = fixture(UNREACHABLE);
    let refused = f.request(serde_json::json!({"type":"sign_in","harness":"bash"}));
    assert_eq!(
        f.service
            .execute(&refused, f.service.begin().unwrap())
            .await,
        Err("agent_invalid")
    );
    assert_eq!(
        f.run(serde_json::json!({"type":"sign_in","harness":"codex"}))
            .await,
        Err("harness_missing")
    );
    *f.spawner.present.lock().unwrap() = true;
    f.run(serde_json::json!({"type":"sign_in","harness":"claude-code"}))
        .await
        .unwrap();
    let script = f.home.paths.data.join("omarchy-buzz/scripts/agent-login");
    assert_eq!(
        *f.spawner.spawned.lock().unwrap(),
        [vec![
            script.to_str().unwrap().to_string(),
            "claude-code".into()
        ]]
    );
    // Only the session variables reach the login script.
    let env = f.spawner.spawned_env.lock().unwrap()[0].clone();
    assert_eq!(env, harness::login_environment(std::env::vars_os()));
    assert!(env.iter().all(|(key, _)| {
        let key = key.to_str().unwrap();
        harness::LOGIN_ENV.contains(&key) || key.starts_with("XDG_")
    }));
}

#[tokio::test]
async fn harness_status_comes_from_the_reviewed_scripts() {
    let f = fixture(UNREACHABLE);
    f.service.inspect_harnesses().await;
    for h in f.service.snapshot().harnesses {
        assert_eq!(
            (h.bundle, h.signed_in),
            ("missing", None),
            "absent scripts are never run"
        );
    }
    assert!(f.spawner.calls.lock().unwrap().is_empty());
    f.ready("claude-code");
    f.spawner.outputs.lock().unwrap().insert(
        super::harness::status_argv(&f.home.paths, "codex").join(" "),
        "unknown\n".into(),
    );
    f.service.inspect_harnesses().await;
    let h = f.service.snapshot().harnesses;
    assert_eq!(h[0].id, "claude-code");
    assert_eq!((h[0].bundle, h[0].signed_in), ("ready", Some(true)));
    assert_eq!((h[1].bundle, h[1].signed_in), ("missing", None));
    // The installed checker runs, never the bundle's own copy.
    let data = &f.home.paths.data;
    assert!(f.spawner.calls.lock().unwrap().contains(&vec![
        data.join("omarchy-buzz/scripts/agent-bundle")
            .to_str()
            .unwrap()
            .to_string(),
        "--check".into(),
        "--output".into(),
        data.join("omarchy-buzz/agent-claude-code")
            .to_str()
            .unwrap()
            .to_string(),
        "--scripts".into(),
        data.join("omarchy-buzz/scripts")
            .to_str()
            .unwrap()
            .to_string(),
        "claude-code".into()
    ]));
}

/// Makes `harness`'s check print `word` with `code`, as `agent-bundle` does.
fn bundle_state(f: &Fixture, harness: &str, word: &str, code: i32) {
    let key = super::harness::check_argv(&f.home.paths, harness).join(" ");
    f.spawner
        .outputs
        .lock()
        .unwrap()
        .insert(key.clone(), format!("{word}\n"));
    f.spawner.codes.lock().unwrap().insert(key, code);
}
fn bundle_of(f: &Fixture, harness: &str) -> &'static str {
    f.service
        .snapshot()
        .harnesses
        .into_iter()
        .find(|h| h.id == harness)
        .unwrap()
        .bundle
}

#[tokio::test]
async fn stale_launchers_are_reported_only_with_the_stale_exit_status() {
    let f = fixture(UNREACHABLE);
    f.ready("codex");
    bundle_state(&f, "codex", "stale", 3);
    f.service.inspect_harnesses().await;
    assert_eq!(bundle_of(&f, "codex"), "stale");
    // `stale` with another status, or another word with status 3, is missing.
    for (word, code) in [("stale", 0), ("stale", 1), ("ready", 3), ("missing", 3)] {
        bundle_state(&f, "codex", word, code);
        f.service.inspect_harnesses().await;
        assert_eq!(bundle_of(&f, "codex"), "missing", "{word} {code}");
    }
    bundle_state(&f, "codex", "ready", 0);
    f.service.inspect_harnesses().await;
    assert_eq!(bundle_of(&f, "codex"), "ready");
}

#[tokio::test]
async fn refresh_bundle_runs_the_installed_script_and_reports_the_result() {
    let f = fixture(UNREACHABLE);
    let refused = f.request(serde_json::json!({"type":"refresh_bundle","harness":"bash"}));
    assert_eq!(
        f.service
            .execute(&refused, f.service.begin().unwrap())
            .await,
        Err("agent_invalid")
    );
    // Without the installed script nothing runs.
    assert_eq!(
        f.run(serde_json::json!({"type":"refresh_bundle","harness":"codex"}))
            .await,
        Err("harness_missing")
    );
    assert!(f.spawner.calls.lock().unwrap().is_empty());
    f.ready("codex");
    bundle_state(&f, "codex", "stale", 3);
    // The refresh did not help: still stale.
    assert_eq!(
        f.run(serde_json::json!({"type":"refresh_bundle","harness":"codex"}))
            .await,
        Err("bundle_stale")
    );
    let refresh = super::harness::refresh_argv(&f.home.paths, "codex");
    let data = &f.home.paths.data;
    assert_eq!(
        refresh,
        [
            data.join("omarchy-buzz/scripts/agent-bundle")
                .to_str()
                .unwrap(),
            "--refresh-launcher",
            "--output",
            data.join("omarchy-buzz/agent-codex").to_str().unwrap(),
            "--scripts",
            data.join("omarchy-buzz/scripts").to_str().unwrap(),
            "codex",
        ]
    );
    assert!(f.spawner.calls.lock().unwrap().contains(&refresh));
    // A refresh that left the bundle ready is done; readiness was re-read after it.
    bundle_state(&f, "codex", "ready", 0);
    f.spawner.calls.lock().unwrap().clear();
    f.run(serde_json::json!({"type":"refresh_bundle","harness":"codex"}))
        .await
        .unwrap();
    let calls = f.spawner.calls.lock().unwrap().clone();
    let check = super::harness::check_argv(&f.home.paths, "codex");
    let at = |argv: &Vec<String>| calls.iter().position(|c| c == argv).unwrap();
    assert!(at(&refresh) < at(&check));
    assert_eq!(bundle_of(&f, "codex"), "ready");
    // A bundle the refresh refused (for example a tampered adapter) is missing.
    bundle_state(&f, "codex", "missing", 1);
    assert_eq!(
        f.run(serde_json::json!({"type":"refresh_bundle","harness":"codex"}))
            .await,
        Err("harness_missing")
    );
}

#[tokio::test]
async fn start_refuses_a_stale_bundle_until_it_is_refreshed() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let relay = relay(|_| Answer::Accept, false).await;
    let f = fixture(&relay.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    f.ready("codex");
    bundle_state(&f, "codex", "stale", 3);
    f.control.calls.lock().unwrap().clear();
    assert_eq!(
        f.run(serde_json::json!({"type":"start_agent","agentId":id}))
            .await,
        Err("bundle_stale")
    );
    assert!(f.control.calls.lock().unwrap().is_empty());
    assert!(std::fs::symlink_metadata(f.home.paths.unit_file(&id)).is_err());
    // The refresh (here: the fake check now answers ready) lets it start.
    bundle_state(&f, "codex", "ready", 0);
    f.run(serde_json::json!({"type":"refresh_bundle","harness":"codex"}))
        .await
        .unwrap();
    f.run(serde_json::json!({"type":"start_agent","agentId":id}))
        .await
        .unwrap();
    assert!(!f.control.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn status_frames_have_exactly_the_contract_shape_and_fit_the_bound() {
    let f = fixture(UNREACHABLE);
    let id = f.create(serde_json::json!({})).await.unwrap();
    let frame = envelope("status", Some(&id), "1-2", &f.service.snapshot());
    let keys = |v: &serde_json::Value| {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(
        keys(&frame),
        [
            "capabilities",
            "id",
            "instanceId",
            "status",
            "type",
            "version"
        ]
    );
    assert_eq!(frame["capabilities"], serde_json::json!(["agent_manager"]));
    assert_eq!(
        keys(&frame["status"]),
        [
            "activeRelay",
            "agents",
            "harnesses",
            "modelProbe",
            "pending"
        ]
    );
    assert_eq!(
        frame["status"]["modelProbe"],
        serde_json::json!({"agentId":null,"state":"idle","model":"","detail":null})
    );
    assert_eq!(
        keys(&frame["status"]["harnesses"][0]),
        ["bundle", "id", "signedIn"]
    );
    assert_eq!(
        keys(&frame["status"]["agents"][0]),
        [
            "acpCommand",
            "answersDms",
            "community",
            "description",
            "enrolled",
            "harness",
            "id",
            "identity",
            "instances",
            "instructions",
            "lastError",
            "model",
            "name",
            "published",
            "relay",
            "respondTo",
            "rooms",
            "startAtLogin",
            "unit",
            "workspace"
        ]
    );
    assert_eq!(
        keys(&frame["status"]["agents"][0]["instances"][0]),
        [
            "community",
            "lastError",
            "published",
            "relay",
            "rooms",
            "startAtLogin",
            "state",
            "unit"
        ]
    );
    assert_eq!(
        keys(&frame["status"]["pending"]),
        ["category", "detail", "requestId", "state", "type"]
    );
    assert_eq!(
        keys(&error_frame(&id, "1-2", "agent_busy")),
        ["category", "id", "instanceId", "type", "version"]
    );
    // Worst case: 16 agents with 16 KiB of instructions that all need escaping.
    let mut worst = f.service.snapshot();
    let mut agent = worst.agents[0].clone();
    agent.instructions = "\"".repeat(store::INSTRUCTIONS_BYTES);
    agent.name = "\u{10FFFF}".repeat(64);
    agent.description = "\u{10FFFF}".repeat(256);
    agent.rooms = vec![ROOM_A.into(); 8];
    agent.workspace = format!("/{}", "w".repeat(1023));
    agent.relay = format!("wss://{}/", "r".repeat(2030));
    agent.community = "\u{10FFFF}".repeat(16);
    let instance = InstanceView {
        relay: agent.relay.clone(),
        community: "\u{10FFFF}".repeat(64),
        rooms: vec![ROOM_A.into(); 8],
        unit: format!("omarchy-buzz-agent-{}-{}.service", agent.id, "f".repeat(12)),
        start_at_login: true,
        published: true,
        last_error: Some("relay_unavailable".into()),
        state: "unknown",
    };
    agent.instances = vec![instance; store::MAX_INSTANCES];
    worst.agents = vec![agent; 16];
    worst.active_relay = Some(format!("wss://{}/", "r".repeat(2030)));
    let bytes =
        serde_json::to_vec(&envelope("status", Some(&id), &"i".repeat(128), &worst)).unwrap();
    assert!(
        bytes.len() < crate::protocol::RESPONSE_LIMIT,
        "{}",
        bytes.len()
    );
}

fn login_status(f: &Fixture, harness: &str, word: &str) {
    f.spawner.outputs.lock().unwrap().insert(
        super::harness::status_argv(&f.home.paths, harness).join(" "),
        format!("{word}\n"),
    );
}
fn signed_in(f: &Fixture, harness: &str) -> Option<bool> {
    let views = f.service.snapshot().harnesses;
    views
        .into_iter()
        .find(|h| h.id == harness)
        .unwrap()
        .signed_in
}

#[tokio::test]
async fn sign_in_rechecks_status_every_five_seconds_until_signed_in() {
    let f = fixture(UNREACHABLE);
    f.ready("claude-code");
    login_status(&f, "claude-code", "signed-out");
    f.service.inspect_harnesses().await;
    assert_eq!(signed_in(&f, "claude-code"), Some(false));
    // Without a sign-in the cadence is a minute.
    let start = Instant::now();
    assert!(!f.service.harness_inspection_due(start + SIGN_IN_INSPECTION));
    f.run(serde_json::json!({"type":"sign_in","harness":"claude-code"}))
        .await
        .unwrap();
    let t = Instant::now();
    assert!(!f
        .service
        .harness_inspection_due(t + SIGN_IN_INSPECTION - Duration::from_secs(1)));
    assert!(f.service.harness_inspection_due(t + SIGN_IN_INSPECTION));
    // A claimed inspection is not claimed twice while it runs.
    assert!(!f
        .service
        .harness_inspection_due(t + Duration::from_secs(30)));
    f.service.inspect_harnesses().await;
    assert_eq!(signed_in(&f, "claude-code"), Some(false));
    // The browser login completes; the next five-second check shows it.
    login_status(&f, "claude-code", "signed-in");
    assert!(!f.service.harness_inspection_due(t + Duration::from_secs(9)));
    assert!(f
        .service
        .harness_inspection_due(t + Duration::from_secs(10)));
    f.service.inspect_harnesses().await;
    assert_eq!(signed_in(&f, "claude-code"), Some(true));
    // Signed in: back to the one-minute cadence.
    let after = Instant::now();
    assert!(!f
        .service
        .harness_inspection_due(after + Duration::from_secs(30)));
    assert!(f.service.harness_inspection_due(after + HARNESS_INSPECTION));
}

#[tokio::test]
async fn sign_in_fast_window_ends_after_two_minutes() {
    let f = fixture(UNREACHABLE);
    f.ready("codex");
    login_status(&f, "codex", "signed-out");
    // A refused sign-in does not start the window.
    assert_eq!(
        f.run(serde_json::json!({"type":"sign_in","harness":"bash"}))
            .await,
        Err("agent_invalid")
    );
    assert!(!f
        .service
        .harness_inspection_due(Instant::now() + SIGN_IN_INSPECTION));
    f.run(serde_json::json!({"type":"sign_in","harness":"codex"}))
        .await
        .unwrap();
    let t = Instant::now();
    let mut checks = 0;
    let mut at = t;
    while at < t + SIGN_IN_WINDOW {
        at += Duration::from_secs(1);
        if f.service.harness_inspection_due(at) {
            checks += 1;
            f.service.inspect_harnesses().await;
        }
    }
    // Every five seconds for two minutes, never signed in.
    assert_eq!(checks, 24);
    assert_eq!(signed_in(&f, "codex"), Some(false));
    // The check at the window's end already returns to the minute cadence.
    assert!(!f
        .service
        .harness_inspection_due(at + Duration::from_secs(5)));
    assert!(!f
        .service
        .harness_inspection_due(at + Duration::from_secs(59)));
    assert!(f.service.harness_inspection_due(at + HARNESS_INSPECTION));
}

fn detail(f: &Fixture) -> Option<&'static str> {
    f.service.snapshot().pending.unwrap().detail
}

#[tokio::test]
async fn models_must_belong_to_the_harness_when_saved() {
    let f = fixture(UNREACHABLE);
    // A Claude alias on Codex, a Codex id on Claude Code, a slash: refused with the detail.
    for (harness, model) in [
        ("codex", "opus"),
        ("claude-code", "gpt-5.5"),
        ("claude-code", "anthropic/claude"),
    ] {
        assert_eq!(
            f.create(serde_json::json!({"harness":harness,"model":model}))
                .await,
            Err("agent_invalid"),
            "{harness} {model}"
        );
        let expected = store::valid_model(model).then_some(models::NOT_FOR_HARNESS);
        // A model outside the store's character rule fails the field rules first.
        assert_eq!(detail(&f), expected, "{model}");
    }
    assert!(f.service.snapshot().agents.is_empty());
    // Other refusals carry no detail.
    assert_eq!(
        f.create(serde_json::json!({"name":""})).await,
        Err("agent_invalid")
    );
    assert_eq!(detail(&f), None);
    let claude = f
        .create(serde_json::json!({"harness":"claude-code","model":"opus"}))
        .await
        .unwrap();
    let codex = f
        .create(serde_json::json!({"model":"gpt-5.5"}))
        .await
        .unwrap();
    assert_eq!(detail(&f), None);
    // Switching the harness alone must bring a matching model.
    for fields in [
        serde_json::json!({"harness":"codex"}),
        serde_json::json!({"model":"o3"}),
        serde_json::json!({"model":"claude opus"}),
    ] {
        assert_eq!(
            f.run(serde_json::json!({"type":"update_agent","agentId":claude,"fields":fields}))
                .await,
            Err("agent_invalid"),
            "{fields}"
        );
    }
    assert_eq!(f.stored(&claude).model, "opus");
    f.run(serde_json::json!({"type":"update_agent","agentId":claude,
        "fields":{"harness":"codex","model":"o3"}}))
        .await
        .unwrap();
    assert_eq!(
        (
            f.stored(&claude).harness.as_str(),
            f.stored(&claude).model.as_str()
        ),
        ("codex", "o3")
    );
    // Empty is the harness default for either harness.
    f.run(serde_json::json!({"type":"update_agent","agentId":codex,
        "fields":{"harness":"claude-code","model":""}}))
        .await
        .unwrap();
    // A model saved before the patterns existed stays until it is edited.
    f.service
        .store
        .lock()
        .unwrap()
        .get_mut(&codex)
        .unwrap()
        .model = "GPT-5".into();
    f.run(serde_json::json!({"type":"update_agent","agentId":codex,"fields":{"name":"Kept"}}))
        .await
        .unwrap();
    assert_eq!(f.stored(&codex).model, "GPT-5");
    assert_eq!(
        f.run(
            serde_json::json!({"type":"update_agent","agentId":codex,"fields":{"model":"GPT-6"}})
        )
        .await,
        Err("agent_invalid")
    );
    assert_eq!(detail(&f), Some(models::NOT_FOR_HARNESS));
}

fn probe(f: &Fixture) -> ModelProbe {
    f.service.snapshot().model_probe
}

#[tokio::test]
async fn probe_model_is_gated_and_runs_the_launcher_in_probe_mode() {
    let f = fixture(UNREACHABLE);
    let id = f
        .create(serde_json::json!({"harness":"claude-code"}))
        .await
        .unwrap();
    let request = serde_json::json!({"type":"probe_model","agentId":id});
    // The harness default is not probed: there is no model to check.
    assert_eq!(f.run(request.clone()).await, Err("agent_invalid"));
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"model":"sonnet"}}))
        .await
        .unwrap();
    // No reviewed scripts: nothing is inspected as ready, nothing runs.
    assert_eq!(f.run(request.clone()).await, Err("harness_missing"));
    f.ready("claude-code");
    bundle_state(&f, "claude-code", "stale", 3);
    assert_eq!(f.run(request.clone()).await, Err("bundle_stale"));
    bundle_state(&f, "claude-code", "ready", 0);
    login_status(&f, "claude-code", "signed-out");
    assert_eq!(f.run(request.clone()).await, Err("not_signed_in"));
    assert!(f.spawner.probes.lock().unwrap().is_empty());
    assert_eq!(probe(&f), ModelProbe::default());
    assert_eq!(
        f.run(serde_json::json!({"type":"probe_model","agentId":"00000000-0000-4000-8000-00000000aaaa"}))
            .await,
        Err("agent_invalid")
    );

    login_status(&f, "claude-code", "signed-in");
    *f.spawner.probe_answer.lock().unwrap() = Some((Some(0), "OK\n".into()));
    f.run(request.clone()).await.unwrap();
    assert_eq!(
        probe(&f),
        ModelProbe {
            agent_id: Some(id.clone()),
            state: "ok",
            model: "sonnet".into(),
            detail: Some(models::OK)
        }
    );
    let (argv, env) = f.spawner.probes.lock().unwrap()[0].clone();
    let data = &f.home.paths.data;
    let state = &f.home.paths.state;
    let path = |p: std::path::PathBuf| p.to_str().unwrap().to_string();
    // The transient unit gets exactly the filtered environment, as --setenv.
    let setenv: Vec<String> = env
        .iter()
        .map(|(k, v)| format!("--setenv={}={}", k.to_str().unwrap(), v.to_str().unwrap()))
        .collect();
    let mut expected: Vec<String> = [
        "/usr/bin/systemd-run",
        "--user",
        "--pipe",
        "--wait",
        "--collect",
        "--quiet",
        "-p",
        "MemoryMax=2G",
        "-p",
        "TasksMax=128",
        "-p",
        "RuntimeMaxSec=90",
    ]
    .map(String::from)
    .to_vec();
    expected.extend(setenv);
    expected.extend(
        [
            "--",
            &path(data.join("omarchy-buzz/agent-claude-code/launcher/room-agent")),
            "--probe-model",
            "sonnet",
            "--harness",
            "claude-code",
            "--profile",
            &path(state.join("omarchy-buzz-agent-preview/claude-code")),
            "--bundle",
            &path(data.join("omarchy-buzz/agent-claude-code")),
        ]
        .map(String::from),
    );
    assert_eq!(argv, expected);
    // No relay, owner, identity, workspace or key reaches the probe.
    for word in [
        "--relay",
        "--identity",
        "--owner",
        "--workspace",
        "--auth-tag",
    ] {
        assert!(!argv.iter().any(|a| a == word), "{word}");
    }
    assert!(env
        .iter()
        .all(|(k, _)| models::PROBE_ENV.contains(&k.to_str().unwrap())));

    // Each synthetic outcome through the spawner fake.
    for (answer, state, sentence) in [
        (
            (Some(1), "There's an issue with the selected model (sonnet). It may not exist or you may not have access to it.".to_string()),
            "unavailable",
            models::UNAVAILABLE,
        ),
        (
            (Some(1), "Not logged in · Please run /login".into()),
            "not_signed_in",
            models::SIGNED_OUT,
        ),
        ((None, String::new()), "failed", models::TIMED_OUT),
        ((Some(1), "\u{0}\u{7}garbage".into()), "failed", models::FAILED),
        ((Some(0), "I cannot".into()), "failed", models::UNEXPECTED),
    ] {
        *f.spawner.probe_answer.lock().unwrap() = Some(answer);
        // The request is done: the outcome is the probe state, not a category.
        f.run(request.clone()).await.unwrap();
        assert_eq!(
            (probe(&f).state, probe(&f).detail),
            (state, Some(sentence))
        );
        let status = serde_json::to_string(&f.service.snapshot()).unwrap();
        assert!(!status.contains("garbage") && !status.contains("/login"));
    }

    // While it runs: `running`, and no other mutation is accepted.
    let (release, hold) = std::sync::mpsc::channel();
    *f.spawner.probe_hold.lock().unwrap() = Some(hold);
    *f.spawner.probe_answer.lock().unwrap() = Some((Some(0), "OK".into()));
    let r = f.request(request.clone());
    let slot = f.service.begin().unwrap();
    let service = f.service.clone();
    let task = tokio::spawn(async move { service.execute(&r, slot).await });
    let until = Instant::now() + Duration::from_secs(10);
    while probe(&f).state != "running" {
        assert!(Instant::now() < until, "probe never reported running");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(probe(&f).detail, None);
    assert!(
        f.service.begin().is_none(),
        "a second mutation was admitted"
    );
    release.send(()).unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(probe(&f).state, "ok");

    // Editing the model forgets the result; so does deleting the agent.
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"model":"haiku"}}))
        .await
        .unwrap();
    assert_eq!(probe(&f), ModelProbe::default());
    f.run(request.clone()).await.unwrap();
    assert_eq!(probe(&f).model, "haiku");
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"name":"Renamed"}}))
        .await
        .unwrap();
    assert_eq!(probe(&f).state, "ok", "an unrelated edit keeps the result");
    f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(probe(&f), ModelProbe::default());
}

/// Two communities, `first` active: names "Alpha" and "Beta".
fn two_communities(f: &Fixture, first: &str, second: &str) {
    let mut config = f.config.lock().unwrap();
    config.relay = Some(first.into());
    config.communities = vec![
        crate::config::Community {
            relay: first.into(),
            name: "Alpha".into(),
            joined_at: 1,
        },
        crate::config::Community {
            relay: second.into(),
            name: "Beta".into(),
            joined_at: 2,
        },
    ];
}
fn switch_to(f: &Fixture, relay: &str) {
    f.config.lock().unwrap().relay = Some(relay.into());
    f.service.publish();
}
fn kinds(relay: &crate::agents_service::test_support::Relay) -> Vec<u16> {
    relay
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.event.kind.as_u16())
        .collect()
}

#[tokio::test]
async fn agents_stay_bound_to_the_community_they_were_created_in() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let alpha = relay(|_| Answer::Accept, false).await;
    let beta = relay(|_| Answer::Accept, false).await;
    let f = fixture(&alpha.url);
    two_communities(&f, &alpha.url, &beta.url);
    let id = f
        .create(serde_json::json!({"model":"gpt-5.5"}))
        .await
        .unwrap();
    // Created in, and bound to, the active community.
    assert_eq!(f.stored(&id).relay, alpha.url);
    let view = f.agent(&id);
    assert_eq!(
        (view.relay.as_str(), view.community.as_str()),
        (alpha.url.as_str(), "Alpha")
    );
    assert_eq!(
        f.service.snapshot().active_relay.as_deref(),
        Some(alpha.url.as_str())
    );
    assert_eq!(*f.rooms.1.lock().unwrap(), [alpha.url.clone()]);
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(kinds(&alpha), [30175, 30177, 9000, 0]);
    assert!(kinds(&beta).is_empty());
    assert!(f
        .keyring
        .owner_relays
        .lock()
        .unwrap()
        .iter()
        .all(|r| r == &alpha.url));

    // The maintainer switches to the second community.
    switch_to(&f, &beta.url);
    let status = f.service.snapshot();
    assert_eq!(status.active_relay.as_deref(), Some(beta.url.as_str()));
    assert_eq!(
        (f.agent(&id).relay, f.agent(&id).community),
        (alpha.url.clone(), "Alpha".to_string())
    );
    f.keyring.owner_relays.lock().unwrap().clear();

    // Start, start at login: the unit names the agent's own relay.
    f.ready("codex");
    f.run(serde_json::json!({"type":"start_agent","agentId":id}))
        .await
        .unwrap();
    let unit_text = std::fs::read_to_string(f.home.paths.unit_file(&id)).unwrap();
    assert!(
        unit_text.contains(&format!("\"--relay\" \"{}\"", alpha.url)),
        "{unit_text}"
    );
    assert!(!unit_text.contains(&beta.url));
    let argv = unit::exec_argv(
        &f.home.paths,
        &f.stored(&id),
        &f.owner.public_key().to_hex(),
    )
    .unwrap();
    let at = argv.iter().position(|w| w == "--relay").unwrap();
    assert_eq!(argv[at + 1], alpha.url);
    std::fs::remove_file(f.home.paths.unit_file(&id)).unwrap();
    f.run(serde_json::json!({"type":"set_start_at_login","agentId":id,"enabled":true}))
        .await
        .unwrap();
    assert!(std::fs::read_to_string(f.home.paths.unit_file(&id))
        .unwrap()
        .contains(&format!("\"--relay\" \"{}\"", alpha.url)));
    f.run(serde_json::json!({"type":"stop_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(f.agent(&id).unit, "inactive");

    // A republishing edit goes to the agent's relay with that community's
    // owner key, never to the active one.
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"name":"Renamed"}}))
        .await
        .unwrap();
    assert_eq!(kinds(&alpha), [30175, 30177, 9000, 0, 30175, 30177, 0]);
    assert!(kinds(&beta).is_empty());
    assert!(f.agent(&id).published);
    assert_eq!(*f.keyring.owner_relays.lock().unwrap(), [alpha.url.clone()]);

    // Rooms can only be checked in the active community's verified catalog:
    // changing them (or enrolling again) waits for a switch back; nothing is
    // asked of the other community's catalog.
    f.rooms.1.lock().unwrap().clear();
    assert_eq!(
        f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"rooms":[ROOM_B]}}))
            .await,
        Err("relay_unavailable")
    );
    assert_eq!(f.stored(&id).rooms, [ROOM_A]);
    assert_eq!(
        f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
            .await,
        Err("relay_unavailable")
    );
    assert!(f.rooms.1.lock().unwrap().is_empty());

    // The model probe takes no relay at all and runs whatever is active.
    *f.spawner.probe_answer.lock().unwrap() = Some((Some(0), "OK\n".into()));
    f.run(serde_json::json!({"type":"probe_model","agentId":id}))
        .await
        .unwrap();
    assert_eq!(probe(&f).state, "ok");
    let (probe_argv, _) = f.spawner.probes.lock().unwrap()[0].clone();
    assert!(!probe_argv
        .iter()
        .any(|w| w == "--relay" || w == &alpha.url || w == &beta.url));

    // A new agent belongs to the now active community.
    let second = f.create(serde_json::json!({})).await.unwrap();
    assert_eq!(f.stored(&second).relay, beta.url);
    assert_eq!(f.agent(&second).community, "Beta");
    assert_eq!(*f.rooms.1.lock().unwrap(), [beta.url.clone()]);
    f.run(serde_json::json!({"type":"enroll_agent","agentId":second}))
        .await
        .unwrap();
    assert_eq!(kinds(&beta), [30175, 30177, 9000, 0]);

    // Back in the first community, rooms can change again.
    switch_to(&f, &alpha.url);
    f.run(
        serde_json::json!({"type":"update_agent","agentId":id,"fields":{"rooms":[ROOM_A, ROOM_B]}}),
    )
    .await
    .unwrap();
    assert_eq!(kinds(&alpha).last(), Some(&0));
    assert!(kinds(&alpha).ends_with(&[30175, 30177, 9000, 0]));

    // Deleting leaves the agent's own community's rooms, wherever we are.
    switch_to(&f, &beta.url);
    let before = kinds(&beta).len();
    f.run(serde_json::json!({"type":"delete_agent","agentId":id}))
        .await
        .unwrap();
    assert_eq!(&kinds(&alpha)[kinds(&alpha).len() - 2..], [9001, 9001]);
    assert_eq!(kinds(&beta).len(), before);

    // A community the configuration no longer lists is named by its host.
    f.config
        .lock()
        .unwrap()
        .communities
        .retain(|c| c.relay != beta.url);
    f.config.lock().unwrap().relay = Some(alpha.url.clone());
    f.service.publish();
    assert_eq!(f.agent(&second).community, crate::config::host(&beta.url));
}

#[tokio::test]
async fn create_needs_a_listed_active_community_and_an_identity() {
    let f = fixture(UNREACHABLE);
    // The active relay is not in the community list.
    f.config.lock().unwrap().communities.clear();
    assert_eq!(
        f.create(serde_json::json!({})).await,
        Err("relay_unavailable")
    );
    // No identity.
    *f.config.lock().unwrap() = Config {
        relay: Some(UNREACHABLE.into()),
        identity: None,
        communities: Vec::new(),
    }
    .normalized();
    assert_eq!(
        f.create(serde_json::json!({})).await,
        Err("relay_unavailable")
    );
    // No configuration at all: status says so.
    *f.config.lock().unwrap() = Config::default();
    assert_eq!(
        f.create(serde_json::json!({})).await,
        Err("relay_unavailable")
    );
    f.service.publish();
    assert_eq!(f.service.snapshot().active_relay, None);
    assert!(f.service.snapshot().agents.is_empty());
    assert!(f.rooms.1.lock().unwrap().is_empty());
}

#[tokio::test]
async fn opening_the_service_migrates_personas_to_the_first_community() {
    let f = fixture(UNREACHABLE);
    let id = f.create(serde_json::json!({})).await.unwrap();
    // A version 1 store, as written before personas had a relay.
    let mut persona = serde_json::to_value(f.stored(&id)).unwrap();
    persona.as_object_mut().unwrap().remove("relay");
    let value = serde_json::json!({"version": 1, "agents": [persona]});
    std::fs::write(
        f.home.paths.store_file(),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    // The configuration's first community, even when another is active.
    two_communities(&f, "wss://first.example/", "wss://second.example/");
    f.config.lock().unwrap().relay = Some("wss://second.example/".into());
    let config = f.config.clone();
    let service = Service::open(
        f.home.paths.clone(),
        Deps {
            control: f.control.clone(),
            keyring: f.keyring.clone(),
            spawner: f.spawner.clone(),
            rooms: f.rooms.clone(),
        },
        Box::new(move || Ok(config.lock().unwrap().clone())),
    )
    .unwrap();
    let agent = service.snapshot().agents[0].clone();
    assert_eq!(agent.id, id);
    assert_eq!(
        (agent.relay.as_str(), agent.community.as_str()),
        ("wss://first.example/", "Alpha")
    );
    assert_eq!(
        service.snapshot().active_relay.as_deref(),
        Some("wss://second.example/")
    );
    assert_eq!(
        store::Store::open(&f.home.paths).unwrap().agents[0]
            .first()
            .relay,
        "wss://first.example/"
    );
    // Without any community the old store is refused, not guessed at.
    std::fs::write(
        f.home.paths.store_file(),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let opened = Service::open(
        f.home.paths.clone(),
        Deps {
            control: f.control.clone(),
            keyring: f.keyring.clone(),
            spawner: f.spawner.clone(),
            rooms: f.rooms.clone(),
        },
        Box::new(|| Ok(Config::default())),
    );
    assert_eq!(opened.err(), Some("store_invalid"));
}

/// The kind and `h`/`p` tags of every event a relay saw.
fn events(relay: &crate::agents_service::test_support::Relay) -> Vec<(u16, String, String)> {
    relay
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| {
            let tag = |name: &str| {
                s.event
                    .tags
                    .iter()
                    .find(|t| t.as_slice()[0] == name)
                    .map(|t| t.as_slice()[1].clone())
                    .unwrap_or_default()
            };
            (s.event.kind.as_u16(), tag("h"), tag("p"))
        })
        .collect()
}
fn unit_calls(f: &Fixture, verb: &str) -> Vec<String> {
    f.control
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|argv| argv.get(2).map(String::as_str) == Some(verb))
        .map(|argv| argv[3].clone())
        .collect()
}

#[tokio::test]
async fn an_enrolled_agent_joins_a_second_community_as_its_own_instance() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let alpha = relay(|_| Answer::Accept, false).await;
    let beta = relay(|_| Answer::Accept, false).await;
    let f = fixture(&alpha.url);
    two_communities(&f, &alpha.url, &beta.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    let identity = f.stored(&id).identity.unwrap();
    let h = store::relay_hash(&beta.url);
    let second_unit = format!("omarchy-buzz-agent-{id}-{h}.service");
    let first_unit = format!("omarchy-buzz-agent-{id}.service");

    // Refusals. Only the active community: nothing is asked of a catalog.
    f.rooms.1.lock().unwrap().clear();
    let join = |agent: &str, relay: &str, rooms: serde_json::Value| serde_json::json!({"type":"enroll_agent_in","agentId":agent,"relay":relay,"rooms":rooms});
    assert_eq!(
        f.run(join(&id, &beta.url, serde_json::json!([ROOM_B])))
            .await,
        Err("relay_unavailable")
    );
    assert!(f.rooms.1.lock().unwrap().is_empty());
    // Already there.
    assert_eq!(
        f.run(join(&id, &alpha.url, serde_json::json!([ROOM_B])))
            .await,
        Err("agent_invalid")
    );
    // An unknown agent, and one never enrolled.
    switch_to(&f, &beta.url);
    assert_eq!(
        f.run(join(
            "00000000-0000-4000-8000-00000000dead",
            &beta.url,
            serde_json::json!([ROOM_B])
        ))
        .await,
        Err("agent_invalid")
    );
    switch_to(&f, &alpha.url);
    let unenrolled = f.create(serde_json::json!({})).await.unwrap();
    switch_to(&f, &beta.url);
    assert_eq!(
        f.run(join(&unenrolled, &beta.url, serde_json::json!([ROOM_B])))
            .await,
        Err("agent_invalid")
    );
    // Rooms not in that community's verified catalog, or no catalog.
    assert_eq!(
        f.run(join(
            &id,
            &beta.url,
            serde_json::json!(["00000000-0000-4000-8000-0000000000b9"])
        ))
        .await,
        Err("agent_invalid")
    );
    *f.rooms.0.lock().unwrap() = Err("relay_unavailable");
    assert_eq!(
        f.run(join(&id, &beta.url, serde_json::json!([ROOM_B])))
            .await,
        Err("relay_unavailable")
    );
    *f.rooms.0.lock().unwrap() = Ok(vec![ROOM_A.into(), ROOM_B.into(), ROOM_C.into()]);
    // A community the configuration does not list is never joined.
    f.config.lock().unwrap().relay = Some("wss://unlisted.example/".into());
    assert_eq!(
        f.run(join(
            &id,
            "wss://unlisted.example/",
            serde_json::json!([ROOM_B])
        ))
        .await,
        Err("relay_unavailable")
    );
    switch_to(&f, &beta.url);
    assert!(kinds(&beta).is_empty());
    assert_eq!(f.agent(&id).instances.len(), 1);
    f.keyring.owner_relays.lock().unwrap().clear();

    // Joined: the same identity and attestation, admitted with the owner's
    // key for that community, published there and nowhere else.
    let before = kinds(&alpha).len();
    f.run(join(&id, &beta.url, serde_json::json!([ROOM_B])))
        .await
        .unwrap();
    assert_eq!(
        events(&beta),
        [
            (30175, String::new(), String::new()),
            (30177, String::new(), String::new()),
            (9000, ROOM_B.to_string(), identity.clone()),
            (0, String::new(), String::new()),
        ]
    );
    let profile = beta.seen.lock().unwrap()[3].clone();
    assert_eq!(profile.author.to_hex(), identity);
    assert_eq!(kinds(&alpha).len(), before);
    assert_eq!(*f.keyring.owner_relays.lock().unwrap(), [beta.url.clone()]);
    assert_eq!(f.keyring.agents.lock().unwrap().len(), 1);
    let view = f.agent(&id);
    assert_eq!(
        (view.relay.as_str(), view.rooms.clone()),
        (alpha.url.as_str(), vec![ROOM_A.to_string()])
    );
    assert_eq!(
        view.instances,
        [
            InstanceView {
                relay: alpha.url.clone(),
                community: "Alpha".into(),
                rooms: vec![ROOM_A.into()],
                unit: first_unit.clone(),
                start_at_login: false,
                published: true,
                last_error: None,
                state: "unknown",
            },
            InstanceView {
                relay: beta.url.clone(),
                community: "Beta".into(),
                rooms: vec![ROOM_B.into()],
                unit: second_unit.clone(),
                start_at_login: false,
                published: true,
                last_error: None,
                state: "unknown",
            }
        ]
    );
    let second = f
        .service
        .store
        .lock()
        .unwrap()
        .get(&id)
        .unwrap()
        .persona(&beta.url)
        .unwrap();
    assert!(!second.primary);
    assert_eq!(
        std::path::Path::new(&second.workspace),
        f.home.paths.default_workspace(&format!("{id}-{h}"))
    );
    assert!(std::path::Path::new(&second.workspace).is_dir());
    assert_eq!(second.member_rooms, [ROOM_B]);
    // Already there now.
    assert_eq!(
        f.run(join(&id, &beta.url, serde_json::json!([ROOM_C])))
            .await,
        Err("agent_invalid")
    );

    // Start each instance: its own unit, relay, rooms and workspace; the
    // identity, attestation and instructions are shared.
    f.ready("codex");
    f.run(serde_json::json!({"type":"start_agent","agentId":id,"relay":beta.url}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(f.home.paths.unit_file(&format!("{id}-{h}"))).unwrap();
    for word in [
        format!("\"--relay\" \"{}\"", beta.url),
        format!("\"--room\" \"{ROOM_B}\""),
        format!("\"--workspace\" \"{}\"", second.workspace),
        format!("\"--identity\" \"{identity}\""),
    ] {
        assert!(text.contains(&word), "{word}\n{text}");
    }
    assert!(!text.contains(&alpha.url) && !text.contains(ROOM_A));
    assert!(!f.home.paths.unit_file(&id).exists());
    assert_eq!(unit_calls(&f, "start"), [second_unit.clone()]);
    f.run(serde_json::json!({"type":"start_agent","agentId":id}))
        .await
        .unwrap();
    assert!(std::fs::read_to_string(f.home.paths.unit_file(&id))
        .unwrap()
        .contains(&format!("\"--relay\" \"{}\"", alpha.url)));
    assert_eq!(
        unit_calls(&f, "start"),
        [second_unit.clone(), first_unit.clone()]
    );
    let states: Vec<_> = f.agent(&id).instances.iter().map(|i| i.state).collect();
    assert_eq!(states, ["active", "active"]);
    assert_eq!(f.agent(&id).unit, "active");
    // Start at login and stop, per instance.
    f.run(serde_json::json!({"type":"set_start_at_login","agentId":id,"enabled":true,"relay":beta.url}))
        .await
        .unwrap();
    assert_eq!(unit_calls(&f, "enable"), [second_unit.clone()]);
    let logins: Vec<_> = f
        .agent(&id)
        .instances
        .iter()
        .map(|i| i.start_at_login)
        .collect();
    assert_eq!(logins, [false, true]);
    assert!(!f.agent(&id).start_at_login);
    f.run(serde_json::json!({"type":"stop_agent","agentId":id,"relay":beta.url}))
        .await
        .unwrap();
    assert_eq!(unit_calls(&f, "stop"), [second_unit.clone()]);
    let states: Vec<_> = f.agent(&id).instances.iter().map(|i| i.state).collect();
    assert_eq!(states, ["active", "inactive"]);
    // A relay the agent is not enrolled in is refused.
    assert_eq!(
        f.run(
            serde_json::json!({"type":"stop_agent","agentId":id,"relay":"wss://unlisted.example/"})
        )
        .await,
        Err("agent_invalid")
    );

    // Rooms change per instance, in the active community only.
    f.run(
        serde_json::json!({"type":"update_agent","agentId":id,"relay":beta.url,
        "fields":{"rooms":[ROOM_B, ROOM_C]}}),
    )
    .await
    .unwrap();
    assert_eq!(
        &events(&beta)[4..],
        [
            (30175, String::new(), String::new()),
            (30177, String::new(), String::new()),
            (9000, ROOM_C.to_string(), identity.clone()),
            (0, String::new(), String::new()),
        ]
    );
    assert_eq!(kinds(&alpha).len(), before);
    assert_eq!(f.stored(&id).rooms, [ROOM_A]);
    assert_eq!(f.agent(&id).instances[1].rooms, [ROOM_B, ROOM_C]);
    // The first instance's rooms are the other community's: refused here.
    assert_eq!(
        f.run(serde_json::json!({"type":"update_agent","agentId":id,"fields":{"rooms":[ROOM_C]}}))
            .await,
        Err("relay_unavailable")
    );
    // A definition edit republishes in every community and stops every
    // running instance only when it changes how the agent runs.
    f.run(serde_json::json!({"type":"update_agent","agentId":id,"relay":beta.url,"fields":{"name":"Renamed"}}))
        .await
        .unwrap();
    assert_eq!(&kinds(&alpha)[before..], [30175, 30177, 0]);
    assert_eq!(&kinds(&beta)[8..], [30175, 30177, 0]);
    assert!(f.agent(&id).instances.iter().all(|i| i.published));
    assert_eq!(f.agent(&id).instances[0].state, "active");
    f.run(
        serde_json::json!({"type":"update_agent","agentId":id,"fields":{"respondTo":"mentions"}}),
    )
    .await
    .unwrap();
    let states: Vec<_> = f.agent(&id).instances.iter().map(|i| i.state).collect();
    assert_eq!(states, ["inactive", "inactive"]);

    // Leaving the first community (from the second): its rooms are left,
    // its unit removed, the instance dropped; the other keeps its names.
    f.control.calls.lock().unwrap().clear();
    let alpha_before = kinds(&alpha).len();
    f.run(serde_json::json!({"type":"leave_agent_community","agentId":id,"relay":alpha.url}))
        .await
        .unwrap();
    assert_eq!(
        &events(&alpha)[alpha_before..],
        [(9001, ROOM_A.to_string(), identity.clone())]
    );
    assert!(!f.home.paths.unit_file(&id).exists());
    assert_eq!(unit_calls(&f, "disable"), [first_unit.clone()]);
    let view = f.agent(&id);
    assert_eq!(view.instances.len(), 1);
    assert_eq!(
        (view.relay.as_str(), view.community.as_str()),
        (beta.url.as_str(), "Beta")
    );
    assert_eq!(view.instances[0].unit, second_unit);
    assert!(f.home.paths.unit_file(&format!("{id}-{h}")).exists());
    // The last community cannot be left: delete the agent instead.
    assert_eq!(
        f.run(serde_json::json!({"type":"leave_agent_community","agentId":id,"relay":beta.url}))
            .await,
        Err("agent_invalid")
    );
    assert_eq!(f.agent(&id).instances.len(), 1);

    // Back in the first community: a new instance there gets a suffixed name.
    switch_to(&f, &alpha.url);
    f.run(join(&id, &alpha.url, serde_json::json!([ROOM_A])))
        .await
        .unwrap();
    let ha = store::relay_hash(&alpha.url);
    assert_eq!(
        f.agent(&id).instances[1].unit,
        format!("omarchy-buzz-agent-{id}-{ha}.service")
    );

    // Delete leaves every community's rooms, removes every unit, keeps the
    // identity unless asked to forget it.
    f.control.calls.lock().unwrap().clear();
    let (a0, b0) = (kinds(&alpha).len(), kinds(&beta).len());
    f.run(serde_json::json!({"type":"delete_agent","agentId":id,"forget":true}))
        .await
        .unwrap();
    assert_eq!(
        &events(&beta)[b0..],
        [
            (9001, ROOM_B.to_string(), identity.clone()),
            (9001, ROOM_C.to_string(), identity.clone())
        ]
    );
    assert_eq!(
        &events(&alpha)[a0..],
        [(9001, ROOM_A.to_string(), identity.clone())]
    );
    assert!(!f.home.paths.unit_file(&format!("{id}-{h}")).exists());
    assert_eq!(unit_calls(&f, "disable"), [second_unit.clone()]);
    assert!(f.service.store.lock().unwrap().get(&id).is_none());
    assert!(!f.keyring.agents.lock().unwrap().contains_key(&identity));
    assert!(f.service.snapshot().agents.iter().all(|a| a.id != id));
}

static REJECT_ADDS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[tokio::test]
async fn a_failed_join_keeps_the_instance_for_a_retry() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let alpha = relay(|_| Answer::Accept, false).await;
    let beta = relay(
        |kind| {
            if kind == 9000 && REJECT_ADDS.load(std::sync::atomic::Ordering::SeqCst) {
                Answer::Reject
            } else {
                Answer::Accept
            }
        },
        false,
    )
    .await;
    let f = fixture(&alpha.url);
    two_communities(&f, &alpha.url, &beta.url);
    let id = f.create(serde_json::json!({})).await.unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    switch_to(&f, &beta.url);
    REJECT_ADDS.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        f.run(serde_json::json!({"type":"enroll_agent_in","agentId":id,"relay":beta.url,"rooms":[ROOM_B]}))
            .await,
        Err("enroll_failed")
    );
    let instance = f.agent(&id).instances[1].clone();
    assert_eq!(
        (instance.published, instance.last_error.as_deref()),
        (false, Some("enroll_failed"))
    );
    // Not published: it cannot start.
    f.ready("codex");
    assert_eq!(
        f.run(serde_json::json!({"type":"start_agent","agentId":id,"relay":beta.url}))
            .await,
        Err("agent_invalid")
    );
    REJECT_ADDS.store(false, std::sync::atomic::Ordering::SeqCst);
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id,"relay":beta.url}))
        .await
        .unwrap();
    let instance = f.agent(&id).instances[1].clone();
    assert_eq!((instance.published, instance.last_error), (true, None));
    assert_eq!(kinds(&beta), [30175, 30177, 9000, 30175, 30177, 9000, 0]);
    // The first instance was not touched.
    assert!(f.agent(&id).instances[0].published);
}

/// The maintainer's machine through the service: a version 1 store with a
/// running agent opens as version 2 and keeps managing the same unit.
#[tokio::test]
async fn the_service_keeps_managing_a_version_1_agents_running_unit() {
    let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
    let alpha = relay(|_| Answer::Accept, false).await;
    let f = fixture(&alpha.url);
    let id = f
        .create(serde_json::json!({"answersDms":true}))
        .await
        .unwrap();
    f.run(serde_json::json!({"type":"enroll_agent","agentId":id}))
        .await
        .unwrap();
    f.ready("codex");
    f.run(serde_json::json!({"type":"start_agent","agentId":id}))
        .await
        .unwrap();
    let unit_path = f.home.paths.unit_file(&id);
    let unit_text = std::fs::read_to_string(&unit_path).unwrap();
    let legacy = f.stored(&id);
    // Rewrite the store as version 1, as 0.0.26 wrote it.
    let v1 = serde_json::json!({"version": 1, "agents": [serde_json::to_value(&legacy).unwrap()]});
    std::fs::write(
        f.home.paths.store_file(),
        serde_json::to_vec_pretty(&v1).unwrap(),
    )
    .unwrap();
    let config = f.config.clone();
    let service = Service::open(
        f.home.paths.clone(),
        Deps {
            control: f.control.clone(),
            keyring: f.keyring.clone(),
            spawner: f.spawner.clone(),
            rooms: f.rooms.clone(),
        },
        Box::new(move || Ok(config.lock().unwrap().clone())),
    )
    .unwrap();
    f.control.calls.lock().unwrap().clear();
    service.inspect_units().await;
    let view = service.snapshot().agents[0].clone();
    let name = format!("omarchy-buzz-agent-{id}.service");
    assert_eq!(view.unit, "active");
    assert_eq!(view.instances.len(), 1);
    assert_eq!(
        (view.instances[0].unit.as_str(), view.instances[0].state),
        (name.as_str(), "active")
    );
    assert_eq!(view.workspace, legacy.workspace);
    assert!(view
        .workspace
        .ends_with(&format!("/omarchy-buzz-room-workspaces/{id}")));
    assert_eq!(view.rooms, legacy.rooms);
    assert_eq!(
        *f.control.calls.lock().unwrap(),
        [vec![
            "/usr/bin/systemctl".to_string(),
            "--user".into(),
            "show".into(),
            "--property=ActiveState".into(),
            name.clone()
        ]]
    );
    // Restarting it writes the same unit file, byte for byte.
    let slot = service.begin().unwrap();
    let stop = f.request(serde_json::json!({"type":"stop_agent","agentId":id}));
    service.execute(&stop, slot).await.unwrap();
    let slot = service.begin().unwrap();
    let start = f.request(serde_json::json!({"type":"start_agent","agentId":id}));
    service.execute(&start, slot).await.unwrap();
    assert_eq!(std::fs::read_to_string(&unit_path).unwrap(), unit_text);
    assert_eq!(unit_calls(&f, "start"), [name]);
}
