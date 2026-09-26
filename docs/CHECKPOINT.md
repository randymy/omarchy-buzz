# Development checkpoint — 2026-09-26

Current version: **0.0.3 messaging development preview**, not a community release.
Implemented: source-grounded design; native hosted/custom setup; Rust daemon and
QML bridge; Secret Service identity enrollment; bounded local IPC and systemd
units; exact-ID connection freshness; signed room discovery; conservative recent
history snapshots; plain-text sends with durable metadata-only delivery tracking;
and exact public-key mentions selected from a verified room roster. Optional
self-asserted names never establish agent classification or authority.

Unread counts, verified human/agent badges, notifications, live conversation
updates, ACP execution and approvals remain unfinished. No production identity
is enrolled, no relay is configured, and no production messages were sent.
Hosted setup uses the official buzz.xyz handoff; connecting the community URL
and existing identity remains manual. Self-hosted relays use the same adapter.

## Current handoff

The deferred pending-request issue is fixed and installed. The actor rejects
changed room/text/mentions/generation under a pending UUID through a correlated
reply, while preserving the original watched receipt and acknowledgement.
Identical replay does not re-sign; another pending request is busy. Lost actor
replies are explicitly unknown, and QML retains rejected drafts without treating
late receipts as acknowledgement of changed intent.

Runtime source is committed in `617bd73`; the installed plugin was updated to
`17c1cf9`, including notice packaging, downstream documentation and clearer
incompatible-helper update guidance.
The installed 0.0.3 helper SHA256 is
`cfaa915036f1cf26b553a34ec93dbc3c0f893b11f9f106cc0e7eb6844054bebf`.
The prior installed helper is preserved as
`~/.local/bin/omarchy-buzz.before-617bd73`, in addition to earlier `.previous`
backups. Native panel summon passed after service and shell restart; service
inspection reports active/running with core dumps disabled.

A verified local ARM64 development archive and checksum manifest are saved in
`artifacts/local-20260926-checked/` (ignored by Git). The archive is 7.7 MB and
matches the installed binary. It includes a 252-package dependency notice
inventory, with 19 review flags and no compliance or public-release claim.
Five cached packages lack notice text. An exact-commit nostr notice and its
verified provenance are separately saved under ignored
`artifacts/supplemental-notices/`; it has not yet been integrated into the
archive inventory. Four Bitcoin packages lack exact source revision metadata,
so their notice provenance remains unresolved. See
[DEPENDENCY_NOTICES.md](DEPENDENCY_NOTICES.md).
All public publication and production-relay work remains deferred.

## Validation and installed state

The installed ARM64 Rust suite passed 85 tests (0 failures). Actual rebuilt
helper IPC and QML/helper bridge tests pass, as do offscreen QML rejection
checks and the composer Process fixture. Notice and packaging suites pass six
offline tests. Tests cover
synthetic signed HTTP/WS flows, freshness, authentication, membership revocation,
request/scope fencing, durable-ledger failure/restart behavior, exact recipients,
and IPC bounds. Actual daemon/bridge and QML composer process tests pass, as do
isolated Secret Service and inherited-socket activation tests. These fixtures
are not certification against a deployed Buzz relay.

The development plugin was removed with the native manager and reinstalled from
this local Git repository. Native enable/summon and the setup panel passed a live
visual check. The matching 0.0.3 helper is installed at
`~/.local/bin/omarchy-buzz`; previous helper/unit copies retain `.previous`
suffixes. The service has `LimitCORE=0`. Its socket is enabled for future logins;
without enrollment it does not connect to a relay. The tested shell needed
`omarchy restart shell` to load changed QML reliably. Plugin rescan is asynchronous;
wait for the plugin to appear before enabling it.

The optional Super+B binding is installed. Conflict detection, effective native
binding, clean Hyprland configuration, removal/reinstall and preservation of
unrelated bindings passed. A physical keypress and multiple-monitor behavior
remain unverified. Remove only the owned binding with `scripts/desktop-shortcut
remove` when needed; the script retains backups and refuses modified blocks.

## Updating Buzz

The helper pins official Buzz revision
`781d39510cf23cfe224e8f521ae06a23377e06de`. The official upstream HEAD check at
07:52 UTC matched this pin. `scripts/check-upstream` and the daily GitHub workflow
prepare tested draft updates, never automatic deployment. The private remote `randymy/omarchy-buzz` now exists. Initial remote validation
passed in run 36278075520. The daily update workflow is on the default branch;
a completed scheduled check or successful draft creation is not yet established. Plugin/QML updates
do not replace the helper binary. See [UPDATES.md](UPDATES.md).

## Next release gates

1. Validate against an actual isolated Buzz relay, following
   [RELAY_TESTING.md](RELAY_TESTING.md), and resolve upstream WS resource bounds
   described in `helper/WS_UPSTREAM.md`.
2. Resolve missing third-party notice texts and review the generated dependency
   inventory before distributing binaries. Current local archives remain
   development previews. See [PACKAGING.md](PACKAGING.md).
3. Run remote CI and target-platform checks; verify physical keyboard and
   multi-monitor behavior. Complete local unread and native notifications.
4. Add separately supervised upstream ACP, truthful agent state and the first
   end-to-end human/agent demo. Do not use the production relay as a test fixture.
5. Publish the independent repository and submit the community listing when
   release gates pass. vPerps consumes the same plugin; no fork is required.

## Shutdown-safe build recovery

Source, lockfile, installed binary, units and shortcut persist across shutdown.
The temporary build output at `/tmp/omarchy-buzz-build-20260926` does not.
The Buzz checkout's generated `.hermit/rust/registry` is a symlink to
`/tmp/omarchy-buzz-cargo-registry`, moved because the home filesystem is nearly
full. A verified 52 MB archive of registry downloads/index is saved at
`~/.cache/omarchy-buzz/cargo-registry-20260926.tar.gz`. Restore the symlink target
on the next boot before building (Cargo reextracts source from the saved crates):

```sh
mkdir -p /tmp/omarchy-buzz-cargo-registry
tar -xzf ~/.cache/omarchy-buzz/cargo-registry-20260926.tar.gz -C /tmp/omarchy-buzz-cargo-registry
cd ~/Projects/buzz
./bin/cargo test --locked --offline \
  --manifest-path ~/Projects/omarchy-buzz/helper/Cargo.toml \
  --target-dir /tmp/omarchy-buzz-build-20260926 -j 2 \
  --config profile.dev.debug=0 --config profile.test.debug=0
```

The inspected wrapper supplies Rust 1.95. Keep build targets on a filesystem with
enough free space. No source depends on these temporary paths. Local Git history
is the primary checkpoint; a Git bundle under `~/.cache/omarchy-buzz/` provides a
second local copy. Committed source is backed up to the private GitHub remote;
ignored artifacts remain local. No public release or marketplace listing exists. No private relay addresses or desktop screenshots are committed.
Upstream Buzz, Omarchy and vPerps source files remain unmodified.

Prepared next: manual disposable real-relay workflow and ignored component
conformance test. Local compilation and six runner tests pass; remote relay
execution remains pending. ACP limitations are in ACP_VALIDATION.md.
