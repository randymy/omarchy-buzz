# Buzz for Omarchy

A community-first, native Omarchy surface for collaboration with people and AI agents through [Block's Buzz](https://github.com/block/buzz).

**Status: M2 read-only development preview (`0.0.2`), not the messaging MVP.** The native bar and panel use a separate local helper for setup, authenticated connection state, joined stream rooms, and recent message snapshots. Sending, unread counts, verified human/agent badges, notifications, and agent execution remain unfinished. Production UI never displays synthetic rooms or agent activity.

The helper owns identity access through Linux Secret Service. Enrollment uses hidden terminal input; QML receives only public connection information. Missing or failed helpers leave the panel usable with setup instructions and Retry. Authentication is not a claim that room synchronization works.

The community plugin will remain generic. vPerps is a downstream integration, described in the design, rather than a separate plugin fork. This project is independently maintained and is not presented as an official Block or Omarchy product.

## Compatibility

Development targets the built-in Omarchy bar and the shell interfaces inspected at [`7b336b1`](https://github.com/omacom/omarchy/tree/7b336b1b0da722e7bb864a7136f91e784ef731bf). Required runtime dependencies are Omarchy's Quickshell, QtQuick/Layouts/Controls, Wayland/Hyprland modules, and `qs.Ui`/`qs.Commons`; these come from the Omarchy installation. Other bar implementations may not expose the service facade. The panel can be summoned separately.

Manifest validation, a native sample-panel render, ARM64 helper tests, isolated Secret Service enrollment, inherited-socket idle reactivation, and actual QML/helper process integration have passed locally. The installed native setup panel and systemd socket activation also passed a live desktop check. Full keyboard, multi-monitor, and removal verification remain pending. Do not infer support for every 4.x snapshot from the package version.

## Try the local development preview

Use a disposable Omarchy session first. From a **committed checkout** of this repository:

```bash
omarchy plugin validate .
mkdir -p "$HOME/.config/omarchy/plugins"
git clone --no-hardlinks "$PWD" "$HOME/.config/omarchy/plugins/community.buzz"
omarchy-shell shell rescanPlugins
omarchy plugin enable community.buzz --section right
omarchy-shell shell summon community.buzz '{}'
```

Git refuses to replace an existing destination. Keep any existing plugin of this ID; do not overwrite it. `community.buzz` is a development ID pending marketplace uniqueness review. This repository has not been published or submitted to the marketplace.

Click **Buzz** to toggle the panel, then use **Close** or Escape. Without the separately installed helper/socket, the panel shows an unavailable state. No menu entry, shortcut, service, or credentials are installed by the plugin itself. Omarchy's enable command changes its plugin configuration through the native manager.

## Install the development helper

Build with Rust 1.95, a C compiler, and pkg-config on Linux:

```bash
cargo build --release --locked --manifest-path helper/Cargo.toml
```

Install the resulting `helper/target/release/omarchy-buzz` binary at `~/.local/bin/omarchy-buzz`, and follow [helper supervision](service/README.md) to install and enable its user socket. Review existing files before replacement. There is no automatic binary download. A functioning Linux Secret Service is required for enrollment and authentication.

You do not need to run a relay. Choose **Buzz hosted** to create or join a
Block-hosted community through [buzz.xyz](https://buzz.xyz), or **Custom relay**
for a relay you or someone else operates. Hosted communities have their own
addresses and access rules; there is no shared public default relay. Complete
upstream account/invitation setup first, then use your community's assigned
relay URL. See [upstream hosted-community guidance](https://block.github.io/buzz/support.html).

The current connection preview opens upstream hosted setup in your browser;
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

## Remove

```bash
omarchy plugin disable community.buzz
omarchy plugin remove community.buzz
```

The native manager unloads the plugin and removes its Git checkout. Commit or save any development changes first. If you installed the helper separately, follow its [service cleanup instructions](service/README.md) too. Plugin removal does not remove the helper units or identity. Preserve credentials and configuration unless you explicitly choose to delete them. The source repository is retained.

## Architecture and next work

[DESIGN.md](DESIGN.md) records the inspected interfaces, security boundary, release gates, and milestones. The QML service consumes a versioned, bounded presentation protocol through the helper bridge. Signed room discovery and recent history are projected in Rust. History is a partial snapshot, with explicit truncation and unavailable-content markers; it does not claim complete edits/deletions or live synchronization. Synthetic rooms are available only in explicit test mode.

Release gates still include conformance against an isolated real Buzz relay and bounded upstream WebSocket buffering. Next in M2 are safe message delivery, exact mentions, profile classification, and local unread accounting. vPerps-specific behavior remains in downstream configuration and separate integrations.

## License

[Apache-2.0](LICENSE). The UI implementation is original; Omarchy components are imported at runtime. Upstream Omarchy and Buzz retain their own licenses and trademarks.
