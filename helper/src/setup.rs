//! Panel-assisted setup (`setup_assist`): choose the relay and create a new
//! identity without a terminal. Only reachable while the helper is not
//! authenticated (checked by `ipc` and again by the connection actor). The
//! generated secret goes straight from the `nostr` crate to Secret Service; it
//! never enters a frame, a log line or an error category.
use crate::{config, enrollment};
use std::{path::PathBuf, sync::Arc};
use zeroize::Zeroizing;

/// Fixed categories returned to the panel.
pub const CATEGORIES: [&str; 7] = [
    "setup_invalid_relay",
    "identity_exists",
    "identity_unavailable",
    "relay_unavailable",
    "setup_busy",
    "setup_not_allowed",
    "config_unavailable",
];

/// Where setup reads and writes: the configuration directory and the
/// identity secret store. `system()` is the real helper configuration.
#[derive(Clone)]
pub(crate) struct Setup {
    pub dir: Result<PathBuf, &'static str>,
    pub secrets: Arc<dyn enrollment::IdentitySecrets>,
}
impl Setup {
    pub fn system() -> Self {
        Self {
            dir: config::dir(),
            secrets: Arc::new(enrollment::SecretService),
        }
    }
    fn dir(&self) -> Result<PathBuf, &'static str> {
        self.dir.clone().map_err(|_| "config_unavailable")
    }
    pub fn load(&self) -> Result<config::Config, &'static str> {
        config::load_from(&self.dir.clone()?)
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, &'static str> + Send + 'static,
) -> Result<T, &'static str> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| "config_unavailable")?
}

/// `omarchy-buzz setup relay URL`, from the panel: same validation, and a
/// different relay clears the identity exactly as the command does.
pub(crate) async fn set_relay(setup: &Setup, url: &str) -> Result<(), &'static str> {
    let relay = config::canonical_relay(url).map_err(|_| "setup_invalid_relay")?;
    let dir = setup.dir()?;
    blocking(move || {
        let current = config::load_from(&dir).map_err(|_| "config_unavailable")?;
        config::save_to(&dir, &config::with_relay(current, relay)).map_err(|_| "config_unavailable")
    })
    .await
}

/// Generates a new identity on this device and returns its public key (hex).
pub(crate) async fn create_identity(setup: &Setup) -> Result<String, &'static str> {
    create_with(setup, nostr::Keys::generate()).await
}

pub(crate) async fn create_with(setup: &Setup, keys: nostr::Keys) -> Result<String, &'static str> {
    let dir = setup.dir()?;
    let load_dir = dir.clone();
    let current =
        blocking(move || config::load_from(&load_dir).map_err(|_| "config_unavailable")).await?;
    if current.identity.is_some() {
        return Err("identity_exists");
    }
    let relay = current.relay.clone().ok_or("setup_invalid_relay")?;
    let candidate = keys.public_key();
    let identity = candidate.to_hex();
    let secret = Zeroizing::new(keys.secret_key().to_secret_hex());
    drop(keys);
    let secrets = setup.secrets.clone();
    enrollment::check_then_store(
        candidate,
        crate::catalog::relay_signer(&relay, None),
        || async move {
            tokio::task::spawn_blocking(move || {
                enrollment::store_in(&dir, secrets.as_ref(), current, secret, identity, true)
            })
            .await
            .map_err(|_| "identity_unavailable")?
        },
    )
    .await
    .map_err(category)?;
    Ok(candidate.to_hex())
}

fn category(error: &'static str) -> &'static str {
    match error {
        "identity_scope_changed" => "setup_busy",
        "identity_unavailable" | "identity_is_relay_signer" => "identity_unavailable",
        "config_unavailable" | "invalid_config" => "config_unavailable",
        "unconfigured" => "setup_invalid_relay",
        // Discovery failures: unreachable, oversized, redirected or no signer.
        _ => "relay_unavailable",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[derive(Default)]
    pub(crate) struct FakeSecrets {
        pub items: Mutex<BTreeMap<String, Zeroizing<String>>>,
        pub fail: Mutex<bool>,
    }
    impl enrollment::IdentitySecrets for FakeSecrets {
        fn store(&self, account: &str, secret: &Zeroizing<String>) -> Result<(), &'static str> {
            if *self.fail.lock().unwrap() {
                return Err("identity_unavailable");
            }
            self.items
                .lock()
                .unwrap()
                .insert(account.into(), secret.clone());
            Ok(())
        }
        fn forget(&self, account: &str) -> Result<(), &'static str> {
            self.items.lock().unwrap().remove(account);
            Ok(())
        }
    }

    pub(crate) fn fixture() -> (Setup, Arc<FakeSecrets>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("omarchy-buzz-setup-{}", uuid::Uuid::new_v4()));
        let secrets = Arc::new(FakeSecrets::default());
        (
            Setup {
                dir: Ok(dir.clone()),
                secrets: secrets.clone(),
            },
            secrets,
            dir,
        )
    }

    /// A loopback NIP-11 endpoint answering `requests` GET /info requests with
    /// `status` and `body`. Returns its `ws://` origin.
    pub(crate) async fn info_server(
        status: u16,
        body: String,
        requests: usize,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    let mut byte = [0; 1];
                    assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                    head.push(byte[0]);
                    assert!(head.len() < 16384);
                }
                let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                assert!(head.starts_with("get /info "), "{head}");
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/nostr+json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (origin, task)
    }

    fn with_relay(dir: &std::path::Path, relay: &str) {
        config::save_to(
            dir,
            &config::Config {
                relay: Some(relay.into()),
                identity: None,
            },
        )
        .unwrap();
    }

    #[tokio::test]
    async fn set_relay_persists_like_the_command_and_refuses_invalid_urls() {
        let (setup, _, dir) = fixture();
        for bad in [
            "http://example.com",
            "ws://example.com",
            "wss://user@example.com",
            "wss://example.com/path",
            "not a url",
        ] {
            assert_eq!(
                set_relay(&setup, bad).await.unwrap_err(),
                "setup_invalid_relay",
                "{bad}"
            );
        }
        assert!(!dir.exists(), "a refused relay wrote configuration");
        set_relay(&setup, "wss://Example.com").await.unwrap();
        assert_eq!(
            setup.load().unwrap().relay.as_deref(),
            Some("wss://example.com/")
        );
        // Same relay keeps the identity; a different one clears it.
        let identity = nostr::Keys::generate().public_key().to_hex();
        config::save_to(
            &dir,
            &config::Config {
                relay: Some("wss://example.com/".into()),
                identity: Some(identity.clone()),
            },
        )
        .unwrap();
        set_relay(&setup, "wss://example.com").await.unwrap();
        assert_eq!(setup.load().unwrap().identity, Some(identity));
        set_relay(&setup, "wss://other.example").await.unwrap();
        let c = setup.load().unwrap();
        assert_eq!(
            (c.relay.as_deref(), c.identity),
            (Some("wss://other.example/"), None)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn create_refuses_existing_identity_and_missing_relay() {
        let (setup, secrets, dir) = fixture();
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "setup_invalid_relay"
        );
        config::save_to(
            &dir,
            &config::Config {
                relay: Some("ws://127.0.0.1:1/".into()),
                identity: Some(nostr::Keys::generate().public_key().to_hex()),
            },
        )
        .unwrap();
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "identity_exists"
        );
        assert!(secrets.items.lock().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn create_stores_secret_and_saves_only_the_public_key() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (setup, secrets, dir) = fixture();
        let signer = nostr::Keys::generate().public_key();
        let (origin, server) = info_server(
            200,
            serde_json::json!({"self": signer.to_hex()}).to_string(),
            1,
        )
        .await;
        with_relay(&dir, &origin);
        let identity = create_identity(&setup).await.unwrap();
        server.await.unwrap();
        let c = setup.load().unwrap();
        assert_eq!(c.identity.as_deref(), Some(identity.as_str()));
        let items = secrets.items.lock().unwrap();
        let secret = items
            .get(&format!("{origin}|{identity}"))
            .expect("stored under relay|identity");
        let keys = nostr::Keys::parse(secret.as_str()).unwrap();
        assert_eq!(keys.public_key().to_hex(), identity);
        let file = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(
            !file.contains(secret.as_str()),
            "secret written to configuration"
        );
        drop(items);
        // A second attempt is refused: the identity now exists.
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "identity_exists"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn relay_signer_key_and_relay_failures_never_reach_storage() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (setup, secrets, dir) = fixture();
        let relay = nostr::Keys::generate();
        let (origin, server) = info_server(
            200,
            serde_json::json!({"self": relay.public_key().to_hex()}).to_string(),
            1,
        )
        .await;
        with_relay(&dir, &origin);
        assert_eq!(
            create_with(&setup, relay.clone()).await.unwrap_err(),
            "identity_unavailable"
        );
        server.await.unwrap();
        let (origin, server) = info_server(500, "{}".into(), 1).await;
        with_relay(&dir, &origin);
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "relay_unavailable"
        );
        server.await.unwrap();
        let (origin, server) = info_server(200, "{\"name\":\"no signer\"}".into(), 1).await;
        with_relay(&dir, &origin);
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "relay_unavailable"
        );
        server.await.unwrap();
        // Nothing listens here.
        let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}/", closed.local_addr().unwrap());
        drop(closed);
        with_relay(&dir, &origin);
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "relay_unavailable"
        );
        assert!(secrets.items.lock().unwrap().is_empty());
        assert!(setup.load().unwrap().identity.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn secret_store_failure_leaves_configuration_unchanged() {
        let _network_fixture = crate::NETWORK_TEST_LOCK.lock().await;
        let (setup, secrets, dir) = fixture();
        *secrets.fail.lock().unwrap() = true;
        let signer = nostr::Keys::generate().public_key();
        let (origin, server) = info_server(
            200,
            serde_json::json!({"self": signer.to_hex()}).to_string(),
            1,
        )
        .await;
        with_relay(&dir, &origin);
        assert_eq!(
            create_identity(&setup).await.unwrap_err(),
            "identity_unavailable"
        );
        server.await.unwrap();
        assert!(setup.load().unwrap().identity.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn categories_are_fixed() {
        for e in [
            "identity_scope_changed",
            "identity_unavailable",
            "identity_is_relay_signer",
            "config_unavailable",
            "invalid_config",
            "unconfigured",
            "discovery_unavailable",
            "discovery_redirect_rejected",
            "anything else",
        ] {
            assert!(CATEGORIES.contains(&category(e)), "{e}");
        }
    }
}
