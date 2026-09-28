# Proposed upstream contribution: bounded Buzz WebSocket connections

Status: **local patch, not submitted or adopted**. The installed helper remains
0.0.5 against Buzz `781d39510cf23cfe224e8f521ae06a23377e06de` with the
existing release gate. No Buzz fork, deployed relay change or dependency-pin
change is involved. The patch is Apache-2.0, matching Buzz and this repository.

Official Buzz refs were refreshed on 2026-09-28. Its main branch is now
`ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43`; the WebSocket client, ACP harness
and managed-agent integration paths are unchanged from our pinned source.
The patch applies to those unchanged client files. Source hashes and the test
base are recorded in `tests/upstream-ws/provenance.json`.

## Problem and resulting behavior

A relay can flood unrelated messages while the current client waits for AUTH
or an OK receipt. The replay queue has no count or byte budget, callers cannot
set transport frame/message limits, and ping chatter can extend a receive call.
The proposed `ConnectionOptions` API gives callers explicit finite budgets while
preserving the existing connection entry points with finite defaults.

The patch:

- Passes frame/message limits to Tungstenite before JSON parsing, including
  aggregate fragmented-message limits.
- Tracks replay message counts and original UTF-8 wire bytes through all
  enqueue, removal and drain paths. Overflow drops the socket immediately,
  clears buffered state and returns a typed `ResourceLimit` error.
- Limits AUTH challenges consistently to 1024 bytes, and delivers a challenge
  buffered during an OK wait exactly once rather than duplicating it through
  both pending and buffered state.
- Bounds the entire connect, authenticate, receive, publish, write and graceful
  close operations. Ping/pong work cannot extend an outer operation deadline.
- Uses a bounded JSON writer for outbound frames and logs only byte counts,
  never outgoing frame contents.

Defaults: 1 MiB per frame/message, 256 buffered messages, 8 MiB buffered wire
bytes, 20-second connect, 40-second total authentication, 10-second standalone
write and 5-second close. Publish keeps its existing 30-second constant but now
includes the write in that deadline. Existing per-phase AUTH constants remain.
These are proposed policy choices for upstream review, not newly certified
production thresholds. Callers with legitimate larger messages can use explicit
options. Adding error variants may require changes to exhaustive downstream
matches even though existing constructor signatures remain available.

Wire-byte accounting is **not exact heap accounting**: parsing and owned types
have overhead, bounded by message size/count. The writer bounds its serialized
output, not the caller's already-created `Value` or `Event`. Generic URL/JSON/
relay errors still follow the existing upstream error API; callers must not log
sensitive errors blindly. The plugin continues to use category-only errors.

## Reproduce the isolated tests

The preparer reads exact committed files from an existing Buzz clone, checks
SHA256 hashes, and applies the patch only inside a new staging directory. It
never edits the clone, installed helper or production lockfile. Use a fresh
output path:

```sh
python3 scripts/prepare-ws-contribution \
  --buzz-source ../buzz --output /tmp/buzz-ws-review
cargo test --locked --offline \
  --manifest-path /tmp/buzz-ws-review/crates/buzz-ws-client/Cargo.toml
```

Rust 1.95 was used. The standalone harness uses a committed dependency lock and
the same upstream dependency declarations, without rebuilding unrelated Buzz
workspace components. Offline mode requires the locked crate sources to already
be cached. Normal Cargo network policy applies if dependencies must be fetched.
Tests use disposable loopback sockets and generated synthetic keys only.

All **16 tests pass**, including the three original constant tests and thirteen
resource cases: both AUTH queue budgets, both OK-wait queue budgets, oversized
frames, oversized fragmented messages, bounded outbound serialization, stalled
handshake, silent auth deadline, ping chatter, successful signed authentication,
exact buffer accounting and single-use buffered challenges. Test function counts
combine some related cases. The patch was reconstructed into a second clean
staging directory and tested there too.

This establishes the isolated client behavior, not full Buzz workspace, desktop,
relay or Omarchy compatibility. Before upstream adoption, run its workspace
checks, review default limits and compatibility, then test the plugin against an
actual upstream commit containing the accepted API. Do not replace the pinned
client with this local patch to declare the release gate passed.
