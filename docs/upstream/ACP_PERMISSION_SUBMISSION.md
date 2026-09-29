# ACP explicit permission-mode contribution packet

Status: prepared for review, not submitted or applied to the product. The
one-file proposal is [`acp-permission-mode.patch`](acp-permission-mode.patch).
It applies cleanly with `git apply --check` to official Buzz
`4ef23609b7025bc356a9ea078834d57b69ec33cf` (September 29, 2026).
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

## Exact validation still needed before submission

1. On a fresh isolated worktree at the intended official Buzz base, apply only
   `acp-permission-mode.patch`, then run `cargo fmt -p buzz-acp -- --check`,
   `cargo test -p buzz-acp --all-targets`, and `cargo clippy -p buzz-acp
   --all-targets -- -D warnings` with the workspace's locked toolchain.
2. Add a fake ACP peer integration test through the real session-preparation
   path: missing/malformed/unadvertised explicit mode must end before
   `session/prompt`; advertised mode with application error or timeout must do
   the same. An accepted mode must send the exact session ID and mode once.
   Assert no fallback prompt and no broader mode retry.
3. Recheck the branch against the then-current head of PRs #7487 and #4626,
   reconcile their assertions and source overlap, and run the repository's
   required CI. The existing four transport-double tests in
   `tests/upstream-acp-permissions` exercise extracted functions only; the
   preparer is pinned to Buzz `781d395` and is not evidence of a production
   path test at `4ef2360`.

Do not merge or adopt a changed ACP dependency in the product until the actual
combined policy and room-agent boundaries have been reviewed and exercised.
