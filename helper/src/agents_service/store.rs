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
    pub fn default_workspace(&self, id: &str) -> PathBuf {
        self.workspaces().join(id)
    }
    pub fn units_dir(&self) -> PathBuf {
        self.config.join("systemd/user")
    }
    pub fn unit_file(&self, id: &str) -> PathBuf {
        self.units_dir().join(unit_name(id))
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

pub fn unit_name(id: &str) -> String {
    format!("omarchy-buzz-agent-{id}.service")
}

/// One persona. Fields after `start_at_login` are service-private: they never
/// appear in IPC status frames and never in a published managed-agent record.
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
    /// The owner's NIP-OA `auth` tag JSON for `identity` (public, not secret).
    pub auth_tag: Option<String>,
    /// True only after the relay's `OK` for every enrollment publication.
    pub published: bool,
    /// Rooms whose add-member command the relay acknowledged.
    pub member_rooms: Vec<String>,
    /// `created_at` of the last kind 30175/30177 publication (monotonic).
    pub published_at: u64,
    pub last_error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    agents: Vec<Persona>,
}

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
            && self.identity.as_deref().is_none_or(canonical_key)
            && (self.identity.is_some() || (self.auth_tag.is_none() && !self.published))
            && self.auth_tag.as_deref().is_none_or(|tag| {
                self.identity.as_deref().is_some_and(|identity| {
                    nostr::PublicKey::from_hex(identity)
                        .is_ok_and(|key| buzz_sdk::nip_oa::verify_auth_tag(tag, &key).is_ok())
                })
            })
            && self.member_rooms.iter().all(|r| self.rooms.contains(r))
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
    pub agents: Vec<Persona>,
}
impl Store {
    /// Loads the store, or an empty one if the file does not exist yet. Any
    /// invalid content is refused rather than repaired.
    pub fn open(paths: &Paths) -> Result<Self, &'static str> {
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
        let file: File = serde_json::from_slice(&bytes).map_err(|_| "store_invalid")?;
        let store = Self {
            path,
            agents: file.agents,
        };
        if file.version != 1 || !store.consistent() {
            return Err("store_invalid");
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
            && self.agents.iter().all(Persona::valid)
    }
    pub fn save(&self) -> Result<(), &'static str> {
        if !self.consistent() {
            return Err("agent_invalid");
        }
        let bytes = serde_json::to_vec_pretty(&File {
            version: 1,
            agents: self.agents.clone(),
        })
        .map_err(|_| "store_unavailable")?;
        write_private(&self.path, &bytes)
    }
    pub fn get(&self, id: &str) -> Option<&Persona> {
        self.agents.iter().find(|a| a.id == id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Persona> {
        self.agents.iter_mut().find(|a| a.id == id)
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
/// Workspace rules from the contract. `own` is the persona's id; `others` are
/// the other personas. Returns `workspace_refused` for every refusal.
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
    // Nothing under ~/.local/state/omarchy-buzz*, except this persona's own
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
        .any(|other| other.id != own && overlaps(path, Path::new(&other.workspace)))
    {
        return Err(REFUSED);
    }
    if !private_directory(path) {
        return Err(REFUSED);
    }
    Ok(())
}
/// Creates the default workspace at 0700 if absent. Its parent is created at
/// 0700 when missing; an existing parent is never changed, but must be an
/// unlinked directory of this user that others cannot write.
pub fn create_default_workspace(paths: &Paths, id: &str) -> Result<PathBuf, &'static str> {
    let parent = paths.workspaces();
    if fs::symlink_metadata(&parent).is_err() {
        ensure_private_directory(&parent).map_err(|_| "workspace_refused")?;
    }
    let m = fs::symlink_metadata(&parent).map_err(|_| "workspace_refused")?;
    if !unlinked(&parent) || !m.is_dir() || m.uid() != uid() || m.mode() & 0o022 != 0 {
        return Err("workspace_refused");
    }
    let path = paths.default_workspace(id);
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
