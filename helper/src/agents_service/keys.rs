//! Secret Service access for owner and agent keys, behind a trait so tests
//! and `OMARCHY_BUZZ_AGENTS_FAKE_CONTROL=1` never reach a real keyring.
use crate::config::Config;
use nostr::Keys;
use std::{collections::BTreeMap, sync::Mutex};
use zeroize::Zeroizing;

/// Service name the bundle launcher (`scripts/room-agent`) looks up with
/// `secret-tool lookup service omarchy-buzz.room-agent.v1 account <identity>`.
pub const AGENT_SERVICE: &str = "omarchy-buzz.room-agent.v1";

pub trait Keyring: Send + Sync {
    /// The owner's signing keys (the helper's enrolled identity).
    fn owner_keys(&self, config: &Config) -> Result<Keys, &'static str>;
    /// Stores an agent secret (64 lowercase hex characters) for `identity`.
    fn store_agent(&self, identity: &str, secret: &Zeroizing<String>) -> Result<(), &'static str>;
    fn agent_keys(&self, identity: &str) -> Result<Keys, &'static str>;
    fn forget_agent(&self, identity: &str) -> Result<(), &'static str>;
}

fn checked(identity: &str, secret: &str) -> Result<Keys, &'static str> {
    let keys = Keys::parse(secret).map_err(|_| "identity_invalid")?;
    if keys.public_key().to_hex() != identity {
        return Err("identity_invalid");
    }
    Ok(keys)
}

/// The helper's Secret Service path (`keyring` crate, sync Secret Service).
/// Agent items carry an additional `account` attribute so the launcher's
/// `secret-tool` lookup (service + account) finds them.
pub struct SecretService;
impl SecretService {
    fn entry(identity: &str) -> Result<keyring::Entry, &'static str> {
        let mut credential =
            keyring::secret_service::SsCredential::new_with_target(None, AGENT_SERVICE, identity)
                .map_err(|_| "identity_unavailable")?;
        credential
            .attributes
            .insert("account".into(), identity.into());
        Ok(keyring::Entry::new_with_credential(Box::new(credential)))
    }
}
impl Keyring for SecretService {
    fn owner_keys(&self, config: &Config) -> Result<Keys, &'static str> {
        crate::auth::read_keys(config)
    }
    fn store_agent(&self, identity: &str, secret: &Zeroizing<String>) -> Result<(), &'static str> {
        checked(identity, secret)?;
        Self::entry(identity)?
            .set_password(secret.as_str())
            .map_err(|_| "identity_unavailable")
    }
    fn agent_keys(&self, identity: &str) -> Result<Keys, &'static str> {
        let secret =
            Zeroizing::new(Self::entry(identity)?.get_password().map_err(|e| match e {
                keyring::Error::NoEntry => "identity_missing",
                _ => "identity_unavailable",
            })?);
        checked(identity, &secret)
    }
    fn forget_agent(&self, identity: &str) -> Result<(), &'static str> {
        match Self::entry(identity)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("identity_unavailable"),
        }
    }
}

/// In-memory keyring. `owner` is the synthetic owner key a test configures.
#[derive(Default)]
pub struct FakeKeyring {
    pub owner: Mutex<Option<Keys>>,
    pub agents: Mutex<BTreeMap<String, Zeroizing<String>>>,
    pub fail_store: Mutex<bool>,
    /// The relay of every owner-key lookup (the Secret Service account is
    /// `relay|identity`, one per community).
    pub owner_relays: Mutex<Vec<String>>,
}
impl Keyring for FakeKeyring {
    fn owner_keys(&self, config: &Config) -> Result<Keys, &'static str> {
        self.owner_relays
            .lock()
            .unwrap()
            .push(config.relay.clone().unwrap_or_default());
        let keys = self
            .owner
            .lock()
            .unwrap()
            .clone()
            .ok_or("identity_missing")?;
        if config.identity.as_deref() != Some(keys.public_key().to_hex().as_str()) {
            return Err("identity_invalid");
        }
        Ok(keys)
    }
    fn store_agent(&self, identity: &str, secret: &Zeroizing<String>) -> Result<(), &'static str> {
        checked(identity, secret)?;
        if *self.fail_store.lock().unwrap() {
            return Err("identity_unavailable");
        }
        self.agents
            .lock()
            .unwrap()
            .insert(identity.into(), secret.clone());
        Ok(())
    }
    fn agent_keys(&self, identity: &str) -> Result<Keys, &'static str> {
        let agents = self.agents.lock().unwrap();
        checked(identity, agents.get(identity).ok_or("identity_missing")?)
    }
    fn forget_agent(&self, identity: &str) -> Result<(), &'static str> {
        self.agents.lock().unwrap().remove(identity);
        Ok(())
    }
}
