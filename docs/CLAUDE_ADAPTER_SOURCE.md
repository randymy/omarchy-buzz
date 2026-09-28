# Claude Agent ACP: pinned source assessment

Reviewed 2026-09-28 from the public `agentclientprotocol/claude-agent-acp`
`main` commit [`18de37624071b48e95aed9ec5382823e2d72cd39`](https://github.com/agentclientprotocol/claude-agent-acp/tree/18de37624071b48e95aed9ec5382823e2d72cd39).
The source package identifies itself as version `0.82.0` in
[`package.json`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/package.json#L1-L18).
This is source inspection only. No adapter was installed or run; no account,
credential store, configuration, environment values, or login status was read.
It does not certify the package published to npm or the binary on this host.

## What the adapter actually offers

[`initialize`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2381-L2509)
selects auth methods from the *client's* terminal capability. In a local
environment, it can offer `claude-ai-login` (“Claude Subscription”) with
`--cli auth login --claudeai` and `console-login` (“Anthropic Console,” API
usage billing) with `--cli auth login --console`. Both have ACP type `terminal`.
In a remote environment it instead offers `claude-login` with `--cli`, intended
for the native `/login` TUI. These methods are omitted when neither standard
`auth.terminal` nor the adapter's nonstandard `_meta.terminal-auth` is true;
`--hide-claude-auth` suppresses the Claude subscription choices. The
nonstandard extension includes a `command` field and should not be treated as
the standard terminal method descriptor.

The adapter's ACP
[`authenticate`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2622-L2664)
only handles legacy `gateway` and `gateway-bedrock` methods. It does not
perform either Claude or Console login. Terminal login needs the client to
launch the configured adapter executable with the method arguments and then
reinitialize, as described by the [ACP authentication draft](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/draft/authentication.mdx).
Buzz's current harness advertises terminal capability but sends ACP
`authenticate` for any advertised ID; see
[AGENT_AUTH_COMPATIBILITY.md](AGENT_AUTH_COMPATIBILITY.md) and the local
[fail-closed upstream proposal](upstream/ACP_TERMINAL_AUTH.md). That proposal
removes a false capability; it does not implement subscription login.

The terminal wrapper in
[`src/index.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/index.ts#L1-L39)
forwards `--cli` arguments to a native Claude binary. By default the adapter
resolves the binary shipped as a platform-specific optional dependency of the
Claude Agent SDK; `CLAUDE_CODE_EXECUTABLE` can override it. The same resolution
is used for the adapter's CLI auth probe and its SDK session executable
([`claudeCliPath`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L1716-L1761),
[`createSession`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8690-L8702)).
Therefore a separate `claude` executable on PATH is not proof of which
native binary this adapter will run.

## Credential choice and evidence boundary

The adapter does **not** expose an ACP selector that pins a normal Claude
session to subscription versus Anthropic API billing. The terminal method
chooses a *login flow*; later sessions inherit process environment, user and
project/local settings, per-session options, and any client provider override
([`createSession`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8602-L8701)).
Its code explicitly treats an `apiKeyHelper` or `ANTHROPIC_API_KEY` as able to
outrank a stored subscription, and treats third-party backends and tokens as
separate credential classes
([`auth-status.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/auth-status.ts#L194-L246),
[`hide-claude-auth.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/hide-claude-auth.ts#L58-L127)).
The `--hide-claude-auth` option is a **one-way** guard to refuse subscription
use, with a documented detection window; it is not a matching API-key guard
for an explicit subscription preference
([`hide-claude-auth.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/hide-claude-auth.ts#L1-L39)).

There is useful *status*, but it is not a synchronous payer guarantee.
[`auth-status.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/auth-status.ts#L1-L47)
defines a push-only `_auth/status_update` with kinds `account`, `api_key`,
`gateway`, `external`, and `none`. The adapter asynchronously probes its
resolved Claude binary with `auth status --json` at initialize and prompt
start, and can use an SDK session `AccountInfo` when populated
([`probeCliAuthStatus`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2750-L2827),
[`fromAccountInfo`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/auth-status.ts#L122-L190)).
The mapping prefers an API-key source over a simultaneous subscription plan.
Probe failure can mean **no push**, and the probe is not awaited before a
prompt. It can therefore be absent or stale at the moment a turn starts.

More decisively, when `providers/set` reroutes model traffic, the extension
intentionally continues reporting the **agent-owned login**, not the active
provider's credential. The adapter says so explicitly in
[`publishSessionAccountIdentity`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2698-L2737).
Thus even a recent `kind: account` cannot by itself prove that a particular
turn was charged to the Claude subscription. The status payload also has no
per-turn charge receipt or refusal to run on a mismatched payer. Treat it as
an informative identity signal, and preserve `unknown` when it is missing or
when routing/settings could change the effective credential. An explicit
no-fallback workflow still needs a supported effective-mode check at the
actual session/turn boundary, with provider overrides and credential precedence
accounted for.

## Linux ARM64 packaging

The source package is a Node executable requiring Node `>=22` and pins
`@anthropic-ai/claude-agent-sdk` `0.3.280`
([`package.json`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/package.json#L1-L72)).
Its lockfile lists optional `linux-arm64` **glibc** and `linux-arm64-musl`
native packages at that SDK version, each with a platform-specific npm tarball
and integrity hash
([`package-lock.json`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/package-lock.json#L50-L125)).
The resolver prefers the libc-matching package and errors if neither optional
binary is installed, unless `CLAUDE_CODE_EXECUTABLE` supplies an alternative.
This establishes a source-level ARM64 distribution path, **not** local
installation, executable verification, native login compatibility, or an ACP
run on this machine. An exact npm artifact and native binary still need to be
pinned and tested before an ARM64 support claim.
