//! The agent manager: persona store, enrollment, units and status. One
//! mutating request runs at a time; every change republishes status.
use super::{
    enroll,
    harness::{self, HarnessView, Spawner},
    keys::Keyring,
    models,
    request::{Fields, Request},
    rooms::RoomSource,
    store::{self, Agent, Paths, Persona, Store},
    unit::{self, Op, UnitControl},
};
use crate::config::Config;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{watch, OwnedMutexGuard},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// Cadence of the harness readiness and sign-in scripts.
pub const HARNESS_INSPECTION: Duration = Duration::from_secs(60);
/// Cadence while a completed `sign_in` waits for its harness to report
/// signed-in, for at most `SIGN_IN_WINDOW`.
pub const SIGN_IN_INSPECTION: Duration = Duration::from_secs(5);
pub const SIGN_IN_WINDOW: Duration = Duration::from_secs(120);

/// When the daemon next re-reads harness status.
struct HarnessSchedule {
    due: Instant,
    // The harness a completed `sign_in` started a login for, and the end of
    // its fast re-check window.
    watching: Option<(String, Instant)>,
    // A scheduled inspection is still running; the next one waits for it
    // (each script may take up to its 15 s deadline).
    running: bool,
}

pub struct Deps {
    pub control: Arc<dyn UnitControl>,
    pub keyring: Arc<dyn Keyring>,
    pub spawner: Arc<dyn Spawner>,
    pub rooms: Arc<dyn RoomSource>,
}

pub type ConfigLoader = Box<dyn Fn() -> Result<Config, &'static str> + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub harness: String,
    pub model: String,
    pub acp_command: String,
    pub rooms: Vec<String>,
    pub respond_to: String,
    pub workspace: String,
    pub identity: Option<String>,
    pub enrolled: bool,
    pub unit: &'static str,
    pub start_at_login: bool,
    pub answers_dms: bool,
    pub published: bool,
    pub last_error: Option<String>,
    /// The canonical relay of the agent's community.
    pub relay: String,
    /// That community's local name from the configuration, or the relay's
    /// host when the configuration no longer lists it.
    pub community: String,
    /// Every community the agent is enrolled in, the first one first (whose
    /// fields the top-level `relay`, `community`, `rooms`, `workspace`,
    /// `unit`, `startAtLogin`, `published` and `lastError` repeat).
    pub instances: Vec<InstanceView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    pub relay: String,
    pub community: String,
    pub rooms: Vec<String>,
    /// The instance's unit name (`omarchy-buzz-agent-<id>[-<h>].service`).
    pub unit: String,
    pub start_at_login: bool,
    pub published: bool,
    pub last_error: Option<String>,
    /// `active`, `inactive`, `failed` or `unknown`, as the top-level `unit`.
    pub state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub request_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub state: &'static str,
    pub category: Option<&'static str>,
    /// A fixed detail code (`models::DETAILS`) for some failures, else null.
    pub detail: Option<&'static str>,
}

/// The last model probe: `idle` (never run, or reset by an edit of that
/// agent's model or harness), `running`, or its outcome with one fixed
/// sentence (`models::SENTENCES`). Never the probe's output.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProbe {
    pub agent_id: Option<String>,
    pub state: &'static str,
    pub model: String,
    pub detail: Option<&'static str>,
}
impl Default for ModelProbe {
    fn default() -> Self {
        Self {
            agent_id: None,
            state: "idle",
            model: String::new(),
            detail: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The configuration's active community, or null when none is configured.
    pub active_relay: Option<String>,
    pub harnesses: Vec<HarnessView>,
    pub agents: Vec<AgentView>,
    pub pending: Option<Pending>,
    pub model_probe: ModelProbe,
}

pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"capabilities":["agent_manager"],"status":s})
}
pub fn error_frame(id: &str, instance: &str, category: &str) -> serde_json::Value {
    serde_json::json!({"version":1,"type":"error","id":id,"instanceId":instance,"category":category})
}

/// The fixed error categories of the contract.
pub const CATEGORIES: [&str; 10] = [
    "agent_invalid",
    "agent_busy",
    "agent_limit",
    "harness_missing",
    "bundle_stale",
    "not_signed_in",
    "enroll_failed",
    "unit_failed",
    "workspace_refused",
    "relay_unavailable",
];
/// Internal failures are reported only through the fixed categories.
fn fixed(category: &'static str) -> &'static str {
    if CATEGORIES.contains(&category) {
        category
    } else {
        "agent_invalid"
    }
}

pub struct Service {
    paths: Paths,
    deps: Deps,
    config: ConfigLoader,
    store: Mutex<Store>,
    harnesses: Mutex<Vec<HarnessView>>,
    schedule: Mutex<HarnessSchedule>,
    // Unit state per instance key (`Persona::key`).
    units: Mutex<BTreeMap<String, &'static str>>,
    pending: Mutex<Option<Pending>>,
    // The detail of the running request's refusal, reported with `failed`.
    detail: Mutex<Option<&'static str>>,
    probe: Mutex<ModelProbe>,
    mutation: Arc<tokio::sync::Mutex<()>>,
    status: watch::Sender<Status>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

impl Service {
    pub fn open(paths: Paths, deps: Deps, config: ConfigLoader) -> Result<Arc<Self>, &'static str> {
        // Personas saved before agents belonged to a community are bound to
        // the relay of their unit file, else to the first community.
        let store = Store::open_with(&paths, || {
            config()
                .ok()
                .and_then(|c| c.communities.first().map(|c| c.relay.clone()))
        })?;
        let harnesses = store::HARNESSES
            .iter()
            .map(|id| HarnessView {
                id: (*id).into(),
                bundle: "missing",
                signed_in: None,
            })
            .collect();
        let (status, _) = watch::channel(Status::default());
        let service = Arc::new(Self {
            paths,
            deps,
            config,
            store: Mutex::new(store),
            harnesses: Mutex::new(harnesses),
            schedule: Mutex::new(HarnessSchedule {
                due: Instant::now() + HARNESS_INSPECTION,
                watching: None,
                running: false,
            }),
            units: Mutex::new(BTreeMap::new()),
            pending: Mutex::new(None),
            detail: Mutex::new(None),
            probe: Mutex::new(ModelProbe::default()),
            mutation: Arc::new(tokio::sync::Mutex::new(())),
            status,
        });
        service.publish();
        Ok(service)
    }
    pub fn watch(&self) -> watch::Receiver<Status> {
        self.status.subscribe()
    }
    pub fn snapshot(&self) -> Status {
        self.status.borrow().clone()
    }
    pub fn idle(&self) -> bool {
        self.mutation.try_lock().is_ok()
    }
    /// Recomputes and republishes status (always notifies subscribers).
    pub fn publish(&self) {
        // Read before taking any lock: the loader reads the configuration file.
        let config = (self.config)().ok();
        let community = |relay: &str| {
            config
                .as_ref()
                .and_then(|c| c.community(relay))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| crate::config::host(relay))
        };
        let store = self.store.lock().unwrap();
        let units = self.units.lock().unwrap();
        // An agent without an identity has no unit: it is not running.
        let state = |p: &Persona| {
            if p.identity.is_none() {
                "inactive"
            } else {
                units.get(&p.key()).copied().unwrap_or("unknown")
            }
        };
        let agents = store
            .agents
            .iter()
            .map(|a| {
                let p = a.first();
                AgentView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    description: p.description.clone(),
                    instructions: p.instructions.clone(),
                    harness: p.harness.clone(),
                    model: p.model.clone(),
                    acp_command: p.acp_command.clone(),
                    rooms: p.rooms.clone(),
                    respond_to: p.respond_to.clone(),
                    workspace: p.workspace.clone(),
                    identity: p.identity.clone(),
                    enrolled: p.identity.is_some(),
                    unit: state(&p),
                    start_at_login: p.start_at_login,
                    answers_dms: p.answers_dms,
                    published: p.published,
                    last_error: p.last_error.clone(),
                    relay: p.relay.clone(),
                    community: community(&p.relay),
                    instances: a
                        .personas()
                        .iter()
                        .map(|i| InstanceView {
                            relay: i.relay.clone(),
                            community: community(&i.relay),
                            rooms: i.rooms.clone(),
                            unit: store::unit_name(&i.key()),
                            start_at_login: i.start_at_login,
                            published: i.published,
                            last_error: i.last_error.clone(),
                            state: state(i),
                        })
                        .collect(),
                }
            })
            .collect();
        let status = Status {
            active_relay: config.as_ref().and_then(|c| c.relay.clone()),
            harnesses: self.harnesses.lock().unwrap().clone(),
            agents,
            pending: self.pending.lock().unwrap().clone(),
            model_probe: self.probe.lock().unwrap().clone(),
        };
        drop((store, units));
        self.status.send_replace(status);
    }

    /// Reads the unit state of every instance of every enrolled agent, then
    /// republishes.
    pub async fn inspect_units(&self) {
        let keys: Vec<String> = {
            let store = self.store.lock().unwrap();
            store
                .personas()
                .iter()
                .filter(|p| p.identity.is_some())
                .map(Persona::key)
                .collect()
        };
        for key in keys {
            let control = self.deps.control.clone();
            let name = store::unit_name(&key);
            let state = blocking(move || control.state(&name)).await;
            self.units.lock().unwrap().insert(key, state);
        }
        let known: Vec<String> = {
            let store = self.store.lock().unwrap();
            store.personas().iter().map(Persona::key).collect()
        };
        self.units
            .lock()
            .unwrap()
            .retain(|key, _| known.contains(key));
        self.publish();
    }
    /// Reruns the harness readiness scripts, then republishes.
    pub async fn inspect_harnesses(&self) {
        let paths = self.paths.clone();
        let spawner = self.deps.spawner.clone();
        let views = blocking(move || harness::inspect(&paths, spawner.as_ref())).await;
        {
            let mut schedule = self.schedule.lock().unwrap();
            schedule.running = false;
            let signed_in = schedule.watching.as_ref().is_some_and(|(id, _)| {
                views
                    .iter()
                    .any(|v| &v.id == id && v.signed_in == Some(true))
            });
            if signed_in {
                schedule.watching = None;
                schedule.due = Instant::now() + HARNESS_INSPECTION;
            }
        }
        *self.harnesses.lock().unwrap() = views;
        self.publish();
    }
    /// Whether the periodic harness inspection is due at `now`. A true answer
    /// claims it: the caller runs `inspect_harnesses` once.
    pub fn harness_inspection_due(&self, now: Instant) -> bool {
        let mut schedule = self.schedule.lock().unwrap();
        if now < schedule.due || schedule.running {
            return false;
        }
        if schedule
            .watching
            .as_ref()
            .is_some_and(|(_, until)| now >= *until)
        {
            schedule.watching = None;
        }
        schedule.due = now
            + if schedule.watching.is_some() {
                SIGN_IN_INSPECTION
            } else {
                HARNESS_INSPECTION
            };
        schedule.running = true;
        true
    }

    /// Claims the single mutation slot, or `None` (`agent_busy`).
    pub fn begin(&self) -> Option<OwnedMutexGuard<()>> {
        self.mutation.clone().try_lock_owned().ok()
    }
    fn set_pending(
        &self,
        request: &Request,
        state: &'static str,
        category: Option<&'static str>,
        detail: Option<&'static str>,
    ) {
        *self.pending.lock().unwrap() = Some(Pending {
            request_id: request.id.clone(),
            kind: request.kind.clone(),
            state,
            category,
            detail,
        });
        self.publish();
    }
    /// Refuses a create or update whose model does not belong to its harness.
    fn refuse_model(&self) -> &'static str {
        *self.detail.lock().unwrap() = Some(models::NOT_FOR_HARNESS);
        "agent_invalid"
    }
    fn set_probe(&self, probe: ModelProbe) {
        *self.probe.lock().unwrap() = probe;
        self.publish();
    }
    /// Forgets the last probe when it was of this agent.
    fn reset_probe(&self, id: &str) {
        let mut probe = self.probe.lock().unwrap();
        if probe.agent_id.as_deref() == Some(id) {
            *probe = ModelProbe::default();
        }
    }
    /// Runs a mutating request while holding the slot from `begin`.
    pub async fn execute(
        &self,
        request: &Request,
        _slot: OwnedMutexGuard<()>,
    ) -> Result<(), &'static str> {
        *self.detail.lock().unwrap() = None;
        self.set_pending(request, "working", None, None);
        let result = self.dispatch(request).await.map_err(fixed);
        let detail = self.detail.lock().unwrap().take();
        match result {
            Ok(()) => self.set_pending(request, "done", None, None),
            Err(category) => self.set_pending(request, "failed", Some(category), detail),
        }
        result
    }
    async fn dispatch(&self, r: &Request) -> Result<(), &'static str> {
        let agent = || r.agent_id.clone().ok_or("agent_invalid");
        // The instance a request names, or the agent's first.
        let at = || -> Result<(String, String), &'static str> {
            let id = agent()?;
            let relay = self.instance_relay(&id, r.relay.as_deref())?;
            Ok((id, relay))
        };
        match r.kind.as_str() {
            "create_agent" => self.create(r.fields.clone().ok_or("agent_invalid")?).await,
            "update_agent" => {
                let (id, relay) = at()?;
                self.update(&id, &relay, r.fields.clone().ok_or("agent_invalid")?)
                    .await
            }
            "delete_agent" => self.delete(&agent()?, r.forget.unwrap_or(false)).await,
            "enroll_agent" => {
                let (id, relay) = at()?;
                self.enroll(&id, &relay).await
            }
            "enroll_agent_in" => {
                self.enroll_in(
                    &agent()?,
                    r.relay.as_deref().ok_or("agent_invalid")?,
                    r.rooms.clone().ok_or("agent_invalid")?,
                )
                .await
            }
            "leave_agent_community" => {
                self.leave(&agent()?, r.relay.as_deref().ok_or("agent_invalid")?)
                    .await
            }
            "start_agent" => {
                let (id, relay) = at()?;
                self.start(&id, &relay).await
            }
            "stop_agent" => {
                let (id, relay) = at()?;
                self.stop(&id, &relay).await
            }
            "probe_model" => self.probe_model(&agent()?).await,
            "set_start_at_login" => {
                let (id, relay) = at()?;
                self.set_start_at_login(&id, &relay, r.enabled.ok_or("agent_invalid")?)
                    .await
            }
            "sign_in" => self.sign_in(r.harness.as_deref().unwrap_or("")).await,
            "refresh_bundle" => {
                self.refresh_bundle(r.harness.as_deref().unwrap_or(""))
                    .await
            }
            _ => Err("agent_invalid"),
        }
    }

    fn load_config(&self) -> Result<Config, &'static str> {
        (self.config)().map_err(|_| "relay_unavailable")
    }
    /// The owner: one identity for every community.
    fn owner(&self) -> Result<String, &'static str> {
        self.load_config()?.identity.ok_or("relay_unavailable")
    }
    /// The community a new agent belongs to: the active one, which the
    /// configuration must list.
    fn active_community(&self) -> Result<String, &'static str> {
        let config = self.load_config()?;
        config
            .relay
            .clone()
            .filter(|relay| config.identity.is_some() && config.community(relay).is_some())
            .ok_or("relay_unavailable")
    }
    /// The owner keys' configuration for one community (Secret Service keeps
    /// the owner secret under `relay|identity` per community).
    fn owner_config(&self, relay: &str) -> Result<Config, &'static str> {
        Ok(Config {
            relay: Some(relay.to_owned()),
            identity: Some(self.owner()?),
            communities: Vec::new(),
        })
    }
    /// Rooms are checked against the helper's verified catalog, which exists
    /// only for the active community: an agent of another community cannot
    /// have its rooms checked until that community is active again.
    async fn verify_rooms(&self, relay: &str, rooms: &[String]) -> Result<(), &'static str> {
        let config = self.load_config()?;
        let owner = config.identity.clone().ok_or("relay_unavailable")?;
        if config.relay.as_deref() != Some(relay) {
            return Err("relay_unavailable");
        }
        let joined = self.deps.rooms.joined_rooms(relay, &owner).await?;
        if rooms.iter().all(|room| joined.contains(room)) {
            Ok(())
        } else {
            Err("agent_invalid")
        }
    }
    /// The agent's first instance, in flat form.
    fn persona(&self, id: &str) -> Result<Persona, &'static str> {
        self.store
            .lock()
            .unwrap()
            .get(id)
            .map(Agent::first)
            .ok_or("agent_invalid")
    }
    /// The agent's instance in `relay`'s community, in flat form.
    fn persona_at(&self, id: &str, relay: &str) -> Result<Persona, &'static str> {
        self.store
            .lock()
            .unwrap()
            .get(id)
            .and_then(|a| a.persona(relay))
            .ok_or("agent_invalid")
    }
    /// The relay of the instance a request names (it must exist), or of the
    /// agent's first instance when it names none.
    fn instance_relay(&self, id: &str, relay: Option<&str>) -> Result<String, &'static str> {
        let store = self.store.lock().unwrap();
        let agent = store.get(id).ok_or("agent_invalid")?;
        match relay {
            None => Ok(agent.instances[0].relay.clone()),
            Some(relay) => agent
                .instance(relay)
                .map(|i| i.relay.clone())
                .ok_or("agent_invalid"),
        }
    }
    /// The relays of every instance of an agent, first first.
    fn relays(&self, id: &str) -> Result<Vec<String>, &'static str> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .get(id)
            .ok_or("agent_invalid")?
            .instances
            .iter()
            .map(|i| i.relay.clone())
            .collect())
    }
    /// Saves the store after `change`; the old store is kept on failure.
    fn modify(&self, change: impl FnOnce(&mut Store)) -> Result<(), &'static str> {
        let mut store = self.store.lock().unwrap();
        let previous = store.agents.clone();
        change(&mut store);
        let result = store.save();
        if result.is_err() {
            store.agents = previous;
        }
        drop(store);
        self.publish();
        result.map_err(|_| "agent_invalid")
    }
    /// Replaces (or inserts) the definition and the instance of
    /// `persona.relay` (a new agent, or a new instance of an existing one)
    /// and saves; the old store is kept on failure.
    fn commit(&self, persona: Persona) -> Result<(), &'static str> {
        self.modify(move |store| match store.get_mut(&persona.id) {
            Some(agent) => agent.absorb(persona),
            None => store.agents.push(Agent::from_persona(persona)),
        })
    }
    /// Every other instance (of this agent and of the others), for the
    /// workspace rules: each instance is a separate agent there.
    fn others(&self, key: &str) -> Vec<Persona> {
        self.store
            .lock()
            .unwrap()
            .personas()
            .into_iter()
            .filter(|p| p.key() != key)
            .collect()
    }
    /// Resolves `""` to the instance's default workspace (creating it), then
    /// applies the workspace rules.
    fn workspace(&self, requested: &str, key: &str) -> Result<String, &'static str> {
        let value = if requested.is_empty() {
            let path = store::create_default_workspace(&self.paths, key)?;
            path.to_str().ok_or("workspace_refused")?.to_owned()
        } else {
            requested.to_owned()
        };
        let others = self.others(key);
        let refs: Vec<&Persona> = others.iter().collect();
        store::check_workspace(&self.paths, &value, key, &refs)?;
        Ok(value)
    }

    async fn create(&self, fields: Fields) -> Result<(), &'static str> {
        if self.store.lock().unwrap().agents.len() >= store::MAX_AGENTS {
            return Err("agent_limit");
        }
        let relay = self.active_community()?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut persona = Persona {
            id: id.clone(),
            name: fields.name.ok_or("agent_invalid")?,
            description: fields.description.ok_or("agent_invalid")?,
            instructions: fields.instructions.ok_or("agent_invalid")?,
            harness: fields.harness.ok_or("agent_invalid")?,
            model: fields.model.ok_or("agent_invalid")?,
            acp_command: fields.acp_command.ok_or("agent_invalid")?,
            rooms: fields.rooms.ok_or("agent_invalid")?,
            respond_to: fields.respond_to.ok_or("agent_invalid")?,
            // Validated below, once the field rules pass.
            workspace: "/".into(),
            identity: None,
            start_at_login: fields.start_at_login.ok_or("agent_invalid")?,
            answers_dms: fields.answers_dms.ok_or("agent_invalid")?,
            relay,
            auth_tag: None,
            published: false,
            member_rooms: Vec::new(),
            published_at: 0,
            last_error: None,
            // The instance an agent is created with is named by its id.
            primary: true,
        };
        let requested = fields.workspace.ok_or("agent_invalid")?;
        if !persona.valid() || !(requested.is_empty() || store::canonical_path(&requested)) {
            return Err("agent_invalid");
        }
        if !models::valid_for(&persona.harness, &persona.model) {
            return Err(self.refuse_model());
        }
        self.verify_rooms(&persona.relay, &persona.rooms).await?;
        persona.workspace = self.workspace(&requested, &persona.key())?;
        self.commit(persona)
    }

    /// Stops one instance's unit when it is not inactive.
    async fn stop_if_running(&self, p: &Persona) -> Result<(), &'static str> {
        if p.identity.is_none() {
            return Ok(());
        }
        let control = self.deps.control.clone();
        let name = store::unit_name(&p.key());
        let running = blocking(move || control.state(&name)).await;
        self.units.lock().unwrap().insert(p.key(), running);
        if running != "inactive" {
            self.stop(&p.id, &p.relay).await?;
        }
        Ok(())
    }

    /// `relay` names the instance whose rooms and workspace may change;
    /// definition fields apply to (and republish in) every instance.
    async fn update(&self, id: &str, relay: &str, fields: Fields) -> Result<(), &'static str> {
        let old = self.persona_at(id, relay)?;
        let mut new = old.clone();
        if fields.start_at_login.is_some() {
            return Err("agent_invalid");
        }
        if let Some(v) = fields.name {
            new.name = v;
        }
        if let Some(v) = fields.description {
            new.description = v;
        }
        if let Some(v) = fields.instructions {
            new.instructions = v;
        }
        // A model saved before the harness patterns existed is kept until
        // the model or harness is edited.
        let model_edited = fields.model.is_some() || fields.harness.is_some();
        if let Some(v) = fields.harness {
            new.harness = v;
        }
        if let Some(v) = fields.model {
            new.model = v;
        }
        if let Some(v) = fields.acp_command {
            new.acp_command = v;
        }
        if let Some(v) = fields.rooms {
            new.rooms = v;
        }
        if let Some(v) = fields.respond_to {
            new.respond_to = v;
        }
        if let Some(v) = fields.answers_dms {
            new.answers_dms = v;
        }
        let requested = fields.workspace;
        if requested
            .as_deref()
            .is_some_and(|w| !(w.is_empty() || store::canonical_path(w)))
        {
            return Err("agent_invalid");
        }
        // Dropped rooms stay in `member_rooms` until the relay acknowledges
        // their removal (kind 9001) during the republication below.
        if !new.valid() {
            return Err("agent_invalid");
        }
        if model_edited && !models::valid_for(&new.harness, &new.model) {
            return Err(self.refuse_model());
        }
        if new.rooms != old.rooms {
            self.verify_rooms(&old.relay, &new.rooms).await?;
        }
        if let Some(requested) = requested {
            new.workspace = self.workspace(&requested, &old.key())?;
        }
        // The definition is every instance's: a harness or answering change
        // stops each running instance; rooms and workspace only this one.
        let definition_restart = new.harness != old.harness
            || new.respond_to != old.respond_to
            || new.answers_dms != old.answers_dms;
        let instance_restart = new.workspace != old.workspace || new.rooms != old.rooms;
        let all = self
            .store
            .lock()
            .unwrap()
            .get(id)
            .map(Agent::personas)
            .unwrap_or_default();
        for p in &all {
            if definition_restart || (instance_restart && p.relay == old.relay) {
                self.stop_if_running(p).await?;
            }
        }
        let definition_changed = new.name != old.name
            || new.description != old.description
            || new.instructions != old.instructions
            || new.harness != old.harness
            || new.model != old.model
            || new.respond_to != old.respond_to;
        // Instances whose records change: all of them for the definition,
        // this one for its rooms.
        let republish: Vec<String> = if new.identity.is_none() {
            Vec::new()
        } else {
            all.iter()
                .filter(|p| definition_changed || (p.relay == old.relay && new.rooms != old.rooms))
                .map(|p| p.relay.clone())
                .collect()
        };
        let probe_stale = new.model != old.model || new.harness != old.harness;
        let stale = republish.clone();
        self.modify(move |store| {
            if let Some(agent) = store.get_mut(id) {
                agent.absorb(new);
                for instance in agent.instances.iter_mut() {
                    if stale.contains(&instance.relay) {
                        instance.published = false;
                    }
                }
            }
        })?;
        if probe_stale {
            self.reset_probe(id);
            self.publish();
        }
        let mut first_error = None;
        for relay in republish {
            if let Err(category) = self.publish_records(id, &relay).await {
                first_error.get_or_insert(category);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Removes an enrolled agent's instance from every room it is still a
    /// member of in that community (kind 9001 per room, owner-signed). On
    /// failure the acknowledged removals are recorded and the category
    /// returned; nothing else changes. Memberships added under a previous
    /// owner identity are not attempted: the current owner cannot remove them
    /// on the owner's behalf.
    async fn leave_rooms(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        let Some(identity) = persona.identity.clone() else {
            return Ok(());
        };
        if persona.member_rooms.is_empty() {
            return Ok(());
        }
        let agent = nostr::PublicKey::from_hex(&identity).map_err(|_| "agent_invalid")?;
        let config = self.owner_config(relay)?;
        let keyring = self.deps.keyring.clone();
        let owner = blocking(move || keyring.owner_keys(&config)).await;
        let Ok(owner) = owner else {
            let mut persona = persona;
            persona.last_error = Some("enroll_failed".into());
            self.commit(persona)?;
            return Err("enroll_failed");
        };
        let attested = persona.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent)
                .is_ok_and(|key| key == owner.public_key())
        });
        if !attested {
            return Ok(());
        }
        let at = enroll::next_timestamp(persona.published_at);
        let report = enroll::leave_rooms(relay, &owner, &agent, &persona.member_rooms, at).await;
        let mut persona = self.persona_at(id, relay)?;
        persona
            .member_rooms
            .retain(|room| !report.removed_rooms.contains(room));
        if let Some(category) = report.error {
            persona.last_error = Some(category.into());
            self.commit(persona)?;
            return Err(category);
        }
        self.commit(persona)
    }
    /// Stops, disables and removes an instance's unit file, if there is one.
    async fn remove_unit(&self, key: &str) -> Result<(), &'static str> {
        let unit_path = self.paths.unit_file(key);
        if std::fs::symlink_metadata(&unit_path).is_ok() {
            let control = self.deps.control.clone();
            let name = store::unit_name(key);
            blocking(move || {
                control.run(Op::Stop, &name)?;
                control.run(Op::Disable, &name)
            })
            .await?;
            std::fs::remove_file(&unit_path).map_err(|_| "unit_failed")?;
            let control = self.deps.control.clone();
            blocking(move || control.daemon_reload()).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: &str, forget: bool) -> Result<(), &'static str> {
        // Leave the relay rooms of every community first: if that fails, the
        // agent, its units and its identity are kept so that the deletion can
        // be retried (rooms already left stay left).
        for relay in self.relays(id)? {
            self.leave_rooms(id, &relay).await?;
        }
        let personas = self
            .store
            .lock()
            .unwrap()
            .get(id)
            .map(Agent::personas)
            .ok_or("agent_invalid")?;
        for p in &personas {
            self.remove_unit(&p.key()).await?;
        }
        let identity = personas[0].identity.clone();
        if forget {
            if let Some(identity) = identity {
                let keyring = self.deps.keyring.clone();
                blocking(move || keyring.forget_agent(&identity))
                    .await
                    .map_err(|_| "enroll_failed")?;
            }
        }
        let dir = self.paths.agent_dir(id);
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&dir).map_err(|_| "agent_invalid")?,
            Ok(_) => std::fs::remove_file(&dir).map_err(|_| "agent_invalid")?,
            Err(_) => {}
        }
        self.modify(|store| store.agents.retain(|a| a.id != id))?;
        {
            let mut units = self.units.lock().unwrap();
            for p in &personas {
                units.remove(&p.key());
            }
        }
        self.reset_probe(id);
        self.publish();
        Ok(())
    }

    /// Leaves one community: the owner's kind 9001 for every room of that
    /// instance, then its unit is stopped and removed and the instance
    /// dropped. Its workspace and the records published there (30175/30177)
    /// stay. The last instance cannot be left: deleting the agent does that.
    async fn leave(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        if self.relays(id)?.len() < 2 {
            return Err("agent_invalid");
        }
        self.leave_rooms(id, relay).await?;
        self.remove_unit(&persona.key()).await?;
        self.modify(|store| {
            if let Some(agent) = store.get_mut(id) {
                agent.instances.retain(|i| i.relay != relay);
            }
        })?;
        self.units.lock().unwrap().remove(&persona.key());
        self.publish();
        Ok(())
    }

    /// Writes the instructions (when not empty) and attestation files (0600)
    /// into the agent's private 0700 directory, removing a stale instructions
    /// file when the instructions were cleared. Every instance shares them.
    fn write_private_files(&self, persona: &Persona) -> Result<(), &'static str> {
        store::ensure_private_directory(&self.paths.agent_dir(&persona.id))?;
        let instructions = self.paths.instructions_file(&persona.id);
        if persona.instructions.is_empty() {
            match std::fs::remove_file(&instructions) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("state_unavailable"),
            }
        } else {
            store::write_private(&instructions, persona.instructions.as_bytes())?;
        }
        if let Some(tag) = &persona.auth_tag {
            store::write_private(&self.paths.auth_tag_file(&persona.id), tag.as_bytes())?;
        }
        Ok(())
    }

    /// Enrolls (or re-enrolls) one instance: generates the identity when the
    /// agent has none, (re)attests it, publishes in that community.
    async fn enroll(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let mut persona = self.persona_at(id, relay)?;
        self.verify_rooms(relay, &persona.rooms).await?;
        let config = self.owner_config(relay)?;
        let keyring = self.deps.keyring.clone();
        let owner = blocking(move || keyring.owner_keys(&config))
            .await
            .map_err(|_| "enroll_failed")?;
        let fresh = persona.identity.is_none();
        let agent = match persona.identity.clone() {
            None => {
                let keys = nostr::Keys::generate();
                let identity = keys.public_key().to_hex();
                let secret = Zeroizing::new(keys.secret_key().to_secret_hex());
                let keyring = self.deps.keyring.clone();
                let stored = identity.clone();
                blocking(move || keyring.store_agent(&stored, &secret))
                    .await
                    .map_err(|_| "enroll_failed")?;
                persona.identity = Some(identity);
                keys
            }
            Some(identity) => {
                let keyring = self.deps.keyring.clone();
                blocking(move || keyring.agent_keys(&identity))
                    .await
                    .map_err(|_| "enroll_failed")?
            }
        };
        // (Re)attest when missing or when the owner identity changed. The
        // attestation is every instance's: memberships recorded under the
        // previous owner are forgotten in every community, and the other
        // instances are enrolled again there.
        let attested_by_owner = persona.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent.public_key())
                .is_ok_and(|key| key == owner.public_key())
        });
        let reattest = !attested_by_owner;
        if reattest {
            persona.auth_tag = Some(enroll::attestation(&owner, &agent.public_key())?);
            persona.member_rooms.clear();
        }
        persona.published = false;
        let saved = persona.clone();
        let result = self.modify(move |store| {
            if let Some(agent) = store.get_mut(&saved.id) {
                agent.absorb(saved);
                if reattest {
                    for instance in agent.instances.iter_mut() {
                        instance.member_rooms.clear();
                        instance.published = false;
                    }
                }
            }
        });
        if let Err(category) = result {
            // Do not leave an unreferenced fresh secret behind.
            if let (true, Some(identity)) = (fresh, persona.identity.clone()) {
                let keyring = self.deps.keyring.clone();
                let _ = blocking(move || keyring.forget_agent(&identity)).await;
            }
            return Err(category);
        }
        self.write_private_files(&persona)
            .map_err(|_| "enroll_failed")?;
        self.publish_with(id, relay, &owner, &agent).await
    }

    /// Adds an enrolled agent to another community: the active one, with
    /// 1–8 rooms of its verified catalog. The agent keeps its identity and
    /// the owner's attestation; the new instance gets its own unit name and
    /// default workspace (`<id>-<h>`). A failed publication keeps the
    /// instance unpublished with `lastError`; `enroll_agent` with its relay
    /// retries it.
    async fn enroll_in(
        &self,
        id: &str,
        relay: &str,
        rooms: Vec<String>,
    ) -> Result<(), &'static str> {
        let first = self.persona(id)?;
        let count = self.relays(id)?.len();
        let identity = first.identity.clone().ok_or("agent_invalid")?;
        if first.auth_tag.is_none()
            || self.persona_at(id, relay).is_ok()
            || count >= store::MAX_INSTANCES
            || !store::valid_rooms(&rooms)
        {
            return Err("agent_invalid");
        }
        if self.active_community()? != relay {
            return Err("relay_unavailable");
        }
        self.verify_rooms(relay, &rooms).await?;
        let config = self.owner_config(relay)?;
        let keyring = self.deps.keyring.clone();
        let keys = blocking(move || {
            Ok::<_, &'static str>((keyring.owner_keys(&config)?, keyring.agent_keys(&identity)?))
        })
        .await;
        let (owner, agent) = keys.map_err(|_| "enroll_failed")?;
        // The shared attestation must be this owner's: after an owner change
        // the agent is enrolled again in its first community first.
        let attested = first.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent.public_key())
                .is_ok_and(|key| key == owner.public_key())
        });
        if !attested {
            return Err("enroll_failed");
        }
        let mut persona = Persona {
            relay: relay.to_owned(),
            primary: false,
            rooms,
            workspace: "/".into(),
            start_at_login: false,
            published: false,
            member_rooms: Vec::new(),
            published_at: 0,
            last_error: None,
            ..first
        };
        if !persona.valid() {
            return Err("agent_invalid");
        }
        persona.workspace = self.workspace("", &persona.key())?;
        self.commit(persona.clone())?;
        self.write_private_files(&persona)
            .map_err(|_| "enroll_failed")?;
        self.publish_with(id, relay, &owner, &agent).await
    }

    /// Republishes an enrolled agent's records in one community after an edit.
    async fn publish_records(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        let identity = persona.identity.clone().ok_or("agent_invalid")?;
        let config = self.owner_config(relay)?;
        let keyring = self.deps.keyring.clone();
        let keys = blocking(move || {
            Ok::<_, &'static str>((keyring.owner_keys(&config)?, keyring.agent_keys(&identity)?))
        })
        .await;
        let (owner, agent) = match keys {
            Ok(keys) => keys,
            Err(_) => return self.record_failure(id, relay, "enroll_failed"),
        };
        let attested = persona.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent.public_key())
                .is_ok_and(|key| key == owner.public_key())
        });
        if !attested {
            // The owner changed since enrollment: enroll again explicitly.
            return self.record_failure(id, relay, "enroll_failed");
        }
        self.write_private_files(&persona)
            .map_err(|_| "enroll_failed")?;
        self.publish_with(id, relay, &owner, &agent).await
    }
    fn record_failure(
        &self,
        id: &str,
        relay: &str,
        category: &'static str,
    ) -> Result<(), &'static str> {
        let mut persona = self.persona_at(id, relay)?;
        persona.published = false;
        persona.last_error = Some(category.into());
        self.commit(persona)?;
        Err(category)
    }
    async fn publish_with(
        &self,
        id: &str,
        relay: &str,
        owner: &nostr::Keys,
        agent: &nostr::Keys,
    ) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        let report = enroll::publish_all(relay, owner, agent, &persona).await;
        let mut persona = self.persona_at(id, relay)?;
        persona.member_rooms.extend(report.member_rooms);
        persona
            .member_rooms
            .retain(|room| !report.removed_rooms.contains(room));
        if let Some(at) = report.published_at {
            persona.published_at = persona.published_at.max(at);
        }
        persona.published = report.error.is_none();
        persona.last_error = report.error.map(str::to_owned);
        self.commit(persona)?;
        match report.error {
            Some(category) => Err(category),
            None => Ok(()),
        }
    }

    /// Renders and installs one instance's unit file, then reloads systemd.
    async fn install_unit(&self, persona: &Persona) -> Result<(), &'static str> {
        let owner = self.owner()?;
        let text = unit::render(&self.paths, persona, &owner)?;
        self.write_private_files(persona)
            .map_err(|_| "unit_failed")?;
        let dir = self.paths.units_dir();
        if std::fs::symlink_metadata(&dir).is_err() {
            std::fs::create_dir_all(&dir).map_err(|_| "unit_failed")?;
        }
        let owned = std::fs::symlink_metadata(&dir).is_ok_and(|m| {
            use std::os::unix::fs::MetadataExt;
            m.is_dir() && m.uid() == rustix::process::getuid().as_raw() && m.mode() & 0o022 == 0
        });
        if !owned || !store::unlinked(&dir) {
            return Err("unit_failed");
        }
        store::write_private(&self.paths.unit_file(&persona.key()), text.as_bytes())
            .map_err(|_| "unit_failed")?;
        let control = self.deps.control.clone();
        blocking(move || control.daemon_reload()).await
    }

    async fn start(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        if persona.identity.is_none() || !persona.published {
            return Err("agent_invalid");
        }
        self.harness_ready(&persona.harness).await?;
        let key = persona.key();
        let others = self.others(&key);
        let refs: Vec<&Persona> = others.iter().collect();
        store::check_workspace(&self.paths, &persona.workspace, &key, &refs)?;
        self.install_unit(&persona).await?;
        let control = self.deps.control.clone();
        let name = store::unit_name(&key);
        let enable = persona.start_at_login;
        let result = blocking(move || {
            if enable {
                control.run(Op::Enable, &name)?;
            }
            control.run(Op::Start, &name)
        })
        .await;
        self.inspect_units().await;
        result
    }

    /// Re-reads harness readiness; refuses a stale or missing bundle and a
    /// harness that does not report signed in.
    async fn harness_ready(&self, harness: &str) -> Result<(), &'static str> {
        self.inspect_harnesses().await;
        let view = self
            .harnesses
            .lock()
            .unwrap()
            .iter()
            .find(|h| h.id == harness)
            .cloned()
            .ok_or("harness_missing")?;
        match view.bundle {
            "ready" => {}
            "stale" => return Err("bundle_stale"),
            _ => return Err("harness_missing"),
        }
        if view.signed_in != Some(true) {
            return Err("not_signed_in");
        }
        Ok(())
    }

    /// Runs one bounded model turn in the agent's sandbox view (see
    /// `models`). The request is done once a result is known, whatever it
    /// is; the result is `status.modelProbe`.
    async fn probe_model(&self, id: &str) -> Result<(), &'static str> {
        let persona = self.persona(id)?;
        if persona.model.is_empty() || !models::valid_for(&persona.harness, &persona.model) {
            return Err("agent_invalid");
        }
        self.harness_ready(&persona.harness).await?;
        let launcher = unit::launcher(&self.paths, &persona.harness);
        let env = models::probe_environment(std::env::vars_os());
        let argv = models::probe_argv(&self.paths, &persona.harness, &persona.model, &env);
        let spawner = self.deps.spawner.clone();
        if !blocking({
            let spawner = spawner.clone();
            move || spawner.present(&launcher)
        })
        .await
        {
            return Err("harness_missing");
        }
        let probe = |state, detail| ModelProbe {
            agent_id: Some(id.to_owned()),
            state,
            model: persona.model.clone(),
            detail,
        };
        self.set_probe(probe("running", None));
        let (state, detail) = match blocking(move || spawner.probe(&argv, &env)).await {
            Ok((code, output)) => models::classify(code, &output),
            Err(_) => ("failed", models::FAILED),
        };
        self.set_probe(probe(state, Some(detail)));
        Ok(())
    }

    async fn stop(&self, id: &str, relay: &str) -> Result<(), &'static str> {
        let persona = self.persona_at(id, relay)?;
        if persona.identity.is_none() {
            return Ok(());
        }
        let control = self.deps.control.clone();
        let name = store::unit_name(&persona.key());
        let result = blocking(move || control.run(Op::Stop, &name)).await;
        self.inspect_units().await;
        result
    }

    async fn set_start_at_login(
        &self,
        id: &str,
        relay: &str,
        enabled: bool,
    ) -> Result<(), &'static str> {
        let mut persona = self.persona_at(id, relay)?;
        let key = persona.key();
        let installed = std::fs::symlink_metadata(self.paths.unit_file(&key)).is_ok();
        if persona.identity.is_some() && (enabled || installed) {
            if enabled {
                self.install_unit(&persona).await?;
            }
            let control = self.deps.control.clone();
            let name = store::unit_name(&key);
            let op = if enabled { Op::Enable } else { Op::Disable };
            blocking(move || control.run(op, &name)).await?;
        }
        persona.start_at_login = enabled;
        self.commit(persona)
    }

    /// Replaces a harness bundle's launcher files with the installed scripts,
    /// then re-reads readiness: done when the bundle is `ready`, otherwise
    /// `bundle_stale` (still stale) or `harness_missing`.
    async fn refresh_bundle(&self, harness: &str) -> Result<(), &'static str> {
        if !store::valid_harness(harness) {
            return Err("agent_invalid");
        }
        let script = harness::bundle_script(&self.paths);
        let argv = harness::refresh_argv(&self.paths, harness);
        let spawner = self.deps.spawner.clone();
        blocking(move || {
            if !spawner.present(&script) {
                return Err("harness_missing");
            }
            // The outcome is read back through `--check` below.
            let _ = spawner.output(&argv);
            Ok(())
        })
        .await?;
        self.inspect_harnesses().await;
        let bundle = self
            .harnesses
            .lock()
            .unwrap()
            .iter()
            .find(|h| h.id == harness)
            .map(|h| h.bundle);
        match bundle {
            Some("ready") => Ok(()),
            Some("stale") => Err("bundle_stale"),
            _ => Err("harness_missing"),
        }
    }

    async fn sign_in(&self, harness: &str) -> Result<(), &'static str> {
        if !store::valid_harness(harness) {
            return Err("agent_invalid");
        }
        let script = harness::login_script(&self.paths);
        let argv = harness::login_argv(&self.paths, harness);
        let env = harness::login_environment(std::env::vars_os());
        let spawner = self.deps.spawner.clone();
        blocking(move || {
            if !spawner.present(&script) {
                return Err("harness_missing");
            }
            spawner.spawn(&argv, &env).map_err(|_| "harness_missing")
        })
        .await?;
        // The login finishes in a detached terminal and browser: re-check its
        // status often for a while rather than waiting for the next minute.
        let now = Instant::now();
        let mut schedule = self.schedule.lock().unwrap();
        schedule.watching = Some((harness.to_owned(), now + SIGN_IN_WINDOW));
        schedule.due = schedule.due.min(now + SIGN_IN_INSPECTION);
        Ok(())
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
