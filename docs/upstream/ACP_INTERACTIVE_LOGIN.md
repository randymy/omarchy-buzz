# Reuse upstream authentication with a bounded terminal handoff

Status: local proposal against Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`, not installed or submitted.
`acp-interactive-login.patch` is an **alternative** to the smaller
`acp-terminal-auth.patch`; do not apply both. The permission-mode and WebSocket
proposals are separate.

## Existing Buzz code we reuse

Buzz Desktop already implements account connection in
`desktop/src-tauri/src/commands/agent_auth.rs`: `discover_acp_auth_methods`,
`connect_acp_runtime_blocking`, `uses_terminal_auth`,
`run_claude_subscription_login` and `launch_terminal_auth`. It discovers methods
through `buzz-acp auth-methods`, routes ordinary methods to `authenticate`, and
handles terminal methods itself. The standalone harness lacks that last path.
This is a distinction between two upstream entry points, not absence of Buzz
authentication generally.

The desktop implementation is useful as a workflow reference, but should not
be copied verbatim into QML or the plugin helper. Its launcher accepts an
adapter-supplied command (including the legacy `_meta.terminal-auth` extension),
uses terminal-specific shell launchers, and some paths report a launch rather
than wait for login completion. The current
[ACP terminal contract](https://raw.githubusercontent.com/agentclientprotocol/agent-client-protocol/main/docs/protocol/v1/draft/authentication.mdx)
derives the executable from client configuration, adds descriptor arguments,
waits for success and reconnects. The proposal implements that contract in the
upstream harness so all downstream clients can reuse it.

Provider authentication stays in the existing adapter/native CLI. No OAuth,
token refresh, credential copying or provider-specific login protocol is added.
Codex advertises ordinary agent-owned ChatGPT/API methods; Claude advertises
terminal subscription/Console methods. See
[Codex source](../CODEX_ADAPTER_SOURCE.md) and
[Claude source](../CLAUDE_ADAPTER_SOURCE.md).

## Proposed behavior

- Ordinary ACP clients advertise no terminal capability. The CLI enables it
  when a foreground controlling terminal is usable. Discovery also accepts an
  explicit `--terminal-auth` assertion from an external terminal-capable UI;
  the existing desktop discovery caller supplies it to preserve its behavior.
- Normalize configured arguments before resolving npm-shim symlinks. Resolve
  the configured executable once. A descriptor cannot replace that executable.
- Require a unique advertised method ID. Absent type follows ACP's `agent`
  default. Explicit unknown/null types, invalid arguments and duplicate IDs fail.
- For a standard terminal method, shut down discovery, launch the same adapter
  with base arguments plus login arguments, wait, then initialize a fresh ACP
  connection. Never send ACP `authenticate` for this path.
- Restrict descriptor environment additions to `ACP_INTERACTIVE_LOGIN=1`.
  Other additions are explicitly unsupported. Discovery, login and reconnect
  use an allowlisted inherited environment; API-key, loader and raw-adapter-log
  variables are not inherited. This is intentionally not an API-key injection
  interface. Native login stores/settings can still affect effective billing.
- Child stdio attaches to the validated controlling `/dev/tty`, with its own
  foreground process group. The parent restores foreground ownership and
  terminal settings and kills the remaining group on exit, timeout or handled
  cancellation. It waits for the direct child. Errors are fixed categories;
  interactive output stays in the user's terminal, outside QML and logs.

The environment restriction can make previously inherited provider overrides
unavailable. That is surfaced as failure, never fallback to another payer.
The existing native credential store is still used; no home/config directory
is copied. This proposal does not guarantee effective subscription mode from
successful login or initialize alone.

## Validation and limits

The staging tool copies immutable Git files and applies only in a new directory.
The fixture compiles the **exact patched `run_authenticate` and terminal module**.
Its ACP client is a test double; the terminal process and OS lifecycle are real.
No real adapter, account, relay or provider is contacted.

```sh
python3 scripts/prepare-acp-terminal-contribution \
  --buzz-source ../buzz --output /tmp/buzz-acp-terminal-review
cargo test --locked --manifest-path /tmp/buzz-acp-terminal-review/policy-test/Cargo.toml
cargo build --locked --manifest-path /tmp/buzz-acp-terminal-review/policy-test/Cargo.toml
python3 tests/acp_terminal.py \
  /tmp/buzz-acp-terminal-review/policy-test/target/debug/buzz-acp-terminal-contribution
```

Twelve PTY/process cases cover success, interactive input, nonzero exit, timeout,
SIGTERM cancellation, remaining child-group cleanup, reconnect failure,
command/environment injection rejection, duplicate methods, ordinary agent auth
and no-terminal refusal. They check that terminal flow never sends ACP
`authenticate`, that reconnect happens only after successful login, and that
terminal ownership/settings are restored. Two unit tests cover descriptor
bounds and types. An independent review identified mismatched inherited TTYs;
attaching the child to the validated controlling terminal fixes that case.

The original fixture is Linux ARM64 evidence. The full ACP integration follow-up
below adds Linux x86_64 coverage. Neither builds the whole Buzz/Tauri workspace
or tests actual provider sign-in. macOS needs its own PTY checks;
non-Unix terminal execution is unsupported. Existing desktop login execution
remains upstream's implementation; its command-metadata behavior is not silently
replaced by this patch. Upstream should consolidate both entry points on a
reviewed shared executor after full integration tests.

Process-group cleanup is not a sandbox: a hostile executable can deliberately
escape its group, and SIGKILL of the supervisor cannot run Rust cleanup. Other
Buzz launch transformations and custom adapters need compatibility review.
Global ACP version negotiation, safe persistent Buzz-key input, automatic tool
approval, effective-payer verification and provider credentials remain separate
gates. No real-agent support or public-release readiness is claimed here.

## Full ACP integration follow-up

[Manual run 36476363859](https://github.com/randymy/omarchy-buzz/actions/runs/36476363859)
passed on Ubuntu 24.04 x86_64, using Rust 1.95.0. It applied this patch together
with the WS resource-limit and permission-mode proposals to a disposable checkout
of the immutable base, built the complete `buzz-acp` crate and compiled its full
unit-test target. Two terminal descriptor tests and the existing initialize
format test passed; 966 other unit tests were not run (967 and 968 were filtered
respectively in the two targeted invocations).

Nine black-box tests then exercised the actual `buzz-acp` binary and ACP client
against `tests/upstream-acp-terminal/fake-stdio-peer.py`. They covered terminal
success/input, nonzero exit, cancellation, reinitialize failure, rejected method
descriptors, ordinary ACP authenticate, no-terminal refusal and explicit desktop
discovery capability. Caller environment contained only synthetic key markers;
the peer verified those markers were absent from its own environment. These
tests use no Rust client/normalization double.

The source pins, patch hashes and sanitized test summary are retained in
[evidence](../evidence/acp-integration-36476363859.json). The workflow
`.github/workflows/acp-integration.yml` is manual-only. The installed 0.0.6 plugin
and production relay are unchanged. Actual adapters, provider accounts, billing
mode, desktop/Tauri integration and real-agent permissions remain unverified.
