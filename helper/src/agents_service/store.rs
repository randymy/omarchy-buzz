//! Persona store (`docs/AGENTS_SERVICE.md`, "Persona record"): one owner-only
//! JSON file, replaced atomically, with every field validated on load and save.
//! The private agent key never enters this file; only its public identity and
//! the owner's (public) NIP-OA attestation do.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

pub const MAX_AGENTS: usize = 16;
pub const HARNESSES: [&str; 2] = ["claude-code", "codex"];
pub const ACP_COMMAND: &str = "buzz-acp";
pub const RESPOND_TO: [&str; 2] = ["owner-only", "mentions"];
const NAME_CHARS: usize = 64;
const DESCRIPTION_CHARS: usize = 256;
pub const INSTRUCTIONS_BYTES: usize = 16 * 1024;
const MODEL_CHARS: usize = 64;
const MAX_ROOMS: usize = 8;
/// Communities one agent can be enrolled in ("instances"). Bounded so that the
/// store (1 MiB) and a status frame stay within their limits in the worst case.
pub const MAX_INSTANCES: usize = 4;
/// Acknowledged memberships, including dropped rooms still to be left.
pub const MAX_MEMBER_ROOMS: usize = 64;
const PATH_BYTES: usize = 1024;
// 16 agents with 16 KiB instructions each, plus JSON escaping of the permitted
// whitespace, stay well below this bound.
const STORE_BYTES: u64 = 1024 * 1024;

/// Locations derived from the process environment. Tests and the smoke check
/// redirect them with `HOME` and the XDG base directory variables.
#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub state: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
}
impl Paths {
    pub fn from_env() -> Result<Self, &'static str> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or("home_unavailable")?;
        let xdg = |name: &str, fallback: &str| -> Result<PathBuf, &'static str> {
            match std::env::var_os(name) {
                Some(value) => {
                    let path = PathBuf::from(value);
                    if path.is_absolute() {
                        Ok(path)
                    } else {
                        Err("state_unavailable")
                    }
                }
                None => Ok(home.join(fallback)),
            }
        };
        Ok(Self {
            state: xdg("XDG_STATE_HOME", ".local/state")?,
            config: xdg("XDG_CONFIG_HOME", ".config")?,
            data: xdg("XDG_DATA_HOME", ".local/share")?,
            home,
        })
    }
    pub fn store_dir(&self) -> PathBuf {
        self.state.join("omarchy-buzz/agents")
    }
    pub fn store_file(&self) -> PathBuf {
        self.store_dir().join("personas.json")
    }
    /// The agent's private directory: instructions and attestation files.
    pub fn agent_dir(&self, id: &str) -> PathBuf {
        self.store_dir().join(id)
    }
    pub fn instructions_file(&self, id: &str) -> PathBuf {
        self.agent_dir(id).join("instructions.md")
    }
    pub fn auth_tag_file(&self, id: &str) -> PathBuf {
        self.agent_dir(id).join("auth-tag.json")
    }
    pub fn workspaces(&self) -> PathBuf {
        self.state.join("omarchy-buzz-room-workspaces")
    }
    /// The default workspace of an instance (`key`: `Persona::key`).
    pub fn default_workspace(&self, key: &str) -> PathBuf {
        self.workspaces().join(key)
    }
    pub fn units_dir(&self) -> PathBuf {
        self.config.join("systemd/user")
    }
    /// The unit file of an instance (`key`: `Persona::key`).
    pub fn unit_file(&self, key: &str) -> PathBuf {
        self.units_dir().join(unit_name(key))
    }
    pub fn bundle(&self, harness: &str) -> PathBuf {
        self.data.join(format!("omarchy-buzz/agent-{harness}"))
    }
    pub fn profile(&self, harness: &str) -> PathBuf {
        self.state.join("omarchy-buzz-agent-preview").join(harness)
    }
    /// Reviewed scripts installed by a later, human-run step.
    pub fn scripts_dir(&self) -> PathBuf {
        self.data.join("omarchy-buzz/scripts")
    }
}

/// The unit of an instance (`key`: `Persona::key`): `omarchy-buzz-agent-<id>.service`
/// for the agent's primary instance, `omarchy-buzz-agent-<id>-<h>.service` for
/// the others.
pub fn unit_name(key: &str) -> String {
    format!("omarchy-buzz-agent-{key}.service")
}

/// The first 12 hex digits of SHA-256 of a canonical relay: the suffix that
/// names an agent's unit and default workspace in a further community.
pub fn relay_hash(relay: &str) -> String {
    use nostr::hashes::{sha256, Hash};
    sha256::Hash::hash(relay.as_bytes()).to_string()[..12].to_owned()
}

/// One agent in one community: the agent's definition and identity with one
/// of its instances. The store keeps `Agent` records (version 2); this flat
/// form is how every path (unit, enrollment, workspace rules) sees one
/// instance, and the shape of a version 1 store's records. Fields after
/// `start_at_login` are service-private: they never appear in IPC status
/// frames and never in a published managed-agent record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Persona {
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
    pub start_at_login: bool,
    /// Launch without a room filter so the agent also answers direct messages
    /// (to its owner only; `respondTo` still applies in rooms). Stores written
    /// before this field existed load it as false.
    #[serde(default)]
    pub answers_dms: bool,
    /// The canonical relay of the community this agent belongs to: it is
    /// created (and enrolled) in the active community and stays bound to it
    /// when another community becomes active. A store written before this
    /// field existed is migrated once on load (`Store::open_with`).
    #[serde(default)]
    pub relay: String,
    /// The owner's NIP-OA `auth` tag JSON for `identity` (public, not secret).
    pub auth_tag: Option<String>,
    /// True only after the relay's `OK` for every enrollment publication.
    pub published: bool,
    /// Rooms whose add-member command the relay acknowledged and whose
    /// remove-member command it has not. Entries no longer in `rooms` are
    /// dropped rooms still to be left (kind 9001) on the next publication.
    pub member_rooms: Vec<String>,
    /// `created_at` of the last kind 30175/30177 publication (monotonic).
    pub published_at: u64,
    pub last_error: Option<String>,
    /// The instance's unit and default workspace are named by the agent id
    /// alone (the instance the agent was created with, or a version 1
    /// record); otherwise by `<id>-<relay_hash>`. Not part of a version 1
    /// record.
    #[serde(skip)]
    pub primary: bool,
}

/// The agent's membership in one community (store version 2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Instance {
    pub relay: String,
    /// Named without a relay suffix (see `Persona::primary`).
    pub primary: bool,
    pub rooms: Vec<String>,
    pub workspace: String,
    pub start_at_login: bool,
    pub published: bool,
    pub member_rooms: Vec<String>,
    pub published_at: u64,
    pub last_error: Option<String>,
}

/// One agent (store version 2): definition and identity, shared by every
/// community it is enrolled in, and one instance per community.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub harness: String,
    pub model: String,
    pub acp_command: String,
    pub respond_to: String,
    pub answers_dms: bool,
    pub identity: Option<String>,
    pub auth_tag: Option<String>,
    pub instances: Vec<Instance>,
}

impl Agent {
    /// A new agent from its first instance's flat form.
    pub fn from_persona(p: Persona) -> Self {
        let mut agent = Self {
            id: p.id.clone(),
            name: String::new(),
            description: String::new(),
            instructions: String::new(),
            harness: String::new(),
            model: String::new(),
            acp_command: String::new(),
            respond_to: String::new(),
            answers_dms: false,
            identity: None,
            auth_tag: None,
            instances: Vec::new(),
        };
        agent.absorb(p);
        agent
    }
    /// One instance in flat form.
    pub fn view(&self, i: &Instance) -> Persona {
        Persona {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            instructions: self.instructions.clone(),
            harness: self.harness.clone(),
            model: self.model.clone(),
            acp_command: self.acp_command.clone(),
            rooms: i.rooms.clone(),
            respond_to: self.respond_to.clone(),
            workspace: i.workspace.clone(),
            identity: self.identity.clone(),
            start_at_login: i.start_at_login,
            answers_dms: self.answers_dms,
            relay: i.relay.clone(),
            auth_tag: self.auth_tag.clone(),
            published: i.published,
            member_rooms: i.member_rooms.clone(),
            published_at: i.published_at,
            last_error: i.last_error.clone(),
            primary: i.primary,
        }
    }
    /// The first instance: the default of every request that names no relay,
    /// and the top-level `relay`/`community` of status.
    pub fn first(&self) -> Persona {
        self.view(&self.instances[0])
    }
    pub fn instance(&self, relay: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.relay == relay)
    }
    pub fn persona(&self, relay: &str) -> Option<Persona> {
        self.instance(relay).map(|i| self.view(i))
    }
    pub fn personas(&self) -> Vec<Persona> {
        self.instances.iter().map(|i| self.view(i)).collect()
    }
    /// Takes the definition and identity from `p`, and the instance of
    /// `p.relay` (added at the end when the agent has none there yet).
    pub fn absorb(&mut self, p: Persona) {
        let instance = Instance {
            relay: p.relay,
            primary: p.primary,
            rooms: p.rooms,
            workspace: p.workspace,
            start_at_login: p.start_at_login,
            published: p.published,
            member_rooms: p.member_rooms,
            published_at: p.published_at,
            last_error: p.last_error,
        };
        self.name = p.name;
        self.description = p.description;
        self.instructions = p.instructions;
        self.harness = p.harness;
        self.model = p.model;
        self.acp_command = p.acp_command;
        self.respond_to = p.respond_to;
        self.answers_dms = p.answers_dms;
        self.identity = p.identity;
        self.auth_tag = p.auth_tag;
        match self
            .instances
            .iter_mut()
            .find(|i| i.relay == instance.relay)
        {
            Some(slot) => *slot = instance,
            None => self.instances.push(instance),
        }
    }
    /// Every instance passes the persona rules; 1–4 instances in distinct
    /// communities with distinct unit names, at most one primary; an agent
    /// without an identity has exactly one (it is added elsewhere only once
    /// enrolled).
    pub fn valid(&self) -> bool {
        let views = self.personas();
        let relays: std::collections::BTreeSet<&str> =
            self.instances.iter().map(|i| i.relay.as_str()).collect();
        let keys: std::collections::BTreeSet<String> = views.iter().map(Persona::key).collect();
        (1..=MAX_INSTANCES).contains(&self.instances.len())
            && relays.len() == self.instances.len()
            && keys.len() == self.instances.len()
            && self.instances.iter().filter(|i| i.primary).count() <= 1
            && (self.identity.is_some() || self.instances.len() == 1)
            && views.iter().all(Persona::valid)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV1 {
    version: u32,
    agents: Vec<Persona>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    agents: Vec<Agent>,
}
/// Only the version, to choose the file's shape.
#[derive(Deserialize)]
struct Version {
    version: u32,
}
pub const STORE_VERSION: u32 = 2;

pub fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
pub fn canonical_key(value: &str) -> bool {
    nostr::PublicKey::from_hex(value).is_ok_and(|key| key.to_hex() == value)
}
/// Characters that can hide or reorder visible text: bidi controls and marks,
/// zero-width characters and the byte-order mark.
fn invisible(ch: char) -> bool {
    matches!(ch,
        '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2069}' | '\u{feff}')
}
fn label(value: &str, min: usize, max: usize) -> bool {
    let count = value.chars().count();
    (min..=max).contains(&count)
        && !value.chars().any(|c| c.is_control() || invisible(c))
        && (min == 0 || !value.trim().is_empty())
}
pub fn valid_name(value: &str) -> bool {
    label(value, 1, NAME_CHARS)
}
pub fn valid_description(value: &str) -> bool {
    label(value, 0, DESCRIPTION_CHARS)
}
/// Text only: newline, carriage return and tab are the permitted controls.
pub fn valid_instructions(value: &str) -> bool {
    value.len() <= INSTRUCTIONS_BYTES
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}
pub fn valid_harness(value: &str) -> bool {
    HARNESSES.contains(&value)
}
pub fn valid_model(value: &str) -> bool {
    value.len() <= MODEL_CHARS
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
pub fn valid_rooms(rooms: &[String]) -> bool {
    let unique: std::collections::BTreeSet<&String> = rooms.iter().collect();
    (1..=MAX_ROOMS).contains(&rooms.len())
        && unique.len() == rooms.len()
        && rooms.iter().all(|r| canonical_uuid(r))
}
/// An absolute, normalized, printable path: no `.`/`..`, repeated or trailing
/// separators. It is written into a unit file, so it is also bounded.
pub fn canonical_path(value: &str) -> bool {
    let path = Path::new(value);
    value.len() <= PATH_BYTES
        && value.starts_with('/')
        && !value.chars().any(|c| c.is_control() || invisible(c))
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && path.to_str() == Some(value)
        && path.components().collect::<PathBuf>().as_os_str() == value
}

impl Persona {
    /// What names this instance's unit and default workspace: the agent id
    /// for the primary instance, else `<id>-<first 12 hex of SHA-256(relay)>`.
    pub fn key(&self) -> String {
        if self.primary {
            self.id.clone()
        } else {
            format!("{}-{}", self.id, relay_hash(&self.relay))
        }
    }
    /// Field rules independent of the filesystem.
    pub fn valid(&self) -> bool {
        uuid::Uuid::parse_str(&self.id)
            .is_ok_and(|id| id.to_string() == self.id && id.get_version_num() == 4)
            && valid_name(&self.name)
            && valid_description(&self.description)
            && valid_instructions(&self.instructions)
            && valid_harness(&self.harness)
            && valid_model(&self.model)
            && self.acp_command == ACP_COMMAND
            && valid_rooms(&self.rooms)
            && RESPOND_TO.contains(&self.respond_to.as_str())
            && canonical_path(&self.workspace)
            && crate::config::canonical_relay(&self.relay).as_deref() == Ok(self.relay.as_str())
            && self.identity.as_deref().is_none_or(canonical_key)
            && (self.identity.is_some() || (self.auth_tag.is_none() && !self.published))
            && self.auth_tag.as_deref().is_none_or(|tag| {
                self.identity.as_deref().is_some_and(|identity| {
                    nostr::PublicKey::from_hex(identity)
                        .is_ok_and(|key| buzz_sdk::nip_oa::verify_auth_tag(tag, &key).is_ok())
                })
            })
            && self.member_rooms.len() <= MAX_MEMBER_ROOMS
            && self.member_rooms.iter().all(|r| canonical_uuid(r))
            && self
                .member_rooms
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.member_rooms.len()
            && (self.identity.is_some() || self.member_rooms.is_empty())
            && self.last_error.as_deref().is_none_or(|c| {
                c.len() <= 32 && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
    }
}

fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}
/// No component of `path` (or any ancestor) is a symbolic link.
pub fn unlinked(path: &Path) -> bool {
    path.ancestors().all(|p| {
        p.as_os_str().is_empty()
            || fs::symlink_metadata(p).is_ok_and(|m| !m.file_type().is_symlink())
    })
}
/// Like `scripts/room-sandbox` `private_directory`: an existing directory with
/// no linked path component, owned by this user, with no group/other access.
pub fn private_directory(path: &Path) -> bool {
    path.is_absolute()
        && unlinked(path)
        && fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_dir() && m.uid() == uid() && m.mode() & 0o077 == 0)
}
/// Creates (if needed) a directory the store owns and tightens it to 0700.
pub fn ensure_private_directory(path: &Path) -> Result<(), &'static str> {
    if !path.is_absolute() {
        return Err("state_unavailable");
    }
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                if fs::symlink_metadata(parent).is_err() {
                    fs::create_dir_all(parent).map_err(|_| "state_unavailable")?;
                }
            }
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err("state_unavailable"),
            }
        }
        Err(_) => return Err("state_unavailable"),
    }
    let m = fs::symlink_metadata(path).map_err(|_| "state_unavailable")?;
    if !unlinked(path) || !m.is_dir() || m.uid() != uid() {
        return Err("state_insecure");
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| "state_unavailable")
}
/// Atomically replaces `path` with `bytes` at mode 0600 (temporary file in the
/// same directory, fsync, rename, directory fsync).
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    let dir = path.parent().ok_or("state_unavailable")?;
    let name = path.file_name().ok_or("state_unavailable")?;
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.uid() != uid()) {
        return Err("state_insecure");
    }
    let temp = dir.join(format!(
        ".{}.{}.new",
        name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| "state_unavailable")?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| "state_unavailable")?;
        fs::rename(&temp, path).map_err(|_| "state_unavailable")?;
        fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|_| "state_unavailable")
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub struct Store {
    path: PathBuf,
    pub agents: Vec<Agent>,
}
impl Store {
    /// Loads the store, or an empty one if the file does not exist yet. Any
    /// invalid content is refused rather than repaired; a version 1 persona
    /// without a `relay` is refused (see `open_with` for the migration).
    #[cfg(test)]
    pub fn open(paths: &Paths) -> Result<Self, &'static str> {
        Self::open_with(paths, || None)
    }
    /// `open`, wrapping a version 1 store into version 2 (one primary
    /// instance per persona: same relay, rooms, workspace and unit name).
    /// Version 1 personas written before `relay` existed first get the relay
    /// their generated unit file names (`--relay` in the `ExecStart` of
    /// `omarchy-buzz-agent-<id>.service`, read only), else
    /// `first_community()` (the configuration's first community, asked at
    /// most once). A unit file that exists but is unsafe or names no single
    /// valid relay, or no relay at all, refuses the store (`store_invalid`).
    /// The wrapped store is validated like any other; the version 1 bytes are
    /// kept in `personas.v1.json` and the version 2 store is written once,
    /// atomically, before it is used (a failed write refuses it, nothing
    /// else is changed).
    pub fn open_with(
        paths: &Paths,
        first_community: impl FnOnce() -> Option<String>,
    ) -> Result<Self, &'static str> {
        ensure_private_directory(&paths.store_dir())?;
        let path = paths.store_file();
        let m = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    agents: Vec::new(),
                })
            }
            Err(_) => return Err("store_unavailable"),
        };
        if !m.is_file() || m.uid() != uid() || m.mode() & 0o077 != 0 || m.len() > STORE_BYTES {
            return Err("store_invalid");
        }
        let bytes = fs::read(&path).map_err(|_| "store_unavailable")?;
        let version: Version = serde_json::from_slice(&bytes).map_err(|_| "store_invalid")?;
        let (agents, wrapped) = match version.version {
            1 => (wrap_v1(paths, &bytes, first_community)?, true),
            STORE_VERSION => {
                let file: File = serde_json::from_slice(&bytes).map_err(|_| "store_invalid")?;
                (file.agents, false)
            }
            _ => return Err("store_invalid"),
        };
        let store = Self { path, agents };
        if !store.consistent() {
            return Err("store_invalid");
        }
        if wrapped {
            write_private(&paths.store_dir().join("personas.v1.json"), &bytes)
                .map_err(|_| "store_unavailable")?;
            store.save().map_err(|_| "store_unavailable")?;
        }
        Ok(store)
    }
    fn consistent(&self) -> bool {
        let ids: std::collections::BTreeSet<&str> =
            self.agents.iter().map(|a| a.id.as_str()).collect();
        let identities: Vec<&str> = self
            .agents
            .iter()
            .filter_map(|a| a.identity.as_deref())
            .collect();
        let unique_identities: std::collections::BTreeSet<&&str> = identities.iter().collect();
        self.agents.len() <= MAX_AGENTS
            && ids.len() == self.agents.len()
            && unique_identities.len() == identities.len()
            && self.agents.iter().all(Agent::valid)
    }
    pub fn save(&self) -> Result<(), &'static str> {
        if !self.consistent() {
            return Err("agent_invalid");
        }
        let bytes = serde_json::to_vec_pretty(&File {
            version: STORE_VERSION,
            agents: self.agents.clone(),
        })
        .map_err(|_| "store_unavailable")?;
        write_private(&self.path, &bytes)
    }
    pub fn get(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == id)
    }
    /// Every instance of every agent, in flat form.
    pub fn personas(&self) -> Vec<Persona> {
        self.agents.iter().flat_map(Agent::personas).collect()
    }
}

/// A version 1 store's personas, each wrapped into an agent with one
/// primary instance (after the `relay` migration of `open_with`).
fn wrap_v1(
    paths: &Paths,
    bytes: &[u8],
    first_community: impl FnOnce() -> Option<String>,
) -> Result<Vec<Agent>, &'static str> {
    let file: FileV1 = serde_json::from_slice(bytes).map_err(|_| "store_invalid")?;
    let mut personas = file.agents;
    let mut fallback = Some(first_community);
    let mut first: Option<Option<String>> = None;
    for persona in personas.iter_mut().filter(|a| a.relay.is_empty()) {
        persona.relay = match unit_relay(paths, &persona.id)? {
            Some(relay) => relay,
            None => {
                if first.is_none() {
                    first = Some(fallback.take().and_then(|f| f()));
                }
                first.clone().flatten().ok_or("store_invalid")?
            }
        };
    }
    Ok(personas
        .into_iter()
        .map(|mut persona| {
            // The unit (`omarchy-buzz-agent-<id>.service`) and workspace a
            // version 1 agent already has stay its own.
            persona.primary = true;
            Agent::from_persona(persona)
        })
        .collect())
}

/// Generated unit files are small; anything larger was not written here.
const UNIT_BYTES: u64 = 64 * 1024;

/// The words of a generated `ExecStart=` line: every word double-quoted with
/// `\\`, `\"`, `%%` and `$$` escapes (`unit::quote`). Anything else is `None`.
fn unquote_words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut chars = line.chars();
    loop {
        match chars.next() {
            None => return Some(words),
            Some('"') => {}
            Some(_) => return None,
        }
        let mut word = String::new();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => match chars.next()? {
                    c @ ('\\' | '"') => word.push(c),
                    _ => return None,
                },
                '%' if chars.next()? == '%' => word.push('%'),
                '$' if chars.next()? == '$' => word.push('$'),
                '%' | '$' => return None,
                c => word.push(c),
            }
        }
        words.push(word);
        match chars.next() {
            None => return Some(words),
            Some(' ') => {}
            Some(_) => return None,
        }
    }
}

/// The relay recorded in an agent's generated unit file, read only: `None`
/// when there is no unit file; `store_invalid` when one exists but is not a
/// user-owned, unlinked regular file of at most 64 KiB with exactly one
/// `ExecStart=` line holding exactly one canonical `--relay <url>`.
pub fn unit_relay(paths: &Paths, id: &str) -> Result<Option<String>, &'static str> {
    const INVALID: &str = "store_invalid";
    let path = paths.unit_file(id);
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(INVALID),
    };
    if !m.is_file() || m.uid() != uid() || m.len() > UNIT_BYTES || !unlinked(&path) {
        return Err(INVALID);
    }
    let text = fs::read_to_string(&path).map_err(|_| INVALID)?;
    let mut exec = text.lines().filter_map(|l| l.strip_prefix("ExecStart="));
    let (Some(line), None) = (exec.next(), exec.next()) else {
        return Err(INVALID);
    };
    let words = unquote_words(line).ok_or(INVALID)?;
    let mut relays = words
        .windows(2)
        .filter(|pair| pair[0] == "--relay")
        .map(|pair| pair[1].clone());
    match (relays.next(), relays.next()) {
        (Some(relay), None)
            if crate::config::canonical_relay(&relay).as_deref() == Ok(relay.as_str()) =>
        {
            Ok(Some(relay))
        }
        _ => Err(INVALID),
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
/// Workspace rules from the contract. `own` is the instance's key
/// (`Persona::key`); `others` are the other instances, of this agent and of
/// every other one: each instance is a separate agent for these rules.
/// Returns `workspace_refused` for every refusal.
pub fn check_workspace(
    paths: &Paths,
    workspace: &str,
    own: &str,
    others: &[&Persona],
) -> Result<(), &'static str> {
    const REFUSED: &str = "workspace_refused";
    if !canonical_path(workspace) {
        return Err(REFUSED);
    }
    let path = Path::new(workspace);
    // Never $HOME itself or anything containing it.
    if paths.home.starts_with(path) {
        return Err(REFUSED);
    }
    let mut forbidden = vec![
        paths.home.join(".config"),
        paths.config.clone(),
        paths.home.join(".ssh"),
        paths.home.join(".gnupg"),
        // Harness bundles are mounted read-only; room-sandbox refuses overlap.
        paths.data.join("omarchy-buzz"),
    ];
    for harness in HARNESSES {
        forbidden.push(paths.bundle(harness));
        forbidden.push(paths.profile(harness));
    }
    if forbidden.iter().any(|root| overlaps(path, root)) {
        return Err(REFUSED);
    }
    // Nothing under ~/.local/state/omarchy-buzz*, except this instance's own
    // default workspace. Containing a state directory is refused as well.
    let default = paths.default_workspace(own);
    for state in [paths.home.join(".local/state"), paths.state.clone()] {
        if state.starts_with(path) {
            return Err(REFUSED);
        }
        if let Ok(rest) = path.strip_prefix(&state) {
            let first = rest
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or("");
            if first.starts_with("omarchy-buzz") && path != default {
                return Err(REFUSED);
            }
        }
    }
    if others
        .iter()
        .any(|other| other.key() != own && overlaps(path, Path::new(&other.workspace)))
    {
        return Err(REFUSED);
    }
    if !private_directory(path) {
        return Err(REFUSED);
    }
    Ok(())
}
/// Creates the default workspace of an instance (`key`) at 0700 if absent. Its parent is created at
/// 0700 when missing; an existing parent is never changed, but must be an
/// unlinked directory of this user that others cannot write.
pub fn create_default_workspace(paths: &Paths, key: &str) -> Result<PathBuf, &'static str> {
    let parent = paths.workspaces();
    if fs::symlink_metadata(&parent).is_err() {
        ensure_private_directory(&parent).map_err(|_| "workspace_refused")?;
    }
    let m = fs::symlink_metadata(&parent).map_err(|_| "workspace_refused")?;
    if !unlinked(&parent) || !m.is_dir() || m.uid() != uid() || m.mode() & 0o022 != 0 {
        return Err("workspace_refused");
    }
    let path = paths.default_workspace(key);
    match fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("workspace_refused"),
    }
    Ok(path)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
