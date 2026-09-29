# Optional native shortcut

Buzz supports two presentations of the same view. Open it from the bar or
Super+B, then choose **Window** for a normal resizable, tilable desktop window.
Choose **Overlay** to return to the quick panel. The same content instance and
helper service preserve the selected room, thread and in-memory draft while
switching. This is one view in either mode, not two independent chat clients.

To open directly as a normal window:

```sh
omarchy-shell shell summon community.buzz '{"mode":"window"}'
```

The native window close control and **Close · Esc** both close the view through
Omarchy's panel lifecycle. The bar/shortcut keeps its existing toggle behavior:
if Buzz is already open it closes it; the next fresh open defaults to the
overlay. Window size and presentation preference are not persisted yet.

The short native fixture `scripts/preview --presentation` requires a running
Wayland session and briefly displays synthetic content. It uses isolated
configuration/state and no helper, relay, or identity. It checks switching and
host close behavior; it does not certify physical keyboard or multi-monitor UX.

After installing and enabling the plugin, run these from its checkout:

```sh
./scripts/desktop-shortcut status
./scripts/desktop-shortcut install
```

This optionally adds **Super+B** to your existing user `hypr/bindings.lua`, under `XDG_CONFIG_HOME` or `~/.config`. It uses Omarchy's native `o.bind` with the fixed command `omarchy-shell shell toggle community.buzz '{}'`. It does not install the helper, enable the plugin, add menu entries, or accept arbitrary commands or chords.

The installer reads effective bindings through `hyprctl -j binds` immediately before editing. It refuses an existing Super+B binding it does not own, duplicate effective bindings, invalid IPC data, or an unreachable desktop. It never unbinds another shortcut. An existing exact owned block makes installation idempotent. A block present but not effective is reported for review, without adding a second copy. The current Lua backend exposes an opaque `__lua` dispatcher ID rather than
the command text; verification combines its unique chord/description with the
exact owned source block. Run from the active Hyprland session; another session's environment is insufficient.

The tool requires an existing regular bindings file, refuses symlinks, creates a private byte-for-byte backup alongside it, and atomically replaces the file while preserving its mode and all unrelated bytes. After saving, it runs `hyprctl reload`, `hyprctl configerrors`, and checks the effective binding. If validation fails, it reports the saved edit and backup path; review the error before retrying. It does not blindly restore a backup over later user changes.

Remove only this shortcut with:

```sh
./scripts/desktop-shortcut remove
```

Removal requires the exact unchanged owned marker block. It preserves unrelated changes made after installation and refuses to remove a modified or duplicated block. Backups remain for review. Removing the plugin does not automatically remove this optional shortcut; remove it separately.

Fixture checks use temporary config files and a mocked `hyprctl`; they never change the desktop:

```sh
python3 tests/desktop_shortcut.py
```

After native installation, confirm `status` says `installed` and physically press Super+B twice to open and close the panel. The fixture suite verifies file ownership boundaries, conflict refusal, IPC failures, backups, reload validation, and byte preservation. Physical keyboard and native panel behavior remain a separate graphical smoke check.

The native binding convention is defined in the pinned Omarchy source at `default/hypr/helpers.lua` (`o.bind`); stock shell toggles use it in `default/hypr/bindings/utilities.lua`. The revision is recorded in `DESIGN.md`.

Message and reply headers prefer the Buzz profile display name (falling back
to the profile name) from the current room's verified recipient snapshot. Hover
a header to see the exact public key. Names are self-asserted labels, not unique
identities; mentions continue to target exact keys. Missing profile names use a
short key. The current recipient snapshot is bounded to 20 members, so historical
authors outside that snapshot may still display a key.
