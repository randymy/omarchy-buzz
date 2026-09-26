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
    let path = dir()?.join("config.toml");
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
    let d = dir()?;
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
    }
}
