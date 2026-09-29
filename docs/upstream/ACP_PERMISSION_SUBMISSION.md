# ACP explicit permission-mode contribution packet

Status: prepared for review, not submitted or applied to the product. The
one-file proposal is [`acp-permission-mode.patch`](acp-permission-mode.patch).
The expanded patch and complete staged series apply cleanly to official Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. The earlier policy-only
patch applied to `4ef23609b7025bc356a9ea078834d57b69ec33cf`;
recheck that newer base before submission.
The `buzz-acp` and `buzz-ws-client` trees have no changes between that revision
and `8519db1532efd6cda8f72bb6454c00fc3f87cfba`. The applicability check
used an isolated `/tmp` extraction and did not modify either Buzz checkout.

## Proposed upstream scope

In `crates/buzz-acp/src/pool.rs`, reject an explicitly selected nondefault
mode when `session/new` does not advertise its exact ID. Propagate every
`session/set_config_option` error so a failed setter cannot release a prompt
under the adapter's default policy. Preserve the current timeout, default mode,
and existing tool-permission behavior. Do not claim a successful setter reply
proves the provider's effective policy. Do not bundle the separate tool-request
denial, key isolation, subscription, or WebSocket proposals into this PR.

The proposed patch does not alter the current `bypass-permissions` default or
automatic `allow_once` path. Those remain separate blockers for a safe room
agent. It is useful upstream hardening, not permission approval or an agent
release on its own.

## Open upstream overlap

- [PR #7487](https://github.com/block/buzz/pull/7487) changes the same `pool.rs`
  startup path to report requested/advertised permission IDs and returned
  state. Its description explicitly retains silent unsupported-mode skipping.
  This proposal changes that behavior, so review should decide whether to
  integrate the fail-closed rule there or land this narrow patch first. Rebase
  after either merges; do not present the proposals as independent behavior.
- [PR #4626](https://github.com/block/buzz/pull/4626) adds production-path
  tests for the existing advertisement gate and setter timeout. Its negative
  test expects an unadvertised mode to emit no setter request; the proposed
  error preserves that wire behavior but needs an assertion that session
  preparation aborts before a prompt.
- [PR #7797](https://github.com/block/buzz/pull/7797) addresses owner decisions
  for ACP tool permission requests. It touches `pool.rs` but does not resolve
  explicit startup mode fallback. Keep the concerns separate during review.

## Current validation and remaining submission checks

The patch now contains four fake-peer tests through the actual
`run_isolated_prompt` path. At the isolated official `781d395` base,
`cargo test --locked -p buzz-acp explicit_permission_mode_integration_tests --lib`
passed 4/4: missing, malformed and unadvertised modes never prompted; an
advertised mode rejected by the setter or timed out before prompting; an
accepted `plan` sent exactly one setter with the expected session ID and mode,
then one prompt. Exact request-method sequences also rule out fallback and
broader-mode retry. This used a scripted ACP subprocess, no relay or model.
The final patch differs from that tested source only by Rust formatting and
test-module placement. The patched `pool.rs` passed Rust 1.95 `rustfmt --check`.
The local toolchain lacks `cargo-fmt`, so the package-wide Cargo format command
was not run.

The staged WS, permission, interactive-login, key-isolation, harness-reply,
tool-denial and auth-timeout patches all passed `git apply --check` and applied
in that order on a fresh disposable `781d395` checkout. A combined `buzz-acp`
test compile reached its final link, which failed because the temporary
filesystem ran out of space. The temporary build directories were removed;
this is not a passing combined test or a source compilation failure.

Before submission:

1. Recheck the expanded patch on the intended official base, then run the
   package-wide format, full `buzz-acp` test and Clippy gates with its locked
   toolchain in a runner with adequate disk space.
2. Recheck the branch against the then-current head of PRs #7487 and #4626,
   reconcile their assertions and source overlap, and run the repository's
   required CI. The older four transport-double tests in
   `tests/upstream-acp-permissions` remain separate from the new production
   path tests; neither test set establishes effective provider permissions.

Do not merge or adopt a changed ACP dependency in the product until the actual
combined policy and room-agent boundaries have been reviewed and exercised.
