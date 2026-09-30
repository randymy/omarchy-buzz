//! Harness readiness and provider sign-in through the reviewed scripts, run
//! with fixed argv arrays only (never a shell):
//!
//! - `<bundle>/launcher/agent-bundle --check <harness>` prints `ready` or
//!   `missing`; only an exit status of 0 with `ready` counts as ready.
//! - `<scripts>/agent-login --status <harness>` prints `signed-in`,
//!   `signed-out` or `unknown`; `unknown`, any other output, a failure to run
//!   or a timeout is reported as `null`.
//! - `<scripts>/agent-login <harness>` opens the vendor login in a terminal;
//!   the service only starts it and never reads its output.
//!
//! `<bundle>` is `~/.local/share/omarchy-buzz/agent-<harness>` and `<scripts>`
//! is `~/.local/share/omarchy-buzz/scripts` (XDG data directory), where a
//! later, human-reviewed installation step places the reviewed scripts.
use super::store::{Paths, HARNESSES};
use serde::Serialize;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

const CHECK_DEADLINE: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessView {
    pub id: String,
    pub bundle: &'static str,
    pub signed_in: Option<bool>,
}

pub trait Spawner: Send + Sync {
    /// Whether a reviewed script is present and safe to run.
    fn present(&self, script: &Path) -> bool;
    /// Runs to completion (bounded) and returns its exit status (`None` when
    /// killed or timed out) and at most 64 KiB of standard output.
    fn output(&self, argv: &[String]) -> Result<(Option<i32>, String), &'static str>;
    /// Starts a detached process and returns without waiting for it.
    fn spawn(&self, argv: &[String]) -> Result<(), &'static str>;
}

pub fn bundle_script(paths: &Paths, harness: &str) -> PathBuf {
    paths.bundle(harness).join("launcher/agent-bundle")
}
pub fn login_script(paths: &Paths) -> PathBuf {
    paths.scripts_dir().join("agent-login")
}
fn argv(script: &Path, args: &[&str]) -> Vec<String> {
    std::iter::once(script.to_string_lossy().into_owned())
        .chain(args.iter().map(|a| (*a).to_owned()))
        .collect()
}
pub fn check_argv(paths: &Paths, harness: &str) -> Vec<String> {
    argv(&bundle_script(paths, harness), &["--check", harness])
}
pub fn status_argv(paths: &Paths, harness: &str) -> Vec<String> {
    argv(&login_script(paths), &["--status", harness])
}
pub fn login_argv(paths: &Paths, harness: &str) -> Vec<String> {
    argv(&login_script(paths), &[harness])
}

/// Current readiness of both harnesses. Absent scripts report `missing`/`null`.
pub fn inspect(paths: &Paths, spawner: &dyn Spawner) -> Vec<HarnessView> {
    HARNESSES
        .iter()
        .map(|harness| {
            let ready = spawner.present(&bundle_script(paths, harness))
                && matches!(spawner.output(&check_argv(paths, harness)),
                    Ok((Some(0), out)) if out.trim_end() == "ready");
            let signed_in = if spawner.present(&login_script(paths)) {
                // The printed word decides; a timeout or signal is unknown.
                match spawner.output(&status_argv(paths, harness)) {
                    Ok((Some(_), out)) if out.trim_end() == "signed-in" => Some(true),
                    Ok((Some(_), out)) if out.trim_end() == "signed-out" => Some(false),
                    _ => None,
                }
            } else {
                None
            };
            HarnessView {
                id: (*harness).into(),
                bundle: if ready { "ready" } else { "missing" },
                signed_in,
            }
        })
        .collect()
}

/// Runs the reviewed scripts. A script must be an unlinked regular file owned
/// by this user, executable, and not writable by group or others.
pub struct Processes;
impl Spawner for Processes {
    fn present(&self, script: &Path) -> bool {
        super::store::unlinked(script)
            && std::fs::symlink_metadata(script).is_ok_and(|m| {
                m.is_file()
                    && m.uid() == rustix::process::getuid().as_raw()
                    && m.mode() & 0o022 == 0
                    && m.mode() & 0o100 != 0
            })
    }
    fn output(&self, argv: &[String]) -> Result<(Option<i32>, String), &'static str> {
        super::unit::run_bounded(argv, CHECK_DEADLINE, true)
            .map(|(code, out)| (code, String::from_utf8_lossy(&out).into_owned()))
    }
    fn spawn(&self, argv: &[String]) -> Result<(), &'static str> {
        let (program, args) = argv.split_first().ok_or("spawn_failed")?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| "spawn_failed")?;
        // Reap it in the background; its outcome is observed via `--status`.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// In-memory spawner: scripts are absent unless `present` is set; outputs
/// come from `outputs` keyed by the argv joined with spaces (exit status 0).
#[derive(Default)]
pub struct FakeSpawner {
    pub present: Mutex<bool>,
    pub outputs: Mutex<std::collections::BTreeMap<String, String>>,
    pub calls: Mutex<Vec<Vec<String>>>,
    pub spawned: Mutex<Vec<Vec<String>>>,
}
impl Spawner for FakeSpawner {
    fn present(&self, _script: &Path) -> bool {
        *self.present.lock().unwrap()
    }
    fn output(&self, argv: &[String]) -> Result<(Option<i32>, String), &'static str> {
        self.calls.lock().unwrap().push(argv.to_vec());
        match self.outputs.lock().unwrap().get(&argv.join(" ")) {
            Some(out) => Ok((Some(0), out.clone())),
            None => Ok((Some(1), String::new())),
        }
    }
    fn spawn(&self, argv: &[String]) -> Result<(), &'static str> {
        self.spawned.lock().unwrap().push(argv.to_vec());
        Ok(())
    }
}
