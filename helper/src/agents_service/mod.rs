//! `omarchy-buzz agents-daemon` and `agents-bridge`: the agent manager service
//! of `docs/AGENTS_SERVICE.md`. It listens on
//! `$XDG_RUNTIME_DIR/omarchy-buzz/agents.sock` with the helper's private
//! directory, peer-uid and framing rules (`crate::ipc`).
//!
//! `OMARCHY_BUZZ_AGENTS_FAKE_CONTROL=1` replaces unit control, the keyring,
//! the script spawner and the room source with in-memory fakes (development and
//! `tests/agents_smoke.py` only); `OMARCHY_BUZZ_AGENTS_FAKE_ROOMS` then lists
//! the synthetic joined rooms (comma-separated UUIDs).
pub mod enroll;
pub mod harness;
pub mod keys;
pub mod request;
pub mod rooms;
pub mod service;
pub mod store;
#[cfg(test)]
mod test_support;
pub mod unit;

use service::{envelope, error_frame, Deps, Service};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::BufReader,
    net::UnixStream,
    sync::mpsc,
    time::{Duration, Instant},
};

pub const SOCKET: &str = "agents.sock";
const MAX_CLIENTS: usize = 8;
const IDLE_EXIT: Duration = Duration::from_secs(30);
const UNIT_INSPECTION: Duration = Duration::from_secs(15);
const HARNESS_INSPECTION: Duration = Duration::from_secs(60);

fn deps_from_env() -> Deps {
    if std::env::var("OMARCHY_BUZZ_AGENTS_FAKE_CONTROL").as_deref() == Ok("1") {
        let rooms: Vec<String> = std::env::var("OMARCHY_BUZZ_AGENTS_FAKE_ROOMS")
            .unwrap_or_default()
            .split(',')
            .filter(|r| store::canonical_uuid(r))
            .map(str::to_owned)
            .collect();
        Deps {
            control: Arc::new(unit::FakeControl::default()),
            keyring: Arc::new(keys::FakeKeyring::default()),
            spawner: Arc::new(harness::FakeSpawner::default()),
            rooms: Arc::new(rooms::FixedRooms::new(rooms)),
        }
    } else {
        Deps {
            control: Arc::new(unit::Systemctl),
            keyring: Arc::new(keys::SecretService),
            spawner: Arc::new(harness::Processes),
            rooms: Arc::new(rooms::HelperCatalog),
        }
    }
}

/// One IPC client: `hello`, then answers and (after `subscribe`) every status.
pub(crate) async fn client(
    s: UnixStream,
    service: Arc<Service>,
    instance: String,
) -> Result<(), &'static str> {
    crate::ipc::peer_allowed(&s)?;
    let (read, mut out) = s.into_split();
    let mut read = BufReader::new(read);
    let mut status = service.watch();
    let snapshot = status.borrow_and_update().clone();
    crate::ipc::write(&mut out, &envelope("hello", None, &instance, &snapshot)).await?;
    let (done_tx, mut done) = mpsc::channel::<String>(MAX_CLIENTS);
    let mut subscribed = false;
    let mut partial = Vec::new();
    loop {
        tokio::select! {
            line = crate::protocol::read_line_buffered(&mut read, &mut partial) => {
                let Some(line) = line? else { return Ok(()) };
                let r = match request::parse(&line) {
                    Ok(r) if r.instance_id == instance => r,
                    Ok(r) => {
                        crate::ipc::write(&mut out, &error_frame(&r.id, &instance, "agent_invalid")).await?;
                        continue;
                    }
                    Err(Some(id)) => {
                        crate::ipc::write(&mut out, &error_frame(&id, &instance, "agent_invalid")).await?;
                        continue;
                    }
                    // Not a request frame at all: end the session.
                    Err(None) => return Err("invalid_request"),
                };
                if r.kind == "subscribe" {
                    subscribed = true;
                    let service = service.clone();
                    tokio::spawn(async move {
                        service.inspect_units().await;
                        service.inspect_harnesses().await;
                    });
                    let snapshot = status.borrow_and_update().clone();
                    crate::ipc::write(&mut out, &envelope("status", Some(&r.id), &instance, &snapshot)).await?;
                    continue;
                }
                let Some(slot) = service.begin() else {
                    crate::ipc::write(&mut out, &error_frame(&r.id, &instance, "agent_busy")).await?;
                    continue;
                };
                let service = service.clone();
                let done_tx = done_tx.clone();
                // The request completes even if this client disconnects.
                tokio::spawn(async move {
                    let _ = service.execute(&r, slot).await;
                    let _ = done_tx.send(r.id).await;
                });
            },
            Some(id) = done.recv() => {
                let snapshot = status.borrow_and_update().clone();
                crate::ipc::write(&mut out, &envelope("status", Some(&id), &instance, &snapshot)).await?;
            },
            changed = status.changed(), if subscribed => {
                changed.map_err(|_| "daemon_unavailable")?;
                let snapshot = status.borrow_and_update().clone();
                crate::ipc::write(&mut out, &envelope("status", None, &instance, &snapshot)).await?;
            }
        }
    }
}

pub async fn daemon(keep: bool) -> Result<(), &'static str> {
    let paths = store::Paths::from_env()?;
    let path = crate::ipc::runtime_socket(SOCKET)?;
    let service = Service::open(paths, deps_from_env(), Box::new(crate::config::load))?;
    let (listener, standalone) = crate::ipc::listen(&path)?;
    let instance = crate::ipc::instance_id()?;
    let count = Arc::new(AtomicUsize::new(0));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "signal_unavailable")?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut idle = Instant::now();
    let mut units_due = Instant::now() + UNIT_INSPECTION;
    let mut harnesses_due = Instant::now() + HARNESS_INSPECTION;
    loop {
        tokio::select! {
            _ = terminate.recv() => break,
            signal = tokio::signal::ctrl_c() => { signal.map_err(|_| "signal_unavailable")?; break; },
            _ = tick.tick() => {
                let now = Instant::now();
                if count.load(Ordering::SeqCst) > 0 || !service.idle() {
                    idle = now;
                    if now >= units_due {
                        units_due = now + UNIT_INSPECTION;
                        let service = service.clone();
                        tokio::spawn(async move { service.inspect_units().await });
                    }
                    if now >= harnesses_due {
                        harnesses_due = now + HARNESS_INSPECTION;
                        let service = service.clone();
                        tokio::spawn(async move { service.inspect_harnesses().await });
                    }
                } else if !keep && idle.elapsed() > IDLE_EXIT {
                    break;
                }
            },
            accepted = listener.accept() => {
                let (s, _) = accepted.map_err(|_| "ipc_unavailable")?;
                if count.load(Ordering::SeqCst) >= MAX_CLIENTS { drop(s); continue; }
                count.fetch_add(1, Ordering::SeqCst);
                let count = count.clone();
                let service = service.clone();
                let instance = instance.clone();
                tokio::spawn(async move { let _ = client(s, service, instance).await; count.fetch_sub(1, Ordering::SeqCst); });
            }
        }
    }
    drop(listener);
    if standalone {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Frames that are not requests at all are refused locally; every request is
/// forwarded so the service answers refusals with an error frame.
fn bridge_check(line: &[u8]) -> Result<(), &'static str> {
    match request::parse(line) {
        Err(None) => Err("invalid_request"),
        _ => Ok(()),
    }
}

pub async fn bridge() -> Result<(), &'static str> {
    crate::ipc::bridge_to(&crate::ipc::runtime_socket(SOCKET)?, bridge_check).await
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod ipc_tests;
