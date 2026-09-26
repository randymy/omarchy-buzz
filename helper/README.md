# M1 helper

This Linux Rust helper supplies identity setup, authenticated connection status,
and bounded local IPC. It does not send chat, query rooms, subscribe to room
history, launch agents, or implement approvals. `authenticated` means NIP-42
accepted the identity; it does not mean room synchronization completed.

Commands:

- `omarchy-buzz inspect`: public configuration only.
- `omarchy-buzz setup relay wss://relay.example`: saves a canonical origin;
  changing origin clears the configured identity reference.
- `omarchy-buzz setup identity enroll`: explicit interactive terminal only;
  hidden input, no key arguments/environment/files. Stores the submitted key
  in Secret Service `omarchy-buzz.identity.v1`, keyed by origin and public key.
  It never reads Buzz Desktop's namespace or creates a new identity.
- `omarchy-buzz daemon [--keep-running]`: standalone or systemd socket-activated.
- `omarchy-buzz ui-bridge`: bounded newline JSON stdin/stdout proxy.

Config is `XDG_CONFIG_HOME/omarchy-buzz/config.toml` (default `~/.config`).
Socket is `XDG_RUNTIME_DIR/omarchy-buzz/control.sock`; parent is 0700, socket
0600, and daemon/bridge check same-UID peers. This does not isolate same-UID
malicious processes. A standalone daemon refuses an existing socket; remove a
stale socket manually only after confirming no daemon owns it. An inherited
socket must use that exact path and permissions. Without `--keep-running`, the
daemon exits after 30 seconds without a UI client. Authentication errors require
an explicit retry. The daemon reads configuration on startup; restart after setup.

Protocol version 1 requests are JSON lines with exactly `version`, `id`, `type`;
allowed types are `get_snapshot`, `subscribe`, `retry_connection`. Request IDs
are 1–128 ASCII alphanumeric, underscore or hyphen. Lines including newline
are capped at 64 KiB. Eight clients maximum; output writes time out after ten
seconds. Invalid requests receive a category-only error and the connection closes.
Hello/status responses expose public identity/origin, helper instance/generation,
connection category, and capability `connection_status`. No raw relay content,
errors or credential material is forwarded. UI EOF closes the bridge connection.

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
