//! Harness readiness and provider sign-in through the reviewed scripts, run
//! with fixed argv arrays only (never a shell):
//!
//! - `<bundle>/launcher/agent-bundle --check <harness>` prints `ready` or
//!   `missing`; only an exit status of 0 with `ready` counts as ready.
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
}

/// In-memory spawner: scripts are absent unless `present` is set; outputs
/// come from `outputs` keyed by the argv joined with spaces (exit status 0).
#[derive(Default)]
pub struct FakeSpawner {
    pub present: Mutex<bool>,
    pub outputs: Mutex<std::collections::BTreeMap<String, String>>,
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
        match self.outputs.lock().unwrap().get(&argv.join(" ")) {
            Some(out) => Ok((Some(0), out.clone())),
            None => Ok((Some(1), String::new())),
        }
    }
    fn spawn(&self, argv: &[String], env: &[(OsString, OsString)]) -> Result<(), &'static str> {
        self.spawned.lock().unwrap().push(argv.to_vec());
        self.spawned_env.lock().unwrap().push(env.to_vec());
        Ok(())
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
