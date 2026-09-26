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
fields. Allowed requests currently only read status, subscribe, or retry. There
is no arbitrary command, signing, credential-export, or agent-launch request.

QML launches a local executable using an argv array, never a shell command
derived from relay content. Displayed text is plain text. Raw helper errors and
upstream transport payloads are not logged or forwarded to QML. No tracing
subscriber is installed because upstream debug output can contain signed data.

TLS is required for remote relays. A valid signature proves authorship, not
truth, completeness, privilege, or permission to execute an action. A compromised
relay can withhold or replay information. The current status-only helper does
not ingest room events or grant approvals. Future event projection must verify
signatures and admission rules using upstream primitives before presentation.

## Release gates

- Upstream WS authentication buffers and frame controls need explicit resource
  bounds. Outer deadlines and systemd memory limits are partial containment.
- Authenticated connection acceptance, rejection, reconnect, and transport
  conformance need an isolated relay test; private keyring retrieval alone does
  not prove these behaviors.
- No absolute always-current connection claim: an idle broken link may remain
  undetected until the transport reports failure; heartbeat work is outstanding.
- ACP permission defaults and credential handling require separate review before
  integrated agent launch. This plugin currently launches no agents.
- Future approval controls must defer enforcement to an authoritative backend.

Tests use synthetic identities and isolated configuration, sockets, and keyrings.
Never use private conversations, production identities, or trading/deployment
actions as test fixtures. Report vulnerabilities without including credentials,
private payloads, or unredacted desktop screenshots.
