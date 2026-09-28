# Reject unsupported or failed explicit ACP permission modes

Proposal against Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`.
`acp-permission-mode.patch` changes only `crates/buzz-acp/src/pool.rs`.
It is staged independently for upstream review; it is neither applied to the
installed Buzz checkout nor consumed by the plugin helper.

## Problem and change

When a nondefault mode such as `plan` is requested, current session preparation
silently skips applying it if `session/new` does not advertise it. It also
continues after application-level errors from `session/set_config_option`.
The prompt can therefore run under another policy from the one requested.

The proposal rejects an unadvertised nondefault mode before sending a mode
change, and propagates every mode-change error, including application errors.
The existing timeout remains fatal. There is no retry in a different mode.

This is deliberately a partial correction. It does not change the current
`bypass-permissions` default or automatic `allow_once` decisions. Nor does a
successful setter response certify the adapter's effective permissions. It
adds no human decision channel, authority enforcement or safe-launch claim.
The other gates in [ACP_READINESS.md](../ACP_READINESS.md) still apply.

## Reproduce

```sh
python3 scripts/prepare-acp-permission-contribution \
  --buzz-source ../buzz --output /tmp/buzz-acp-permission-review
cargo test --locked --offline \
  --manifest-path /tmp/buzz-acp-permission-review/policy-test/Cargo.toml
```

The preparer reads source from the immutable Git revision, applies the patch
only in a new directory, and compiles the exact patched support guard and async
setter function, together with the upstream error enum. Transport and mode
objects are explicit test doubles. The test timeout is 15 ms; production keeps
its existing timeout. A source assertion checks that session preparation calls
both functions with error propagation. These checks do not compile the full
harness or execute an agent, relay, login or model.

Four tests cover missing/malformed/unsupported advertisements, an exact supported
mode, successful setter invocation, application/transport errors and an
unanswered setter. The latter cases must return an error without fallback.
Full upstream workspace and fake-peer integration tests remain required before
merging upstream or adopting a new production dependency revision.
