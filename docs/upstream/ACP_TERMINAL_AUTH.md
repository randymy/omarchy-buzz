# Proposed upstream correction: ACP terminal authentication capability

Status: **local, unsubmitted patch** against Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. Its two pure policy/capability tests pass in an isolated harness; the full ACP
harness has not been compiled or run against an adapter, and the patch has not
been applied to the installed helper. The Buzz checkout
remains unchanged. The patch is [acp-terminal-auth.patch](acp-terminal-auth.patch).

## Problem

`crates/buzz-acp/src/acp.rs::build_client_capabilities` currently sends both
`clientCapabilities.auth.terminal=true` and `_meta.terminal-auth=true` to every
adapter. Yet `crates/buzz-acp/src/lib.rs::run_authenticate` accepts any
advertised method ID and sends an ACP `authenticate` request, without inspecting
its type. A terminal method calls for a separate interactive agent invocation;
the current harness has no such flow. The capability therefore invites a
method the command cannot execute correctly. The existing initialize test even
requires this false capability. See [the wider adapter/auth assessment](../AGENT_AUTH_COMPATIBILITY.md).

## Proposed change: fail closed now

The patch sends `auth.terminal=false` and removes the nonstandard
`_meta.terminal-auth` extension while preserving the unrelated goose
notification capability. Before any ACP `authenticate` request, the command
checks that exactly one advertised method has the requested ID and its type is
`agent` or absent (which the [ACP authentication draft](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/draft/authentication.mdx)
defaults to `agent`). It rejects terminal,
unknown, malformed or explicit-null types, missing IDs, and duplicate IDs. It
still prints raw adapter methods for `auth-methods`; listing a
method is not a claim Buzz can execute it. No terminal command, credentials,
agent session, or relay operation is launched by this correction.

The patch updates the initialize-capability assertion and adds synthetic
method-selection tests. `git apply --check` succeeded against the pinned Buzz
checkout. The exact pure selector and capability functions were extracted from the patched
source and compiled with a locked standalone harness; both tests pass. This
does not compile the asynchronous command path or establish end-to-end login.
Upstream should run
the `buzz-acp` unit tests and review the exact adapter method types before
merging. Adapters may stop offering terminal methods when Buzz advertises
`false`; that is the intended honest behavior, not proof that subscription
login now works.

## Later interactive alternative

If Buzz chooses to support terminal authentication, it should implement a
separate bounded interactive process using the configured, trusted agent
executable plus the method-advertised arguments and allowed environment. The
method descriptor must not supply the executable. Buzz should wait for an
explicit successful exit, then spawn and initialize a fresh ACP connection.
A terminal method must not
be sent as an ACP `authenticate` request. The implementation needs cancellation,
TTY handling, child cleanup, provenance of the launched binary, and redacted
errors. Only then should it advertise `auth.terminal=true` or a matching
extension, with synthetic tests for success, cancellation, nonzero exit, and
no ACP authenticate call on the terminal path. This document does not provide
terminal execution or a credential interface.

This correction does **not** establish subscription reuse or billing mode for
Claude Code or Codex. Those require the adapter-specific, explicit
subscription-versus-usage choice and effective-mode evidence described in
[AGENT_AUTH_COMPATIBILITY.md](../AGENT_AUTH_COMPATIBILITY.md). It also does not
resolve relay-key propagation, inherited provider credentials, or agent
permissions in [ACP_READINESS.md](../ACP_READINESS.md).

## Reproduce the pure policy checks

```sh
python3 scripts/prepare-acp-auth-contribution \
  --buzz-source ../buzz --output /tmp/buzz-acp-auth-review
cargo test --locked --offline \
  --manifest-path /tmp/buzz-acp-auth-review/policy-test/Cargo.toml
```

The preparer reads the immutable base from Git, applies the patch only in a new
staging directory and extracts the actual pure functions and selector tests.
It does not copy a substitute implementation, execute an adapter, inspect a login
store or contact a provider. The capability test checks that goose's unrelated
notification capability is preserved. The full existing ACP initialize test is
updated in the patch but is not run by this focused harness.

## Interactive alternative now staged

[ACP_INTERACTIVE_LOGIN.md](ACP_INTERACTIVE_LOGIN.md) describes an alternative
patch that adds the actual terminal handoff and preserves desktop discovery
through an explicit capability flag. It reuses the native adapter's existing
login flow. Apply either proposal to the pinned base, not both. Its real PTY
fixture is stronger lifecycle evidence, but full Buzz/desktop integration and
provider account validation remain unverified.
