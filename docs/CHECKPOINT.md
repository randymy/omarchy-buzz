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

## Shutdown handoff

Final delivery-review source changes are saved but are **not installed**. They
preserve a pending receipt on repeated/busy sends and add a signed request UUID
tag to distinguish identical intentional submissions in the same second. The
installed helper remains the previously validated 78-test build. Resume by
resolving the pending-replay issue below, then
rebuilding, testing and installing the helper.

Known follow-up: a repeated pending UUID with different text/mentions currently
returns the original receipt. Although it never re-signs or republishes, the
receipt can be mistaken for acknowledgement of changed intent. Add an explicit
correlated rejection without replacing the active watched receipt, ideally via
a typed internal command with a one-shot reply. Retest original acknowledgement
visibility and changed-intent rejection before installing this source.

Local packaging tooling and three offline tests are saved; no final archive has
been produced. All publication and production-relay work remains deferred.

## Validation and installed state

The final saved ARM64 Rust suite passed 81 tests (0 failures). The installed
binary remains the preceding 78-test build. Tests cover
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
prepare tested draft updates, never automatic deployment. There is no remote,
so the scheduled workflow and remote CI are **not active yet**. Plugin/QML updates
do not replace the helper binary. See [UPDATES.md](UPDATES.md).

## Next release gates

1. Validate against an actual isolated Buzz relay, following
   [RELAY_TESTING.md](RELAY_TESTING.md), and resolve upstream WS resource bounds
   described in `helper/WS_UPSTREAM.md`.
2. Finish third-party license/notice inventory before distributing binaries;
   current local archives are development previews. See [PACKAGING.md](PACKAGING.md).
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
second local copy, not a remote backup. No public remote or marketplace listing
exists. No private relay addresses or desktop screenshots are committed.
Upstream Buzz, Omarchy and vPerps source files remain unmodified.
