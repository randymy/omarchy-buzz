# Sending implementation plan

The development preview implements a narrow human-authored kind-9 sender using
Buzz revision `781d39510cf23cfe224e8f521ae06a23377e06de`.
Upstream paths below are relative to the Buzz repository. Source inspection is
not runtime verification of a deployed relay.

## Upstream contract

`crates/buzz-sdk/src/builders.rs:240` exports:

```rust
pub fn build_message(
    channel_id: Uuid,
    content: &str,
    thread_ref: Option<&ThreadRef>,
    mentions: &[&str],
    broadcast: bool,
    media_tags: &[Vec<String>],
    emoji_tags: &[Vec<String>],
) -> Result<EventBuilder, SdkError>
```

Sign its result once with `builder.sign_with_keys(&keys)?`. Initially use no
thread reference, `broadcast=false`, and empty media/emoji tags. The builder
limits content to 64 KiB and mentions to 50 before deduplication. Mention strings
are lowercased and deduplicated, but the builder does not establish public-key
validity or channel membership. Parse public keys and apply explicit mention
policy in the helper. `ThreadRef` in `crates/buzz-sdk/src/lib.rs:29` contains root
and parent `nostr::EventId`; replies require verified room/thread context.

`crates/buzz-ws-client/src/connection.rs:96` provides
`send_event(Event) -> Result<OkResponse, WsClientError>`. It sends `EVENT` and
waits up to 30 seconds for an exact-ID `OK`. A negative acknowledgement is an
`OkResponse` with `accepted=false`, not a Rust error. `OkResponse` fields are
`event_id`, `accepted`, and `message` (`src/message.rs:51`). Do not await
`send_event` inside the helper's observation loop: its private receive loop
would interrupt COUNT freshness and history/catalog processing. Use upstream
`send_raw(&serde_json::Value)` and route `RelayMessage::Ok` through the existing
connection actor, keeping exact-ID acknowledgement matching in the helper.

The relay's `crates/buzz-relay/src/handlers/event.rs:634` requires authenticated
WS identity and matching message author. Shared ingestion in
`crates/buzz-relay/src/handlers/ingest.rs:2300` verifies signatures, permits
timestamp drift of at most 900 seconds, caps event content at 256 KiB, and checks
author and required scope (`MessagesWrite` for kind 9). The membership gate at
line 807 allows members or open channels; joined-room-only sending is additional
helper policy. Relay authorization remains final. Thread references are resolved
before storage. The duplicate-storage branch at line 3350 returns
`accepted=true`, message `duplicate:` for an already stored event. Authorization
and timestamp validation happen first, so an old retry may be rejected even if
the event was previously stored.

## Implemented sender and acknowledgement

The connection actor accepts a bounded sender command with one pending send. UI input contains only a stable request UUID, canonical room
UUID, bounded text, and validated selected mention public keys. It cannot supply
URLs, headers, raw events, arbitrary kinds, or signing instructions. Fence work
against the current identity/origin generation, fresh connection, trusted relay
pin, and current joined-room catalog. Do not publish while catalog authorization
is unavailable.

Sign once, persist the request-to-event-ID association, then transmit the signed
event. The signed event exists in memory only for transmission; the current preview
does not retain it for retry. A positive exact-ID `OK`
means acknowledged; a negative exact-ID `OK` means rejected. Map reason strings
to static categories instead of forwarding arbitrary relay text. Socket write
completion alone never means accepted.

Once transmission starts, timeout, cancellation, or disconnect means
`delivery_unknown`. The preview offers no retransmission. Any later retry feature must reuse the
original event rather than re-sign automatically. Identical-event deduplication does not establish
exactly-once delivery or agent execution. Preserve COUNT deadlines while waiting
for acknowledgement, and cancel queued work on scope changes.

## Implemented durable ledger; reconciliation deferred

Before publication, persist only request UUID, canonical origin, human public
key, room UUID, event ID, and outcome. Store no private key, message text, raw
signed event, or unkeyed text digest. Signed events themselves contain plaintext;
unkeyed message hashes can expose predictable text through dictionary attacks.
Use a private directory and file, exclusive daemon ownership, atomic replacement,
file fsync and directory fsync. Persistence failure must prevent transmission.

Never sign again for an existing request UUID. While the intent remains in
memory, conflicting reuse is rejected. After restart, unresolved entries remain
unknown and a reused request is refused. A future feature may reconcile records through a narrow authorized exact-event-ID query,
verifying signature, author and room. An absent result is inconclusive. Without
persisted plaintext the original event cannot be resent after restart; creating
a new request must be explicit because it may duplicate an earlier accepted
message. Bound ledger size and refuse new sends when unresolved records exhaust
capacity rather than silently discarding their association.

## Verification and dependencies

`buzz-sdk` is pinned at the same exact Git revision; its `buzz-core` dependency must
remain at that revision. SDK dependencies are core, nostr, UUID, serde,
serde_json, and thiserror. Core adds chrono and crypto/encoding dependencies.
The resolved lockfile and Linux ARM64 build include those dependencies. Other
platforms still require their own build and runtime evidence.

Synthetic tests must verify the actual SDK signature and exercise unrelated-ID
OK, negative OK, lost acknowledgement, unchanged-event retry, scope change,
COUNT responsiveness, persistence failure before EVENT, conflicting request
reuse, restart without re-signing, and unresolved-ledger capacity. An isolated
actual Buzz relay should subsequently verify membership, timestamp and duplicate
storage behavior; mocks cannot establish those policies. Never use production
credentials or invoke agents to validate sending.

## Remaining upstream limits

`connection.rs:121` logs the complete outgoing frame at debug level. Retain the
helper's lack of a tracing subscriber or suppress upstream frame logging before
enabling logging infrastructure. Message text must not reach diagnostics.
The public client does not expose strict WebSocket frame/message and buffered
replay budgets; private acknowledgement loops can accumulate unrelated messages.
See [WS_UPSTREAM.md](../helper/WS_UPSTREAM.md). Actor multiplexing avoids the
publish acknowledgement loop but does not solve upstream frame limits. Do not
claim these resource limits are fixed by the sender.

## Current operational limits

Input is limited to 4096 UTF-8 bytes and 20 distinct canonical mention keys; the
native composer supplies selected keys from the current verified room roster.
Names are optional self-asserted profile hints; typed `@name` text is not resolved. It has no reply/media inputs.
Terminal outcomes preserve the draft except for a matching acknowledgement of
unchanged text. Rejected or reused submissions require an explicit new request;
unknown outcomes require starting a new draft and may already have been delivered.

The ledger is limited to 256 KiB and 1024 records, whichever is reached first,
without automatic pruning. It lives under XDG_STATE_HOME (default
`~/.local/state/omarchy-buzz/delivery`). Capacity/permission failures disable new
sends. Reconciliation, safe archival and same-event retry are future work.
Synchronous bounded fsync avoids cancellation between reservation and publication,
but a stalled filesystem can delay the helper actor. The shell remains separate.
An acknowledgement whose outcome write fails remains acknowledged in that running
helper’s memory; after restart, only durable evidence is available.
