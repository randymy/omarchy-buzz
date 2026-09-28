# Unresolved upstream WebSocket resource limits

The pinned `buzz-ws-client` revision is
`781d39510cf23cfe224e8f521ae06a23377e06de`. The helper retains upstream
NIP-42/protocol/signing; it has not copied or forked its connection implementation.

`NostrWsConnection::connect` uses `connect_async` with its transport defaults.
Its `VecDeque<RelayMessage>` accumulates unrelated frames during authentication
and acknowledgment waits without a frame-count/byte cap. The private socket
prevents callers from supplying stricter Tungstenite limits. Outer deadlines,
helper byte limits and systemd MemoryMax mitigate impact but do not establish
strict pre-auth memory bounds. This limitation remains a release gate.

Proposed upstream API change:

- Add a supported `ConnectionOptions`/`connect_authenticated_with_options` API
  while preserving existing entry points. Options specify overall connect/auth
  deadlines, maximum frame bytes, maximum message bytes, and replay-buffer
  frame and serialized-byte budgets.
- Pass message/frame budgets through `connect_async_with_config`; reject a
  transport message exceeding them before JSON materialization.
- Account buffered frames/bytes on every push, removal and drain, including
  `wait_for_auth_challenge`, `wait_for_ok`, `pending_challenge` and returned
  frames. Overflow closes the connection and yields a typed resource-limit
  error rather than dropping authentication-relevant frames or silently losing
  subscriptions.
- Expose bounded ping/pong acknowledgment if transport liveness is intended to
  avoid database requests, with exact nonce correlation and an overall deadline.
  The current public API exposes no WebSocket ping/pong control.
- Tests exercise oversized frames before parsing, continuous small-frame floods
  during auth/OK waits, buffer drains/accounting, challenge paths, and bounded
  timeout despite irrelevant activity. Disable content logging by default.

Current helper freshness uses supported NIP-45 `COUNT` against its own kind-0
profile, correlated by exact request ID, initially and every20 seconds with a
five-second response deadline. It claims authentication freshness only after the
matching acknowledgment. Unrelated activity cannot extend this deadline. This
is neither presence nor process health. Network failures retry five times with
1/2/4/8/16-second delays; authentication, storage and protocol failures require
explicit Retry. Successful matched probes reset the network failure budget.

## Prepared contribution, 2026-09-28

A concrete [upstream patch and reproducible test harness](../docs/upstream/WS_RESOURCE_LIMITS.md)
now implement bounded transport/replay/output and operation deadlines. All 16
isolated client tests pass. The official current client files are unchanged at
`ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43`. The patch has not been submitted,
merged or adopted by the installed helper; this release gate remains open.
