# Buzz integration helper

This Linux Rust helper supplies identity setup, authenticated connection status,
joined-room discovery, recent stream snapshots, and bounded local IPC. It does
not subscribe to live room history, launch agents, or implement approvals.
Plain kind-9 sending uses the pinned Buzz SDK and explicit delivery outcomes. `authenticated` means NIP-42
accepted the identity; it does not mean room synchronization completed.

Commands:

- `omarchy-buzz --version`: helper version, protocol and pinned Buzz revision; no config/keyring access.
- `omarchy-buzz inspect`: public configuration only.
- `omarchy-buzz setup relay wss://relay.example`: saves a canonical origin;
  changing origin clears the configured identity reference.
- `omarchy-buzz setup identity enroll`: explicit interactive terminal only;
  hidden input, no key arguments/environment/files. Stores the submitted key
  in Secret Service `omarchy-buzz.identity.v1`, keyed by origin and public key.
  It never reads Buzz Desktop's namespace or creates a new identity.
  Before saving, HTTPS discovery must succeed and the submitted key must differ
  from the relay's signing identity. `identity_is_relay_signer` means a server key
  was supplied; use a personal identity. Discovery failure leaves enrollment unchanged.
- `omarchy-buzz daemon [--keep-running]`: standalone or systemd socket-activated.
- `omarchy-buzz ui-bridge`: bounded newline JSON stdin/stdout proxy.

Config is `XDG_CONFIG_HOME/omarchy-buzz/config.toml` (default `~/.config`).
Socket is `XDG_RUNTIME_DIR/omarchy-buzz/control.sock`; parent is 0700, socket
0600, and daemon/bridge check same-UID peers. This does not isolate same-UID
malicious processes. A standalone daemon refuses an existing socket; remove a
stale socket manually only after confirming no daemon owns it. An inherited
socket must use that exact path and permissions. Without `--keep-running`, the
daemon exits after 30 seconds without a UI client. Authentication errors require
an explicit retry. Retry reloads configuration after setup. A pending keyring operation must finish before a queued reload; repeated Retry does not create duplicate keyring requests.

Protocol version 1 requests are JSON lines with `version`, `id`, `type`;
allowed types are `get_snapshot`, `subscribe`, `retry_connection`, and
`fetch_recent`, `fetch_recipients`, and `send_message`. Room requests require a canonical UUID
`roomId` from the current discovered catalog. Sending also requires a canonical
request UUID, current helper `instanceId`/`generation`, text of at most 4096 UTF-8
bytes, and at most 20 distinct canonical mention public keys. Nonempty mentions must appear in the current selected-room recipient snapshot;
the native composer supplies exact keys selected from that list. Typed names
alone are not resolved. Request IDs
are 1–128 ASCII alphanumeric, underscore or hyphen. Lines including newline
are capped at 64 KiB. Eight clients maximum; output writes time out after ten
seconds. Invalid requests receive a category-only error and the connection closes.
Hello/status responses expose public identity/origin, helper instance/generation,
connection category, and capabilities `connection_status`, `room_catalog`,
`room_history`, `message_send`, `room_recipients`, `history_auto_refresh`, `room_activity`, `agent_profiles`. At most twenty rooms and twenty projected message rows are
returned. Message previews are plain text capped at 768 UTF-8 bytes, with
explicit truncation. No raw events, backend errors or credential material is forwarded. UI EOF closes the bridge connection.

The helper deliberately installs no tracing subscriber: upstream WS debug
payloads must remain disabled. Adding logging must preserve this requirement.
No enrolled identity means no relay authentication attempt. Do not use a real
identity or live relay merely to test IPC.

Build with Rust 1.95, the tested toolchain and inspected Buzz pin. An older
minimum supported version has not been established for this resolved lockfile.
Commit the generated Cargo.lock after dependency resolution. Linux Secret
Service requires a usable session D-Bus/keyring; generic storage failures report unavailable without asserting that the store
is locked. A lookup still pending after 15 seconds reports identity_access_pending;
only one lookup remains active. Secret Service may itself display an unlock
prompt and wait indefinitely. Daemon runtime shutdown is bounded to two seconds;
interactive enrollment still waits for its prompt. No plaintext fallback exists. Compilation, ARM64/runtime Secret Service and
socket activation must be verified before claiming support.

Room discovery uses signed metadata and the configured relay’s NIP-11 `self`
identity, pinned for the daemon lifetime. Periodic exact-ID COUNT responses
bound connection freshness. Recent history uses a signed NIP-CW query with
whole-page limits and verified bounds, edits and deletions; uncertain authority
hides affected content. A valid snapshot does not prove relay completeness.
Explicit room refresh, reauthentication, scope change and disconnection clear history.
After selection, background refresh runs five seconds after the previous request
finishes, with one bounded history fetch at a time. It preserves the current view
while fetching, but clears it on failure or authorization uncertainty. Catalog
revalidation still clears history, then refetches authorized selections. This
keeps the selected conversation current. A separate bounded worker rotates across other catalog rooms, at most one request every five seconds after completion. Per-room summaries contain only a monotonic observed counter and baseline epoch, never message bodies. Baselines and gaps produce no increments; reconnects reset the tracker. This is not global or synchronized unread state.
See [history semantics](../docs/MESSAGING_NEXT.md) for limits and source references.

Sending reserves an event ID durably before any EVENT write. Only a matching
positive relay OK produces acknowledged status. Rejection, unknown delivery,
preflight failure and acknowledgement are distinct. The15-second receipt deadline
runs beside connection probes. Repeated request IDs never produce a new event;
after restart, old request IDs are refused because text was not persisted.
The ledger holds at most 1024 records and 256 KiB; either capacity limit disables
further reservations. There is no automatic pruning or retransmission. Keep the
ledger on upgrade; an archive/reconciliation workflow is still a release task.
Ledger writes use synchronous fsync within the isolated helper: a stalled
filesystem can delay its event loop despite the byte cap. No UI thread performs
these writes. See [sending](../docs/SENDING.md) for failure semantics.

Recipient discovery verifies a relay-signed room roster and current identity
membership, then projects at most 20 keys and optional signed kind-0 display names.
Names are self-asserted hints of at most 64 UTF-8 bytes. Missing, malformed or
conflicting profiles fall back to keys. The helper fetches no avatars or NIP05
URLs. At most ten optional signed kind-10100 profiles label roster identities only as self-described agents with unknown execution state; absence never proves a human identity. Room/scope/authentication changes
clear the snapshot; the sender validates selected keys against the current one.
