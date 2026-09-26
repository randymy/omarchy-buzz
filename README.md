# Buzz for Omarchy

A community-first, native Omarchy surface for collaboration with people and AI agents through [Block's Buzz](https://github.com/block/buzz).

**Status: M0 development preview (`0.0.1`). Sample data only.** The bar widget opens a native panel with two example rooms. There is no relay connection, identity handling, message sending, notification delivery, or agent execution yet. This is not the messaging MVP.

The plugin uses Omarchy theme colors, provides keyboard-focusable room controls, closes with Escape, and shares sample state through one QML service. The bar and panel explicitly identify this as a demo. No credentials, helper process, or Buzz installation are required for this preview.

The community plugin will remain generic. vPerps is a downstream integration, described in the design, rather than a separate plugin fork. This project is independently maintained and is not presented as an official Block or Omarchy product.

## Compatibility

Development targets the built-in Omarchy bar and the shell interfaces inspected at [`7b336b1`](https://github.com/omacom/omarchy/tree/7b336b1b0da722e7bb864a7136f91e784ef731bf). Required runtime dependencies are Omarchy's Quickshell, QtQuick/Layouts/Controls, Wayland/Hyprland modules, and `qs.Ui`/`qs.Commons`; these come from the Omarchy installation. Other bar implementations may not expose the service facade. The panel can be summoned separately.

Manifest validation and an offscreen panel render have been checked locally. Full live-shell enable/disable, hot reload, keyboard focus, multi-monitor placement, and removal verification remain pending. Do not infer support for every 4.x snapshot from the package version.

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

Click **Buzz · DEMO** to toggle the panel. Select either sample room, then use **Close** or Escape. The preview cannot send messages; it does not read or write Buzz configuration. No menu entry, shortcut, background service, or credentials are installed. Omarchy's enable command changes only its plugin configuration through the native manager.

This local clone can be updated after committing changes to the source checkout:

```bash
omarchy plugin update community.buzz
```

For direct UI iteration, edit the installed development copy; Omarchy watches user plugins. An update will refuse conflicting local changes. See [development and validation](docs/DEVELOPMENT.md) for the full smoke procedure and pinned validator.

## Remove

```bash
omarchy plugin disable community.buzz
omarchy plugin remove community.buzz
```

The native manager unloads the plugin and removes its Git checkout. Commit or save any development changes first. M0 creates no separate service, stored identity, or user data to remove. The source repository is retained.

## Architecture and next work

[DESIGN.md](DESIGN.md) records the inspected interfaces, security boundary, release gates, and milestones. The QML service currently consumes only a bundled synthetic snapshot. The proposed helper protocol is not implemented, and these sample models must not be used to accept arbitrary external JSON.

M1 adds a separately supervised helper, secure identity enrollment, and a real relay connection. vPerps-specific behavior remains in downstream configuration and separate integrations.

## License

[Apache-2.0](LICENSE). The UI implementation is original; Omarchy components are imported at runtime. Upstream Omarchy and Buzz retain their own licenses and trademarks.
