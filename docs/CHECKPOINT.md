# Development checkpoint — 2026-09-26

Saved for resuming after the developer disconnects. This is not a release claim.

Implemented: source-grounded DESIGN; native plugin; Rust daemon/bridge with
Secret Service enrollment; status-only QML integration; bounded IPC; Retry
configuration reload; optional systemd units; tests and security documentation.
Messaging, agent execution, notifications, and approvals remain unimplemented.

Passed: pinned manifest validation, nine Rust tests on ARM64, isolated daemon/
bridge smoke, inherited socket idle reactivation, isolated GNOME Secret Service
enrollment/retrieval, offscreen sample/protocol assertions, and actual offscreen
QML-to-helper setup/retry/missing-helper/daemon-exit integration.

Next steps:

1. Review and verify installed systemd service/socket end to end. The developer
   socket was started, not enabled for future logins; no identity is configured.
2. Update the installed development plugin from this committed source and
   inspect the production setup panel visually. The installed copy still shows
   the older sample UI; the installed helper predates Retry configuration reload.
3. Test reload, disable/remove, and recovery with the actual shell.
4. Complete isolated authenticated relay tests, transport limits/heartbeat, and
   the signed HTTP query seam before claiming M1 complete or starting messaging.
5. Add activation/keyring checks to appropriate CI jobs; CI has not run remotely.

Local build artifacts were moved to `/tmp/omarchy-buzz-build-20260926` because
the home filesystem is almost full. They are disposable and may vanish after
reboot. Rebuild with Rust 1.95 and the committed lockfile. The inspected Buzz
checkout's `bin/cargo` wrapper supplies this toolchain; run it from that checkout.
The successful local build used `--target-dir /tmp/omarchy-buzz-build-20260926`
and `--config profile.dev.debug=0 --config profile.test.debug=0`.

No production Buzz identity or private relay configuration has been used or
committed. No remote repository or marketplace listing exists yet. Keep private
relay addresses and desktop screenshots out of the public repository. vPerps
and upstream source trees remain unmodified.
