# Development and validation

The current preview includes authenticated messaging and selected-room automatic
history refresh. Use synthetic fixtures for automated checks, never production
rooms as test fixtures. Earlier M0/M1 evidence below is historical.

For 0.0.4, `node tests/activity_observer.cjs` checks the bounded notification
observer and `./scripts/preview --activity` exercises QML boundaries with a fake
notification executable. `tests/helper_keyring.py` uses a private Secret Service
and synthetic NIP-11 endpoint to verify human enrollment and relay-key refusal.
The Rust suite includes automatic history refresh, failure clearing and reauth
fencing. GitHub validation remains manual-only.

## Repeatable manifest check

Install Bash, Git, jq, and find. Keep an Omarchy source checkout at the revision recorded in DESIGN.md: `7b336b1b0da722e7bb864a7136f91e784ef731bf`. The check reads that checkout; it does not download dependencies or change its revision.

```bash
OMARCHY_SOURCE=/path/to/omarchy ./scripts/validate
```

Without `OMARCHY_SOURCE`, the script uses the sibling `../omarchy` checkout. It requires the exact pinned HEAD and an unchanged `bin/omarchy-plugin-validate`, then runs that upstream validator against this repository. A mismatch fails with a diagnostic; use a separate checkout for a different version rather than changing a working source tree merely to run this check.

The validator checks manifest JSON/schema, required fields, plugin ID restrictions, declared kind entry points, relative existing entry-point paths, default bar section, and disallowed internal symlinks. It does not parse/load QML, certify the scoped service wiring, exercise user interactions, or establish crash isolation. M0 itself has no backend; the initial M1 helper now has separate tests described below.

GitHub Actions runs the same command with sibling plugin and Omarchy checkouts. Both checkout actions use the immutable [actions/checkout v4.2.2 commit](https://github.com/actions/checkout/commit/11bd71901bbe5b1630ceea73d27597364c9af683); Omarchy uses its exact tested commit. CI executes the pinned validator without a compositor. Record CI success separately from graphical results.

## Offscreen component check

With Quickshell installed, run `./scripts/preview`. It runs the pinned validator, stages a disposable configuration and runtime directory, loads the actual service/widget/panel-content components, checks scoped widget routing and room selection, then exits. It never enables the plugin, starts Omarchy, or changes the desktop configuration. Its test shell facade is a mock, not proof that the real shell injects it correctly.

To save the rendered sample panel:

```bash
mkdir -p artifacts
BUZZ_PREVIEW_OUTPUT="$PWD/artifacts/m0-preview.png" ./scripts/preview
```

`artifacts/` is ignored by Git. The render uses the installed theme through upstream components. Offscreen window-mask warnings are expected; restricted environments may also reject the test-only Quickshell IPC server. This test does not use that server. The check does not instantiate the Wayland `PanelWindow`, exercise physical keyboard focus, or replace the live-shell smoke procedure below.

Local evidence on 2026-09-26: pinned and installed manifest validators passed; offscreen component assertions passed; an 820×570 sample-panel image was inspected for layout/contrast. QML analysis passed with the native modules available and dynamic-property/unqualified-access/Quickshell platform-type warnings suppressed; that limited analysis is not a clean full-lint or runtime claim. Outside the IPC-restricted sandbox, native plugin enable/summon/hide worked and the shell answered `ping` afterward. A live desktop capture confirmed the centered themed panel and bar widget; the private desktop capture is not a repository fixture. At that checkpoint, keyboard interaction, multiple monitors, and full removal/reload lifecycle checks remained pending; see subsequent evidence below. CI is configured but has not run on GitHub because this repository is still local.

## Graphical smoke check

Run this in a disposable Omarchy VM/session using the tested source/package combination from DESIGN.md. A source version string alone is insufficient to establish compatibility. Do not reset the developer's desktop or modify packaged `/usr/share/omarchy` files. Preserve existing user configuration in the disposable session before installation.

1. Run manifest validation. Copy this repository into `~/.config/omarchy/plugins/community.buzz/` in the disposable session; do not replace an existing plugin directory. Run `omarchy-shell shell rescanPlugins`, then `omarchy plugin enable community.buzz --section right`.
2. Verify the widget uses native theme colors, has readable horizontal and vertical geometry, and shows only clearly labeled synthetic/unknown state. Check that one shared service supplies all widget instances when multiple monitors are available.
3. Open the panel using the widget, then `omarchy-shell shell summon community.buzz '{}'`. Check keyboard focus, Escape/close behavior, and `omarchy-shell shell toggle community.buzz '{}'` twice. Record behavior on the focused monitor and other monitors; panel placement is plugin implementation behavior, not an IPC guarantee.
4. Check the synthetic panel's text and states. Confirm no control claims a live connection, accepted send, real unread count, agent health, or approval. Confirm no helper, relay, or agent process starts.
5. With the panel open, run `omarchy-shell shell rescanPlugins`. Verify the service and UI reconstruct without duplicate widgets, stale panel state, or shell errors. Save a harmless QML change in the disposable copy to exercise user-plugin hot reload, then undo that change. `keepLoaded` services would retain their code until shell restart; this skeleton uses normal reload lifetimes.
6. Run `omarchy plugin disable community.buzz`; verify widget/panel disappear and synthetic service activity stops. Re-enable and summon again. Run `omarchy plugin remove community.buzz` and verify the shell remains usable. Native removal does not manage future external helper units.
7. Inspect `journalctl --user -t omarchy-shell --since '10 minutes ago'` for load/binding errors. Record Omarchy package/source revisions, architecture, commands, monitor/theme observations, failures, and unavailable checks. A skipped graphical check is not a pass.

Optional shortcut and menu installation are separate later setup work. M0 smoke checking uses IPC directly and does not change keybindings. Exact Super+B is free in the inspected baseline, but effective bindings must be checked before any opt-in binding is installed.

When a visual change is made, record direct visual verification in addition to the manifest check. Do not claim all of M0's real-shell acceptance evidence from a headless CI run.

## Initial M1 helper evidence

On 2026-09-26, Rust 1.95 on Linux ARM64 built the pinned helper and passed all
seven configuration/protocol tests. The isolated `tests/helper_smoke.py` passed
unconfigured status, subscribe/snapshot, malformed and oversized requests,
bridge EOF, and SIGTERM socket cleanup. The test uses private temporary XDG
directories without an identity or session D-Bus. No relay authentication occurs.
A sandbox that denies Unix sockets cannot run this process test.

```bash
cargo test --locked --manifest-path helper/Cargo.toml
cargo build --locked --manifest-path helper/Cargo.toml
python3 tests/helper_smoke.py helper/target/debug/omarchy-buzz
```

The local build used a temporary target directory because the home filesystem
had insufficient free space. CI now includes these checks, but remote CI has
not run. QML still uses sample data. Live keyring/enrollment, authenticated relay
behavior, socket activation, HTTP queries, hostile-relay resource limits, and
x86_64 execution remain unverified. See `service/README.md` for draft supervision
and cleanup instructions; those units have not been installed.

## M1 connection preview checkpoint

The production service now uses the helper bridge; sample data requires explicit
test mode. Nine Rust tests pass, including configuration reload/generation tests.
`./scripts/preview` passes protocol/UI assertions. The actual bridge test passes:

```bash
BUZZ_TEST_HELPER=/absolute/path/to/omarchy-buzz ./scripts/preview --bridge
python3 tests/helper_activation.py /absolute/path/to/omarchy-buzz
python3 tests/helper_keyring.py /absolute/path/to/omarchy-buzz
```

The activation test passed real 30-second idle exit and inherited socket reuse.
The optional keyring test passed enrollment and retrieval with a synthetic key in
a separate D-Bus/Secret Service session. It requires `dbus-run-session` and
`gnome-keyring-daemon`; it never uses the user's keyring. It checks an unavailable
loopback relay, not successful relay authentication. All process/socket tests
need a sandbox that permits their local sockets.

Outstanding: native production panel visual check, installed systemd activation
end to end, auth accept/reject/transport conformance, HTTP seam and upstream
resource limits. The earlier M0/initial M1 evidence above is historical.

## Native activation and authentication transport evidence

The installed plugin at 0.0.2 now displays the production Setup panel, with no
synthetic rooms. Opening/loading the plugin activates the separately installed
systemd helper through its socket. Private desktop captures were inspected and
not committed. Omarchy rescan and disable/re-enable retained old QML after the
update; `omarchy restart shell` loaded the correct version. Source inspection
found an optional guarded component-cache clear and stable entry URLs, consistent
with this observed limitation; the exact Qt cache root cause is not proven.

The Rust suite now has twelve passing tests, including a synthetic loopback WS
relay verifying signed NIP-42 kind, author, relay/challenge tags, exact matching
acknowledgment, safe rejection category, and cached-challenge reauthentication.
These use the production connection function and upstream signing/verification.
They are transport conformance tests, not a deployed Buzz relay certification.

CI now includes inherited-socket reactivation and isolated real Secret Service
tests. Remote CI still has not run. HTTP queries, hostile-peer memory bounds,
heartbeat/reconnect behavior, and messaging remain outstanding.

## Signed HTTP seam and hosted setup

Twenty Rust tests now pass on ARM64. The new fixed-origin `/query` adapter was
validated with synthetic HTTP responses: exact NIP-98 body binding, unique
nonces, redirect refusal, length/chunk bounds, signature/scope rejection, and
concurrency limits. It is not yet wired to room discovery or QML.

The hosted/custom selection passed native visual review. No hosted account was
created and no browser login was initiated. Native plugin disable left the shell
responsive and the helper inactive after idle grace; re-enable reactivated it.
At that checkpoint, full removal and multi-monitor/keyboard checks remained pending.

## Read-only history checkpoint

The read-only checkpoint passed 44 ARM64 Rust tests. Integrated synthetic relay tests
exercise authentication, exact-ID freshness probes, trusted `/info` discovery,
signed joined-room metadata and NIP-CW history, unauthorized-room rejection,
and clearing completed/in-flight history on reauthentication. The history
reducer tests edits, deletions, unresolved authority, signed bounds, scope,
truncation and whole-page limits. Maximum combined catalog/history serialization
fits the 64 KiB IPC frame even with escaped display text.

The rebuilt helper passes isolated daemon/bridge smoke tests and the actual
QML-to-helper process test. Offscreen QML tests cover room selection, stale
responses, malformed history, busy command queues, and disconnection. A synthetic
history screenshot was inspected after reducing redundant notices; it is not a
capture of live conversations. Earlier checkpoint counts above are historical.

These fixtures do not establish compatibility with a deployed Buzz relay.
Sending, profile classification, unread accounting and live history are not
implemented. See [sending plan](SENDING.md) for the next stage.

Installed development plugin `06850f8` and the rebuilt helper passed a native
setup-panel visual check after `omarchy restart shell`. The user service is
running with `LimitCORE=0`; public inspection confirms no relay/identity is
configured. Native history rendering uses synthetic offscreen fixtures because
no production identity was enrolled. Actual-relay validation needs the isolated
environment described in [RELAY_TESTING.md](RELAY_TESTING.md); this machine lacks
its database/container dependencies and has limited home-disk space.

## Scoped sending and exact recipients

The current ARM64 Rust suite passes 78 tests. New coverage includes durable
metadata reservations, file ownership/symlink/hardlink refusal, locked state,
crash/restart ambiguity, capacity limits, exact acknowledgement matching,
persistence failure before publication, preserving observed acknowledgement on
outcome-write failure, 15-second acknowledgement timeout while COUNT remains
fresh, and reauthentication during delivery. Signed roster/profile fixtures
exercise exact mention keys, nonmember refusal, fallback names and scope cleanup.

The rebuilt daemon passes IPC instance/generation fences and offline send denial
without text logging. Isolated activation and private Secret Service tests pass.
The QML composer process fixture verifies actual JSON requests, double-click
suppression, exact-key mentions, one post-ack refresh, and lost receipt preserving
the draft without retry. `./scripts/preview --send-bridge` runs it without a relay.
Shortcut fixtures cover conflicts, byte preservation, owned-block removal and
Lua dispatcher visibility. Native Super+B installation has clean configerrors;
its physical keyboard path remains separate from script/effective-bind checks.

These checks still do not certify the full deployed Buzz relay or ACP runtime.
The upstream tracker’s offline tests validate pin consistency and candidate lock
sources; remote GitHub Actions have not run because there is no public remote yet.

Final recipient review added regression coverage for known membership revocation
blocking plain sends and clearing history, and bidirectional-formatting controls
in display names. QML preserves per-room recipient intent during refresh and
blocks delivery until every selected key is revalidated; removed recipients
require explicit deselection. Native shortcut removal and reinstall both passed
with clean configuration and unrelated bindings preserved. The official GitHub
Buzz HEAD check at 07:52 UTC matched the pinned revision.

Native installation follow-up: `omarchy plugin remove community.buzz --yes`, a
fresh local clone, enable, helper replacement and shell restart all passed.
Plugin rescan is asynchronous; wait until listPlugins shows the plugin before
enabling it. The live setup panel was visually checked after restart. The socket
is now enabled for future user logins; no relay or identity is configured.

## Pending-request binding and packaging follow-up

The installed ARM64 helper now passes 85 Rust tests. A synthetic relay integration
proves that changed text or mentions under a pending UUID receive a correlated
rejection, another UUID receives busy, identical replay retains its receipt,
and the original event still receives its real acknowledgement. Cancellation
checks retain unknown outcomes when delivery cannot be established.

The actual helper IPC test checks the executable's version against both the
plugin manifest and crate, and its backend revision against the source pin.
It also verifies offline rejection does not replace the shared delivery view.
QML component and Process fixtures pass correlated rejection, late receipt,
draft preservation and connection continuity. The rebuilt actual QML/helper
bridge passed outside the local-socket sandbox. Native summon and active user
service checks passed after installing the matched build and restarting shell.

Six offline notice/packaging tests pass. The local development archive checksum
and included binary were verified against the installed helper. Dependency
notices remain explicitly review-required; this is not release certification.

## Real-relay and ACP component evidence, 2026-09-26

Private GitHub CI is now active. Ordinary validation passed at `ee3eb48`
in run 36280596816. The pinned actual relay passed the helper messaging test
in run 36280427402; messaging plus upstream synthetic ACP mention routing
passed in run 36280557661, both at plugin `f276997`. Sanitized summaries are
committed in `docs/evidence/`. Earlier entries above describe their historical
checkpoints, not current capabilities. This is not full daemon/UI, media/Git,
or model-backed agent certification. See CHECKPOINT.md for remaining gates.

## Activity and profile validation (0.0.5)

Run `node tests/room_activity.cjs`, `scripts/preview --room-activity`,
`scripts/preview --notification-preference`, and `scripts/preview --activity`.
The offscreen notification fixtures shadow the native notifier and assert fixed
argv without producing a real desktop alert. Rust tests validate signed agent
profile scope, spoofed identity/status fields, malformed latest profiles,
access-denied revocation, and bounded activity counters.

## Thread preview development (0.0.7)

`scripts/preview --thread-replies` exercises synthetic QML frames: exact root
selection, stale-root rejection, disconnect/generation clearing, unsupported
helpers and malformed rows. `scripts/preview` checks the native panel components.
The helper tests validate signed, request-bound NIP-CW thread windows and scoped
IPC; the manual harness room workflow also fetches its persisted reply through
the same thread reducer. No production relay is used by these tests.

The first view is read-only, depth one, with up to eight replies, manual refresh and an eight-second refresh while
the panel is open. Failed reads pause refresh until an explicit retry. The existing composer sends top-level messages. Pagination, replying
inside a thread, automatic thread notifications and detailed execution state
are separate increments. Responses are bounded to 1 MiB; incoming commands
remain bounded to 64 KiB. Install matching helper/plugin versions together.

## Thread composition and installation (0.0.8)

The new `thread_send` capability enables optional `rootId` on `send_message`.
The helper checks the selected, verified, available history root and matching
thread snapshot; SDK signing and durable request identity bind the destination.
Existing top-level ledger records remain readable. An old helper cannot parse
new thread records; downgrade disables sending instead of forgetting receipts.

Run `scripts/preview --thread-send` for the rendered composer's destination,
draft, receipt, stale-root and older-helper checks. `--send-bridge` verifies
actual process framing and ordinary-send regression behavior. Rust sender tests
cover signed reply tags and root-bound replays, and the isolated real-relay
fixture now publishes and queries a thread reply. It uses synthetic identities.

`tests/helper_install.py` tests trusted-package validation, unit ownership,
target loadability, dry runs and rollback after partial service activation.
Use `scripts/helper-install install --dry-run ARCHIVE` before a local install.
The installer requires this checkout's exact version and Buzz dependency pin.

## Preview modes

`scripts/preview` with no argument checks the sample panel. Each mode below runs one
rendered QML check against synthetic frames or a stdio fixture:
`--send-bridge`, `--thread-send`, `--thread-replies`, `--live-updates`,
`--catalog-refresh`, `--catalog-refresh-send`, `--new-dm`, `--older-history`,
`--last-room`, `--mentions`, `--author-names`, `--identicon` (identicons and
pasted avatar art), `--activity`, `--room-activity`,
`--notification-preference`, `--agents` (the Agents section against a fake agent
service), `--presentation` (needs Wayland) and `--bridge` (needs
`BUZZ_TEST_HELPER` set to a built helper).
