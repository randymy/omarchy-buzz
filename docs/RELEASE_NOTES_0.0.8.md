# 0.0.8 — thread composition and simpler helper setup

Installed and tested development preview; not yet a public release or full room-agent product.

- Reply inside a verified thread from the native panel or normal window.
- Keep room and thread drafts/mention selections separate; ambiguous delivery
  and unavailable roots cannot silently change the destination.
- Install, upgrade or uninstall a trusted local helper package with
  `scripts/helper-install`; preview operations with `--dry-run`. Upgrades retain
  rollback copies, and uninstall preserves identity/configuration/message state.
- Open Buzz from application search using the optional [desktop launcher](NATIVE_LAUNCHER.md).
- Use system D-Bus rather than vendoring it in the helper binary.

Install the matching UI and helper together. The newer UI can display threads
with an older helper, but enables reply composition only when `thread_send` is
advertised. Upstream WebSocket resource bounds, integrated room-agent supervision,
and distribution review remain separate release work.

Existing top-level delivery ledgers load without migration. New thread sends
record their root ID to prevent submission reuse across threads. After a thread
has been sent, downgrading to 0.0.7 makes its strict ledger reader disable
sending. Keep the ledger and return to 0.0.8; do not delete delivery history to
work around a downgrade.

On the tested Omarchy build, a plugin rescan can retain old compiled QML. After
upgrading both components, run `omarchy restart shell`, then reopen Buzz and
verify the rendered window shows authenticated rooms. A healthy helper socket
and matching files alone do not prove the running UI loaded the new code.
