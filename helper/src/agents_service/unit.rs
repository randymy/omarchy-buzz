//! Per-agent systemd user units from the reviewed template, and unit control
//! through argv arrays only. No value is ever passed through a shell.
use super::store::{self, Paths, Persona};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

pub const TEMPLATE: &str = include_str!("../../../service/agent.service.in");
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const CONTROL_DEADLINE: Duration = Duration::from_secs(30);

/// The bundle launcher inside a harness bundle.
pub fn launcher(paths: &Paths, harness: &str) -> PathBuf {
    paths.bundle(harness).join("launcher/room-agent")
}

fn text(path: &Path) -> Result<String, &'static str> {
    let value = path.to_str().ok_or("unit_failed")?;
    if !store::canonical_path(value) {
        return Err("unit_failed");
    }
    Ok(value.to_owned())
}

/// The exact `ExecStart` argv for an enrolled persona.
pub fn exec_argv(
    paths: &Paths,
    persona: &Persona,
    relay: &str,
    owner: &str,
) -> Result<Vec<String>, &'static str> {
    let identity = persona.identity.as_deref().ok_or("unit_failed")?;
    // The attestation must be this owner's for this agent: the launcher
    // refuses a tag that does not name `--owner`.
    let attested = persona.auth_tag.as_deref().is_some_and(|tag| {
        nostr::PublicKey::from_hex(identity)
            .ok()
            .and_then(|agent| buzz_sdk::nip_oa::verify_auth_tag(tag, &agent).ok())
            .is_some_and(|key| key.to_hex() == owner)
    });
    if !persona.valid()
        || !attested
        || !store::canonical_key(owner)
        || owner == identity
        || crate::config::canonical_relay(relay).as_deref() != Ok(relay)
    {
        return Err("unit_failed");
    }
    let mut argv = vec![
        text(&launcher(paths, &persona.harness))?,
        "--harness".into(),
        persona.harness.clone(),
        "--profile".into(),
        text(&paths.profile(&persona.harness))?,
        "--workspace".into(),
        persona.workspace.clone(),
        "--bundle".into(),
        text(&paths.bundle(&persona.harness))?,
        "--relay".into(),
        relay.into(),
    ];
    for room in &persona.rooms {
        argv.extend(["--room".into(), room.clone()]);
    }
    argv.extend([
        "--owner".into(),
        owner.into(),
        "--identity".into(),
        identity.into(),
        "--respond-to".into(),
        persona.respond_to.clone(),
        // The owner's NIP-OA attestation, which the launcher hands to
        // `buzz-acp` as `BUZZ_AUTH_TAG` through its memfd options.
        "--auth-tag".into(),
        text(&paths.auth_tag_file(&persona.id))?,
    ]);
    if !persona.instructions.is_empty() {
        argv.extend([
            "--instructions".into(),
            text(&paths.instructions_file(&persona.id))?,
        ]);
    }
    if !persona.model.is_empty() {
        argv.extend(["--model".into(), persona.model.clone()]);
    }
    Ok(argv)
}

/// systemd command-line quoting: every word double-quoted, with `\` and `"`
/// escaped and specifier (`%`) and variable (`$`) expansion suppressed.
fn quote(word: &str) -> Result<String, &'static str> {
    if word.chars().any(char::is_control) {
        return Err("unit_failed");
    }
    let mut out = String::with_capacity(word.len() + 2);
    out.push('"');
    for ch in word.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    Ok(out)
}

pub fn render(
    paths: &Paths,
    persona: &Persona,
    relay: &str,
    owner: &str,
) -> Result<String, &'static str> {
    let argv = exec_argv(paths, persona, relay, owner)?;
    let exec = argv
        .iter()
        .map(|w| quote(w))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    for marker in ["@AGENT_ID@", "@EXEC_START@"] {
        if TEMPLATE.matches(marker).count() != 1 {
            return Err("unit_failed");
        }
    }
    Ok(TEMPLATE
        .replace("@AGENT_ID@", &persona.id)
        .replace("@EXEC_START@", &exec))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Start,
    Stop,
    Enable,
    Disable,
}
impl Op {
    fn verb(self) -> &'static str {
        match self {
            Op::Start => "start",
            Op::Stop => "stop",
            Op::Enable => "enable",
            Op::Disable => "disable",
        }
    }
}
fn checked_unit(unit: &str) -> Result<&str, &'static str> {
    let id = unit
        .strip_prefix("omarchy-buzz-agent-")
        .and_then(|rest| rest.strip_suffix(".service"))
        .ok_or("unit_failed")?;
    if store::canonical_uuid(id) {
        Ok(unit)
    } else {
        Err("unit_failed")
    }
}
/// The argv for a unit operation. Only generated agent unit names are accepted.
pub fn op_argv(op: Op, unit: &str) -> Result<Vec<String>, &'static str> {
    Ok(vec![
        SYSTEMCTL.into(),
        "--user".into(),
        op.verb().into(),
        checked_unit(unit)?.into(),
    ])
}
pub fn reload_argv() -> Vec<String> {
    vec![SYSTEMCTL.into(), "--user".into(), "daemon-reload".into()]
}
pub fn show_argv(unit: &str) -> Result<Vec<String>, &'static str> {
    Ok(vec![
        SYSTEMCTL.into(),
        "--user".into(),
        "show".into(),
        "--property=ActiveState".into(),
        checked_unit(unit)?.into(),
    ])
}
/// Maps `systemctl show` output to the contract's unit states. Transitional
/// and unrecognized states stay `unknown`; a missing unit is inactive.
pub fn unit_state(show: &str) -> &'static str {
    let value = show
        .lines()
        .find_map(|line| line.strip_prefix("ActiveState="))
        .unwrap_or("");
    match value.trim_end() {
        "active" => "active",
        "inactive" => "inactive",
        "failed" => "failed",
        _ => "unknown",
    }
}

pub trait UnitControl: Send + Sync {
    fn run(&self, op: Op, unit: &str) -> Result<(), &'static str>;
    fn daemon_reload(&self) -> Result<(), &'static str>;
    /// `active`, `inactive`, `failed` or `unknown`.
    fn state(&self, unit: &str) -> &'static str;
}

/// Runs a bounded child without a shell. Standard input and error are closed;
/// at most 64 KiB of standard output is captured when requested. Output is read
/// on its own thread, so a descendant holding the pipe cannot block the caller.
pub fn run_bounded(
    argv: &[String],
    deadline: Duration,
    capture: bool,
) -> Result<(Option<i32>, Vec<u8>), &'static str> {
    use std::io::Read;
    let (program, args) = argv.split_first().ok_or("spawn_failed")?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(if capture {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "spawn_failed")?;
    let reader = child.stdout.take().map(|stdout| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.take(64 * 1024).read_to_end(&mut out);
            let _ = tx.send(out);
        });
        rx
    });
    let until = Instant::now() + deadline;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader
        .and_then(|rx| rx.recv_timeout(Duration::from_secs(1)).ok())
        .unwrap_or_default();
    Ok((status.and_then(|s| s.code()), out))
}

/// `systemctl --user` through fixed argv arrays.
pub struct Systemctl;
impl UnitControl for Systemctl {
    fn run(&self, op: Op, unit: &str) -> Result<(), &'static str> {
        match run_bounded(&op_argv(op, unit)?, CONTROL_DEADLINE, false) {
            Ok((Some(0), _)) => Ok(()),
            _ => Err("unit_failed"),
        }
    }
    fn daemon_reload(&self) -> Result<(), &'static str> {
        match run_bounded(&reload_argv(), CONTROL_DEADLINE, false) {
            Ok((Some(0), _)) => Ok(()),
            _ => Err("unit_failed"),
        }
    }
    fn state(&self, unit: &str) -> &'static str {
        let Ok(argv) = show_argv(unit) else {
            return "unknown";
        };
        match run_bounded(&argv, CONTROL_DEADLINE, true) {
            Ok((Some(0), out)) => unit_state(&String::from_utf8_lossy(&out)),
            _ => "unknown",
        }
    }
}

/// In-memory unit control for tests and `OMARCHY_BUZZ_AGENTS_FAKE_CONTROL=1`.
/// It records the argv the real implementation would run.
#[derive(Default)]
pub struct FakeControl {
    pub calls: Mutex<Vec<Vec<String>>>,
    /// unit -> (ActiveState, enabled)
    pub units: Mutex<BTreeMap<String, (&'static str, bool)>>,
    /// Operations that fail (for tests).
    pub failing: Mutex<Vec<Op>>,
}
impl UnitControl for FakeControl {
    fn run(&self, op: Op, unit: &str) -> Result<(), &'static str> {
        let argv = op_argv(op, unit)?;
        self.calls.lock().unwrap().push(argv);
        if self.failing.lock().unwrap().contains(&op) {
            return Err("unit_failed");
        }
        let mut units = self.units.lock().unwrap();
        let entry = units.entry(unit.into()).or_insert(("inactive", false));
        match op {
            Op::Start => entry.0 = "active",
            Op::Stop => entry.0 = "inactive",
            Op::Enable => entry.1 = true,
            Op::Disable => entry.1 = false,
        }
        Ok(())
    }
    fn daemon_reload(&self) -> Result<(), &'static str> {
        self.calls.lock().unwrap().push(reload_argv());
        Ok(())
    }
    fn state(&self, unit: &str) -> &'static str {
        let Ok(argv) = show_argv(unit) else {
            return "unknown";
        };
        self.calls.lock().unwrap().push(argv);
        let state = self
            .units
            .lock()
            .unwrap()
            .get(unit)
            .map_or("inactive", |u| u.0);
        unit_state(&format!("ActiveState={state}\n"))
    }
}

#[cfg(test)]
#[path = "unit_tests.rs"]
mod tests;
