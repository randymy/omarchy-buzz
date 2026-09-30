//! The agent manager: persona store, enrollment, units and status. One
//! mutating request runs at a time; every change republishes status.
use super::{
    enroll,
    harness::{self, HarnessView, Spawner},
    keys::Keyring,
    request::{Fields, Request},
    rooms::RoomSource,
    store::{self, Paths, Persona, Store},
    unit::{self, Op, UnitControl},
};
use crate::config::Config;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{watch, OwnedMutexGuard};
use zeroize::Zeroizing;

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
    pub published: bool,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub request_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub state: &'static str,
    pub category: Option<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub harnesses: Vec<HarnessView>,
    pub agents: Vec<AgentView>,
    pub pending: Option<Pending>,
}

pub fn envelope(kind: &str, id: Option<&str>, instance: &str, s: &Status) -> serde_json::Value {
    serde_json::json!({"version":1,"type":kind,"id":id,"instanceId":instance,"capabilities":["agent_manager"],"status":s})
}
pub fn error_frame(id: &str, instance: &str, category: &str) -> serde_json::Value {
    serde_json::json!({"version":1,"type":"error","id":id,"instanceId":instance,"category":category})
}

/// The fixed error categories of the contract.
pub const CATEGORIES: [&str; 9] = [
    "agent_invalid",
    "agent_busy",
    "agent_limit",
    "harness_missing",
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
    units: Mutex<BTreeMap<String, &'static str>>,
    pending: Mutex<Option<Pending>>,
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
        let store = Store::open(&paths)?;
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
            units: Mutex::new(BTreeMap::new()),
            pending: Mutex::new(None),
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
        let store = self.store.lock().unwrap();
        let units = self.units.lock().unwrap();
        let agents = store
            .agents
            .iter()
            .map(|p| AgentView {
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
                // An agent without an identity has no unit: it is not running.
                unit: if p.identity.is_none() {
                    "inactive"
                } else {
                    units.get(&p.id).copied().unwrap_or("unknown")
                },
                start_at_login: p.start_at_login,
                published: p.published,
                last_error: p.last_error.clone(),
            })
            .collect();
        let status = Status {
            harnesses: self.harnesses.lock().unwrap().clone(),
            agents,
            pending: self.pending.lock().unwrap().clone(),
        };
        drop((store, units));
        self.status.send_replace(status);
    }

    /// Reads every enrolled agent's unit state, then republishes.
    pub async fn inspect_units(&self) {
        let ids: Vec<String> = {
            let store = self.store.lock().unwrap();
            store
                .agents
                .iter()
                .filter(|p| p.identity.is_some())
                .map(|p| p.id.clone())
                .collect()
        };
        for id in ids {
            let control = self.deps.control.clone();
            let name = store::unit_name(&id);
            let state = blocking(move || control.state(&name)).await;
            self.units.lock().unwrap().insert(id, state);
        }
        let known: Vec<String> = {
            let store = self.store.lock().unwrap();
            store.agents.iter().map(|p| p.id.clone()).collect()
        };
        self.units
            .lock()
            .unwrap()
            .retain(|id, _| known.contains(id));
        self.publish();
    }
    /// Reruns the harness readiness scripts, then republishes.
    pub async fn inspect_harnesses(&self) {
        let paths = self.paths.clone();
        let spawner = self.deps.spawner.clone();
        let views = blocking(move || harness::inspect(&paths, spawner.as_ref())).await;
        *self.harnesses.lock().unwrap() = views;
        self.publish();
    }

    /// Claims the single mutation slot, or `None` (`agent_busy`).
    pub fn begin(&self) -> Option<OwnedMutexGuard<()>> {
        self.mutation.clone().try_lock_owned().ok()
    }
    fn set_pending(&self, request: &Request, state: &'static str, category: Option<&'static str>) {
        *self.pending.lock().unwrap() = Some(Pending {
            request_id: request.id.clone(),
            kind: request.kind.clone(),
            state,
            category,
        });
        self.publish();
    }
    /// Runs a mutating request while holding the slot from `begin`.
    pub async fn execute(
        &self,
        request: &Request,
        _slot: OwnedMutexGuard<()>,
    ) -> Result<(), &'static str> {
        self.set_pending(request, "working", None);
        let result = self.dispatch(request).await.map_err(fixed);
        match result {
            Ok(()) => self.set_pending(request, "done", None),
            Err(category) => self.set_pending(request, "failed", Some(category)),
        }
        result
    }
    async fn dispatch(&self, r: &Request) -> Result<(), &'static str> {
        let agent = || r.agent_id.clone().ok_or("agent_invalid");
        match r.kind.as_str() {
            "create_agent" => self.create(r.fields.clone().ok_or("agent_invalid")?).await,
            "update_agent" => {
                self.update(&agent()?, r.fields.clone().ok_or("agent_invalid")?)
                    .await
            }
            "delete_agent" => self.delete(&agent()?, r.forget.unwrap_or(false)).await,
            "enroll_agent" => self.enroll(&agent()?).await,
            "start_agent" => self.start(&agent()?).await,
            "stop_agent" => self.stop(&agent()?).await,
            "set_start_at_login" => {
                self.set_start_at_login(&agent()?, r.enabled.ok_or("agent_invalid")?)
                    .await
            }
            "sign_in" => self.sign_in(r.harness.as_deref().unwrap_or("")).await,
            _ => Err("agent_invalid"),
        }
    }

    fn scope(&self) -> Result<(String, String), &'static str> {
        let config = (self.config)().map_err(|_| "relay_unavailable")?;
        match (config.relay, config.identity) {
            (Some(relay), Some(owner)) => Ok((relay, owner)),
            _ => Err("relay_unavailable"),
        }
    }
    async fn verify_rooms(&self, rooms: &[String]) -> Result<(), &'static str> {
        let (relay, owner) = self.scope()?;
        let joined = self.deps.rooms.joined_rooms(&relay, &owner).await?;
        if rooms.iter().all(|room| joined.contains(room)) {
            Ok(())
        } else {
            Err("agent_invalid")
        }
    }
    fn persona(&self, id: &str) -> Result<Persona, &'static str> {
        self.store
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("agent_invalid")
    }
    /// Replaces (or inserts) a persona and saves; the old store is kept on failure.
    fn commit(&self, persona: Persona) -> Result<(), &'static str> {
        let mut store = self.store.lock().unwrap();
        let previous = store.agents.clone();
        match store.get_mut(&persona.id) {
            Some(slot) => *slot = persona,
            None => store.agents.push(persona),
        }
        let result = store.save();
        if result.is_err() {
            store.agents = previous;
        }
        drop(store);
        self.publish();
        result.map_err(|_| "agent_invalid")
    }
    fn others(&self, id: &str) -> Vec<Persona> {
        self.store
            .lock()
            .unwrap()
            .agents
            .iter()
            .filter(|p| p.id != id)
            .cloned()
            .collect()
    }
    /// Resolves `""` to the default workspace (creating it), then applies the
    /// workspace rules.
    fn workspace(&self, requested: &str, id: &str) -> Result<String, &'static str> {
        let value = if requested.is_empty() {
            let path = store::create_default_workspace(&self.paths, id)?;
            path.to_str().ok_or("workspace_refused")?.to_owned()
        } else {
            requested.to_owned()
        };
        let others = self.others(id);
        let refs: Vec<&Persona> = others.iter().collect();
        store::check_workspace(&self.paths, &value, id, &refs)?;
        Ok(value)
    }

    async fn create(&self, fields: Fields) -> Result<(), &'static str> {
        if self.store.lock().unwrap().agents.len() >= store::MAX_AGENTS {
            return Err("agent_limit");
        }
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
            auth_tag: None,
            published: false,
            member_rooms: Vec::new(),
            published_at: 0,
            last_error: None,
        };
        let requested = fields.workspace.ok_or("agent_invalid")?;
        if !persona.valid() || !(requested.is_empty() || store::canonical_path(&requested)) {
            return Err("agent_invalid");
        }
        self.verify_rooms(&persona.rooms).await?;
        persona.workspace = self.workspace(&requested, &id)?;
        self.commit(persona)
    }

    async fn update(&self, id: &str, fields: Fields) -> Result<(), &'static str> {
        let old = self.persona(id)?;
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
        let requested = fields.workspace;
        if requested
            .as_deref()
            .is_some_and(|w| !(w.is_empty() || store::canonical_path(w)))
        {
            return Err("agent_invalid");
        }
        new.member_rooms.retain(|room| new.rooms.contains(room));
        if !new.valid() {
            return Err("agent_invalid");
        }
        if new.rooms != old.rooms {
            self.verify_rooms(&new.rooms).await?;
        }
        if let Some(requested) = requested {
            new.workspace = self.workspace(&requested, id)?;
        }
        let restart_fields = new.harness != old.harness
            || new.workspace != old.workspace
            || new.rooms != old.rooms
            || new.respond_to != old.respond_to;
        if restart_fields && old.identity.is_some() {
            let control = self.deps.control.clone();
            let name = store::unit_name(id);
            let running = blocking(move || control.state(&name)).await;
            self.units.lock().unwrap().insert(id.into(), running);
            if running != "inactive" {
                self.stop(id).await?;
            }
        }
        let republish = new.identity.is_some()
            && (new.name != old.name
                || new.description != old.description
                || new.instructions != old.instructions
                || new.harness != old.harness
                || new.model != old.model
                || new.rooms != old.rooms
                || new.respond_to != old.respond_to);
        if republish {
            new.published = false;
        }
        self.commit(new)?;
        if republish {
            self.publish_records(id).await
        } else {
            Ok(())
        }
    }

    async fn delete(&self, id: &str, forget: bool) -> Result<(), &'static str> {
        let persona = self.persona(id)?;
        let unit_path = self.paths.unit_file(id);
        if std::fs::symlink_metadata(&unit_path).is_ok() {
            let control = self.deps.control.clone();
            let name = store::unit_name(id);
            blocking(move || {
                control.run(Op::Stop, &name)?;
                control.run(Op::Disable, &name)
            })
            .await?;
            std::fs::remove_file(&unit_path).map_err(|_| "unit_failed")?;
            let control = self.deps.control.clone();
            blocking(move || control.daemon_reload()).await?;
        }
        if forget {
            if let Some(identity) = persona.identity.clone() {
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
        {
            let mut store = self.store.lock().unwrap();
            let previous = store.agents.clone();
            store.agents.retain(|p| p.id != id);
            if store.save().is_err() {
                store.agents = previous;
                drop(store);
                self.publish();
                return Err("agent_invalid");
            }
        }
        self.units.lock().unwrap().remove(id);
        self.publish();
        Ok(())
    }

    /// Writes the instructions (when not empty) and attestation files (0600)
    /// into the agent's private 0700 directory, removing a stale instructions
    /// file when the instructions were cleared.
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

    async fn enroll(&self, id: &str) -> Result<(), &'static str> {
        let mut persona = self.persona(id)?;
        let (relay, owner_hex) = self.scope()?;
        self.verify_rooms(&persona.rooms).await?;
        let keyring = self.deps.keyring.clone();
        let config = Config {
            relay: Some(relay.clone()),
            identity: Some(owner_hex.clone()),
        };
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
        // (Re)attest when missing or when the owner identity changed.
        let attested_by_owner = persona.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent.public_key())
                .is_ok_and(|key| key == owner.public_key())
        });
        if !attested_by_owner {
            persona.auth_tag = Some(enroll::attestation(&owner, &agent.public_key())?);
            persona.member_rooms.clear();
        }
        persona.published = false;
        if let Err(category) = self.commit(persona.clone()) {
            // Do not leave an unreferenced fresh secret behind.
            if let (true, Some(identity)) = (fresh, persona.identity.clone()) {
                let keyring = self.deps.keyring.clone();
                let _ = blocking(move || keyring.forget_agent(&identity)).await;
            }
            return Err(category);
        }
        self.write_private_files(&persona)
            .map_err(|_| "enroll_failed")?;
        self.publish_with(id, &relay, &owner, &agent).await
    }

    /// Republishes an enrolled agent's records after an edit.
    async fn publish_records(&self, id: &str) -> Result<(), &'static str> {
        let persona = self.persona(id)?;
        let (relay, owner_hex) = self.scope()?;
        let identity = persona.identity.clone().ok_or("agent_invalid")?;
        let keyring = self.deps.keyring.clone();
        let config = Config {
            relay: Some(relay.clone()),
            identity: Some(owner_hex),
        };
        let keys = blocking(move || {
            Ok::<_, &'static str>((keyring.owner_keys(&config)?, keyring.agent_keys(&identity)?))
        })
        .await;
        let (owner, agent) = match keys {
            Ok(keys) => keys,
            Err(_) => return self.record_failure(id, "enroll_failed"),
        };
        let attested = persona.auth_tag.as_deref().is_some_and(|tag| {
            buzz_sdk::nip_oa::verify_auth_tag(tag, &agent.public_key())
                .is_ok_and(|key| key == owner.public_key())
        });
        if !attested {
            // The owner changed since enrollment: enroll again explicitly.
            return self.record_failure(id, "enroll_failed");
        }
        self.write_private_files(&persona)
            .map_err(|_| "enroll_failed")?;
        self.publish_with(id, &relay, &owner, &agent).await
    }
    fn record_failure(&self, id: &str, category: &'static str) -> Result<(), &'static str> {
        let mut persona = self.persona(id)?;
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
        let persona = self.persona(id)?;
        let report = enroll::publish_all(relay, owner, agent, &persona).await;
        let mut persona = self.persona(id)?;
        persona.member_rooms.extend(report.member_rooms);
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

    /// Renders and installs the unit file, then reloads systemd.
    async fn install_unit(&self, persona: &Persona) -> Result<(), &'static str> {
        let (relay, owner) = self.scope()?;
        let text = unit::render(&self.paths, persona, &relay, &owner)?;
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
        store::write_private(&self.paths.unit_file(&persona.id), text.as_bytes())
            .map_err(|_| "unit_failed")?;
        let control = self.deps.control.clone();
        blocking(move || control.daemon_reload()).await
    }

    async fn start(&self, id: &str) -> Result<(), &'static str> {
        let persona = self.persona(id)?;
        if persona.identity.is_none() || !persona.published {
            return Err("agent_invalid");
        }
        self.inspect_harnesses().await;
        let view = self
            .harnesses
            .lock()
            .unwrap()
            .iter()
            .find(|h| h.id == persona.harness)
            .cloned()
            .ok_or("harness_missing")?;
        if view.bundle != "ready" {
            return Err("harness_missing");
        }
        if view.signed_in != Some(true) {
            return Err("not_signed_in");
        }
        let others = self.others(id);
        let refs: Vec<&Persona> = others.iter().collect();
        store::check_workspace(&self.paths, &persona.workspace, id, &refs)?;
        self.install_unit(&persona).await?;
        let control = self.deps.control.clone();
        let name = store::unit_name(id);
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

    async fn stop(&self, id: &str) -> Result<(), &'static str> {
        let persona = self.persona(id)?;
        if persona.identity.is_none() {
            return Ok(());
        }
        let control = self.deps.control.clone();
        let name = store::unit_name(id);
        let result = blocking(move || control.run(Op::Stop, &name)).await;
        self.inspect_units().await;
        result
    }

    async fn set_start_at_login(&self, id: &str, enabled: bool) -> Result<(), &'static str> {
        let mut persona = self.persona(id)?;
        let installed = std::fs::symlink_metadata(self.paths.unit_file(id)).is_ok();
        if persona.identity.is_some() && (enabled || installed) {
            if enabled {
                self.install_unit(&persona).await?;
            }
            let control = self.deps.control.clone();
            let name = store::unit_name(id);
            let op = if enabled { Op::Enable } else { Op::Disable };
            blocking(move || control.run(op, &name)).await?;
        }
        persona.start_at_login = enabled;
        self.commit(persona)
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
        .await
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
