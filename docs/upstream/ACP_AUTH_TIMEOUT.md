# Give ACP authentication its advertised ten-minute window

Status: review proposal against Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`; not installed or submitted. Apply `acp-auth-timeout.patch` on that revision. It also applies after the separate `acp-interactive-login.patch` proposal. The patches do not change provider credentials or the terminal login executor.

## Failure and change

`buzz-acp authenticate` wraps `AcpClient::authenticate` in a ten-minute timeout, but `authenticate` calls the shared `send_request` path, whose response wait times out after 60 seconds. A browser or device-code login that completes after one minute therefore fails early. The proposed patch gives only the `authenticate` RPC a ten-minute inner timeout. `initialize`, `session/new`, and other ordinary RPCs keep the 60-second request timeout. The CLI's existing ten-minute outer deadline still bounds the entire authenticate call, including the write phase. The existing 30-second blocked-stdin write limit also remains.

The interactive-login proposal selects terminal methods and runs their separate terminal process; this patch affects its ordinary agent-owned `authenticate` branch. The method selection and terminal process behavior are unchanged.

## Validation done

- `git apply --check` passed against the pinned Buzz checkout.
- In a disposable copy of the files touched by `acp-interactive-login.patch`, that patch applied, and `git apply --check` for this timeout patch then passed.
- No upstream or installed files were edited. No live account, adapter, model, or relay was contacted.

## Focused test proposal

The patch includes two synthetic ACP peer tests. One uses the same 150-millisecond delayed response twice: a 30-millisecond request deadline rejects it, then an injected two-second authentication deadline accepts it with a fresh client. The other withholds the response and verifies that an injected 30-millisecond authentication deadline returns `AcpError::Timeout`; it also pins the ordinary request timeout at 60 seconds. These short test durations exercise the timeout path without waiting ten minutes or using a provider account. They do not establish a measured response after 60 seconds.

On a disposable checkout with the patch applied, run the two new tests and then the full crate suite:

```sh
cargo test -p buzz-acp authentication_can_wait_past_a_short_rpc_deadline
cargo test -p buzz-acp authentication_deadline_is_bounded
env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp -- --test-threads=1
```

The build and tests have not been run for this proposal. Before upstream submission, also exercise the actual `buzz-acp authenticate` command with a synthetic stdio ACP peer that answers after more than 60 seconds, then verify an unanswered peer reaches the ten-minute outer deadline or use a test-only shorter outer deadline. Run repository formatting and required CI gates on the combined patch set. Real provider sign-in and effective billing mode remain separate validation gates.
