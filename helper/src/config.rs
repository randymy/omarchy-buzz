use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub relay: Option<String>,
    pub identity: Option<String>,
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
/// `load` from an explicit configuration directory.
pub fn load_from(dir: &std::path::Path) -> Result<Config, &'static str> {
    let path = dir.join("config.toml");
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(_) => return Err("config_unavailable"),
    };
    if !m.is_file() || m.uid() != rustix::process::getuid().as_raw() || m.len() > 8192 {
        return Err("invalid_config");
    }
    let text = fs::read_to_string(path).map_err(|_| "config_unavailable")?;
    let mut c: Config = toml::from_str(&text).map_err(|_| "invalid_config")?;
    if let Some(r) = &c.relay {
        c.relay = Some(canonical_relay(r)?);
    }
    if let Some(k) = &c.identity {
        nostr::PublicKey::from_hex(k).map_err(|_| "invalid_config")?;
    }
    Ok(c)
}
pub fn save(c: &Config) -> Result<(), &'static str> {
    save_to(&dir()?, c)
}
/// `save` into an explicit configuration directory.
pub fn save_to(d: &std::path::Path, c: &Config) -> Result<(), &'static str> {
    let d = d.to_path_buf();
    fs::create_dir_all(&d).map_err(|_| "config_unavailable")?;
    let m = fs::symlink_metadata(&d).map_err(|_| "config_unavailable")?;
    if !m.is_dir() || m.uid() != rustix::process::getuid().as_raw() {
        return Err("invalid_config");
    }
    fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).map_err(|_| "config_unavailable")?;
    let text = toml::to_string(c).map_err(|_| "invalid_config")?;
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
/// The configuration after choosing `relay` (already canonical): a different
/// relay clears the identity, because an identity is scoped to its relay.
pub fn with_relay(mut c: Config, relay: String) -> Config {
    if c.relay.as_deref() != Some(relay.as_str()) {
        c.identity = None;
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
        let dir =
            std::env::temp_dir().join(format!("omarchy-buzz-config-{}", uuid::Uuid::new_v4()));
        let identity = nostr::Keys::generate().public_key().to_hex();
        let c = Config {
            relay: Some("wss://a.example/".into()),
            identity: Some(identity.clone()),
        };
        let same = with_relay(c.clone(), "wss://a.example/".into());
        assert_eq!(same.identity.as_deref(), Some(identity.as_str()));
        let other = with_relay(c.clone(), "wss://b.example/".into());
        assert!(other.identity.is_none());
        assert_eq!(other.relay.as_deref(), Some("wss://b.example/"));
        assert!(load_from(&dir).unwrap().relay.is_none());
        save_to(&dir, &c).unwrap();
        let loaded = load_from(&dir).unwrap();
        assert_eq!((loaded.relay, loaded.identity), (c.relay, c.identity));
        let mode = fs::metadata(dir.join("config.toml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }
}
