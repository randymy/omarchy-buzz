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
  It never reads Buzz Desktop's namespace or creates a new identity (the
  panel's `create_identity` request does that, see below).
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
daemon exits after 30 seconds without a UI client. A lost relay connection
(timeout, unreachable relay, resource limit, protocol error) is retried without
limit: 1, 2, 4… seconds up to 30, each jittered by up to 25 % and never above
30, with `status.reconnecting` true meanwhile. The delays start over only after
60 seconds of fresh session. A rejected re-authentication or clock skew is
retried at most five times; a rejected first authentication, invalid
configuration and identity errors require an explicit retry. Retry reloads configuration after setup. A pending keyring operation must finish before a queued reload; repeated Retry does not create duplicate keyring requests.

Protocol version 1 requests are JSON lines with `version`, `id`, `type`;
allowed types are `get_snapshot`, `subscribe`, `retry_connection`, and
`fetch_recent`, `fetch_thread`, `close_thread`, `fetch_recipients`, `search_people`, `send_message`, `open_dm`, and the
setup-assist requests `set_relay` (`url`: 1–2048 bytes, no control characters, then the
same canonical check as `setup relay`) and `create_identity` (no other fields). Setup
requests are accepted only while the connection is `unconfigured`, `disconnected` or
`unavailable` (not while a Secret Service unlock is pending); they answer with a status
frame carrying the saved relay or new public `identity`, or an error with one of
`setup_invalid_relay`, `identity_exists`, `identity_unavailable`, `relay_unavailable`,
`setup_busy`, `setup_not_allowed`, `config_unavailable`. Room requests require a canonical UUID
`roomId` from the current discovered catalog. Sending also requires a canonical
request UUID, current helper `instanceId`/`generation`, text of at most 4096 UTF-8
bytes, and at most 20 distinct canonical mention public keys. An optional `rootId`
requires the current verified selected-room root and matching thread snapshot;
it produces an upstream SDK direct thread reply and is bound in the delivery
ledger. Omit it for a top-level message. Nonempty mentions must appear in the current selected-room recipient snapshot;
the native composer supplies exact keys selected from that list. Typed names
alone are not resolved. Request IDs
are 1–128 ASCII alphanumeric, underscore or hyphen. Lines including newline
are capped at 64 KiB. Eight clients maximum; output writes time out after ten
seconds. Invalid requests receive a category-only error and the connection closes.
Hello/status responses expose public identity/origin, helper instance/generation,
connection category, and capabilities `connection_status`, `room_catalog`,
`room_history`, `message_send`, `thread_send`, `thread_replies`, `room_recipients`, `history_auto_refresh`, `room_activity`, `agent_profiles`, `thread_summaries`, `dm_open`, `older_history`, `live_updates`, `setup_assist`. At most twenty rooms and twenty projected message rows are
returned. Message previews are plain text capped at 768 UTF-8 bytes, with
explicit truncation. No raw events, backend errors or credential material is forwarded. UI EOF closes the bridge connection.

The helper deliberately installs no tracing subscriber: upstream WS debug
payloads must remain disabled. Adding logging must preserve this requirement.
No enrolled identity means no relay authentication attempt. Do not use a real
identity or live relay merely to test IPC.

Author names (`author_profiles` capability): history authors the room roster does
not list are looked up with one signed `{"kinds":[0],"authors":[...]}` read of at
most 50 keys after a history refresh (never on a timer, never while one is in
flight, never for this identity or a roster member). Names are verified,
sanitized like roster names and served in `status.profiles` (at most 200, plain
presentation hints). A key read without a name is not asked again for 10 minutes,
a failed read for 60 seconds. The panel also keeps served names in
`$XDG_STATE_HOME/omarchy-buzz/names.json` (per community, at most 1000, validated on load).

Build with Rust 1.95, the tested toolchain and inspected Buzz pin. An older
minimum supported version has not been established for this resolved lockfile.
Keep the committed Cargo.lock pinned. The helper now selects the system D-Bus
library through keyring's `sync-secret-service` feature. On Arch, install
`pkgconf` and `dbus` before building; the target system must provide
`libdbus-1.so.3` at runtime and a usable session D-Bus/Secret Service provider.
Desktop notifications (`notify` request, `desktop_notify` capability) use the same
`dbus` crate directly, pinned `=0.9.12` as `keyring` resolves it (no new package in
Cargo.lock), to call `org.freedesktop.Notifications.Notify` without any subprocess.
The system-linked ARM64 CI build and isolated keyring test passed in
[run 36621010979](https://github.com/randymy/omarchy-buzz/actions/runs/36621010979)
at source `8928adc`; target-machine linkage, IPC, socket reactivation and private keyring tests also passed on Omarchy.
Generic storage failures report unavailable without asserting that the store
is locked. A lookup still pending after 15 seconds reports identity_access_pending;
only one lookup remains active. Secret Service may itself display an unlock
prompt and wait indefinitely. Daemon runtime shutdown is bounded to two seconds;
interactive enrollment still waits for its prompt. No plaintext fallback exists.
Verify Secret Service and socket activation on the target machine before
claiming support there.

Room discovery uses signed metadata and the configured relay’s NIP-11 `self`
identity, pinned for the daemon lifetime. Periodic exact-ID COUNT responses
bound connection freshness; a relay's `CLOSED` refusal of that probe (busy,
rate-limited) also answers it and keeps the session, except `auth-required:`,
which reconnects. Undecodable relay frames on an authenticated connection are
dropped. A joined-room re-check that only times out or finds the relay
unavailable keeps the last verified catalog and its views; a denial, a changed
relay identity or an invalid answer clears them. Recent history uses a signed NIP-CW query with
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
