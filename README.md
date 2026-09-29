# Buzz for Omarchy

A community-first, native Omarchy surface for collaboration with people and AI agents through [Block's Buzz](https://github.com/block/buzz).

**Status: messaging development preview (`0.0.8`), not yet a community release.** The native bar and panel provide authenticated room discovery, recent messages, a bounded reply view with thread composition, plain-text sending, and exact public-key mentions. Local activity badges cover the monitored joined rooms. A verified signed profile can identify a participant as a *self-described agent*; execution state remains unknown. Production UI never displays synthetic activity.

Open **View replies**, then **Reply in thread** to compose a reply. Room and thread drafts are kept separately. **Back to room** returns to a top-level message. A missing thread target disables sending until it can be verified again; it never silently turns your reply into a room message. Update the helper and plugin together for this feature.

Selected-room history refreshes about five seconds after a request finishes. A separate bounded worker checks other rooms in rotation, one every five seconds after completion, up to the catalog's 20-room limit. Slow requests and catalog refreshes increase that interval. Badges count newly observed messages since the local baseline, **not Buzz-synchronized unread messages**. Opening a room's snapshot clears its local badge. Counts are session-only and reset on gaps, failures, or reconnects; missed activity is possible.

**Alerts: on** enables generic notifications for new observed activity outside the visible conversation. This default-off preference survives shell restarts and applies across communities. Alerts contain no message text or room names. Initial snapshots, own messages, and edited/unavailable rows do not alert. These are best-effort hints, not a delivery guarantee.

Enrollment verifies the relay's signing identity before saving a human key and rejects the server signing key. The relay must be reachable. Update helper and plugin together: 0.0.5 adds `room_activity` and `agent_profiles` capabilities. See [ACP readiness](docs/ACP_READINESS.md) for the remaining real-agent launch blockers.

The helper owns identity access through Linux Secret Service. Enrollment uses hidden terminal input; QML receives bounded presentation data and never identity keys. Missing or failed helpers leave the panel usable with setup instructions and Retry. Authentication is not a claim that room synchronization works.

The community plugin will remain generic. vPerps is a downstream integration, described in the design, rather than a separate plugin fork. This project is independently maintained and is not presented as an official Block or Omarchy product.

## Compatibility

Development targets the built-in Omarchy bar and the shell interfaces inspected at [`7b336b1`](https://github.com/omacom/omarchy/tree/7b336b1b0da722e7bb864a7136f91e784ef731bf). Required runtime dependencies are Omarchy's Quickshell, QtQuick/Layouts/Controls, Wayland/Hyprland modules, and `qs.Ui`/`qs.Commons`; these come from the Omarchy installation. Other bar implementations may not expose the service facade. The panel can be summoned separately.

Manifest validation, a native sample-panel render, ARM64 helper tests, isolated Secret Service enrollment, inherited-socket idle reactivation, and actual QML/helper process integration have passed locally. The installed native setup panel and systemd socket activation also passed a live desktop check. Native plugin removal/reinstall and conflict-aware shortcut installation/removal passed. Physical keyboard and multi-monitor verification remain pending. Do not infer support for every 4.x snapshot from the package version.

For another user, see [sharing and room access](docs/SHARING.md).

## Try the local development preview

Use a disposable Omarchy session first. Get the public source (Git refuses to
replace an existing directory):

```bash
git clone https://github.com/randymy/omarchy-buzz.git
cd omarchy-buzz
```

From this **committed checkout**, validate and install the UI:

```bash
omarchy plugin validate .
mkdir -p "$HOME/.config/omarchy/plugins"
git clone --no-hardlinks "$PWD" "$HOME/.config/omarchy/plugins/community.buzz"
omarchy-shell shell rescanPlugins
omarchy plugin enable community.buzz --section right
omarchy-shell shell summon community.buzz '{}'
```

Git refuses to replace an existing destination. Keep any existing plugin of this ID; do not overwrite it. `community.buzz` is a development ID pending marketplace uniqueness review. Source is public on GitHub; the plugin has not been submitted to the marketplace.

Click **Buzz** to toggle the panel, then use **Close** or Escape. An optional,
reversible [Super+B shortcut](docs/NATIVE.md) checks for conflicts before installation. Without the separately installed helper/socket, the panel shows an unavailable state. No menu entry, shortcut, service, or credentials are installed by the plugin itself. Omarchy's enable command changes its plugin configuration through the native manager.

Use **Window** in the panel to keep Buzz open as a normal resizable desktop
window, or **Overlay** to switch back. Both presentations share the same view,
draft and helper. See [native window and shortcut behavior](docs/NATIVE.md).

## Install the development helper

Build with Rust 1.95, a C compiler, pkg-config and D-Bus development headers on
Linux. On Arch, install `pkgconf` and `dbus`; the target also needs
`libdbus-1.so.3` and a working session Secret Service provider:

```bash
cargo build --release --locked --manifest-path helper/Cargo.toml
```

Package the local build and use the [helper installer](service/README.md) to preview and install or upgrade the binary and its user socket. It keeps rollback copies during upgrades. There is no automatic binary download. A functioning Linux Secret Service is required for enrollment and authentication.

You do not need to run a relay. Choose **Buzz hosted** to create or join a
Block-hosted community through [buzz.xyz](https://buzz.xyz), or **Custom relay**
for a relay you or someone else operates. Hosted communities have their own
addresses and access rules; there is no shared public default relay. Complete
upstream account/invitation setup first, then use your community's assigned
relay URL. See [upstream hosted-community guidance](https://block.github.io/buzz/support.html).

The current setup panel opens upstream hosted setup in your browser;
automatic account/identity handoff is not implemented. It never collects hosted
account passwords or bearer tokens in QML. Configure either type of community
and enroll your existing human Buzz identity in a terminal:

```bash
omarchy-buzz setup relay wss://your-relay.example
omarchy-buzz setup identity enroll
```

The enrollment prompt hides input; never pass a private key as an argument or paste it into QML. The helper uses its own Secret Service namespace, not Buzz Desktop's shared secret blob. It never generates a replacement identity. Select **Retry connection** after setup; the helper reloads configuration. TLS is required except for loopback development relays. Details and current limits are in [helper documentation](helper/README.md) and [security](docs/SECURITY.md).

This local clone can be updated after committing changes to the source checkout:

```bash
omarchy plugin update community.buzz
```

For direct UI iteration, edit the installed development copy; Omarchy watches user plugins. An update will refuse conflicting local changes. On the tested package, rescan and disable/re-enable retained old compiled QML after an update. If the UI remains stale, use the supported `omarchy restart shell` command; it restarts the bar/panels, not applications. Do not rename plugin entry points to bypass caching. See [development and validation](docs/DEVELOPMENT.md) for the full smoke procedure and pinned validator.

## Sending in this preview

Choose a discovered room, compose a plain-text message, and select **Send message**.
The panel reports the relay’s acknowledgement; it does not treat a socket write
as delivery. If the connection or receipt is lost, the draft remains and the
outcome is **unknown**. Nothing is resent automatically. A new submission after
an unknown outcome could duplicate a message already accepted by the relay.
Rejected messages require an explicit new submission before another attempt.

Drafts stay in memory. The helper stores only bounded request/event identifiers
and delivery outcomes, never message bodies, in its private state directory.
Keep that delivery record during updates. Select recipients to attach exact mention keys; a typed `@name` alone is not
resolved. Names are self-asserted hints beside public keys, and the roster is
explicitly partial. Agent execution still requires separately configured Buzz ACP.
For a verified thread, select **Reply in thread** to compose a reply; **Back to room** restores the room draft. This requires the matching 0.0.8 helper. Attachments are not supported. See [sending semantics](docs/SENDING.md).

## Staying current

Buzz dependencies are pinned and tested together. The [upstream workflow](docs/UPDATES.md)
is currently manual-only and can prepare draft updates for validation; it does not auto-merge.
Updating the plugin checkout does **not** replace the separate helper binary.
Install the matching helper build and use `omarchy-buzz --version` to inspect
its version and Buzz revision. Scheduled checks remain paused by user request.

## Remove

```bash
omarchy plugin disable community.buzz
omarchy plugin remove community.buzz
```

The native manager unloads the plugin and removes its Git checkout. Commit or save any development changes first. If you installed the helper separately, use its [preview and uninstall commands](service/README.md) too. Plugin removal does not remove the helper units or identity. The helper uninstaller preserves credentials, configuration, and the delivery ledger. The source repository is retained.

## Isolated room-agent preview

A separately supervised stock Codex agent has passed a real ChatGPT-subscription
room test: owner mention, isolated workspace read, and signed reply in the same
thread, verified by the installed helper. This is a manual operator setup, not
automatic agent installation. See [setup, controls and limits](docs/ROOM_AGENTS.md).
Claude room deployment and native agent-state controls remain unfinished.

## Architecture and next work

[DESIGN.md](DESIGN.md) records the inspected interfaces, security boundary, release gates, and milestones. The QML service consumes a versioned, bounded presentation protocol through the helper bridge. Signed room discovery and recent history are projected in Rust. History is a partial snapshot, with explicit truncation and unavailable-content markers; it does not claim complete edits/deletions or live synchronization. Synthetic rooms are available only in explicit test mode.

Real-relay messaging components and synthetic ACP mention routing have passed on disposable CI; see [recorded evidence](docs/CHECKPOINT.md#isolated-relay-progress-2026-09-26). Release gates still include full daemon/UI real-relay coverage, real agent validation, and bounded upstream WebSocket buffering. Local observed activity and self-described agent profiles are implemented; neither proves synchronized unread state or agent execution. vPerps-specific behavior remains in downstream configuration and separate integrations.

Upstream review artifacts are ready for the [WebSocket resource limits](docs/upstream/WS_RESOURCE_LIMITS.md)
and [ACP interactive authentication](docs/upstream/ACP_INTERACTIVE_LOGIN.md).
They are isolated proposals, not installed dependency patches. The
[authentication preview setup](docs/AGENT_PREVIEW_SETUP.md) keeps dedicated
provider profiles and native login outside the shell; it does not enable room
agents. Guarded Codex/ChatGPT and Claude native-subscription model smoke tests
have passed; a separate synthetic relay test verifies signed, persisted room
replies through Buzz's existing endpoint. These are separate checks, not a
complete production room-agent deployment. The [prepared marketplace submission](docs/MARKETPLACE_SUBMISSION.md)
describes only the messaging preview. Subscription support
is a first-class requirement; the [authentication matrix](docs/AGENT_AUTH_COMPATIBILITY.md)
separates native product support from actual ACP verification. The current
[checkpoint](docs/CHECKPOINT.md) tracks harness-owned replies, tool-request denial,
and membership-bound publication proposals. [Codex subscription enforcement](docs/CODEX_SUBSCRIPTION_ENFORCEMENT.md)
distinguishes startup login restrictions from actual Pro-account and billing verification.

## License

[Apache-2.0](LICENSE). The UI implementation is original; Omarchy components are imported at runtime. Upstream Omarchy and Buzz retain their own licenses and trademarks.

Advanced downstream use: [vPerps integration plan](docs/VPERPS_INTEGRATION.md).
