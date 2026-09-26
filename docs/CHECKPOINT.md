# Development checkpoint — 2026-09-26

Implemented: source-grounded design; native hosted/custom setup; Rust daemon/
bridge with Secret Service enrollment; bounded IPC and systemd units; exact-ID
connection freshness; signed HTTP room discovery; conservative recent-history
projection and native room/history panel. Sending, unread, verified human/agent
badges, notifications, and agent execution remain unfinished. No production
identity is enrolled.

Passed: 44 Rust tests on ARM64, including synthetic WS/HTTP room/history flows,
reauthentication cleanup, stale-result fencing, bounds/signature/scope limits,
and maximum combined IPC frame size. Rebuilt daemon/bridge smoke and actual
QML/helper process tests pass. Offscreen history rendering and hostile input
checks pass. Earlier milestones passed isolated Secret Service enrollment and
socket idle reactivation. These are not a deployed Buzz relay certification.

The installed plugin/helper are being updated after this source commit; verify
installed HEAD before assuming the native copy matches source. The developer
socket is started, not enabled for future logins. No relay configuration or
credentials are installed. Hosted setup defaults to the official buzz.xyz
handoff; linking the community URL and existing identity remains manual.
The tested shell needed `omarchy restart shell` to load changed QML reliably.

Next:

1. Validate against an actual isolated Buzz relay; resolve upstream WS buffer
   bounds before release. See helper/WS_UPSTREAM.md.
2. Implement the reviewed safe sending path in docs/SENDING.md, then exact
   mentions/profile classification and local unread accounting.
3. Complete native keyboard/multi-monitor/removal checks and remote CI.
4. Integrate ACP as a separate process after messaging works.

Local build artifacts are in `/tmp/omarchy-buzz-build-20260926`. Disk exhaustion
also required moving the generated Cargo registry to
`/tmp/omarchy-buzz-cargo-registry`, with a symlink at the inspected Buzz checkout's
`.hermit/rust/registry`. Both caches are disposable. After a reboot, remove only
that dangling generated registry symlink and let Cargo recreate its registry on
a filesystem with enough free space, or restore the cache before reboot.
Source and lockfile are committed; no source depends on these temporary paths.
Use Rust 1.95; the inspected Buzz `bin/cargo` wrapper works from its checkout.
Build target overrides and debug=0 kept artifacts off the nearly full home disk.

No public remote/marketplace listing exists. No private relay addresses or desktop
screenshots are committed. Upstream and vPerps source files remain unmodified.
