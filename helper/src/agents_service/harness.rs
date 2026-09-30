//! Harness readiness and provider sign-in through the reviewed scripts, run
//! with fixed argv arrays only (never a shell):
//!
//! - `<scripts>/agent-bundle --check --output <bundle> --scripts <scripts>
//!   <harness>` prints `ready` (exit 0), `stale` (exit 3: the bundle's
//!   `launcher/` differs from the installed scripts) or `missing`; any other
//!   output, status, failure or timeout is `missing`. The installed checker
//!   runs, never the bundle's own copy, which may predate the comparison.
//! - `<scripts>/agent-bundle --refresh-launcher --output <bundle> --scripts
//!   <scripts> <harness>` replaces only the bundle's `launcher/` files; its
//!   outcome is read back through `--check`.
//! - `<scripts>/agent-login --status <harness>` prints `signed-in`,
//!   `signed-out` or `unknown`; `unknown`, any other output, a failure to run
//!   or a timeout is reported as `null`.
//! - `<scripts>/agent-login <harness>` opens the vendor login in a terminal;
//!   the service only starts it and never reads its output. It runs with a
//!   minimal environment ([`login_environment`]); the script detaches the
//!   terminal into its own user scope so it outlives this service.
//!
//! `<bundle>` is `~/.local/share/omarchy-buzz/agent-<harness>` and `<scripts>`
//! is `~/.local/share/omarchy-buzz/scripts` (XDG data directory), where a
//! later, human-reviewed installation step places the reviewed scripts.
use super::store::{Paths, HARNESSES};
use serde::Serialize;
use std::{
    ffi::OsString,
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
    /// Starts a process with exactly `env` and returns without waiting for it.
    fn spawn(&self, argv: &[String], env: &[(OsString, OsString)]) -> Result<(), &'static str>;
    /// Runs a model probe with exactly `env` for at most
    /// [`super::models::PROBE_DEADLINE`] and returns its exit status (`None`
    /// when killed) and at most [`super::models::PROBE_OUTPUT`] bytes of its
    /// combined standard output and error.
    fn probe(
        &self,
        argv: &[String],
        env: &[(OsString, OsString)],
    ) -> Result<(Option<i32>, String), &'static str>;
}

/// Session variables `agent-login` needs to open a terminal on this desktop:
/// these names and every `XDG_*` variable, nothing else (no provider keys,
/// tokens, proxies or Buzz credentials reach the login from the service).
pub const LOGIN_ENV: [&str; 7] = [
    "PATH",
    "HOME",
    "XDG_RUNTIME_DIR",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "HYPRLAND_INSTANCE_SIGNATURE",
];

/// The environment passed to `agent-login`, filtered from `ambient` (the
/// service's own environment) and sorted by name.
pub fn login_environment(
    ambient: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = ambient
        .into_iter()
        .filter(|(key, _)| {
            key.to_str()
                .is_some_and(|k| LOGIN_ENV.contains(&k) || k.starts_with("XDG_"))
        })
        .collect();
    env.sort();
    env.dedup_by(|a, b| a.0 == b.0);
    env
}

pub fn bundle_script(paths: &Paths) -> PathBuf {
    paths.scripts_dir().join("agent-bundle")
}
pub fn login_script(paths: &Paths) -> PathBuf {
    paths.scripts_dir().join("agent-login")
}
fn argv(script: &Path, args: &[&str]) -> Vec<String> {
    std::iter::once(script.to_string_lossy().into_owned())
        .chain(args.iter().map(|a| (*a).to_owned()))
        .collect()
}
fn bundle_argv(paths: &Paths, mode: &str, harness: &str) -> Vec<String> {
    let bundle = paths.bundle(harness);
    let scripts = paths.scripts_dir();
    let (bundle, scripts) = (bundle.to_string_lossy(), scripts.to_string_lossy());
    argv(
        &bundle_script(paths),
        &[mode, "--output", &bundle, "--scripts", &scripts, harness],
    )
}
pub fn check_argv(paths: &Paths, harness: &str) -> Vec<String> {
    bundle_argv(paths, "--check", harness)
}
pub fn refresh_argv(paths: &Paths, harness: &str) -> Vec<String> {
    bundle_argv(paths, "--refresh-launcher", harness)
}
pub fn status_argv(paths: &Paths, harness: &str) -> Vec<String> {
    argv(&login_script(paths), &["--status", harness])
}
pub fn login_argv(paths: &Paths, harness: &str) -> Vec<String> {
    argv(&login_script(paths), &[harness])
}

/// Exit status of `agent-bundle --check` for a stale launcher.
pub const STALE_EXIT: i32 = 3;

/// Current readiness of both harnesses. Absent scripts report `missing`/`null`.
pub fn inspect(paths: &Paths, spawner: &dyn Spawner) -> Vec<HarnessView> {
    HARNESSES
        .iter()
        .map(|harness| {
            let bundle = if spawner.present(&bundle_script(paths)) {
                match spawner.output(&check_argv(paths, harness)) {
                    Ok((Some(0), out)) if out.trim_end() == "ready" => "ready",
                    Ok((Some(STALE_EXIT), out)) if out.trim_end() == "stale" => "stale",
                    _ => "missing",
                }
            } else {
                "missing"
            };
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
                bundle,
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
    fn spawn(&self, argv: &[String], env: &[(OsString, OsString)]) -> Result<(), &'static str> {
        let (program, args) = argv.split_first().ok_or("spawn_failed")?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
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
    fn probe(
        &self,
        argv: &[String],
        env: &[(OsString, OsString)],
    ) -> Result<(Option<i32>, String), &'static str> {
        use super::models::{run_merged, PROBE_DEADLINE, PROBE_OUTPUT};
        run_merged(argv, env, PROBE_DEADLINE, PROBE_OUTPUT)
            .map(|(code, out)| (code, String::from_utf8_lossy(&out).into_owned()))
    }
}

/// In-memory spawner: scripts are absent unless `present` is set; outputs
/// come from `outputs` keyed by the argv joined with spaces, with the exit
/// status from `codes` under the same key (default 0; 1 without an output).
/// A probe answers `probe_answer` (default: exit 1, no output) and is
/// recorded with its environment in `probes`.
#[derive(Default)]
pub struct FakeSpawner {
    pub probe_answer: Mutex<Option<(Option<i32>, String)>>,
    /// When set, a probe waits for one message on it before answering.
    pub probe_hold: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    pub probes: Mutex<Vec<(Vec<String>, Vec<(OsString, OsString)>)>>,
    pub present: Mutex<bool>,
    pub outputs: Mutex<std::collections::BTreeMap<String, String>>,
    pub codes: Mutex<std::collections::BTreeMap<String, i32>>,
    pub calls: Mutex<Vec<Vec<String>>>,
    pub spawned: Mutex<Vec<Vec<String>>>,
    pub spawned_env: Mutex<Vec<Vec<(OsString, OsString)>>>,
}
impl Spawner for FakeSpawner {
    fn present(&self, _script: &Path) -> bool {
        *self.present.lock().unwrap()
    }
    fn output(&self, argv: &[String]) -> Result<(Option<i32>, String), &'static str> {
        self.calls.lock().unwrap().push(argv.to_vec());
        let key = argv.join(" ");
        match self.outputs.lock().unwrap().get(&key) {
            Some(out) => {
                let code = self.codes.lock().unwrap().get(&key).copied().unwrap_or(0);
                Ok((Some(code), out.clone()))
            }
            None => Ok((Some(1), String::new())),
        }
    }
    fn spawn(&self, argv: &[String], env: &[(OsString, OsString)]) -> Result<(), &'static str> {
        self.spawned.lock().unwrap().push(argv.to_vec());
        self.spawned_env.lock().unwrap().push(env.to_vec());
        Ok(())
    }
    fn probe(
        &self,
        argv: &[String],
        env: &[(OsString, OsString)],
    ) -> Result<(Option<i32>, String), &'static str> {
        self.probes
            .lock()
            .unwrap()
            .push((argv.to_vec(), env.to_vec()));
        if let Some(hold) = self.probe_hold.lock().unwrap().take() {
            let _ = hold.recv();
        }
        Ok(self
            .probe_answer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or((Some(1), String::new())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(values: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        values
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    #[test]
    fn login_environment_keeps_only_session_variables() {
        let ambient = pairs(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("OPENAI_API_KEY", "synthetic"),
            ("ANTHROPIC_API_KEY", "synthetic"),
            ("BUZZ_PRIVATE_KEY", "synthetic"),
            ("HTTPS_PROXY", "http://proxy.invalid"),
            ("LD_PRELOAD", "/tmp/x.so"),
            ("XDG_CURRENT_DESKTOP", "Hyprland"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("PATH", "/usr/bin"),
            ("HOME", "/home/example"),
            ("DISPLAY", ":1"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
            ("TERM", "xterm"),
            ("XDGX", "no"),
        ]);
        assert_eq!(
            login_environment(ambient),
            pairs(&[
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
                ("DISPLAY", ":1"),
                ("HOME", "/home/example"),
                ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
                ("PATH", "/usr/bin"),
                ("WAYLAND_DISPLAY", "wayland-1"),
                ("XDG_CURRENT_DESKTOP", "Hyprland"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ])
        );
    }

    #[test]
    fn spawned_processes_get_exactly_the_given_environment() {
        let dir = std::env::temp_dir().join(format!("omarchy-buzz-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let out = dir.join("env.json");
        let partial = dir.join("env.tmp");
        let script = "import json, os, sys; open(sys.argv[1], 'w').write(json.dumps(dict(os.environ))); os.rename(sys.argv[1], sys.argv[2])";
        Processes
            .spawn(
                &[
                    "/usr/bin/python3".into(),
                    "-c".into(),
                    script.into(),
                    partial.to_str().unwrap().into(),
                    out.to_str().unwrap().into(),
                ],
                &pairs(&[("PATH", "/usr/bin"), ("WAYLAND_DISPLAY", "wayland-9")]),
            )
            .unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !out.exists() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut seen: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        // Python may add its own C-locale coercion variable.
        seen.remove("LC_CTYPE");
        assert_eq!(
            seen,
            [("PATH", "/usr/bin"), ("WAYLAND_DISPLAY", "wayland-9")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        );
    }
}
