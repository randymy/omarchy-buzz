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
explicit Retry, with one exception: when the relay rejects a mid-session
re-authentication of a connection that had authenticated (observed transiently
on September 30), the helper reconnects on the same five-attempt budget and
shows `connecting` with category `auth_rejected` meanwhile. Rejections of those
reconnects stay in the same budget; once it is spent the state is
`disconnected`/`auth_rejected` until Retry. A rejected first authentication after
start or Retry (a wrong or revoked identity) still stops at once. Successful
matched probes reset the failure budget and end the automatic retries.

## Prepared contribution, 2026-09-28

A concrete [upstream patch and reproducible test harness](../docs/upstream/WS_RESOURCE_LIMITS.md)
now implement bounded transport/replay/output and operation deadlines. All 21
isolated client/consumer tests pass. The official current client files are unchanged at
`ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43`. The patch is submitted as [draft PR #7976](https://github.com/block/buzz/pull/7976).
It has not been merged or adopted by the installed helper; this release gate
remains open.

## Live subscription mitigation, 2026-09-30

Branch `live-updates` keeps one `REQ` open for the selected room
(`helper/src/live.rs`), which widens what the relay may send while the pinned
client is inside `authenticate`. The helper therefore:

- Sends `["CLOSE", id]` for the live subscription and marks it down before every
  mid-session `authenticate` (the relay's `AUTH` arm in `auth.rs`). It re-arms only
  after the exact liveness `COUNT` restores freshness and a new verified head page
  arrives. The initial `connect_authenticated` runs before any subscription exists.
  Sends and DM opens already use `send_raw` with `OK` matching in the observer loop,
  never `send_event`/`wait_for_ok`, so no other upstream wait buffers frames.
- Closes the subscription on room change, `fetch_recent`, `Retry`, shutdown and
  every connection exit, and after more than 200 unverifiable frames in 60 s
  (5-minute pause, polling meanwhile).

What this bounds: during an `authenticate` wait only live frames already in
flight when `CLOSE` was written can join the replay queue (the pinned relay
deregisters the subscription before answering `CLOSED`, `handlers/close.rs:15-32`).
Outside authentication, live frames are consumed one at a time by `next_event`;
each is at most one Schnorr verification of an event under 64 KiB and at most one
debounced page refetch (300 ms, 2 s when busy).

What it does not bound: the upstream pre-auth `VecDeque` still has no frame or
byte cap, transport limits are still Tungstenite's defaults, and a relay can still
send unsolicited frames or large frames during that window. An unparseable
`EVENT` frame still fails `next_event` and drops the connection
(`relay_protocol_error`, explicit Retry), as any malformed frame did before. The
unit's `MemoryMax=256M` remains the hard memory bound, and this release gate
stays open until an upstream option such as draft PR #7976 is adopted.
