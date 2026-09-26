# Development checkpoint — 2026-09-26

Implemented: source-grounded design; native hosted/custom connection setup;
Rust daemon/bridge with Secret Service enrollment and Retry reload; bounded IPC;
systemd units; signed HTTP query seam; CI and security documentation. No messaging,
agent execution, notifications, or approvals yet. No production identity enrolled.

Passed: pinned manifest validation, twenty Rust tests on ARM64, isolated daemon/
bridge, inherited socket idle reactivation, private GNOME Secret Service enrollment/
retrieval, QML protocol/bridge tests, synthetic NIP-42 and NIP-98 transport tests.
Native setup screen inspected; systemd activation and disable/idle/re-enable passed.
Hosted setup defaults to the official buzz.xyz handoff; linking community URL and
existing identity remains manual. No global public relay is assumed.

Installed plugin has the hosted/custom UI. Installed helper predates the new
HTTP seam (which is intentionally unwired). The developer socket is started, not
enabled for future logins. No relay configuration or credentials are installed.
The shell required `omarchy restart shell` after plugin code updates because
rescan and disable/re-enable retained old compiled QML on this package.

Next:

1. Resolve upstream WS buffer/frame bounds and heartbeat/recovery guarantees.
2. Establish relay signer trust/capability discovery before using the HTTP seam
   to project membership and room metadata. No generic signing/request IPC.
3. Validate real isolated Buzz relay behavior, then proceed to M2 messaging.
4. Complete native keyboard/multi-monitor/removal checks and remote CI.

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
