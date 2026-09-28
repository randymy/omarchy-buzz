# Proposed harness-only relay identity

Status: local upstream contribution proposal, unsubmitted and uninstalled.
Base: Buzz `781d39510cf23cfe224e8f521ae06a23377e06de` **after** the WS limits,
permission-mode and interactive-login proposals, in that order. The
[key-isolation patch](acp-key-isolation.patch) builds on the shared spawn policy
introduced by interactive login; it is not a standalone patch against raw HEAD.

## Problem and supported behavior

The current ACP harness accepts a relay key in argv/environment, writes a Git
keyfile and forwards the key to the adapter and configured MCP server. Those
are explicit upstream capabilities, but conflict with this project's requirement
that relay signing stay outside agent processes and the QML interface.

The proposal adds Linux `--private-key-fd <number>`. The parent supplies one
inherited read-only anonymous pipe containing a key, closes the write end, and passes only
the descriptor number in argv. The harness accepts at most 128 bytes and requires
EOF within three seconds. It rejects stdio descriptors, invalid descriptors,
non-pipes, write ends, malformed keys and oversized/stalled input. It sets
close-on-exec/nonblocking flags, consumes/closes the descriptor, zeroizes its
input buffer, and reports fixed error categories without echoing key material.
The Linux implementation uses `/proc/self/fdinfo` and `/proc/self/fd` with
safe standard-library/nix APIs; it requires a normal procfs mount. Named FIFOs
are rejected. Non-Linux use fails as unsupported. The caller must own the descriptor and not
reuse/close it concurrently or pass duplicate copies into children.

FD input conflicts with legacy argv/environment key input, including the legacy
alias copied by upstream before parsing. It opts into a harness-only identity:

- Skip temporary Git signing-key creation and the managed Git environment.
- Reject configured credential-dependent MCP and custom base-prompt files.
- Omit the built-in prompt that tells agents to authenticate via Buzz CLI.
- Do not generate the current automatic Codex network-widening override.
- Remove known relay credentials and `GIT_CONFIG*` from the adapter environment
  after all inherited, persona and explicit launch overrides.
- Preserve this policy for initial pools, refill, respawn and local task launch.
- Keep relay signing in the existing harness with upstream `nostr::Keys`.

Legacy input retains its existing behavior. This does not migrate Buzz Desktop's
launcher, install a supervisor, or change the production plugin dependency.
Provider subscription authentication is separate and remains in native adapters.
No API-key fallback is introduced.

## Important capability loss and trust limit

This is **not yet a complete room-agent integration**. Current conversation
prompts tell agents to use signed `buzz` CLI commands for replies; ACP text alone
is not automatically posted as a room message. Removing child signing access
also removes those commands. A future upstream harness-owned reply route or
narrow authenticated signing/broker mechanism is needed to restore that product
behavior. Do not claim this patch alone enables the end-to-end demo.

This prevents deliberate key handoff through the audited spawn/MCP/Git paths;
it is not a hostile-agent sandbox. Same-UID processes may have access to other
files, credential services or debugging surfaces. The supervisor still needs
an explicit environment/filesystem/process policy. Unknown external environment
variables are not universally scrubbed, and the parent must not put the same
secret in other variables, arguments or files. Existing provider credentials,
settings, network overrides and tool auto-approval require separate controls.
No effective billing-mode or subscription support claim follows from this patch.

## Review and validation

`python3 scripts/prepare-acp-key-contribution --buzz-source ../buzz --output
/tmp/new-disposable-directory` reconstructs the exact source subset and verifies
all four patches apply together. It does not mutate the upstream checkout.

The manual ACP integration workflow builds the complete patched crate, runs the
pipe-reader and runtime/child-environment unit tests, then exercises actual CLI
validation with public disposable keys. The CLI fixture rejects configuration
before any relay or agent startup, including the valid-key case. Parent-env
markers exercise inherited relay/Git scrubbing in the real-child unit test.
All tests are synthetic; no persistent identity, provider or production relay
is used. Compilation/runtime results will be recorded separately once complete.

## Next upstream increment: harness-owned replies

At the pinned base, `acp.rs::handle_session_update` logs
`agent_message_chunk` text; `pool.rs::run_isolated_prompt` returns only a
`StopReason`. No automatic conversation reply follows from those chunks.
The existing `pool.rs::post_failure_notice` demonstrates the supported signing
path: `buzz_sdk::build_message`, `sign_with_keys`, then
`relay.rs::RestClient::submit_event`. Reuse those primitives rather than give
the agent a general signing credential or invent a second relay protocol.

The next proposal should bind a bounded reply accumulator to the active ACP
session/turn and to the already-authorized incoming room/thread. Accept only
validated user-facing message text, never thoughts, tool output, stale session
frames or agent-selected destination metadata. Publish through the harness on
an explicitly supported completion condition. Cancellation, truncation, protocol
failure and ambiguous delivery must retain distinct states; retry the same
signed event only when safe, never silently create a duplicate response.

Tests must cover wrong-session frames, interleaved agents, oversized replies,
cancel/failure without a success post, threaded destinations, revoked membership,
and ambiguous acknowledgments against a disposable relay. That work is not
implemented here. The existing automatic tool-decision/default bypass behavior
also remains a separate blocker for real-machine tasks.
