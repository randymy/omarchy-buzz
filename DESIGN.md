# Buzz for Omarchy — design proposal

Status: **ready for design review, not an implementation or a release claim**.

Implementation note (2026-09-26): The M1 connection preview runs natively through the installed systemd helper, with hosted/custom setup and no production sample rooms. Twenty ARM64 Rust tests pass, including synthetic NIP-42 authentication and bounded signed HTTP `/query` conformance. Isolated Secret Service enrollment, socket idle reactivation, and QML/helper integration passed. HTTP transport is not yet wired to room discovery or UI. M1 still has upstream WS resource-limit, heartbeat/recovery, relay trust/discovery, and deployed-relay validation gates. No production identity, relay authentication, messaging, or ACP integration has been exercised.

Research date: 2026-09-25 America/Chicago (some upstream commits are dated 2026-09-26 UTC). Scope: community plugin first; vPerps as an independent downstream consumer. No upstream, installed desktop, or vPerps implementation files were changed. No relay, authenticated CLI, or agent was started during research.

## 1. Decision and product boundary

Build one standalone Omarchy plugin plus a small, separately installed Rust helper. The plugin supplies a bar widget, a summoned panel, and a QML service that shares sanitized state. The helper owns the human Buzz connection and identity access. Independently supervised upstream Buzz ACP harnesses connect agents. A future vMachine authority service remains outside all of these components.

Use upstream Buzz libraries where the current CLI cannot meet our credential or streaming requirements. This is a deliberate refinement of the preferred `QML → helper → CLI/supported interfaces → relay` architecture, not a Buzz fork. The helper is a narrow client adapter, not a relay, generic Nostr client, full Buzz desktop replacement, or agent orchestrator.

The first useful release provides joined stream rooms, recent conversation, safe sending with exact-identity mentions, local unread activity, and honest agent activity hints. Detailed agent work state and approvals are later milestones with real upstream dependencies. A green presence dot must never mean an agent is authorized, executing, or healthy.

## 2. Inspection baseline and evidence

### 2.1 Revisions

| Source | Inspected revision | Notes |
| --- | --- | --- |
| [Omarchy](https://github.com/omacom/omarchy/tree/7b336b1b0da722e7bb864a7136f91e784ef731bf) | `7b336b1b0da722e7bb864a7136f91e784ef731bf` | Fresh default-branch clone in `../omarchy`; source `version` says `4.0.0.alpha`. |
| [Buzz](https://github.com/block/buzz/tree/781d39510cf23cfe224e8f521ae06a23377e06de) | `781d39510cf23cfe224e8f521ae06a23377e06de` | Fresh default-branch clone in `../buzz`; workspace version `0.1.0`, Rust minimum `1.88.0`. Version alone is insufficient to identify this rapidly changing interface. |
| [vPerps](https://github.com/vperps/vperps/tree/1a455281aa94c1b9652e0f04b3216c6b35fc1c6f) | fetched `origin/dev`: `1a455281aa94c1b9652e0f04b3216c6b35fc1c6f` | Existing checkout stays at `86b8282ba8fa9129188266abb8963173db22d659`. Overlay is identical across those revisions. Existing untracked `.claude/worktrees/` preserved. |
| [vPerps Local](https://github.com/tendryl-frank/vperps-local/tree/e28f59d84572cd21042f13342e8665acb9d64317) | `e28f59d84572cd21042f13342e8665acb9d64317` | Local HEAD equals refreshed `origin/main`; clean checkout. |
| [vPerps Windows](https://github.com/tendryl-frank/vperps-windows/tree/d013e461fafd761825e932851532ffa6c734a711) | `d013e461fafd761825e932851532ffa6c734a711` | Local HEAD equals refreshed `origin/main`; clean checkout. |

The local machine is ARM64. `omarchy version` reports package version `4.0.3-1`, while `/usr/share/omarchy/version` reports `4.0.0.alpha`. These are different version surfaces, not proof that all Omarchy 4.x installations share one API. Compatibility must identify tested package/source combinations and required interfaces.

Source references below use paths relative to these repositories, at the revisions above. Public upstream links are immutable. vPerps references may require repository access and are supplementary; installing or building the community plugin must never require that access. Conclusions are source inspection, not runtime certification.

### 2.2 Omarchy findings

| Area | Inspected files/interfaces | Design consequence |
| --- | --- | --- |
| Plugin contract | `docs/omarchy-shell.md`, `shell/services/PluginRegistry.qml`, `bin/omarchy-plugin-validate` | Root `manifest.json`; schema version 1; globally unique ID outside `omarchy.*`; relative existing entry points, no internal symlinks. |
| Composition | `shell/shell.qml`, `shell/services/PluginShellApi.qml`, `shell/plugins/services/media/manifest.json` | One plugin can declare `service`, `bar-widget`, and `panel`. Under the built-in bar, widgets use `bar.shell.serviceFor(moduleName)`; panels can receive their matching `service` or use their own scoped shell. Third-party facade is scoped but is not a sandbox. |
| QML contract | `agents/skills/shell-dev.md`, `shell/Ui/BarWidget.qml`, `shell/Ui/PluginBarApi.qml`, `shell/plugins/bar/Bar.qml` | Entry points are `Item`, not a new `ShellRoot`. Panels/services receive declared `shell`, `manifest`, `omarchyPath` properties; widgets receive `bar`, `moduleName`, `settings`, not direct shell/manifest injection. Use native `qs.Ui`/`qs.Commons` components and theme tokens. |
| Panel control | `shell/shell.qml` functions implementing summon/toggle/hide; `bin/omarchy-shell` | Panel implements `open(payloadJson)`, `close()`, and `opened`. Shell IPC can summon independently of the widget and focused monitor. |
| IPC | `omarchy-shell shell toggle <id> '{}'`; `summon`, `hide`, `call` | Use the shell's wrapper and timeout; do not start another shell process. `call` requires a loaded plugin. UI IPC carries navigation only, never credentials. |
| Process I/O | `shell/plugins/panels/tailscale/Service.qml`, `dropbox/Service.qml`, `network/Panel.qml`, `weather/Panel.qml` | `Quickshell.Io.Process`, argv arrays, `stdinEnabled`/`write`, `SplitParser` or `StdioCollector`. Use asynchronous bounded helper output; no synchronous work on the shell thread. |
| Notifications | `bin/omarchy-notification-send` | Native notification command supports app name, replacement ID, and argv-based `--exec` click navigation. Do not reach into a first-party notification service from community QML. |
| Menu | `docs/menu.md`, `default/omarchy/omarchy-menu.jsonc` | User extension is the single `~/.config/omarchy/extensions/omarchy-menu.jsonc`; merge one owned entry by ID, never overwrite the document. Extensions cannot add provider implementations. |
| Keybindings | `default/hypr/bindings/applications.lua`, `utilities.lua`; inspected local `bindings.lua` | Exact `Super+B` is currently free in those sources; `Super+Shift+B` launches Browser and `Super+Ctrl+B` is Bluetooth. Still inspect effective bindings before opt-in installation. |
| Lifecycle | `shell/services/PluginRegistry.qml`, `shell/shell.qml`, `bin/omarchy-launch-shell` | Omarchy watches user plugins and `shell.json`. A normal service is recreated on reload. Avoid `keepLoaded: true` initially because it retains service code until shell restart. |
| Management | `bin/omarchy-plugin-add`, `-update`, `-remove` | Add clones and validates; no dependency installer hooks. Update fast-forwards and validates, with invalid-update rollback. Remove disables/unloads and removes checkout or handles a local/symlink installation appropriately. External systemd units are not automatically managed. |
| Validation | `test/shell.d/plugin-validate-test.sh`, `plugins-test.sh`, `plugin-add-test.sh`, `docs/testing.md` | Manifest validation does not establish QML runtime safety or graphical correctness. Pin the validator in CI and separately test real shell behavior. |

The native panel belongs in the existing shell. An overlay is available but unnecessary for v0.1. A replacement menu/bar is unnecessary. The QML service owns UI state distribution, not network authentication or agent lifecycle. Initial full widget support targets the built-in Omarchy bar: third-party replacement bars intentionally receive service-less widget facades. On an unsupported bar, do not traverse the object tree to bypass the facade or open duplicate backend sessions; show an unavailable/launch-only widget where the host permits it, with the independent keyboard/menu panel still available. Document and test replacement-bar compatibility separately.

### 2.3 Buzz findings: implemented interfaces versus aspirations

Read `ARCHITECTURE.md`, `VISION.md`, relevant protocol documentation, CLI source, relay source, identity storage, and ACP implementation. In several places broad documentation predates current code or the normative protocol contract; the paths below control the proposed integration.

**Relay and identity.** Buzz uses signed Nostr events with ID, author pubkey, kind, tags, content, timestamp, and signature. The configured host selects the community. NIP-42 authenticates WebSocket sessions; NIP-98 authenticates HTTP requests. Access and membership are enforced by the relay. A signature proves a key signed content, not the truth of a claim or authority to execute a local action. See `crates/buzz-core/src/kind.rs`, `crates/buzz-auth/src/nip42.rs`, `nip98.rs`, `docs/multi-tenant-relay.md`, and `crates/buzz-relay/src/api/bridge.rs`.

**CLI.** The package `buzz-cli` produces the executable **`buzz`** (`crates/buzz-cli/Cargo.toml`). At this revision these are real commands, inspected in `src/lib.rs` and `src/commands/{channels,messages,users}.rs`:

```text
buzz channels list --member
buzz channels members --channel <uuid>
buzz messages get --channel <uuid> --limit 50
buzz messages send --channel <uuid> --content - --mention <exact-pubkey>
buzz users get --pubkey <hex>
```

`--content -` reads stdin. The CLI constructs and signs messages with upstream SDK code, resolves names against channel members, checks explicit mention membership, and returns emitted mention keys. The full JSON message format retains signatures; compact output is inappropriate for verification. Errors have typed exit codes, including authentication and write conflict; an ambiguous write may be `delivery_unknown` and must not be retried as a new message. Some paths normalize malformed JSON to an empty list, so exit zero alone is not proof of a valid response.

The current CLI has **no general live watch subscription or read-state command**. It accepts a private key through `--private-key` or `BUZZ_PRIVATE_KEY`; the internal HTTP client module is private, not a supported Rust API. It does not automatically attach to the desktop application's human identity. Consequently it is useful for developer diagnostics and future adapters, but is not the default authenticated backend under this design's secret-handling requirements.

**Reusable Rust interfaces.** `crates/buzz-ws-client/src/connection.rs` exposes `NostrWsConnection::{connect_authenticated, send_raw, next_event, send_event, disconnect}`; `message.rs` exposes typed relay messages and upstream NIP-42 authentication. `crates/buzz-sdk/src/builders.rs` supplies `build_message` and other event builders; `mentions.rs` supplies mention parsing. `buzz-core::nip10::{parse_thread_markers, ThreadMarkers::resolve}` supplies thread interpretation. Use the same pinned `nostr` dependency for signing and event verification. These are source-level interfaces, not a promise of stable independently published packages.

The WebSocket library needs careful integration: it has a pre-response event buffer, logs outbound frames at debug level, and parsing an event does not itself verify its signature. Disable dependency payload logging, verify before projection, enforce deadlines/resource limits, and resolve buffering limits in M1. Do not forward library error strings wholesale to QML or the journal.

**Room history.** Native read models are more than `messages get`. The [normative NIP-CW contract](https://github.com/block/buzz/blob/781d39510cf23cfe224e8f521ae06a23377e06de/docs/nips/NIP-CW.md) defines `POST /query` channel windows (`top_level`, `include_aux`, `include_summaries`) and thread windows. WebSocket `REQ` does **not** implement those HTTP filter extensions. Channel results contain signed rows, edit/delete/reaction auxiliary events, summaries `39005`, and bounds `39006`; thread mode has bounds `39007`. Use the validated bounds cursor, not a timestamp guessed from visible rows. NIP-CW supersedes `docs/bridge-channel-window.md` where they disagree, including scan-position cursor and closure limits. No shared client timeline reducer was found in `buzz-core`; correct projection is explicit adapter work or an upstream reusable-client extraction.

**Unread state.** `docs/nips/NIP-RS.md` and `desktop/src/features/channels/readState/` implement encrypted own-identity read-state synchronization using kind `30078`. It is not a simple relay counter or a receipt of what other users read. `crates/buzz-relay/src/api/bridge/read_state_snapshot.rs` adds a specific full-state snapshot contract. Porting this subsystem into the first helper would exceed a thin MVP. v0.1 provides explicitly local unread activity, never claiming to match desktop/mobile counts.

**Human identity storage.** Desktop uses `desktop/src-tauri/src/{app_state.rs,app_state_keyring.rs,identity_storage.rs,secret_store.rs}`. Depending on build and state it can use the OS keyring, a private local file, or an environment-provided key. The keyring entry is a shared JSON blob of secrets, not a stable external identity-export API. Do not scrape, import, modify, or migrate that blob automatically. Do not treat a locked store as permission to generate a replacement identity.

**Self-hosting and audit.** Production deployment is under `deploy/compose/`; local `just setup` may reuse desktop development databases, so it is not our test-isolation mechanism. The relay uses Postgres, Redis, and object storage. `crates/buzz-audit/src/lib.rs` provides a tamper-evident chain; it does not attest every local agent syscall or prevent a compromised relay from withholding events. The plugin connects to a configured relay and does not install one.

**Architecture support.** `.github/workflows/release.yml` currently publishes Linux x86_64 desktop artifacts. ARM64 strings elsewhere do not establish Linux ARM64 artifact availability. Build and test our helper on ARM64 and x86_64; the Buzz desktop application is optional, so its ARM64 packaging is not a prerequisite for native messaging. ACP adapter availability on ARM64 remains a separate M4 gate.

### 2.4 ACP, activity, workflows, and Git

| Capability | Current reality and exact source | Consequence |
| --- | --- | --- |
| Agent launch | `crates/buzz-acp/src/config.rs`, `acp.rs`, `pool.rs`; default is `goose acp`, other adapters are `codex-acp` and `claude-agent-acp` | Select adapters explicitly. Do not assume raw `codex` or `claude` terminals speak this interface or that existing sessions can be attached. |
| Identity and routing | `lib.rs::resolve_agent_owner`, `filter.rs::match_event`, `docs/agent-profile-identity.md` | Agents have their own keys. Signed `p` tags address exact identities; visible `@codex` alone is insufficient. Default inbound author policy is owner-only. Shared team operation requires deliberate upstream owner/respond-to configuration. |
| Agent pools | `pool.rs` | Multiple subprocess slots can share one identity; they are not automatically separate named agents. |
| Presence/activity | `docs/agent-availability.md`, `relay.rs::build_typing_event`, `buzz-core/src/kind.rs` | Kinds `20001`, `20002`, `40902` express presence/typing/snapshots, not execution authority or guaranteed process health. |
| Agent identity | `buzz-core/src/kind.rs`, `desktop/src-tauri/src/nostr_convert/agent_directory.rs` | Signed kind `10100` is agent self-description; managed `30177` needs ownership validation. A display name or bot badge alone is not verified runtime identity. |
| Detailed telemetry | `crates/buzz-acp/src/observer.rs`, `lib.rs` observer handling, `crates/buzz-core/src/agent_turn_metric.rs` | `ObserverHandle` is an in-process bus. Kind `24200` is owner-encrypted ephemeral telemetry/control; `44200` is owner-encrypted durable end-of-turn metrics. Neither is a public global agent-status feed. |
| Lifecycle controls | `crates/buzz-acp/src/filter.rs`, `lib.rs` | Owner-signed exact control messages and observer controls exist. Request acceptance is not proof of process termination; implement only after exact identity/session authorization is understood. |
| Tool approval | `crates/buzz-acp/src/acp.rs` permission request handling; `config.rs` permission mode | Harness currently selects `allow_once` when offered; default permission mode is `bypass-permissions`. A plugin approval dialog would not intercept this automatically. |
| Workflow approval | `crates/buzz-workflow/src/lib.rs`, `executor.rs`; relay `handlers/command_executor.rs` | Engine marks suspended approval runs failed with `approval_not_supported` (WF-08). Grant/deny handlers existing in isolation do not make end-to-end approvals work. |
| Git | `buzz-sdk/src/builders.rs`, `crates/buzz-acp/src/git.rs`, relay `api/git/policy.rs` | Buzz has repository, issue, PR, patch, and status events, plus relay Git policy. This does not automatically synchronize GitHub or authorize arbitrary external merges. ACP Git setup can write a temporary 0600 Nostr key file; assess that before enabling it. |

A future detailed dashboard can map `turn_started` and `turn_completed` to narrowly defined states for an authorized observer. It must handle missing ephemeral frames and process-local sequence resets. Do not infer completed work from the absence of typing, or thinking/reviewing/waiting-for-approval from prose. More detailed status remains unknown when evidence is unavailable.

### 2.5 vPerps downstream findings

The existing overlay already demonstrates an independent integration: `overlay/README.md`, `install.sh`, `omarchy/extensions/vperps-menu.jsonc`, `omarchy/plugins/vperps.health/manifest.json`, `omarchy/hypr/vperps-windows.lua`, `bin/vperps*`, and `systemd/user/`.

- The overlay merges an owned menu block, installs window webapps, supplies `vperps.health`, and discovers CLI verbs by filename. Buzz can be another independent package consumed by this overlay.
- vPerps Local serves its API on loopback 8080 and a separate stream daemon on 8091; installed wrappers use `VPERPS_LOCAL_DIR`. The tunnel unit is presently a `/bin/true` placeholder. Registry fallback fixtures are not proof of a populated, attributed runtime registry.
- Local's `ai_registry.py` gates tool permissions explicitly; `claude_agent.py` has CLI tool and path restrictions with documented limitations. Inspected runtime lanes are Claude CLI and Ollama; a product reference to Codex does not establish a current Local Codex integration.
- `credentials.py` uses keyring-backed individual credentials, while some existing system/team credentials remain configuration-based. `server.py` loopback/CORS settings are not a hostile-local-process security boundary. No Buzz process should receive those credentials or direct signing access.
- Core `AGENTS.md` requires explicit review requests, prohibits agents placing real orders, and reserves production deployment/migrations for operator approval. A Buzz mention must not bypass those rules. `CLAUDE.md` describes repository-specific attribution/review work; it is not a universal agent-launch protocol.
- `docs/business/VPERPS_LOCAL_PRD.md` describes signed Core, separate signing, delegated keys, immutable images, attestation, and isolated vendor guests. The download overlay explicitly lacks several of these features. No implemented vMachine authority service was established by this inspection; the user-provided vMachine model is the design direction, not a current API.

## 3. Proposed process architecture

```text
Omarchy shell (one existing Quickshell process)
  BuzzBarWidget + BuzzPanel
              │ shared normalized state
         BuzzService.qml
              │ async argv process; bounded JSON lines on stdin/stdout
              ▼
  omarchy-buzz ui-bridge (no credential access)
              │ Unix socket; same UID; versioned request/response/event messages
              ▼
  omarchy-buzz daemon (systemd user service)
      ├── human Buzz identity through helper-owned OS secret-store entry
      ├── local unread state, bounded caches, source validation
      ├── upstream buzz-ws-client + buzz-sdk + buzz-core
      └── narrow authenticated HTTP adapter for supported Buzz query interfaces
              │ NIP-42 WebSocket + NIP-98 HTTPS
              ▼
           Buzz relay
              ▲
              │ independent authenticated identities/connections
    upstream buzz-acp instances (separate supervision and permissions)
              │ ACP stdio
              ├── codex-acp
              ├── claude-agent-acp
              └── goose acp

Later: external vPerps event publishers / vMachine authority adapters
       communicate through supported Buzz interfaces and explicit local adapters.
       They never receive authority from a QML property or a chat badge.
```

The bridge is a subcommand of the same binary, not another language/runtime dependency. It provides framed input and output bounds before Qt parsing and prevents QML from directly implementing socket framing or authentication. The daemon is a single instance per user, with one active community in v0.1. Agent processes are not its children. Explicit helper setup enables a systemd user socket unit: a bridge connection activates the daemon; after the last UI client disconnects the daemon can stop following an idle grace period. The socket unit remains available without a background relay connection. Use restart-on-failure with rate limits, not restart-always, so a normal idle exit remains stopped. Background notifications require an explicit keep-running setting. No second daemon is spawned by QML as a fallback.

### 3.1 IPC contract (new project interface, not an existing Buzz API)

Socket: `$XDG_RUNTIME_DIR/omarchy-buzz/control.sock`, inside a user-owned 0700 directory, mode 0600, with peer UID checks. No localhost TCP server, browser endpoint, bearer token, arbitrary shell command, or generic signing method. The daemon obtains its endpoint from local setup, never from relay content.

Use a versioned JSON envelope carrying `version`, request ID or sequence, helper instance ID, community generation, message type, and typed payload. Initial hello negotiates capabilities and returns sanitized public identity, configured origin, and compatibility state. Responses from an earlier instance/community are ignored.

v0.1 allowlist: `get_snapshot`, `select_room`, `fetch_recent`, `send_message`, `mark_visible_read`, `retry_connection`, and `subscribe`. `send_message` accepts bounded text, a selected room ID, exact mention IDs, optional validated reply target, and local request ID. It does not accept a raw event, URL override, private key, arbitrary event kind, executable, or shell string. Configuration and identity setup happen outside this protocol.

Proposed limits to validate in M1: 64 KiB JSON line, 16 KiB composed message, 50 visible messages per page, 200 retained messages per selected room, bounded room/profile catalogs, at most 20 monitored rooms by default, and coalesced UI updates no faster than 10 Hz. Send snapshots in bounded chunks with a generation and explicit completion marker, not one arbitrarily large JSON line. Truncate previews with an explicit indicator, not signed payloads before verification. Larger supported upstream payloads can be reported as unavailable in the compact panel. Over-budget data or IPC clients produce a partial/error state; never unbounded memory growth. Limit bridge output even if the daemon misbehaves. Daemon resource limits protect the desktop against library-level buffers as well as our own caches.

### 3.2 Native UI

One service, one bar-widget kind, one panel kind. Tentative generic ID: `community.buzz`; final namespace uniqueness must be checked against the marketplace before M0 freezes it. Repository hosting under an organization does not require trading branding in the ID or product UI.

The bar combines a connection glyph, local unread count/partial marker, and a restrained activity mark. Tooltip explains the source: e.g. “Connected · 3 unread here · Codex active recently.” Error status takes precedence over stale green indicators. No permanently flashing animation and no “approval required” state without a supported authority adapter.

The panel displays community, identity, connection freshness, room list, recent messages, timestamps, sender names with key disambiguation, agent self-description badges, and a composer. Replies are labeled and use upstream thread markers. Initial history is one bounded page, with later navigation through explicit fetches or the optional upstream app. It is not a full forum, file browser, huddle UI, or workspace administrator.

Use plain-text rendering in v0.1. Remote HTML, QML, rich-text image tags, remote avatars, and auto-fetched previews are not executed or loaded. Render links as inert text until a locally allowed navigation action exists. Notification contents are similarly bounded.

Proposed optional shortcut:

```text
Super+B → omarchy-shell shell toggle community.buzz '{}'
```

Check effective bindings at setup time and offer a different binding if occupied. Do not unbind an existing shortcut silently. Menu entry, shortcut, and notifications all summon the same panel via owned navigation IDs. Hot reload reconnects the UI bridge and preserves helper state; it must not create another daemon, resend a message, or spawn agents.

## 4. Security and trust boundary

### 4.1 Identity ownership and setup

The **daemon alone owns access to the human Buzz identity used by this integration**. It stores it in a dedicated OS secret-store namespace, using an established Linux keyring/Secret Service library; QML receives only a public key and non-secret state such as locked/configured. Credential setup is a separate local CLI flow with no-echo terminal input or a supported secure upstream handoff. No key in argv, environment, clipboard, temporary file, logs, or IPC. Do not create a plaintext fallback if the keyring is unavailable.

Use an existing human identity only through explicit secure enrollment; do not silently create a second user or read Buzz Desktop's multi-secret blob. If a user chooses a new identity, show that it requires relay membership and is not their existing desktop identity. Shared cross-client identity provisioning is a usability question and an M1 release gate. Storage access failures report `identity_unavailable` because the current keyring wrapper cannot reliably distinguish locked from unavailable; a pending lookup is identified separately. Public context is retained and sends remain disabled.

Agent identities and LLM credentials remain with independently configured ACP processes. The base helper never needs exchange keys, wallets, GitHub tokens, cloud credentials, vPerps sessions, or vMachine leases. Detailed owner telemetry requires explicit support and authorization in M4; do not import agent private keys to read it.

### 4.2 Honest isolation guarantees

The UI bridge, daemon, and agent processes have independent failure lifetimes. Bound resource use and IPC, and sandbox the daemon where compatible (`NoNewPrivileges`, no core dumps, limited filesystem access, memory/task limits, restart backoff). Exact systemd protections must be tested with keyring access on both target architectures; a unit file is not proof of isolation.

**Native QML plugins share the Omarchy process. Absolute crash isolation of QML is impossible under the current architecture.** Small asynchronous QML plus bounded trusted bridge output reduces the risk; a QML/native bug can still crash or freeze the shell. If “never destabilize the shell” means an absolute guarantee, use a separate native application process for the panel and keep only the minimal bar integration in QML. That would change the requested native-plugin experience and still leaves the bar in the shared process.

Same-UID socket permissions and peer checks stop other users; they do **not** isolate a malicious plugin or process running as the user. The socket is intentionally able to request ordinary human chat sends, not privileged operations. A compromised shell could request chat sends as the logged-in user. Keyring separation prevents accidental exposure in QML but is not an authority boundary against complete session compromise. Signing/privileged approval would require a separately protected authority service and a trusted confirmation surface.

### 4.3 Invocation and output controls

- Use fixed executable paths selected during local setup and argv arrays. No `sh -c`, command strings from the relay, runtime installers, or user-content interpolation into commands.
- Configuration files carry relay origin, preferred room, helper path, notification settings, and public identity references only. Do not parse shell rc files or dump the environment to discover configuration. Reuse non-secret `BUZZ_RELAY_URL` only when explicitly importing setup; persist one canonical value thereafter.
- Validate origins; reject credentials in URLs. Require HTTPS/WSS outside explicit local development, reject signed-request redirects to another origin, and keep community caches separated by canonical origin plus human pubkey. A community change cancels pending work and clears all room/profile/notification state.
- Verify event IDs/signatures with upstream cryptographic code, membership/channel context, and special relay-only authorship before projecting data. Fetch NIP-11 identity over the configured trusted transport; rotation requires revalidation. A signed self-description is labeled as such.
- Error output uses stable categories and locally generated explanations. Never forward raw HTTP bodies, CLI stderr, signed auth headers, stack locals, agent tool arguments, raw observer frames, or command environments. Disable upstream wire/debug payload logging regardless of user `RUST_LOG` defaults.
- Disable helper core dumps and avoid secrets in panic formatting. Crash isolation cannot promise zero memory disclosure to root or a debugger with equivalent authority. Human chat may itself contain sensitive text; default notifications omit message bodies, local history is memory-only, and logs omit content.

### 4.4 Relay compromise and authority

A compromised relay can deny service, censor/reorder/replay history, forge its own metadata, and observe ordinary room content. Verify authorship, deduplicate, enforce scope/freshness, and show uncertainty, but do not claim cryptographic completeness or trustworthy human intent from the relay alone. Treat all conversation/tool text as untrusted data, including instructions directed at the workstation.

Buzz is the collaboration/evidence plane. Future authority adapters must validate requester, exact action/resource digest, audience, expiry, nonce, current policy, and an authenticated human response; they own replay protection and enforcement. Clicking Approve, adding a reaction, or publishing a generic `human.approval` message must never directly run a deployment, trade, merge, signer, or credential command.

## 5. v0.1 scope and release contract

Included:

1. Independently installable plugin and helper, with documented reversible optional desktop setup.
2. One configured community and one enrolled human identity; visible locked/unconfigured/offline/incompatible states.
3. Joined stream-room list, preferred room, bounded recent history and basic replies, timestamps, author key distinction, human/agent classification evidence.
4. Plain-text send with explicit signed mentions. Draft survives validation/connection failures; success appears only after accepted event acknowledgment. Ambiguous delivery stays visibly unknown.
5. Local unread activity for monitored rooms, with partial/unknown states and an explicit “on this device/plugin” explanation.
6. Bar connection/unread/activity indicators and native panel summon through optional shortcut/menu.
7. Optional, conservative mention/activity notifications; no historical notification flood after initial sync or reconnect.
8. Agent presence/recent typing when authorized upstream evidence is available. No fabricated execution state.

Excluded: agent installation/orchestration, tool approval, workflow execution, cross-device unread sync, identity administration, room creation/membership changes, DMs, attachments, voice, full forums/search, GitHub synchronization, automatic application launch from events, trading integration, and vMachine authority. These exclusions constrain the release, not the long-term architecture.

The acceptance scenario involving an agent investigating vPerps Local is **M4/M6**, not the definition of v0.1. v0.1 must nevertheless send a correct signed mention to an already configured agent and display its room responses.

## 6. Data acquisition and presentation model

The helper exposes small versioned view models, never arbitrary relay objects. Common fields include community generation, source event ID, public author, observation time, freshness, and provenance. Proposed models: `Connection`, `Room`, `Message`, `Participant`, `UnreadSummary`, `ActivityHint`, `SendResult`. Later `AgentRun` and `ApprovalRequest` are capability-gated additions, not new Buzz event kinds.

| UI data | Upstream source | Rules and limits |
| --- | --- | --- |
| Relay status | Authenticated WS session plus completed bounded initial queries | Socket open alone is `connecting/syncing`, not ready. Display last successful sync and per-subscription health. |
| Community name | Configured canonical origin; validated NIP-11 metadata | Metadata name is presentation, never a tenant/security selector. |
| Rooms | Relay kind `39002` member lists for the human key; kind `39000` metadata | Resolve exact room IDs; do not substitute the legacy kind-41 constant. Filter supported joined stream rooms; label unsupported types. Membership discovery limits yield partial catalog, not a false complete list. |
| Recent messages | Authenticated `POST /query` NIP-CW head page, plus WS live events | Partition rows/aux/bounds; verify scope/signatures; keep live overlay separate from history; show recent/partial scope. Reply history uses NIP-CW `thread_window: true` with validated `39007` request binding and its descending scan cursor. Do not substitute the legacy oldest-first `thread_cursor` path, which has no signed bounds. |
| Edits/deletions | Auxiliary closure and live kinds `40003`, `5`, `9005` | Validate target, author/moderation authority and channel before applying. NIP-CW documentation says 1,000 events per auxiliary hop, but pinned `bridge.rs::query_all_pages` drains up to 64 pages of 1,000 per hop without a wire completeness flag: reaching a cap means unknown completeness and requires a partial view or a separately reviewed catch-up path. Not every withheld/missing event is detectable. Extract/share upstream projection behavior where possible; test against desktop fixtures. |
| Sender | Signed author pubkey; latest valid kind-0 profile | Exact keys remain identity. Duplicate names must remain distinguishable. |
| Agent badge | Valid self-authored kind `10100`; later verified managed identity `30177` | Label self-described agents; absence of evidence is “participant,” not proof of human status. Never claim the name establishes a specific installed harness. |
| Mentions | Selected room members, exact public keys; SDK message builder `p` tags | Resolve labels before sending. If ambiguous, require selection and preserve draft. No mass fan-out or auto-add-member side effect. |
| Presence | Authorized `20001` events / successful `40902` snapshot | Failed or absent queries mean unknown; offline only from reliable explicit/snapshot semantics. Presence is not execution state. |
| Activity | Signed `20002` typing in authorized room/thread | Expire quickly and clear on disconnect. Display “active recently” or “typing,” not “working.” |
| Detailed agent states | M4: owner-authorized decrypted `24200` semantic frames; `44200` terminal metrics | Owner-only, ephemeral gaps, stream restart, and freshness handled explicitly; unavailable for arbitrary coworkers. Raw tool content never reaches QML. |
| Notifications | Newly observed eligible messages/activity after initial baseline | Deduplicate by community/identity/event ID; no body by default; honor local mute settings and native desktop behavior. No invented global priority classification. |

### 6.1 Unread and catch-up correctness

v0.1 unread means unread **observed messages here in monitored rooms**, not organization-wide history or cross-device state. First sync establishes a baseline without declaring all historical messages unread. Persist only bounded seen IDs/frontiers and preferences in private helper state; do not persist plaintext history by default. Mark only messages actually rendered while focused as read; merely opening a panel must not mark the entire room read. Own accepted messages do not increment unread.

Subscribe before/while fetching the initial snapshot, buffer under a cap, reconcile by event ID, and close the race before declaring the view current. On reconnect, reconcile a bounded history overlap and deduplicate. Dense timestamps, retention, capacity limits, missing auxiliary state, or a disconnect beyond catch-up capacity produce `partial/unknown`; never return zero as a substitute. Badge may show `3+` or `?` with explanation. A short page is not proof of completeness. Do not use future-dated untrusted timestamps to advance a local frontier blindly.

The Rust adapter must not silently port the whole NIP-RS system. Cross-device synchronization should reuse a future upstream client/read-state module, with explicit migration and interoperability tests.

### 6.2 Sending and duplicate prevention

Generate the signed event once inside the helper using the upstream builder. Before publication, durably record the local request-ID → signed event-ID binding, scoped to community and human identity, without storing the message plaintext or signed event body. Keep signed bytes in memory for same-event retries during that daemon lifetime. Wait for an exact matching accepted acknowledgment; rejection or unrelated `OK` is not success. If the connection fails after submission, reconcile by the same event ID before resending the same signed bytes. Never re-sign a message automatically after ambiguous delivery. After a restart, unresolved durable IDs reconcile first; absence from a relay response is not proof the event was never delivered. Lost in-memory bytes cannot be reconstructed automatically as a new message. Keep delivery unknown and the user's draft, with an explicit duplicate-risk explanation for a deliberate retry. Bound and expire resolved metadata; never claim exactly-once delivery. UI hot reload must not replay an outstanding send.

### 6.3 Narrow HTTP adapter decision

NIP-CW is the strongest existing interface for correct history, but the reusable WebSocket crate does not implement it and the CLI does not expose its full raw filters. Prefer a small upstream extraction exposing the existing authenticated query client/signing routine (currently private in `crates/buzz-cli/src/client.rs`) through an appropriate client crate. This is an upstream contribution proposal, **not an available API**.

M1 must establish a reusable authenticated HTTP seam before M2. If upstream extraction is not available, a narrowly scoped adapter may compose the existing `nostr` event builder/signing primitives with the verified Buzz NIP-98 request contract and a bounded HTTP client; this implements transport glue, not cryptography or relay behavior. Match payload hashing, nonce/replay handling, canonical URL and redirects against upstream contract tests. Record and review this exception explicitly. Do not copy the private client wholesale, use undocumented desktop IPC, or build a fork to expose it. Reporting unsupported NIP-CW instead of implementing its optional standard-filter downgrade is an intentional v0.1 scope restriction; invalid signed responses must never trigger a downgrade.

### 6.4 Implemented M1 query seam

`helper/src/query.rs` now implements the reviewed narrow adapter exception using
upstream `nostr::nips::nip98::HttpData`, `EventBuilder::http_auth`, SHA-256 body
binding, a fresh nonce, and sensitive Authorization headers. The request body is
serialized once and reused for signing and transport. Reqwest uses fixed `/query`
on the configured origin, no proxies/redirects/retries/decompression, deadlines,
a two-request concurrency cap, and bounded incremental response reads. Typed
requests currently cover only joined-room membership and metadata; no arbitrary
HTTP or signing API is exposed to QML.

Synthetic tests verify exact signed bytes, nonce uniqueness, body substitution
rejection, redirect isolation, byte limits, invalid signatures/scopes, and safe
error categories. Returned signed events still require relay-author trust and
catalog reconciliation; a valid signature is not membership authority. This seam
is intentionally unwired until those projection rules are implemented. It is not
a full NIP-CW timeline implementation or a deployed hosted-relay certification.

## 7. Failure behavior

| Failure | Required visible behavior and recovery |
| --- | --- |
| Helper missing | Bar/panel stays usable in setup state with dependency instructions; no downloads or auto-install. |
| Buzz desktop/CLI missing | Native helper path remains usable; optional launch/diagnostics actions show unavailable. |
| No relay/identity | Clear configuration state; composer disabled; no guessed localhost credentials or spontaneous identity creation. |
| Keyring locked/auth rejected/membership revoked | Report locked/auth/access state; stop privileged reads/writes, clear inaccessible room content, preserve local draft safely; explicit retry after correction. No endless authentication loop. |
| Relay offline/network disappears | Mark stale immediately after detection; bounded exponential reconnect with jitter; clear transient agent hints; retain labeled last-known view in memory. |
| Helper dies/OOM/crashes | Bridge reports unavailable; shell remains responsive; bounded systemd restart backoff; re-handshake and full snapshot, never replay sends. |
| CLI changes or crashes | Optional CLI adapter disabled with compatibility error. No effect on upstream-library backend; never reinterpret invalid output as success/empty rooms. |
| Unsupported helper protocol/relay capabilities | Explicit incompatible/unsupported state; disable affected writes/features. Do not silently substitute an incorrect history API. |
| Malformed/oversized/invalidly signed event | Reject before QML; count bounded diagnostics by category without contents. Preserve valid state; disconnect/back off if sustained abuse. |
| Missing EOSE/subscription silently closed | Subscription remains incomplete; explicit timeout/resubscribe; heartbeat alone does not make it current. |
| Incomplete NIP-CW page/invalid cursor/aux cap reached | Discard invalid page or show an explicit unavailable/partial view according to contract; never guess history exhaustion or deletion state. Channel auxiliary completeness is not universally observable. |
| Send timeout | `delivery_unknown` with event-ID reconciliation; keep draft/status; no blind new signed send. |
| ACP unavailable/crashed | Chat continues; agent status unknown/offline only with evidence; no shell restart or replacement harness launched by UI. |
| Agent exited | M4 supervisor/authorized lifecycle evidence updates state; presence may lag. Absence of typing is not exit proof. |
| User disables/removes plugin | UI connection closes; helper disconnects/exits after last client idle grace unless explicitly enabled for background notifications; no agent shutdown side effect. |

## 8. Configuration, distribution, lifecycle, and compatibility

### Hosted and independently operated communities

Self-hosting is optional. Setup offers **Buzz hosted** and **Custom relay** as
first-class choices. Both ultimately configure the same canonical community
origin and human identity; the messaging/agent architecture does not fork by
hosting provider. No credentials or hosted account tokens belong in QML.

The inspected upstream `desktop/src/features/communities/hostedCommunityApi.ts`
uses community-specific `*.communities.buzz.xyz` hosts. Its onboarding components
call Tauri commands for Builderlab browser login, identity binding, community
listing/creation, and joining. These desktop commands are not a supported local
API for an external Omarchy plugin. The official [support guidance](https://block.github.io/buzz/support.html)
directs users to buzz.xyz and explains invitation-based access. A relay URL alone
does not grant membership. Source and public documentation disagree on numeric
account limits, so the plugin must not embed plan/quota promises.

Initially the hosted option opens the fixed official onboarding site only on a
user click, then connects to the resulting community address after upstream
account/invitation setup and explicit secure identity enrollment. It must clearly
identify this manual handoff. Do not invent a global public relay, guess a
community from an email, scrape desktop tokens, or implement a parallel account
system. A future seamless browser handoff must use a reviewed upstream contract
implemented in the helper, with account credentials outside QML.

Planned non-secret helper config: `~/.config/omarchy-buzz/config.toml`. It holds the relay origin, public identity reference, preferred room, monitored rooms, notification mode, optional integration declarations, and later supported agent references. No arbitrary environment map. QML settings contain only presentation preferences/helper executable location. Validate helper path locally; remote messages cannot change it.

Planned state: `~/.local/state/omarchy-buzz/` with 0700 directory/0600 files for unread cursors and send metadata; no credential fallback. Runtime socket disappears with the session. Keep logging category-based and bounded. Background notifications are opt-in so disabling the plugin does not unexpectedly keep a network client active.

Keep the repository small:

```text
manifest.json                   # added in M0, with real QML entry points
plugin/{Service,BarWidget,Panel}.qml
helper/                         # one Rust crate, daemon/bridge/setup subcommands
service/omarchy-buzz.{service,socket}
config/example.toml
tests/fixtures/                 # synthetic, never exported private room history
README.md, AGENTS.md, DESIGN.md, LICENSE
docs/                           # split security/development/integration guides as implemented
```

No empty code scaffold or fake manifest is needed during design review. Proposed license is Apache-2.0, compatible with the intended upstream Rust dependencies; M0 adds the actual license and dependency notices. Omarchy examples are architectural references, not permission to copy license-incompatible source. Check licenses for any copied material separately.

Install the helper as a versioned, independently verifiable package; source build is supported. Publish x86_64 and ARM64 artifacts only after testing them. No binaries auto-downloaded by QML and no `curl | sh`. The normal Omarchy plugin command manages the plugin checkout; explicit helper setup manages its user unit and optional menu/shortcut entries. Installation must explain that both components are needed.

Removal instructions cover disabling/removing the plugin, stopping/disabling/removing both helper socket and service units, and removing only owned optional desktop entries. Preserve user-edited entries and warn on conflict. Preserve identity and read-state by default; explicit credential deletion is separate. Native `omarchy plugin remove` has no reliable external-helper uninstall hook, so use disconnect-on-last-client plus explicit unit removal rather than promise automatic cleanup. No agent or vPerps unit is owned by this plugin.

Pin Buzz crate Git revisions and commit `Cargo.lock`; do not track mutable `main` at build/runtime. Prefer the tested Omarchy source/package combination over a broad “4.x supported” claim. Helper hello advertises protocol version, build revision, tested backend revision, and capabilities; the UI rejects incompatible major versions. Upgrading the Git-managed UI need not upgrade the installed helper simultaneously. Maintain compatibility across at least the documented UI/helper release pair, with a visible mismatch state and rollback instructions.

The marketplace requires a public root manifest, README install/removal instructions, license, dependencies, and unique permanent ID. Listing and subsequent verification refer to exact commits and maintainer review. The current marketplace [submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md) and [security policy](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SECURITY.md) were checked during research; recheck them when publishing. This design stage creates no remote repository, listing, or submission.

## 9. Extension model and vPerps/vMachine consumption

### 9.1 Generic seams

Start with data/configuration seams, not an arbitrary plugin-loading framework:

- Community/room selection, notifications, public agent references, and allowed navigation targets are configuration.
- Producers use supported Buzz commands/SDKs to publish ordinary messages and existing appropriate events under their own identities. UI never runs code received in an event.
- Later helper adapters project supported domain events into a versioned display envelope: source identity/event, observed time, title, summary, severity, room/run/resource references, freshness, and declared supported actions. These are **local view-model fields**, not invented Nostr kinds.
- Navigation uses locally registered handler IDs and validated typed resource IDs. Repository roots and allowed URL origins are configured locally. Do not map arbitrary event text to shell commands, filesystem paths, or custom URL schemes.
- Approval adapters are separately installed integrations with an authoritative backend. The UI passes an opaque request reference and decision intent; the backend verifies the action and human evidence. QML is never the final authorizer.

Do not add a general extension ABI until a real second integration requires it. Tests should first exercise one generic mock producer and a separate vPerps integration package against the same stable view model.

### 9.2 vPerps downstream plan

vPerps's overlay installs the public plugin/helper at tested versions, selects its community/preferred rooms, and adds an owned menu entry. The base repository contains no trading room defaults, ports 8080/8091, venue names, account logic, registry credentials, or deploy commands.

A separate vPerps publisher can read permitted Local service health and publish sanitized operations messages. A separate navigation integration maps repository/PR/application references into the existing vPerps CLI/window launchers. vPerps owns its registry semantics, venue telemetry, redaction, risk policy, and execution permissions. Existing fixtures remain labeled fixtures; they must not become claimed production observations in a Buzz room.

Development agent setup selects the local repository/workspace and obeys its AGENTS instructions. A mention requesting a service investigation is not permission to trade, deploy, migrate production, or disclose credentials. Local agents' existing CLI/tool permissions are not automatically equivalent to an ACP harness policy. Validate each adapter's actual permissions before the demo.

### 9.3 Enterprise events and authority plane

Buzz already has a numeric kind registry, metadata tags, workflow/Git events, owner attestations, and encrypted agent telemetry. That is extensibility, not permission for us to mint arbitrary accepted kinds. The proposed `agent.request`, `policy.decision`, `machine.attestation`, `credential.issued`, and similar names initially belong only to an adapter's versioned internal taxonomy. Upstream any reusable wire contract, including admission/authorization/retention semantics, before publishing a new kind. Never broadcast actual credentials in a credential lifecycle event.

Buzz supplies coordination and evidence across machines. Each future vMachine authority process decides permitted actions, risk limits, credential use, attestation and capability leases. A relay event can reference that process's decision; it cannot override it. A signed Buzz response alone is not a grant unless the authority explicitly validates it under its own policy.

## 10. Engineering validation and milestones

CI should run manifest validation against the pinned Omarchy checkout; Rust formatting/lint and boundary tests; synthetic relay/IPC tests; secret-output tests; dependency/license checks; and build checks for x86_64/ARM64. QML lint with correct imports is useful but not a substitute for loading the plugin. GUI tests run in a disposable Omarchy VM/session, not by resetting the developer's desktop.

Meaningful tests include malformed/oversized input rejected before Qt; wrong-user socket attempts; unknown protocol version; cross-community stale responses; keyring locked without identity replacement; log output containing no fixture sentinel secrets; exact mention recipients; same-event send retry; lost acknowledgment; permission revocation clearing data; membership/agent identity forgery; edited/deleted message handling; timestamp collisions and capped catch-up; helper crash while panel is open; and disable/remove/reload without duplicate services or sends. Avoid tests that merely repeat implementation expressions.

Integration tests need a disposable relay with separate Compose project, ports, storage and synthetic identities, explicitly isolated from Buzz Desktop, vPerps, and production. No use of existing private rooms as test fixtures. Recording a skipped graphical, ARM64, keyring, or agent test is not a pass.

| Milestone | Deliverable | Exit evidence / gate |
| --- | --- | --- |
| Design (this change) | Source-grounded decisions and reviewable scope | Documentation/link review; all unavailable upstream interfaces labeled. No production changes. |
| M0 — development skeleton | Real manifest, generic bar/panel/service, README install/uninstall, license, CI, fixture-only helper contract | Pinned Omarchy validation and real shell load/disable/reload; no live identity needed. Freeze unique plugin ID. |
| M1 — connectivity and identity | Rust daemon/bridge, keyring enrollment, authenticated WS, HTTP query seam, lifecycle bounds | ARM64/x86_64 build and isolated relay smoke test; logs/argv/env clean; malformed input and crash tests. Establish supported keyring setup, signed HTTP seam and library resource limits. |
| M2 — messaging | Joined stream rooms, bounded history/replies, send, exact mentions, local unread | Real relay end-to-end tests including edits/deletions, ambiguity, dedup, access loss, cursor correctness and reconnect. Resolve projection reuse before claiming complete history correctness. |
| M3 — native experience / v0.1 | Theme-aware UI, opt-in shortcut/menu, notifications, documented packaging and compatibility | Multi-monitor/keyboard/lock/DND/error-state verification; reversible setup/removal; user-visible partial states; community release candidate. |
| M4 — ACP integration | Independently configured Codex/Claude/Goose where available, mentions, authorized reliable telemetry | Adapter-by-adapter architecture/permission tests; owner routing; no automatic tool approvals hidden behind UI. Address secret-env/tempfile behavior and authority limitations. |
| M5 — agents surface | Agent/run list, exact identities, room/task/repo links where evidenced, safe supported cancellation | Unknown/stale/ended distinctions, telemetry reconnect gaps, correct target authorization; no implied orchestration. |
| M6 — vPerps consumption spike | Same public plugin installed by overlay; separate room/config/navigation integrations | Isolated observe-only service investigation demo through ACP; no base plugin fork and no money/deploy actions. |
| Later — approvals/workflows | Real authority-backed approval surface | Upstream workflow gate or external authority works end to end, including deny, expiry, replay and action binding. No date or fake implementation promised. |

## 11. Final review checklist and recommendations

### 11.1 Architecture recommendation

Proceed with thin native QML, a separately supervised Rust helper with a credential-free UI bridge, existing Buzz client/SDK primitives, and separately owned ACP processes. Keep the public plugin generic. Choose explicit uncertainty over plausible fake state. No Buzz fork, Omarchy fork, or vPerps-specific plugin fork is needed.

### 11.2 Interfaces intended for use

- **Existing Omarchy:** manifest schema 1; `service`/`bar-widget`/`panel`; own-plugin `PluginShellApi`; `Process` I/O; `omarchy-shell shell toggle/summon`; `omarchy notification send`; JSONC menu extension; optional Lua binding; `omarchy plugin validate/add/update/remove`.
- **Existing Buzz:** pinned `buzz-ws-client`, `buzz-sdk`, `buzz-core`, upstream `nostr` signing/verification; authenticated NIP-01/42 streams; joined-room metadata/member events; SDK message builder; NIP-CW HTTP query contract with NIP-98; profile, presence, and typing events.
- **Later existing upstream surfaces:** ACP adapter commands; exact `p` mentions; owner-authorized observer frames and metrics; narrowly authorized cancel/control; actual workflow handlers only after the missing workflow path is completed.
- **Proposed, not existing:** `omarchy-buzz` IPC/config/view models; reusable upstream authenticated HTTP-client extraction; future reusable unread/timeline module; downstream display/navigation/authority adapter contracts.

### 11.3 MVP scope

v0.1 is native joined-room messaging with exact mentions, local unread, connection state, recent agent activity hints, conservative notifications, and safe packaging. No new agent orchestrator, cross-device read-state implementation, or approval authority. The compelling agent investigation demo follows in M4/M6.

### 11.4 Security risks

Highest risks are same-UID session compromise, secrets accidentally exposed through upstream CLI/ACP conventions or debug logs, permissive ACP tool handling, hostile relay content crossing a launch boundary, incorrect message/edit/delete projection, ambiguous sends, and shared-process QML failure. The design limits exposure but does not claim a sandbox or a complete authority system. M1/M2/M4 each have explicit security gates.

### 11.5 Upstream dependencies

Buzz Rust interfaces and relay contracts are pre-stable and require revision pinning. Correct HTTP history needs a reusable authenticated adapter; client projection/read-state extraction would reduce maintenance. Strong agent execution visibility is owner-scoped, not universally public. ACP permission mediation and non-environment/non-tempfile credential paths need investigation/upstream work before a secure integrated launcher. Workflow approval suspension is unfinished. Omarchy lacks external-helper lifecycle hooks and absolute QML isolation; marketplace listing is an external review process.

### 11.6 Unresolved technical questions

1. Which supported Linux secret-store enrollment/handoff gives an existing Buzz human identity the best experience without accessing the desktop multi-secret blob? Confirm on this ARM64 Omarchy installation using synthetic identities.
2. Can Buzz expose its signed HTTP query client and client-side edit/delete projection as reusable modules? If not, review the narrow adapter scope and conformance burden before M2.
3. Do the pinned WS buffering/deadline behaviors meet hostile-peer limits, or should bounded-buffer support be upstreamed before release? Systemd memory limits protect the shell but do not replace correctness.
4. Which Omarchy package/source combinations and plugin ID will be the first supported release baseline? A version string alone does not establish API compatibility.
5. How can ACP obtain identity/provider credentials without env/tempfile leakage, and which adapter modes actually enforce intended tool restrictions? This is a launch/permission question, not solved by a UI prompt.
6. Which detailed agent state should be shareable with coworkers rather than only its owner? Requires an upstream privacy/authorization contract; no inference from encrypted metrics.
7. Can a reusable upstream read-state client support NIP-RS later without duplicating the desktop subsystem? Until then the local-only unread label is mandatory.
8. Which tested Linux ARM64 builds of each ACP adapter are available, and what packaging is required? Do not let desktop AppImage availability dictate helper support.

These are focused implementation investigations and upstream questions, not requests for the user to answer routine engineering details.

### 11.7 Proposed milestone breakdown

Review this design, then M0 → M1 → M2 → M3/v0.1. Develop and validate independent ACP integration in M4, expose evidence-backed agent runs in M5, and demonstrate unchanged-plugin vPerps consumption in M6. Authority-backed approval follows only when the corresponding backend contract is real. Keep each milestone as small reviewable commits with recorded validation and supported versions.

### 11.8 Conflicts with the prompt or earlier assumptions

- A CLI-only adapter cannot currently supply all requested streaming/read-state/history functions, and ordinary CLI key injection conflicts with our stronger no-secret-argv/environment design. Existing upstream libraries plus a narrow helper adapter are the recommended route.
- The requested global unread count is not a current CLI field. v0.1 intentionally uses local observed unread with explicit partial semantics.
- General-purpose agent “working/thinking/approval required” states cannot be inferred from ordinary room presence. Detailed telemetry is owner-encrypted; live observer frames are not durable history.
- Upstream ACP does not currently provide the proposed human tool-approval UX by default, and workflow approval suspension is explicitly unsupported. Approval cards are not a workaround.
- Absolute “plugin failure never destabilizes Omarchy” conflicts with in-process QML. Separate helper processes isolate helper/agent faults, not every possible QML fault.
- Standard plugin add/remove does not install/uninstall a binary or systemd service. Helper packaging and cleanup need explicit documented ownership.
- `Super+B` is available in the inspected defaults/local file, but cannot be reserved on every user's desktop. Installation must remain opt-in and conflict-aware.
- Buzz Linux desktop ARM64 distribution is not established; native plugin messaging does not require the desktop app. Our earlier desktop-launch-first recommendation is superseded by this source-grounded helper design.
- Buzz's signed event/audit model records collaboration; it does not certify all local agent actions, automatically synchronize GitHub, or implement vMachine authority. Several vPerps appliance/security features remain roadmap items.

This design preserves the product vision while making the first release and every security claim testable.
