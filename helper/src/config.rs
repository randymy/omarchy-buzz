//! The helper's public configuration: one human identity (public key only) and
//! the communities it belongs to, one of them active.
//!
//! Format 2 (`config.toml`):
//!
//! ```toml
//! version = 2
//! identity = "<hex public key>"
//! activeRelay = "wss://community.example/"
//!
//! [[communities]]
//! relay = "wss://community.example/"
//! name = "community"
//! joinedAt = 1759300000
//! ```
//!
//! A format-1 file (`relay = …`, `identity = …`) is migrated in place on first
//! load: the original bytes are copied to `config.v1.toml` (0600) first, then
//! the new file replaces it atomically. The identity is one for every
//! community (Desktop keeps one Nostr identity app-wide); the secret stays in
//! Secret Service under `relay|identity` for each community.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

/// At most this many communities are kept.
pub const COMMUNITIES: usize = 16;
/// A community's local label, in bytes after sanitizing.
pub const NAME_BYTES: usize = 64;
const FILE_BYTES: u64 = 64 * 1024;
const FORMAT: u32 = 2;
/// The latest plausible time (9999-12-31).
const LATEST: u64 = 253_402_300_799;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Community {
    /// Canonical (`canonical_relay`).
    pub relay: String,
    /// The local label: user-entered or derived from the host, sanitized.
    pub name: String,
    /// Unix seconds when this device added it.
    pub joined_at: u64,
}

/// The loaded configuration. `relay` is the active community's relay; every
/// configured community is in `communities` (`normalized` adds the active one
/// when a caller built a `Config` without it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub relay: Option<String>,
    pub identity: Option<String>,
    pub communities: Vec<Community>,
}
impl Config {
    /// The active relay is always listed, once; duplicates keep the first.
    pub fn normalized(mut self) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        self.communities.retain(|c| seen.insert(c.relay.clone()));
        if let Some(relay) = &self.relay {
            if !self.communities.iter().any(|c| &c.relay == relay) {
                self.communities.push(Community {
                    relay: relay.clone(),
                    name: derive_name(relay),
                    joined_at: now(),
                });
            }
        }
        self
    }
    pub fn community(&self, relay: &str) -> Option<&Community> {
        self.communities.iter().find(|c| c.relay == relay)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FileV2 {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_relay: Option<String>,
    #[serde(default)]
    communities: Vec<Community>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV1 {
    relay: Option<String>,
    identity: Option<String>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn canonical_relay(input: &str) -> Result<String, &'static str> {
    if input.len() > 2048 {
        return Err("invalid_relay");
    }
    let mut u = url::Url::parse(input).map_err(|_| "invalid_relay")?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
    {
        return Err("invalid_relay");
    }
    let local = matches!(
        u.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    match u.scheme() {
        "wss" => {}
        "ws" if local => {}
        _ => return Err("insecure_relay"),
    }
    if u.host_str().is_none() {
        return Err("invalid_relay");
    }
    u.set_path("/");
    Ok(u.to_string())
}

/// `host[:port]` of a canonical relay, for display.
pub fn host(relay: &str) -> String {
    url::Url::parse(relay)
        .ok()
        .and_then(|u| {
            let host = u.host_str()?.to_owned();
            Some(match u.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_default()
}

/// Desktop's `deriveCommunityName` (`communityStorage.ts:188-210`): "Local Dev"
/// for this computer, "Buzz (staging)" for a staging host, else the first host
/// label (the second when the first is `relay`).
pub fn derive_name(relay: &str) -> String {
    let Some(host) = url::Url::parse(relay)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
    else {
        return "Community".into();
    };
    if matches!(
        host.as_str(),
        "localhost" | "127.0.0.1" | "[::1]" | "::1" | "0.0.0.0"
    ) {
        return "Local Dev".into();
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.iter().any(|p| *p == "stage" || *p == "staging") {
        return "Buzz (staging)".into();
    }
    let chosen = if parts.len() >= 2 {
        if parts[0] == "relay" {
            parts[1]
        } else {
            parts[0]
        }
    } else {
        host.as_str()
    };
    let name = label(chosen);
    if name.is_empty() {
        "Community".into()
    } else {
        name
    }
}

/// A community label: controls and bidi formatting become spaces, at most
/// `NAME_BYTES` bytes, trimmed (like a profile name, `recipients::sanitize`).
pub fn label(value: &str) -> String {
    crate::recipients::sanitize(value, NAME_BYTES)
}

pub fn dir() -> Result<PathBuf, &'static str> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .ok_or("config_unavailable")?;
    if !base.is_absolute() {
        return Err("config_unavailable");
    }
    Ok(base.join("omarchy-buzz"))
}
pub fn load() -> Result<Config, &'static str> {
    load_from(&dir()?)
}

fn identity_ok(identity: &Option<String>) -> Result<(), &'static str> {
    if let Some(k) = identity {
        nostr::PublicKey::from_hex(k).map_err(|_| "invalid_config")?;
    }
    Ok(())
}

/// The format-2 file, strictly: canonical relays (non-canonical input fails
/// closed rather than being rewritten), unique, bounded, the active relay
/// listed, plausible times and non-empty labels.
fn parse_v2(file: FileV2) -> Result<Config, &'static str> {
    if file.version != FORMAT || file.communities.len() > COMMUNITIES {
        return Err("invalid_config");
    }
    identity_ok(&file.identity)?;
    let mut seen = std::collections::BTreeSet::new();
    for c in &file.communities {
        if canonical_relay(&c.relay).as_deref() != Ok(c.relay.as_str())
            || !seen.insert(c.relay.as_str())
            || c.joined_at > LATEST
            || c.name.is_empty()
            || label(&c.name) != c.name
        {
            return Err("invalid_config");
        }
    }
    match &file.active_relay {
        Some(active) if seen.contains(active.as_str()) => {}
        None if file.communities.is_empty() => {}
        _ => return Err("invalid_config"),
    }
    Ok(Config {
        relay: file.active_relay,
        identity: file.identity,
        communities: file.communities,
    })
}

/// `load` from an explicit configuration directory. A format-1 file is
/// migrated in place (see the module documentation); if the migration cannot
/// be written, the migrated configuration is still returned and the old file
/// is left untouched, so the next load tries again.
pub fn load_from(dir: &std::path::Path) -> Result<Config, &'static str> {
    let path = dir.join("config.toml");
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(_) => return Err("config_unavailable"),
    };
    if !m.is_file() || m.uid() != rustix::process::getuid().as_raw() || m.len() > FILE_BYTES {
        return Err("invalid_config");
    }
    let text = fs::read_to_string(&path).map_err(|_| "config_unavailable")?;
    let table: toml::Table = toml::from_str(&text).map_err(|_| "invalid_config")?;
    if table.contains_key("version") {
        let file: FileV2 = toml::from_str(&text).map_err(|_| "invalid_config")?;
        return parse_v2(file);
    }
    let old: FileV1 = toml::from_str(&text).map_err(|_| "invalid_config")?;
    let relay = old.relay.as_deref().map(canonical_relay).transpose()?;
    identity_ok(&old.identity)?;
    let migrated = Config {
        relay,
        identity: old.identity,
        communities: Vec::new(),
    }
    .normalized();
    if backup_v1(dir, &text).is_ok() {
        let _ = save_to(dir, &migrated);
    }
    Ok(migrated)
}

/// Keeps the format-1 bytes as `config.v1.toml` (or a timestamped name when a
/// different backup already exists). Written atomically, owner-only.
fn backup_v1(dir: &std::path::Path, text: &str) -> Result<PathBuf, &'static str> {
    let mut target = dir.join("config.v1.toml");
    if let Ok(existing) = fs::read_to_string(&target) {
        if existing == text {
            return Ok(target);
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        target = dir.join(format!("config.v1.{nanos}.toml"));
    }
    let temp = dir.join(format!("config.v1.{}.new", std::process::id()));
    let _ = fs::remove_file(&temp);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "config_unavailable")?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| "config_unavailable")?;
    fs::rename(&temp, &target).map_err(|_| "config_unavailable")?;
    Ok(target)
}

pub fn save(c: &Config) -> Result<(), &'static str> {
    save_to(&dir()?, c)
}
/// `save` into an explicit configuration directory (format 2, atomically).
pub fn save_to(d: &std::path::Path, c: &Config) -> Result<(), &'static str> {
    let c = c.clone().normalized();
    if c.communities.len() > COMMUNITIES {
        return Err("invalid_config");
    }
    let d = d.to_path_buf();
    fs::create_dir_all(&d).map_err(|_| "config_unavailable")?;
    let m = fs::symlink_metadata(&d).map_err(|_| "config_unavailable")?;
    if !m.is_dir() || m.uid() != rustix::process::getuid().as_raw() {
        return Err("invalid_config");
    }
    fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).map_err(|_| "config_unavailable")?;
    let file = FileV2 {
        version: FORMAT,
        identity: c.identity,
        active_relay: c.relay,
        communities: c.communities,
    };
    let text = toml::to_string(&file).map_err(|_| "invalid_config")?;
    let temp = d.join(format!("config.{}.new", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "config_unavailable")?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| "config_unavailable")?;
    fs::rename(&temp, d.join("config.toml")).map_err(|_| "config_unavailable")?;
    Ok(())
}
/// The configuration after choosing `relay` (already canonical) during first
/// setup: a different relay clears the identity and the community list, as
/// `omarchy-buzz setup relay` always did. Adding a community with the same
/// identity goes through `communities::join` instead.
pub fn with_relay(mut c: Config, relay: String) -> Config {
    if c.relay.as_deref() != Some(relay.as_str()) {
        c.identity = None;
        c.communities.clear();
    }
    c.relay = Some(relay);
    c
}
pub fn account(c: &Config) -> Result<String, &'static str> {
    Ok(format!(
        "{}|{}",
        c.relay.as_deref().ok_or("unconfigured")?,
        c.identity.as_deref().ok_or("unconfigured")?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!("omarchy-buzz-config-{}", uuid::Uuid::new_v4()))
    }
    fn write(dir: &std::path::Path, text: &str) {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn scope() {
        assert_eq!(
            canonical_relay("wss://Example.com").unwrap(),
            "wss://example.com/"
        );
        for s in [
            "wss://u:p@example.com",
            "wss://example.com/a",
            "wss://example.com/?x=1",
            "ws://example.com",
            "https://example.com",
        ] {
            assert!(canonical_relay(s).is_err(), "{s}");
        }
        assert!(canonical_relay("ws://localhost:3000").is_ok());
        assert!(canonical_relay(&format!("wss://{}.example", "a".repeat(2048))).is_err());
        // Hosted communities use the same transport as independently operated
        // relays; the helper must not require self-hosting or an operator mode.
        assert_eq!(
            canonical_relay("wss://example-team.communities.buzz.xyz").unwrap(),
            "wss://example-team.communities.buzz.xyz/"
        );
    }
    #[test]
    fn relay_change_clears_identity_and_round_trips() {
        let dir = temp();
        let identity = nostr::Keys::generate().public_key().to_hex();
        let c = Config {
            relay: Some("wss://a.example/".into()),
            identity: Some(identity.clone()),
            communities: Vec::new(),
        };
        let same = with_relay(c.clone(), "wss://a.example/".into());
        assert_eq!(same.identity.as_deref(), Some(identity.as_str()));
        let other = with_relay(c.clone().normalized(), "wss://b.example/".into());
        assert!(other.identity.is_none());
        assert!(other.communities.is_empty());
        assert_eq!(other.relay.as_deref(), Some("wss://b.example/"));
        assert!(load_from(&dir).unwrap().relay.is_none());
        save_to(&dir, &c).unwrap();
        let loaded = load_from(&dir).unwrap();
        assert_eq!(
            (loaded.relay.clone(), loaded.identity.clone()),
            (c.relay.clone(), c.identity.clone())
        );
        assert_eq!(loaded.communities.len(), 1);
        assert_eq!(loaded.communities[0].relay, "wss://a.example/");
        assert_eq!(loaded.communities[0].name, "a");
        let mode = fs::metadata(dir.join("config.toml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn format_one_is_migrated_in_place_with_a_backup_once() {
        let dir = temp();
        let identity = nostr::Keys::generate().public_key().to_hex();
        let old = format!("relay = \"wss://Relay.Team.example\"\nidentity = \"{identity}\"\n");
        write(&dir, &old);
        let c = load_from(&dir).unwrap();
        assert_eq!(c.relay.as_deref(), Some("wss://relay.team.example/"));
        assert_eq!(c.identity.as_deref(), Some(identity.as_str()));
        assert_eq!(c.communities.len(), 1);
        assert_eq!(c.communities[0].name, "team");
        assert!(c.communities[0].joined_at > 1_700_000_000);
        // The original bytes are kept, owner-only; the file is now format 2.
        let backup = dir.join("config.v1.toml");
        assert_eq!(fs::read_to_string(&backup).unwrap(), old);
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let text = fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(text.starts_with("version = 2\n"), "{text}");
        assert!(
            text.contains(&identity)
                && text.contains("activeRelay = \"wss://relay.team.example/\"")
        );
        // Idempotent: a second load reads format 2 and writes nothing new.
        let again = load_from(&dir).unwrap();
        assert_eq!(again, c);
        assert_eq!(fs::read_to_string(dir.join("config.toml")).unwrap(), text);
        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries.len(), 2, "{entries:?}");
        // A relay-only (pre-identity) file migrates too.
        let other = temp();
        write(&other, "relay = \"ws://127.0.0.1:1/\"\n");
        let c = load_from(&other).unwrap();
        assert_eq!(c.identity, None);
        assert_eq!(c.communities[0].name, "Local Dev");
        assert_eq!(
            fs::read_to_string(other.join("config.toml")).unwrap(),
            format!(
                "version = 2\nactiveRelay = \"ws://127.0.0.1:1/\"\n\n[[communities]]\nrelay = \"ws://127.0.0.1:1/\"\nname = \"Local Dev\"\njoinedAt = {}\n",
                c.communities[0].joined_at
            )
        );
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn a_different_existing_backup_is_never_overwritten() {
        let dir = temp();
        write(&dir, "relay = \"wss://a.example\"\n");
        fs::write(dir.join("config.v1.toml"), "older\n").unwrap();
        load_from(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("config.v1.toml")).unwrap(),
            "older\n"
        );
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("config.v1.") && n != "config.v1.toml")
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(
            fs::read_to_string(dir.join(&backups[0])).unwrap(),
            "relay = \"wss://a.example\"\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unwritable_migration_keeps_the_old_file_and_still_loads() {
        let dir = temp();
        let identity = nostr::Keys::generate().public_key().to_hex();
        let old = format!("relay = \"wss://a.example\"\nidentity = \"{identity}\"\n");
        write(&dir, &old);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        let c = load_from(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(c.identity.as_deref(), Some(identity.as_str()));
        assert_eq!(fs::read_to_string(dir.join("config.toml")).unwrap(), old);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_files_fail_closed() {
        let identity = nostr::Keys::generate().public_key().to_hex();
        let community = |relay: &str| {
            format!("\n[[communities]]\nrelay = \"{relay}\"\nname = \"a\"\njoinedAt = 1\n")
        };
        for text in [
            "relay = [".to_owned(),
            "unknown = 1\n".to_owned(),
            "relay = \"https://a.example\"\n".to_owned(),
            "identity = \"zz\"\n".to_owned(),
            "version = 3\n".to_owned(),
            "version = 2\nrelay = \"wss://a.example/\"\n".to_owned(),
            format!("version = 2\nidentity = \"{identity}\"\nactiveRelay = \"wss://b.example/\"\n{}", community("wss://a.example/")),
            format!("version = 2\n{}", community("wss://a.example/")),
            format!("version = 2\nactiveRelay = \"wss://A.example\"\n{}", community("wss://A.example")),
            format!("version = 2\nactiveRelay = \"wss://a.example/\"\n{}{}", community("wss://a.example/"), community("wss://a.example/")),
            "version = 2\nactiveRelay = \"wss://a.example/\"\n\n[[communities]]\nrelay = \"wss://a.example/\"\nname = \"\"\njoinedAt = 1\n".to_owned(),
            "version = 2\nactiveRelay = \"wss://a.example/\"\n\n[[communities]]\nrelay = \"wss://a.example/\"\nname = \"a\u{202e}b\"\njoinedAt = 1\n".to_owned(),
            "version = 2\nactiveRelay = \"wss://a.example/\"\n\n[[communities]]\nrelay = \"wss://a.example/\"\nname = \"a\"\njoinedAt = 1\nicon = \"x\"\n".to_owned(),
            format!("version = 2\nactiveRelay = \"wss://c0.example/\"\n{}", (0..17).map(|i| community(&format!("wss://c{i}.example/"))).collect::<String>()),
        ] {
            let dir = temp();
            write(&dir, &text);
            // `invalid_config`, or the relay's own refusal for a format-1 file
            // (as before); the connection actor reports either as invalid.
            let error = load_from(&dir).unwrap_err();
            assert!(
                matches!(error, "invalid_config" | "insecure_relay" | "invalid_relay"),
                "{text}: {error}"
            );
            // Nothing was migrated or backed up.
            assert_eq!(fs::read_to_string(dir.join("config.toml")).unwrap(), text);
            assert!(!dir.join("config.v1.toml").exists());
            fs::remove_dir_all(dir).unwrap();
        }
        let dir = temp();
        write(&dir, &"#".repeat(70 * 1024));
        assert_eq!(load_from(&dir).unwrap_err(), "invalid_config");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_are_derived_like_desktop_and_bounded() {
        assert_eq!(derive_name("wss://team.communities.buzz.xyz/"), "team");
        assert_eq!(derive_name("wss://relay.acme.example/"), "acme");
        assert_eq!(
            derive_name("wss://buzz.stage.example.co/"),
            "Buzz (staging)"
        );
        assert_eq!(derive_name("ws://localhost:3000/"), "Local Dev");
        assert_eq!(derive_name("wss://single/"), "single");
        assert_eq!(label(" a\u{202e}b\u{0007}c "), "a b c");
        assert!(label(&"é".repeat(100)).len() <= NAME_BYTES);
        assert_eq!(host("wss://a.example:4433/"), "a.example:4433");
        assert_eq!(host("wss://a.example/"), "a.example");
    }
}
