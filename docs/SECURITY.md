# Security boundary

QML is a presentation surface in Omarchy's existing shell process. It is not a
sandbox, secrets store, agent orchestrator, or authorization service. Malicious
same-user processes can compromise the user's session; socket permissions do
not prevent that. An isolated helper protects shell responsiveness from helper
failures, but cannot make arbitrary QML failures harmless.

The helper accesses one human Buzz identity in its own Linux Secret Service
namespace, `omarchy-buzz.identity.v1`, scoped by canonical relay origin and
public identity. Configuration stores only that origin and public key. Hidden
terminal enrollment accepts an existing identity; it does not inspect Buzz
Desktop's secret blob, generate an identity, or fall back to a plaintext file.
The panel's explicit `create_identity` request is the only path that generates
one (see *Panel setup assist* below).
Enrollment also requires successful bounded relay discovery and refuses a key
whose public identity equals the relay signer, before writing to the secret store.
This guard is not proof that any other supplied key is unexposed or human-owned.

Optional desktop alerts use only already validated activity projections.
They send fixed generic text through `omarchy notification send`, with fixed argv,
no shell interpolation, a five-second process timeout, and a ten-second cooldown.
No message text, room names, or author names enter notifications. Initial/gap
snapshots establish silent baselines. The bounded in-memory observer stores only
public event IDs, scope and a timestamp floor; it is not persistent unread state.
Private keys never enter the UI protocol or command arguments. Secret Service
encryption and availability depend on the user's OS store. Unlock prompts can
remain pending; the daemon bounds shutdown and does not duplicate key lookups.

The bridge and daemon communicate over a private same-UID Unix socket. Messages
use separate limits: 64 KiB requests and 1 MiB responses, the daemon limits clients and write deadlines, and QML
validates protocol version, helper instance, generation, categories, and public
fields. Allowed requests read status, subscribe, retry, fetch a current
catalog room’s history, or submit a bounded plain-text message for that room.
Sending additionally fences the current helper instance and identity generation.
Mention keys must be selected from a current verified room-roster snapshot;
profile display names never choose or replace a recipient key. No avatar or
NIP05 URL is fetched. Self-asserted names do not certify human or agent identity. There
is no arbitrary command, signing, credential-export, or agent-launch request.

QML launches a local executable using an argv array, never a shell command
derived from relay content. Displayed text is plain text. Raw helper errors and
upstream transport payloads are not logged or forwarded to QML. No tracing
subscriber is installed because upstream debug output can contain signed data.

TLS is required for remote relays. A valid signature proves authorship, not
truth, completeness, privilege, or permission to execute an action. A compromised
relay can withhold or replay information. Room metadata must be signed by the
configured origin’s NIP-11 `self` signer, pinned for the daemon lifetime. History
requires signed room-scoped bounds and verified events. Direct-author edits and
deletions are applied; unresolved owner/moderator authority hides affected text.
Whole-page budgets reject excessive auxiliary data instead of dropping edits.
Even valid bounds do not prove the relay disclosed every relevant change.
History is a partial snapshot and is cleared on scope/authentication changes.
The helper grants no approvals.

Sending signs only the upstream SDK’s fixed kind-9 message shape, with a
helper-owned signed request UUID tag distinguishing intentional submissions. The UI cannot
choose event kinds, headers, relay origins, or signing instructions. A private
same-UID ledger stores request UUID, origin, public identity, room, event ID and
outcome before publication; it stores no content, signed event, unkeyed text
hash, or private key. Files are bounded, symlinks/hardlinks refused, writes use
atomic replacement plus file/directory fsync, and one daemon holds an exclusive
lock. Persistence failures prevent new sends. A crash with a pending record
reopens it as unknown; the same request UUID is never signed again.

Socket write completion is not acceptance. Only a matching positive OK is
acknowledged. Lost receipts, authentication changes and interrupted writes
produce unknown delivery, with no automatic retry. Even an acknowledgement does
not certify exactly-once processing or agent execution. Relay reason strings
never cross the UI boundary. QML drafts remain in memory and are cleared on an
identity/community change; same-user access and crash-memory risks still apply.

## Panel setup assist (`setup_assist`)

Two requests let the panel do first-run setup without a terminal:
`set_relay {url}` and `create_identity {}`. They add no authority over a
working session:

- Both are refused (`setup_not_allowed`, or `setup_busy` while connecting or
  while a Secret Service unlock is pending) unless the helper is
  `unconfigured`, `disconnected` or `unavailable`. The connection actor refuses
  them again if one races an authentication, so a connected identity is never
  replaced by accident.
- `set_relay` applies exactly the `omarchy-buzz setup relay` checks (`wss://`,
  or `ws://` only for loopback; no userinfo, path, query or fragment; at most
  2048 bytes) and saves the configuration the same way: a different relay
  clears the identity reference. It never deletes a stored secret.
- `create_identity` refuses when an identity is already configured
  (`identity_exists`) or no relay is set. It generates a key with the pinned
  `nostr` crate, requires bounded NIP-11 discovery to succeed and the key to
  differ from the relay signer (the same `check_then_store` guard as terminal
  enrollment) before writing anything, then stores the secret in
  `omarchy-buzz.identity.v1` under `relay|public key` and saves only the public
  key to the configuration. If the configuration cannot be saved, the fresh
  secret is removed again.
- The secret exists only inside the helper process (zeroized after use). No
  request carries a key; no status frame, error, log line or QML property holds
  one. The reply is the next status frame, whose `identity` is the public key.
  Errors are fixed categories (`setup_invalid_relay`, `identity_exists`,
  `identity_unavailable`, `relay_unavailable`, `setup_busy`,
  `setup_not_allowed`, `config_unavailable`); discovery and keyring details are
  not forwarded. A setup answer lost after 60 seconds is reported as
  `setup_busy`; the status frame shows whether the change was saved.
- Both go through the same one-slot command queue as other requests. QML copies
  only the validated 64-hex public key to the clipboard, on an explicit click.

A same-UID process could already run `omarchy-buzz setup relay`; these requests
do not widen that. They do let such a process switch an offline helper to
another relay. A new identity belongs to no community; joining still requires
an invitation or an open room and is not part of this surface.

## Release gates

- Upstream WS authentication buffers and frame controls need explicit resource
  bounds. Outer deadlines and systemd memory limits are partial containment.
- Authenticated connection acceptance, rejection, reconnect, and transport
  conformance need an isolated relay test; private keyring retrieval alone does
  not prove these behaviors.
- Connection freshness uses exact-ID COUNT probes every twenty seconds with a
  five-second response deadline. This bounds detection of silent failures; it
  does not certify agent health or live message synchronization.
- ACP permission defaults and credential handling require separate review before
  integrated agent launch. This plugin currently launches no agents.
- Future approval controls must defer enforcement to an authoritative backend.

Tests use synthetic identities and isolated configuration, sockets, and keyrings.
Never use private conversations, production identities, or trading/deployment
actions as test fixtures. Report vulnerabilities without including credentials,
private payloads, or unredacted desktop screenshots.

## Local activity and profile hints (0.0.5)

Background room polling reuses the signature-, membership-, and scope-checked
history projection. One activity worker is bounded to 20 catalog rooms and 512
remembered IDs per room; raw bodies are not retained by that tracker. Failed or
revoked rooms lose activity state. Jobs are cancelled on catalog revalidation
and reauthentication. UI counters are memory-only and never authorize actions.
The only new disk preference is a default-off boolean under
`$XDG_STATE_HOME/omarchy-buzz/notifications.json` (fallback `~/.local/state`).
It applies across communities and carries no identity or message data.

Agent hints require a verified room roster and a signed kind-10100 profile by
that exact key. Names remain self-asserted; profile status/owner/permission fields
are not authority. No process is launched, no approval is granted, and no
provider credential is requested. Detailed ACP gates are in ACP_READINESS.md.
