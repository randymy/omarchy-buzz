//! Model names per harness, and the on-demand model probe.
//!
//! Static check: a persona's `model` must be empty (the harness default) or
//! match its harness's pattern. `create_agent`/`update_agent` refuse anything
//! else with `agent_invalid` and the pending detail `model_not_for_harness`.
//! The patterns are deliberately permissive (new model ids keep appearing) but
//! never admit spaces, slashes, brackets or control characters; they are a
//! subset of the store's `[A-Za-z0-9._:-]{0,64}` rule, which still applies.
//!
//! - Claude Code 2.1.280: the tier aliases `opus`, `sonnet`, `haiku`, `fable`
//!   (the CLI's own alias list, of which `--help` names `fable`, `opus` and
//!   `sonnet`), or a full id `claude-[a-z0-9-]+` (`claude-opus-4-5`,
//!   `claude-sonnet-4-5-20250929`).
//! - Codex 0.158.0: `gpt-[a-z0-9.-]+`, `o[0-9][a-z0-9-]*` or
//!   `codex-[a-z0-9.-]+` (`gpt-5.5`, `o3`, `codex-mini-latest`). Codex has no
//!   aliases; `codex exec --help` takes any `--model <MODEL>`.
//!
//! Live probe (`probe_model`): the harness's own launcher runs
//! `room-agent --probe-model <model>`, which builds the agent's `bwrap` view
//! with the shared harness profile and a throwaway workspace (no relay, no
//! agent key, no `buzz-acp`) and runs one minimal non-interactive vendor turn.
//! The service reads at most [`PROBE_OUTPUT`] bytes of its combined output
//! within [`PROBE_DEADLINE`] and reduces it to a state and one fixed sentence
//! ([`classify`]); the output itself is never stored or shown.
use super::store::{self, Paths};
use std::{
    ffi::OsString,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const CLAUDE_ALIASES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

/// The pending `detail` of a refused create or update (the only detail the
/// service reports so far).
pub const NOT_FOR_HARNESS: &str = "model_not_for_harness";

/// Aliases the harness CLI resolves itself (the panel shows the same list).
#[cfg(test)]
pub fn aliases(harness: &str) -> &'static [&'static str] {
    match harness {
        "claude-code" => &CLAUDE_ALIASES,
        _ => &[],
    }
}

fn rest_of(model: &str, prefix: &str, extra: &[u8]) -> bool {
    model.strip_prefix(prefix).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || extra.contains(&b))
    })
}

/// Whether `model` is empty or names a model of `harness`.
pub fn valid_for(harness: &str, model: &str) -> bool {
    if model.is_empty() {
        return true;
    }
    if !store::valid_model(model) {
        return false;
    }
    match harness {
        "claude-code" => CLAUDE_ALIASES.contains(&model) || rest_of(model, "claude-", b"-"),
        "codex" => {
            rest_of(model, "gpt-", b".-")
                || rest_of(model, "codex-", b".-")
                || (model.len() >= 2
                    && model.as_bytes()[0] == b'o'
                    && model.as_bytes()[1].is_ascii_digit()
                    && model[2..]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
        }
        _ => false,
    }
}

/// The service's bound on one probe. `room-agent` stops its own sandbox
/// earlier (80 s) so that it can remove the throwaway workspace.
pub const PROBE_DEADLINE: Duration = Duration::from_secs(90);
/// At most this much combined output is read (the rest is discarded).
pub const PROBE_OUTPUT: usize = 4 * 1024;
const SYSTEMD_RUN: &str = "/usr/bin/systemd-run";

/// The exact probe argv. The launcher runs in its own transient user scope
/// with the agent unit's memory and task limits: as a child of the agent
/// service it would otherwise count against the service's own 256 MiB bound,
/// which a vendor CLI exceeds.
pub fn probe_argv(paths: &Paths, harness: &str, model: &str) -> Vec<String> {
    let text = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
    vec![
        SYSTEMD_RUN.into(),
        "--user".into(),
        "--scope".into(),
        "--collect".into(),
        "--quiet".into(),
        "-p".into(),
        "MemoryMax=2G".into(),
        "-p".into(),
        "TasksMax=128".into(),
        "--".into(),
        text(super::unit::launcher(paths, harness)),
        "--probe-model".into(),
        model.into(),
        "--harness".into(),
        harness.into(),
        "--profile".into(),
        text(paths.profile(harness)),
        "--bundle".into(),
        text(paths.bundle(harness)),
    ]
}

/// The probe's environment: enough for `systemd-run` to reach the user
/// manager and for the launcher to find `bwrap` and a runtime directory.
pub const PROBE_ENV: [&str; 4] = [
    "PATH",
    "HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
];
pub fn probe_environment(
    ambient: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = ambient
        .into_iter()
        .filter(|(key, _)| key.to_str().is_some_and(|k| PROBE_ENV.contains(&k)))
        .collect();
    env.sort();
    env.dedup_by(|a, b| a.0 == b.0);
    env
}

pub const OK: &str = "The model answered.";
pub const UNAVAILABLE: &str = "The provider does not offer this model to this account.";
pub const SIGNED_OUT: &str = "The provider did not accept the harness sign-in. Sign in again.";
pub const TIMED_OUT: &str = "The probe did not finish in time.";
pub const REFUSED: &str = "The sandbox launcher refused the probe.";
pub const BUSY: &str = "The provider is busy or a usage limit was reached. Try again later.";
pub const UNEXPECTED: &str = "The probe did not get the expected answer.";
pub const FAILED: &str = "The probe failed.";
/// Every probe `detail` sentence (the panel accepts only these).
#[cfg(test)]
pub const SENTENCES: [&str; 8] = [
    OK,
    UNAVAILABLE,
    SIGNED_OUT,
    TIMED_OUT,
    REFUSED,
    BUSY,
    UNEXPECTED,
    FAILED,
];
/// `modelProbe.state` values.
#[cfg(test)]
pub const PROBE_STATES: [&str; 6] = [
    "idle",
    "running",
    "ok",
    "unavailable",
    "not_signed_in",
    "failed",
];

/// Exit status of `room-agent --probe-model` when its own deadline stopped
/// the sandbox (it also prints `probe_timeout`).
pub const TIMEOUT_EXIT: i32 = 124;

// Recognisable vendor error text, matched case-insensitively. Claude Code
// prints `Not logged in · Please run /login`, `Invalid API key · Please run
// /login`, `OAuth token has expired …` or an `authentication_error`; Codex
// `Not logged in`, `401 Unauthorized` or a failed token refresh.
const SIGNED_OUT_TEXT: [&str; 13] = [
    "not logged in",
    "please run /login",
    "invalid api key",
    "authentication_error",
    "oauth token has expired",
    "token has expired",
    "api error: 401",
    "401 unauthorized",
    "status 401",
    "codex login",
    "log in again",
    "could not be refreshed",
    "refresh_token",
];
// Claude Code: `There's an issue with the selected model (…). It may not exist
// or you may not have access to it.` and `not_found_error … model: …`; Codex:
// `The '…' model is not supported when using Codex with a ChatGPT account.`,
// `model_not_found` and `The model `…` does not exist or you do not have access
// to it.`
const UNAVAILABLE_TEXT: [&str; 12] = [
    "issue with the selected model",
    "may not exist or you may not have access",
    "not_found_error",
    "model_not_found",
    "model is not supported",
    "is not supported when using codex",
    "unsupported model",
    "does not exist or you do not have access",
    "invalid model",
    "unknown model",
    "not available on your plan",
    "does not have access to model",
];
const BUSY_TEXT: [&str; 8] = [
    "rate_limit",
    "rate limit",
    "api error: 429",
    "429 too many requests",
    "overloaded",
    "usage limit",
    "hit your limit",
    "api error: 529",
];

fn answered_ok(output: &str) -> bool {
    output.lines().any(|line| {
        line.trim()
            .trim_matches(|c: char| matches!(c, '.' | '!' | '"' | '\'' | '`' | '*'))
            .eq_ignore_ascii_case("ok")
    })
}

/// Reduces a probe's exit status (`None`: killed at the deadline) and at most
/// [`PROBE_OUTPUT`] bytes of combined output to a state and a fixed sentence.
pub fn classify(code: Option<i32>, output: &str) -> (&'static str, &'static str) {
    let text = output.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if code.is_none() || (code == Some(TIMEOUT_EXIT) && text.contains("probe_timeout")) {
        return ("failed", TIMED_OUT);
    }
    if code == Some(0) && answered_ok(output) {
        return ("ok", OK);
    }
    // argparse: `room-agent: error: <category>` before anything ran.
    if text.contains("room-agent: error:") {
        return ("failed", REFUSED);
    }
    if has(&SIGNED_OUT_TEXT) {
        return ("not_signed_in", SIGNED_OUT);
    }
    if has(&UNAVAILABLE_TEXT) {
        return ("unavailable", UNAVAILABLE);
    }
    if has(&BUSY_TEXT) {
        return ("failed", BUSY);
    }
    if code == Some(0) {
        return ("failed", UNEXPECTED);
    }
    ("failed", FAILED)
}

/// Runs the probe with standard output and error on one pipe, without a
/// shell, with exactly `env`. Keeps the first `limit` bytes and discards the
/// rest (so a chatty child never blocks on a full pipe); kills the child at
/// `deadline` (exit status `None`).
pub fn run_merged(
    argv: &[String],
    env: &[(OsString, OsString)],
    deadline: Duration,
    limit: usize,
) -> Result<(Option<i32>, Vec<u8>), &'static str> {
    use std::io::Read;
    let (program, args) = argv.split_first().ok_or("spawn_failed")?;
    let (mut reader, writer) = std::io::pipe().map_err(|_| "spawn_failed")?;
    let error = writer.try_clone().map_err(|_| "spawn_failed")?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(error);
    let spawned = command.spawn();
    // The command holds the write ends; drop them so the reader sees EOF.
    drop(command);
    let mut child = spawned.map_err(|_| "spawn_failed")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = limit.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                }
            }
        }
        let _ = tx.send(kept);
    });
    let until = Instant::now() + deadline;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = rx.recv_timeout(Duration::from_secs(1)).unwrap_or_default();
    Ok((status.and_then(|s| s.code()), out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_accepts_aliases_and_claude_ids_only() {
        for model in [
            "",
            "opus",
            "sonnet",
            "haiku",
            "fable",
            "claude-opus-4-5",
            "claude-sonnet-4-5-20250929",
            "claude-fable-5",
            "claude-3-5-haiku",
        ] {
            assert!(valid_for("claude-code", model), "{model}");
        }
        for model in [
            "gpt-5.5",
            "o3",
            "Opus",
            "claude-",
            "claude",
            "claude-Opus-4",
            "claude-opus-4.5",
            "opus[1m]",
            "sonnet 4",
            "claude-opus/4",
            "claude-opus-4\n",
            "anthropic/claude-opus-4",
            "us.anthropic.claude-opus-4",
            &format!("claude-{}", "a".repeat(64)),
        ] {
            assert!(!valid_for("claude-code", model), "{model:?}");
        }
    }

    #[test]
    fn codex_accepts_gpt_o_series_and_codex_ids_only() {
        for model in [
            "",
            "gpt-5.5",
            "gpt-5.3-codex",
            "gpt-6-pro",
            "o3",
            "o4-mini",
            "o9",
            "codex-mini-latest",
        ] {
            assert!(valid_for("codex", model), "{model}");
        }
        for model in [
            "opus",
            "claude-opus-4-5",
            "gpt-",
            "gpt 5",
            "GPT-5",
            "o",
            "oa",
            "o3.5",
            "openai/gpt-5",
            "gpt-5:latest",
            "gpt-5\t",
            "codex-",
        ] {
            assert!(!valid_for("codex", model), "{model:?}");
        }
        assert!(!valid_for("goose", "gpt-5"));
        assert!(aliases("codex").is_empty());
        assert_eq!(aliases("claude-code"), CLAUDE_ALIASES);
    }

    #[test]
    fn panel_hints_match_the_service_table() {
        // The panel shows the same aliases as chips and applies the same
        // patterns before it sends anything.
        let qml = include_str!("../../../plugin/AgentService.qml");
        assert!(qml.contains(
            r#"readonly property var modelAliases: ({"claude-code": ["opus", "sonnet", "haiku", "fable"], codex: []})"#
        ));
        assert!(qml.contains(r#""claude-code": /^claude-[a-z0-9-]+$/"#));
        assert!(qml.contains(r#"codex: /^(gpt-[a-z0-9.-]+|o[0-9][a-z0-9-]*|codex-[a-z0-9.-]+)$/"#));
        for sentence in SENTENCES {
            assert!(qml.contains(&format!("\"{sentence}\"")), "{sentence}");
        }
        assert!(qml.contains(r#"readonly property var pendingDetails: ["model_not_for_harness"]"#));
        assert!(qml.contains(
            r#"readonly property var probeStates: ["idle", "running", "ok", "unavailable", "not_signed_in", "failed"]"#
        ));
    }

    #[test]
    fn probe_output_is_classified_into_fixed_sentences() {
        let cases: [(Option<i32>, &str, &str, &str); 16] = [
            (Some(0), "OK\n", "ok", OK),
            (Some(0), "Ok.\n", "ok", OK),
            // Codex prints its header on stderr, merged before the answer.
            (
                Some(0),
                "OpenAI Codex v0.158.0\nmodel: gpt-5.5\n--------\ncodex\nOK\ntokens used: 12\n",
                "ok",
                OK,
            ),
            (Some(0), "Sure! Here is a poem.", "failed", UNEXPECTED),
            (Some(0), "", "failed", UNEXPECTED),
            (
                Some(1),
                "There's an issue with the selected model (claude-nope). It may not exist or you may not have access to it. Run --model to pick a different model.",
                "unavailable",
                UNAVAILABLE,
            ),
            (
                Some(1),
                r#"API Error: 404 {"type":"error","error":{"type":"not_found_error","message":"model: claude-nope"}}"#,
                "unavailable",
                UNAVAILABLE,
            ),
            (
                Some(1),
                "ERROR: The 'gpt-9' model is not supported when using Codex with a ChatGPT account.",
                "unavailable",
                UNAVAILABLE,
            ),
            (Some(1), "Not logged in · Please run /login", "not_signed_in", SIGNED_OUT),
            (
                Some(1),
                r#"API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"OAuth token has expired."}}"#,
                "not_signed_in",
                SIGNED_OUT,
            ),
            (Some(1), "ERROR: unexpected status 401 Unauthorized", "not_signed_in", SIGNED_OUT),
            (Some(1), "API Error: 429 rate_limit_error", "failed", BUSY),
            (None, "", "failed", TIMED_OUT),
            (Some(TIMEOUT_EXIT), "room-agent: probe_timeout\n", "failed", TIMED_OUT),
            (
                Some(2),
                "usage: room-agent …\nroom-agent: error: provider_settings_review_required\n",
                "failed",
                REFUSED,
            ),
            (Some(139), "\u{1b}[31m\u{0}\u{7}garbage ###", "failed", FAILED),
        ];
        for (code, output, state, sentence) in cases {
            assert_eq!(classify(code, output), (state, sentence), "{output:?}");
            assert!(PROBE_STATES.contains(&state) && SENTENCES.contains(&sentence));
        }
    }

    #[test]
    fn merged_output_is_bounded_and_the_deadline_kills() {
        let env = [(OsString::from("PATH"), OsString::from("/usr/bin"))];
        let sh =
            |script: &str| -> Vec<String> { vec!["/bin/sh".into(), "-c".into(), script.into()] };
        let (code, out) = run_merged(
            &sh("echo out; echo err >&2; exit 3"),
            &env,
            Duration::from_secs(10),
            PROBE_OUTPUT,
        )
        .unwrap();
        assert_eq!(code, Some(3));
        assert_eq!(String::from_utf8(out).unwrap(), "out\nerr\n");
        // Far more than the limit: kept to the limit, and the child is never
        // blocked on a full pipe.
        let (code, out) = run_merged(
            &sh("head -c 1000000 /dev/zero; echo tail >&2"),
            &env,
            Duration::from_secs(20),
            PROBE_OUTPUT,
        )
        .unwrap();
        assert_eq!((code, out.len()), (Some(0), PROBE_OUTPUT));
        // Only the given environment reaches the child.
        let (_, out) = run_merged(&sh("env"), &env, Duration::from_secs(10), PROBE_OUTPUT).unwrap();
        let seen = String::from_utf8(out).unwrap();
        assert!(
            seen.contains("PATH=/usr/bin") && !seen.contains("HOME="),
            "{seen}"
        );
        let started = Instant::now();
        let (code, _) = run_merged(
            &sh("sleep 30"),
            &env,
            Duration::from_millis(300),
            PROBE_OUTPUT,
        )
        .unwrap();
        assert_eq!(code, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_environment_keeps_only_what_the_launcher_needs() {
        let pairs = |values: &[(&str, &str)]| -> Vec<(OsString, OsString)> {
            values
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v)))
                .collect()
        };
        assert_eq!(
            probe_environment(pairs(&[
                ("ANTHROPIC_API_KEY", "synthetic"),
                ("OPENAI_API_KEY", "synthetic"),
                ("BUZZ_PRIVATE_KEY", "synthetic"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("HOME", "/home/example"),
                ("PATH", "/usr/bin"),
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
                ("WAYLAND_DISPLAY", "wayland-1"),
            ])),
            pairs(&[
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
                ("HOME", "/home/example"),
                ("PATH", "/usr/bin"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ])
        );
    }
}
