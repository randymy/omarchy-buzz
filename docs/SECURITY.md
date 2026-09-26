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
Private keys never enter the UI protocol or command arguments. Secret Service
encryption and availability depend on the user's OS store. Unlock prompts can
remain pending; the daemon bounds shutdown and does not duplicate key lookups.

The bridge and daemon communicate over a private same-UID Unix socket. Messages
are limited to 64 KiB, the daemon limits clients and write deadlines, and QML
validates protocol version, helper instance, generation, categories, and public
fields. Allowed requests read status, subscribe, retry, fetch a current
catalog room’s history, or submit a bounded plain-text message for that room.
Sending additionally fences the current helper instance and identity generation. There
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

Sending signs only the upstream SDK’s fixed kind-9 message shape. The UI cannot
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
