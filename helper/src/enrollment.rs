//! Hidden-terminal enrollment. The configured relay's NIP-11 `self` key must
//! be known before a candidate key may enter this helper's Secret Service.
use std::{future::Future, io::IsTerminal};
use zeroize::Zeroizing;

pub(crate) async fn check_then_store<D, S, F>(
    candidate: nostr::PublicKey,
    discovery: D,
    store: S,
) -> Result<(), &'static str>
where
    D: Future<Output = Result<nostr::PublicKey, &'static str>>,
    S: FnOnce() -> F,
    F: Future<Output = Result<(), &'static str>>,
{
    let signer = discovery.await?;
    if candidate == signer {
        return Err("identity_is_relay_signer");
    }
    store().await
}

/// Where a human identity secret is kept: the helper's own Secret Service
/// namespace (`auth::SERVICE`, account `relay|identity`). Tests use a fake.
pub(crate) trait IdentitySecrets: Send + Sync {
    fn store(&self, account: &str, secret: &Zeroizing<String>) -> Result<(), &'static str>;
    fn forget(&self, account: &str) -> Result<(), &'static str>;
}
pub(crate) struct SecretService;
impl IdentitySecrets for SecretService {
    fn store(&self, account: &str, secret: &Zeroizing<String>) -> Result<(), &'static str> {
        keyring::Entry::new(crate::auth::SERVICE, account)
            .map_err(|_| "identity_unavailable")?
            .set_password(secret.as_str())
            .map_err(|_| "identity_unavailable")
    }
    fn forget(&self, account: &str) -> Result<(), &'static str> {
        match keyring::Entry::new(crate::auth::SERVICE, account)
            .map_err(|_| "identity_unavailable")?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("identity_unavailable"),
        }
    }
}

fn store(
    original: crate::config::Config,
    secret: Zeroizing<String>,
    identity: String,
) -> Result<(), &'static str> {
    store_in(
        &crate::config::dir()?,
        &SecretService,
        original,
        secret,
        identity,
        false,
    )
}

/// Writes the secret, then the public identity to the configuration in `dir`.
/// `fresh` marks a key generated here: if the configuration cannot be saved,
/// its secret is removed again so no unreferenced key is left behind.
pub(crate) fn store_in(
    dir: &std::path::Path,
    secrets: &dyn IdentitySecrets,
    mut original: crate::config::Config,
    secret: Zeroizing<String>,
    identity: String,
    fresh: bool,
) -> Result<(), &'static str> {
    // An enrollment prompt may stay open while another process reconfigures
    // the helper. Reject scope changes observed before writing the key.
    let current = crate::config::load_from(dir)?;
    if current.relay != original.relay || current.identity != original.identity {
        return Err("identity_scope_changed");
    }
    original.identity = Some(identity);
    let account = crate::config::account(&original)?;
    secrets.store(&account, &secret)?;
    let saved = crate::config::save_to(dir, &original);
    if saved.is_err() && fresh {
        let _ = secrets.forget(&account);
    }
    saved
}

pub async fn enroll() -> Result<(), &'static str> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err("terminal_required");
    }
    let config = crate::config::load()?;
    let relay = config.relay.clone().ok_or("unconfigured")?;
    eprintln!("Enroll an existing Buzz identity in this helper's own Secret Service namespace.");
    eprintln!("Use your personal identity, never the relay/server key. The relay must be online for verification.");
    let secret = tokio::task::spawn_blocking(|| {
        rpassword::prompt_password("Private key (hidden): ")
            .map(Zeroizing::new)
            .map_err(|_| "terminal_unavailable")
    })
    .await
    .map_err(|_| "terminal_unavailable")??;
    let candidate = nostr::Keys::parse(secret.as_str())
        .map_err(|_| "identity_invalid")?
        .public_key();
    let identity = candidate.to_hex();
    let result = check_then_store(
        candidate,
        crate::catalog::relay_signer(&relay, None),
        || async move {
            tokio::task::spawn_blocking(move || store(config, secret, identity))
                .await
                .map_err(|_| "identity_unavailable")?
        },
    )
    .await;
    if let Err(category) = result {
        match category {
            "identity_is_relay_signer" => {
                eprintln!("That key signs for the relay. Enter a separate personal Buzz identity.")
            }
            "discovery_unavailable"
            | "discovery_invalid_info"
            | "discovery_signer_unavailable"
            | "discovery_oversized"
            | "discovery_redirect_rejected"
            | "invalid_query_origin" => eprintln!(
                "Could not verify the relay signing key. Check the relay connection and try again."
            ),
            _ => {}
        }
        return Err(category);
    }
    println!(
        "{}",
        serde_json::json!({"identity":candidate.to_hex(),"relay":relay,"status":"enrolled"})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn relay_key_and_failed_discovery_never_reach_storage() {
        let relay = nostr::Keys::generate().public_key();
        let human = nostr::Keys::generate().public_key();
        let writes = Cell::new(0);
        let store = || async {
            writes.set(writes.get() + 1);
            Ok(())
        };
        assert_eq!(
            check_then_store(relay, async { Ok(relay) }, store)
                .await
                .unwrap_err(),
            "identity_is_relay_signer"
        );
        assert_eq!(writes.get(), 0);
        assert_eq!(
            check_then_store(human, async { Err("discovery_unavailable") }, store)
                .await
                .unwrap_err(),
            "discovery_unavailable"
        );
        assert_eq!(writes.get(), 0);
        check_then_store(human, async { Ok(relay) }, store)
            .await
            .unwrap();
        assert_eq!(writes.get(), 1);
    }
}
