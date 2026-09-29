# Development checkpoint — 2026-09-28

Current source version: **0.0.7 thread-view development preview**, not a community release.
Implemented: source-grounded design; native hosted/custom setup; Rust daemon and
QML bridge; Secret Service identity enrollment; bounded local IPC and systemd
units; exact-ID connection freshness; signed room discovery; conservative recent
history snapshots; plain-text sends with durable metadata-only delivery tracking;
and exact public-key mentions selected from a verified room roster. Optional self-asserted names never establish authority. Signed kind-10100
profiles identify only self-described agents with unknown execution state.

Selected-room history refreshes automatically; bounded background polling
provides local activity badges across the shown joined rooms. Content-free
notifications are opt-in with a saved preference. Counts are session-only
observations, not synchronized unread state. Full live synchronization, ACP
execution and authoritative approvals remain unfinished. A separate human identity is enrolled
on the operator's self-hosted relay; the operator confirmed cross-client sending.
Hosted setup uses the official buzz.xyz handoff; connecting the community URL
and existing identity remains manual. Self-hosted relays use the same adapter.

## Current handoff

The entries below preserve milestone history. See the latest dated entry at the
end for installation and validation status; old unconfigured/uninstalled claims
are historical and are superseded by the successful human enrollment.

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
`f9d3a577d01e344d2db0ac610ecb1fc53d55b93a60d7e3891cbe20f7c4072049`
(after the hosted metadata fix described below).
The prior installed helper is preserved as
`~/.local/bin/omarchy-buzz.before-617bd73`, in addition to earlier `.previous`
backups. Native panel summon passed after service and shell restart; service
inspection reports active/running with core dumps disabled.

A verified local ARM64 development archive and checksum manifest are saved in
`artifacts/local-20260926-checked/` (ignored by Git). The archive is 7.7 MB and
matches the installed binary. It includes a 252-package dependency notice
inventory, with 19 review flags and no compliance or public-release claim.
Five cached packages lack notice text in that archive. An exact-commit nostr
notice and its pinned provenance are now tracked under `docs/evidence/notices/`;
the generator can include them explicitly with `--supplemental`, while retaining
a review flag. The existing archive predates that option and was not rebuilt.
Four Bitcoin packages lack exact source revision metadata,
so their notice provenance remains unresolved. See
[DEPENDENCY_NOTICES.md](DEPENDENCY_NOTICES.md).
Public publication and authenticated production-relay work remain deferred.

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

## Isolated relay progress, 2026-09-26

The pinned actual relay now builds, starts, and passes readiness on disposable
GitHub runners. [Run 36280427402](https://github.com/randymy/omarchy-buzz/actions/runs/36280427402)
passed the actual messaging component test and cleanup completed. Its sanitized
summary is saved in `docs/evidence/relay-messaging-36280427402.json`. Earlier runs
found inaccessible MinIO images and unsupported internal-only Docker port
publication. The fixture now explicitly excludes object storage and uses
loopback-published ports on a dedicated bridge; relay egress is not blocked.
No workstation identity or production relay has been used.

Ten runner fixtures pass locally. Source review corrected the revocation test:
Buzz may filter inaccessible data instead of returning 403. The test checks an
owner-visible signed roster change plus the removed member's rejected write;
it does not claim an empty query is an authoritative denial. Fixed protocol
stage labels support sanitized failure diagnostics. The passing run uses plugin
commit `f276997` and the exact Buzz pin above. This proves helper components;
full daemon/QML integration, automatic revocation observation, unread state,
notifications, edit/delete overlays and reconnect/lost-receipt cases still need
real-relay coverage. Git/media/object storage were excluded.

Optional synthetic ACP routing is prepared behind the manual workflow's
`synthetic_acp` input, default false, and executes only after messaging passes.
[ACP run 36280557661](https://github.com/randymy/omarchy-buzz/actions/runs/36280557661)
passed messaging and synthetic ACP routing, with cleanup. Evidence is saved in
`docs/evidence/relay-acp-36280557661.json`. No model-backed agent has been
exercised. ACP key
input and permission limitations remain in ACP_VALIDATION.md. The read-only
upstream tracking workflow completed successfully in run 36279926788; candidate
updates and scheduled execution remain separately unverified.

Final verification: [run 36280910334](https://github.com/randymy/omarchy-buzz/actions/runs/36280910334)
passed both messaging and synthetic ACP at `1f4c22f`, with cleanup complete.
The revised ACP fixture sends one trigger after the supervisor's bounded
post-spawn handshake, then requires a signed room reply; process startup alone
is never agent readiness. This avoids queued readiness prompts contaminating
the bounded excluded-trigger observations. Evidence is saved in
`docs/evidence/relay-acp-36280910334.json`. Local test compilation and ten runner
fixtures also passed. Ordinary CI passed at the same revision in run
36280897277. Production relays and persistent agent identities remain
unused; deployed compatibility and upstream secure identity/permission handling
are the next gates.

## Workflow notification preference

Automatic push/PR validation and daily upstream checks are paused at the user's
request to stop noisy development run notifications. All three workflows remain
available through `workflow_dispatch`; failures remain real failures. Do not
resume automatic triggers or routine CI trial-and-error runs without revisiting
this preference. Prefer local checks and batch remote validation deliberately.
This does not change GitHub account notification settings: a manually triggered
run can still notify its initiating account. Historical test evidence is retained.

## Deployed discovery and hosted metadata fix, 2026-09-27

Read-only checks reached the supplied self-hosted relay on HTTP port 3000:
`/info` reports Buzz 0.2.0 and a signing identity; `/_readiness` reports ready.
Standard HTTPS is unavailable. The helper requires remote HTTPS/WSS, so native
connection is blocked pending a suitable TLS endpoint. No identity, messages,
rooms or relay settings were changed. A usable Mac mini SSH account/alias is
needed to inspect the deployment and prepare that change; no private keys or
passwords should be supplied to the assistant.

The upstream-documented hosted onboarding relay returned valid TLS metadata
advertising 0.2.1. Its inline icon makes the document 35,782 bytes, exceeding our
old 32 KiB cap. The helper now allows a bounded 128 KiB document, still returns
only the signing identity, and retains redirect/timeout/pin checks. Eight catalog
tests pass, including a larger-icon HTTP fixture and exact-limit rejection.
The rebuilt helper is installed with rollback at
`~/.local/bin/omarchy-buzz.before-hosted-metadata-20260927`; service restart passed,
active/running with LimitCORE=0. It remains unconfigured and unenrolled.

See [DEPLOYED_COMPATIBILITY.md](DEPLOYED_COMPATIBILITY.md) for the source-backed
HTTPS plan, hosting requirements and limits of these observations. Neither
deployment's advertised version proves compatibility with all authenticated
interfaces. GitHub workflows remain manual-only; no remote CI was triggered.

## SSH access and TLS provisioning follow-up

The operator authorized a dedicated SSH key, and Mac mini access is working.
Deployment/source inspection found Buzz `8342dfcc5890b81a269a8ec3db73a8a56f76ce79`
with a configured WSS port-3000 origin but a plain HTTP Docker listener.
Tailscale Serve is currently disabled at the tailnet level. Provisioning the
staging HTTPS listener is waiting for the operator to enable Serve through
Tailscale's authenticated browser page. The relay has not been restarted or
reconfigured. Details and the refined origin-preserving plan are in
DEPLOYED_COMPATIBILITY.md. No GitHub workflows were started.

## Secure relay transport completed, 2026-09-27

Tailscale Serve authorization succeeded. HTTPS on the existing tailnet port 3000
now proxies directly to the unchanged loopback HTTP backend. No Docker restart,
image update, community mapping edit or origin change was needed. The temporary
443 staging rule was removed; only the port-3000 rule remains. No Funnel was
enabled. Earlier provisioning/port-move plans above are superseded.

Certificate validation, HTTP 200 metadata/readiness, signer continuity, and an
anonymous WebSocket 101 upgrade passed. These are transport checks, not proof of
authenticated compatibility. Existing plaintext Docker publication is unchanged.

Configured the installed helper's relay URL locally and restarted its service:
active/running, LimitCORE=0. Opened a hidden terminal enrollment prompt on Omarchy;
the operator must supply their existing Buzz identity privately. No credentials
were read by the assistant. Authenticated room access remains the next gate;
no production messages, subscriptions or agent tasks were sent during probing.
See DEPLOYED_COMPATIBILITY.md for scoped rollback. GitHub CI remains manual-only.

## Enrollment and authenticated discovery verified, 2026-09-27

The operator completed hidden-terminal enrollment. Restarted the helper and
observed `authenticated` with no connection error. Catalog discovery reports
`partial` / `room_catalog_partial`, with zero rooms. A read-only database count
check found one relay membership for this identity, zero active channel
memberships, and zero live stream channels on the deployment. The empty view is
consistent with that state. No conversation contents or credentials were read,
and no production room, membership or message was created. Native panel summon
succeeded. History/send and actual-agent verification still need an appropriate
real room and the previously documented agent security prerequisites.

## Room creation exposed a deployed roster mismatch, 2026-09-27

The operator created a development stream on the self-hosted community. Read-only
checks confirm the room exists, its creator is the helper's enrolled identity,
and that identity has an active canonical membership. However, its stored live
kind-39002 event contains only the room's `d` tag and no `p` member tags. The
helper's authenticated `#p` discovery therefore still returns zero rooms. This
supersedes the earlier empty-deployment explanation; identity/origin setup is
correct for the newly created room.

Deployed revision 8342dfc emits discovery events from
`crates/buzz-relay/src/handlers/side_effects.rs::emit_group_discovery_events`,
reading `db.get_members` before signing. Its metadata-edit handler also invokes
this publication path. A normal channel-description edit is a candidate scoped
repair via upstream behavior; it has not been attempted or verified. The precise
cause of the empty snapshot is not established. Do not bypass signed membership
validation, directly edit production events, or claim discovery now works.
No messages or agent tasks were sent, and no production mutation was performed
by the assistant during this diagnosis.

## Identity collision explains empty rosters, 2026-09-27

The helper identity equals the relay's public signing identity. This was checked
using public metadata/config only, without reading a secret. Deployed discovery
signing omits `allow_self_tagging`; nostr drops member tags matching the signer.
Newer inspected Buzz already preserves those tags in its roster writer. This
supersedes the previous unknown-cause diagnosis and description-edit suggestion.
The second described stream shows the same issue.

Stopped the local helper and activation socket while separating human identity
from relay identity; the remote relay is untouched. Asked the operator for only
their original Buzz human public key to check reuse without abandoning accounts.
Next: supported relay admission/channel access for that identity, hidden-terminal
enrollment, restart socket/service, then signed room discovery. Preserve existing
owner access until the replacement works. No remote membership changes, identity
deletion, or upgrade yet. See DEPLOYED_COMPATIBILITY.md for the recovery boundary.

## Human identity migration preparation

The operator supplied both hex and npub encodings; checksum-validated decoding
confirmed they represent the same relay signing identity. No separate existing
human public key is available yet. The deployed container's `buzz-admin --help`
confirms `add-member` is available, so supported server-side admission can be
prepared once a replacement public key exists. No add-member call has run.

Inspected desktop `SignOutSection.tsx`: signing out deletes the local identity,
agent settings and cached data, with explicit backup and typed-confirmation
gates. Do not treat this as a harmless account switch. Offered an independent
macOS user account to preserve the original app state, versus an explicitly
chosen backup/reset of the existing app. Await that choice before directing
destructive laptop steps. The helper remains stopped; relay and rooms unchanged.

## Separate public identity admitted

The operator supplied a distinct human public key and explicitly requested
continuation after being advised that its associated private key had been posted
in chat. The assistant did not use or copy that private key. The deployed
`buzz-admin add-member --pubkey … --role member` command successfully admitted
the supplied public identity; a read-only check confirmed the regular-member role.
No administrator or owner privileges were granted. Do not consider this identity
unexposed or silently reuse the chat private key in subsequent work.

Both requested development streams are private and the new identity is not yet
a channel member. Existing owner access is preserved; a supported room invitation
is still needed. Opened the local hidden enrollment prompt for operator input.
Helper/socket remain stopped until enrollment is verified against the supplied
public key. No channel membership mutation, message send, or relay restart ran.

## Human profile published; desktop invitation search still unresolved

Read-only checks now confirm the new human identity has a kind-0 profile, an
active users row, regular community membership, and a profile in the same
community as the requested private development room. Its generated search vector
matches the supplied display name; the event is channel-less and not future-dated.
The identity is still not a member of either requested room. The operator confirms
they are searching from the original owner profile, but the desktop picker reports
no matches. Database evidence alone does not certify the authenticated HTTP search
response or the desktop's active origin/version. Requested the laptop Buzz version
and a fresh app session on the Mac mini community to narrow that remaining gap.
No new remote mutations or credential access occurred during these checks.

## Desktop invitation failure traced to the same empty roster

Operator reports desktop 0.5.25 and confirms the Mac mini WSS origin in the
original owner session. Screenshot shows the private development stream with a
zero-member count. Inspected 0.5.25 source at 781d395:
`MembersSidebar.tsx` derives `selfMember` from `useChannelMembersQuery`; the
`canAddChannelMembers` policy requires a non-null self role for private streams.
The member search is enabled only when that policy passes. Consequently the
empty signed roster suppresses invitation search for the creator too. The
database's searchable human profile does not overcome this UI gate. Earlier
suggestions to search differently, republish the human profile, or reselect the
community do not repair it.

Next repair work must address the upstream signed roster while preserving
canonical membership and relay identity. Inspected newer buzz-admin exposes
targeted `reconcile-channels --channel`, and newer roster publication preserves
self-tags. The deployed administrator binary only advertises missing-channel
reconciliation; do not assume it supports forced targeted repair. Prepare and
validate a compatible upstream binary/schema path and rollback before production
changes. Do not use SQL membership/event edits or reopen rooms publicly as a
workaround. No upgrade, repair, invite or message was executed in this check.

## Private development-room admission unblocked

Operator explicitly authorized repair. The deployed reconciliation command cannot
replace an existing roster. Further inspection found that even newer upstream
`buzz-admin reconcile-channels --channel` still signs without self-tagging,
unlike the corrected relay roster writer. It was NOT run against production.
Downloaded upstream image 781d395 (digest
`sha256:8096413eb360785f510e4b62deeb749330a29a291423c9996c807433263e6412`)
for inspection only; the running relay and schema were not upgraded.

Instead, completed the already-authorized human admission through the normal
upstream signed operation: `buzz_sdk::build_add_member`, regular member role,
sent through `buzz_ws_client::NostrWsConnection`. A temporary one-shot operator
program loaded the existing owner key from Secret Service, verified the expected
public identity, signed once, and required the matching positive relay receipt.
No private key was read from chat, put in argv/environment, printed, or saved to
disk. Core dumps were disabled. This administration path is not installed or
exposed in plugin IPC/QML. It is not a new permanent plugin capability.

Read-only checks after the acknowledgement confirmed: the new human is an active
regular member of the requested private development room, its key appears in the
live signed 39002 roster, and the original canonical owner remains. No direct SQL
mutation, relay restart, metadata edit, or chat message was used. The deployed
self-tag omission remains for the relay identity; this unblocks the human path
without claiming that upstream bug has been repaired for every identity/room.

Opened hidden-terminal enrollment for the human identity. Helper/socket remain
stopped until configuration matches the supplied human public key. Next verify
enrollment, restart activation/service, and confirm authenticated room discovery.
Temporary maintenance source retained outside the repo at
`/tmp/omarchy-buzz-maintenance-add-member.rs`; binary is in the existing temporary
build tree. No broader admin surface was added to the plugin.

## Human enrollment and native room access verified

Operator completed hidden-terminal enrollment with the intended separate human
public identity. Started the helper socket/service. Its projected status confirms
the expected human identity, authenticated connection, and four discovered rooms,
including the requested private development stream. A scoped read-only history
request returned `snapshot` with zero visible rows and the expected conservative
`history_completeness_unknown` category. No message contents were printed and no
chat message was sent. Native panel summon succeeded. The earlier enrollment and
room-discovery blockers are resolved for this human identity; sending and a
cross-client reply remain to be exercised deliberately by the operator.

The old server-identity Secret Service entry has not been deleted; configuration
now selects the human identity. The original relay's self-tag omission and the
previously documented exposed-human-key limitation remain separate follow-ups.

## 2026-09-28 — automatic history and private activity alerts (0.0.4)

Implemented selected-room verified history polling at five-second intervals after
completion, with one in-flight request, bounded timeouts, and scope/membership
fences. Added the optional `history_auto_refresh` capability; update plugin and
helper together. Enrollment now discovers the configured relay signer and refuses
to store that identity as a human key. Discovery failure leaves enrollment intact.

Panel alerts are opt-in for the current shell session and selected room only.
Initial snapshots, reconnects, catalog uncertainty, history gaps, own messages,
edits and panel-open activity do not alert. Notifications use fixed native argv
and contain no message text or room names. This is not unread synchronization.

Validation: Rust suite 89 passed, two live/ACP tests ignored; six protocol tests
passed again after capability addition. Isolated keyring/enrollment, helper IPC,
send IPC, package tests, JS observer tests, manifest validation, full offscreen
panel and send bridge passed. Two-phase offscreen activity fixture verifies both
suppression and exactly one generic notifier invocation without desktop alerts.
No production message or agent invocation was used. GitHub CI remains manual.

Before installation, the existing helper reported disconnected, no catalog rows,
and an acknowledged delivery (no active send). Recheck connectivity after update;
do not infer the relay is reachable from earlier successful enrollment.

Installed 0.0.4 helper and matching plugin at commit `697495c` through native
`omarchy plugin update`; restarted the user helper and Omarchy shell. The helper
reports authenticated, four rooms, and `history_auto_refresh`; service is active
with `LimitCORE=0`. Native panel summon returned `ok`. Alerts default off.
Previous helper retained at `~/.local/bin/omarchy-buzz.before-0.0.4-20260928`;
previous plugin source is commit `4dda7b0`. Roll back both together with helper
service stopped, then restart helper/socket and shell. No relay changes made.

Next: implement truthful local unread state across rooms, persistent notification
preferences, and broader reconnect/access-revocation testing before the ACP
milestone. ACP permissions/credential propagation and upstream resource limits
remain release gates; this update does not claim a completed public release.

## 2026-09-28 — 0.0.5 local activity, saved preferences, agent discovery

Source implementation adds bounded background polling of the current joined-room
catalog (up to 20), selected-room polling unchanged, and memory-only observed
activity counters. These are not synchronized unread counts. Initial snapshots,
gaps, reauthentication and failures silently rebaseline. Native alerts now cover
new observed activity outside the visible conversation; the default-off boolean
preference persists across shell restarts, independently of identities/secrets.

Signed kind-10100 profiles are queried only for verified room roster authors.
At most ten sanitized self-described agent hints fit the existing 64 KiB status
limit; runtime state is explicitly unknown. Profiles cannot invent ownership,
permissions, process status or authority. Exact-key mentions use the existing
recipient picker. New capabilities: `room_activity`, `agent_profiles`.

A discovered background-revocation/selection race now aborts corresponding
history/recipient jobs and clears stale views; selected-history denial likewise
clears recipients. Synthetic two-room regression tests cover the race and prove
revoking another room preserves the selected room. No production writes or real
agent invocations are used for testing.

Real ACP execution remains blocked by inspected upstream credential propagation
and implicit permission granting. ACP_READINESS.md records exact source evidence,
implemented discovery, and concrete upstream acceptance criteria. No Buzz fork,
Omarchy fork, vPerps modification, fake approval mechanism or live agent launch
was introduced. Full task/branch/PR dashboards require reliable upstream telemetry.

Validation before installation: all 99 Rust tests pass; the two deliberately
ignored real-relay/ACP tests were not run against production. Worst-case combined
status remains below 64 KiB. Actual helper IPC smoke/send-scope tests and isolated
Secret Service enrollment pass. Offscreen component, send bridge, activity,
multiroom activity and preference persistence checks pass; notifier calls use a
fake executable. JS observer tests and package tests pass. Manual-only GitHub
workflow behavior is unchanged. Preflight reports authenticated and acknowledged
delivery, with no send in progress.

Installed 0.0.5 from `ed01778` using the native plugin updater, a matching helper
binary, user-service restart and native shell restart. Installed plugin HEAD
matches source. Native summon returns `ok`; helper reports authenticated, four
joined rooms, a history snapshot and four monitored activity entries. No agent
profile hints are present in the selected room (not fabricated as humans/idle).
Service is active/running with core dumps disabled. Previous 0.0.4 helper is
retained at `~/.local/bin/omarchy-buzz.before-0.0.5-20260928`; previous plugin
implementation is `697495c`. Restore both together with the service stopped if
rollback is needed. No relay configuration, identity or production message was
changed for validation.

Remaining boundaries: real ACP/model execution, truthful per-run dashboard,
stop/cancel and authoritative approvals cannot be completed with the inspected
upstream interfaces under this project's credential/permission requirements.
See ACP_READINESS.md for concrete upstream changes and acceptance tests. Public
community submission also remains gated by upstream WS resource bounds,
dependency-notice/provenance review and broader desktop compatibility checks.
Current local activity is deliberately sampled, session-only and distinct from
Buzz synchronized unread state. Native messaging and the generic vPerps desktop
consumption path are operational without a fork. GitHub workflows stay manual.

## Upstream readiness work, 2026-09-28

Refreshed official Buzz refs to `ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43`.
Client/ACP/managed-agent paths are unchanged; installed production pin remains
781d395. Prepared `docs/upstream/ws-resource-limits.patch` with an isolated
locked harness and source-hash-checked staging tool. All 21 client/consumer tests pass
from both the development staging tree and reconstructed patch. No upstream
checkout, installed binary, relay, identity or production conversation changed.
The contribution is ready for upstream review but has not been submitted; a
local patch is not treated as closing the release gate.

`AGENT_AUTH_COMPATIBILITY.md` records provider subscription support separately
from ACP verification. Actual adapters are absent from the inspected PATH.
Found an additional upstream terminal-auth mismatch: Buzz advertises terminal
auth capability but its authenticate subcommand does not implement that flow.
Account login and paid runs remain deferred; no provider credentials inspected.

Supplemental notice tooling now preserves exact pinned nostr license evidence,
checks hashes/locked package provenance and retains explicit review flags.
The inventory still has 19 flagged packages, including four Bitcoin packages
without full notice text; source SPDX headers are evidence, not legal clearance.
No public release, issue/PR posting or automatic GitHub workflow was performed.

Added a second upstream proposal correcting unsupported ACP terminal-auth
advertising. Review against the official draft preserves its legacy default:
absent method type means agent-driven authentication; explicit terminal/unknown
or malformed types are rejected until implemented. Two tests compile the exact
pure policy/capability functions extracted from the staged patch. The full ACP
harness and real subscription flows are not certified by those tests.

The WebSocket contribution now includes the actual upstream test-client's
exhaustive error mapping and its tests: 21 pass, including 5 consumer tests.
Both proposals have reproducible locked staging harnesses; no installed pin
change or public submission occurred. Notice/package tool tests and upstream
pin-tool self-tests pass. All GitHub workflow jobs remain manual-only.

## Independent review fixes, 2026-09-28

Version 0.0.6 fixes two independent-review findings: atomic catalog/activity
publication and optional profile denial incorrectly revoking room membership.
See [review record](reviews/2026-09-28.md). Final Rust suite: 99 passed, two
explicit live-relay/ACP skips; rebuilt IPC, packaging, activity and manifest
checks passed. Upstream pins and manual-only CI remain unchanged.

Installed 0.0.6 helper and plugin implementation `c70d33b`; restarted the user
helper and native shell. Sanitized verification: authenticated, four joined
rooms, four activity summaries, idle delivery. Native panel summon returned
`ok`. The previous 0.0.5 binary is preserved at
`~/.local/bin/omarchy-buzz.before-0.0.6-20260928`; previous plugin implementation
is `ed01778`. No relay/identity configuration or production message was changed.

## ACP permission and adapter follow-up, 2026-09-28

Prepared a third isolated upstream proposal: reject unadvertised nondefault
permission modes and propagate mode-setter rejection instead of silently
running under defaults. Four focused tests pass from the exact staged functions;
a second agent reviewed caller propagation. See
[proposal](upstream/ACP_PERMISSION_MODE.md) and
[evidence](evidence/acp-permission-mode.json). This does not fix default bypass,
automatic tool approval, secret propagation or prove effective permissions.
No production Buzz dependency or installed binary changed.

Pinned Claude adapter source inspection confirms subscription and Console
terminal-login choices, the bundled native CLI, ARM64 package entries, and
why its asynchronous auth status cannot establish a turn's effective payer.
See [assessment](CLAUDE_ADAPTER_SOURCE.md). No adapter/account/model was run.

Regenerated the ARM64 notice inventory against the 0.0.6 lock: 252 packages,
19 review flags unchanged. Four Bitcoin crates' checksum-matched archives
lack full notices and VCS markers; no unsupported substitute was added. The
new inventory is locally at `/tmp/omarchy-buzz-notices-006`; packaging must
always regenerate rather than reuse older lock evidence.

Corrected two Rust formatting differences from the prior review fix; formatting
check now passes. Manual CI now includes the four permission proposal checks;
no GitHub workflow was triggered. Installed 0.0.6 remains the tested messaging
preview. Real ACP and public release gates remain open.

## Reusing Buzz authentication, 2026-09-28

Confirmed Buzz Desktop already implements account connection and Claude terminal
login in `desktop/src-tauri/src/commands/agent_auth.rs`. The earlier missing-flow
finding concerns the standalone harness. We reuse adapter/native provider login;
no OAuth implementation or credential copying belongs in the plugin. The
desktop launcher cannot be copied unchanged because it accepts descriptor-owned
commands and some paths report launch rather than login completion.

Prepared the alternative [interactive-login proposal](upstream/ACP_INTERACTIVE_LOGIN.md):
configured executable, one-time argument normalization, restricted environment,
separate foreground terminal process, bounded timeout/cancellation, terminal
restoration and fresh ACP initialization after success. Existing desktop method
discovery remains available via an explicit external-terminal capability flag.
Apply this patch OR the smaller capability-disable patch, not both. Neither is
installed or submitted. Full Buzz/Tauri compilation and actual provider login
remain unverified.

Final reconstructed fixture: two descriptor tests and twelve disposable PTY
cases passed. It compiles the exact patched auth command and terminal module
with ACP transport/normalization doubles. Independent review found and prompted
fixes for controlling-terminal descriptors and repeated argument normalization.
See [evidence](evidence/acp-interactive-login.json). Manual-only CI now includes
the fixture; no workflow was triggered.

[Codex adapter inspection](CODEX_ADAPTER_SOURCE.md) confirms ordinary ACP-owned
ChatGPT login, unlike Claude's terminal flow. Both retain native credentials.
Effective-payer evidence, safe Buzz key input, permission decisions and full
upstream integration are still required before real-agent use. No provider
account, credential store, production relay or installed 0.0.6 binary changed.

## Complete ACP build and real-client auth conformance, 2026-09-28

A single explicit manual GitHub run
[36476363859](https://github.com/randymy/omarchy-buzz/actions/runs/36476363859)
succeeded at plugin revision `a4db938327e8e06268505c3ce28673abadb51558`.
It restored the previous synthetic build cache and finished in 5m12s. Local
storage was too constrained for a full Buzz build; no local source/cache was
deleted and no automatic workflow triggers were enabled.

The complete patched `buzz-acp` crate built and its full unit-test target compiled
on Ubuntu 24.04 x86_64/Rust 1.95.0. The WS, permission-mode and interactive-login
proposals apply and compile together. Three targeted upstream unit tests passed.
Nine real-binary authentication tests passed against a fake stdio peer, covering
terminal flow, input, cancellation, failures, reconnect, descriptor validation,
ordinary agent-owned auth and discovery capabilities. This closes the earlier
ACP compile/real-client integration gap; it is not a full desktop build or the
complete upstream test suite. See [evidence](evidence/acp-integration-36476363859.json).

No real adapter/provider, native account, credentials or production relay were
used. Installed 0.0.6 remains unchanged. Next: actual adapter artifact and isolated
initialization validation, then user-driven subscription sign-in when ready;
effective-payer and safe execution requirements remain. The local patches are
still unsubmitted proposals and have not changed the production dependency pin.

## Published adapter discovery verified, 2026-09-28

[Manual run 36478940003](https://github.com/randymy/omarchy-buzz/actions/runs/36478940003)
succeeded in 2m58s at `4b28fb659d9df5c5a3e8da1a1c868e95e02b729c`.
Pinned npm artifacts Codex ACP 2.0.0/native Codex 0.158.0 and Claude ACP
0.82.0/SDK 0.3.280 initialize through the complete patched Buzz harness on
Linux x86_64/Node 22.23.3. Codex advertises agent-owned `chat-gpt` and `api-key`;
Claude advertises terminal `claude-ai-login` and `console-login` only with
explicit terminal capability. Four discovery cases, five probe boundary tests,
nine real-client synthetic auth cases and three targeted upstream unit tests
passed. See [validation](VENDOR_ADAPTER_VALIDATION.md) and its sanitized evidence.

Discovery ran in an offline network/PID namespace with fresh profiles; no account
sign-in, model task, production relay or installed plugin changed. Automatic
workflows remain disabled. ARM64 runtime, real login, effective payer and safe
execution remain gates. Independent review reconfirmed there is no current
key-FD/stdin input in Buzz ACP; adding one alone would not stop mandatory Git
bootstrap from writing and forwarding the relay key. Address that upstream
boundary together with child propagation before a persistent-identity demo.

## Harness-only relay identity conformance, 2026-09-28

Prepared [ACP_KEY_ISOLATION.md](upstream/ACP_KEY_ISOLATION.md) and a fourth local
upstream patch, applied after the WS, permission and interactive-login proposals.
Linux `--private-key-fd` accepts a bounded anonymous read pipe, keeps relay signing
in the harness, disables credential-sharing Git/MCP and the incompatible built-in
CLI prompt, and carries the policy through initial/task/recovery launches.
Known relay credentials/Git config are removed after every child-env override.
Legacy launch behavior is unchanged. No installed component or provider account
was changed and automatic GitHub triggers remain disabled.

The first manual build (36480950556) caught upstream's unsafe-code prohibition;
the reader was rewritten using safe standard-library/nix APIs and Linux procfs.
The second run (36481710299) built successfully and passed seven of eight new
unit tests; a synthetic peer's incorrect response ID caused its handshake timeout.
That fixture was corrected rather than relaxing the isolation checks.

[Run 36482217831](https://github.com/randymy/omarchy-buzz/actions/runs/36482217831)
at `dae221dbcc2b9e78a9dcda8264f37aea1707f092` passes the complete ACP build,
full unit-test target compilation, five pipe tests, three runtime/child-env tests,
eight real-CLI input cases, three existing targeted upstream tests, nine auth
integration cases, five discovery-boundary tests and four real-adapter offline
discovery cases. See [evidence](evidence/acp-key-isolation-36482217831.json).

Important product limit: upstream currently expects the agent's signed Buzz CLI
calls to post normal room replies. FD mode removes that capability; it is not yet
a working room agent. The next upstream reply proposal must capture bounded
user-facing ACP text and sign/publish from the harness to the authorized incoming
room/thread. Tool auto-approval/default bypass and same-UID isolation remain open.

The user reconfirmed Pro subscription support is required: the first real-account
demo must use Pro, not an API-billed substitute. Codex's generated native types
expose `forced_login_method` as a potential enforcement mechanism; matching native
0.158.0 source/precedence still needs inspection before relying on it. Neither
provider's real login or effective billing mode has been tested.

## Harness-owned reply draft (2026-09-28)

Added a fifth incremental upstream proposal, [harness replies](upstream/ACP_HARNESS_REPLIES.md),
with bounded ACP user-facing text capture, fresh-session routing, SDK signing and
single-attempt HTTP delivery. It is unsubmitted and uninstalled. Independent
review caught a dropped input batch during session-guard recycling; that path now
preserves the batch. Review also identified the unresolved open-room membership
removal race described in the proposal; this remains a release blocker.

[Manual run 36490384302](https://github.com/randymy/omarchy-buzz/actions/runs/36490384302)
at `50977e68be423cec8d1d6edd96e8fbf69fddc651` passed in 3m17s: complete ACP
build, compilation of the 991-test unit target, 14 new reply tests (including two
actual ACP-client stdio tests and seven HTTP cases in one transport test), eight
key/runtime unit tests, three earlier targeted tests, eight CLI input cases,
nine synthetic auth cases and five discovery-boundary cases. The entire 991-test
suite was not run. Vendor discovery was not repeated. The earlier run failed on
two test-fixture string borrows, now corrected. See [sanitized evidence](evidence/acp-harness-replies-36490384302.json).

No installed plugin, production relay or account changed. Pro real-login, payer
control, tool policy, the open-room membership fence and full isolated
prompt-to-room acceptance remain outstanding. The preferred membership fix needs
relay-side atomic enforcement; a preflight query alone is insufficient.

## Permission denial, membership fence and subscription startup (2026-09-28)

Two focused source reviewers helped prepare the next proposals. Changes are
saved at `d86847d` and remain unsubmitted/uninstalled:

- [ACP deny policy](upstream/ACP_DENY_TOOL_REQUESTS.md): explicit request rejection,
  propagation through all launch/recovery paths, advertised mode and returned
  mode confirmation. Codex uses `read-only`; `default` is not in its mode catalog.
- [Member-bound replies](upstream/MEMBER_BOUND_EVENTS.md): proposed endpoint with
  a database membership lock shared with removal, exact-route NIP-98 and no
  fallback to ordinary publication. Source review found no lock-order blocker.
- [Codex startup policy](CODEX_SUBSCRIPTION_ENFORCEMENT.md): native 0.158.0 policy
  belongs at app-server startup, not thread CODEX_CONFIG. Added an offline native
  artifact test rejecting a synthetic API login without starting any model task.

Initial permission run `36491742128` built the executable but caught a fixture
attempt to clone upstream's non-Clone Config. Fixed the fixture without changing
production Config. [ACP/native run 36492581585](https://github.com/randymy/omarchy-buzz/actions/runs/36492581585)
passed in 3m53s: full ACP build, 995-test target compilation, four denial/mode
tests, 14 reply tests, prior key/auth regressions, four real-adapter discovery
cases, and actual native Codex 0.158.0 rejecting synthetic API login under a
startup ChatGPT restriction. No real account or model task was used. See
[evidence](evidence/acp-policy-subscription-36492581585.json). Provider routing,
real Pro entitlement and effective task billing remain unverified.
Postgres/relay run `36492581271` passed both database tests but caught missing
imports in the HTTP test fixture. Corrected run [36493588467](https://github.com/randymy/omarchy-buzz/actions/runs/36493588467)
at `f8db0ce46b755ec5347e090e89952df5aab109fe` passed: full relay/ACP builds,
two database membership/removal tests, one actual HTTP router test and one
transport test containing eight cases. The full unit suites were not run. See
[evidence](evidence/member-bound-36493588467.json). The endpoint is still an
unsubmitted proposal, unavailable on deployed relays. Four new Python probe
tests and five discovery tests pass locally. No account or installed component changed.

Next: constrain subscription provider routing and validate the complete synthetic
prompt-to-room lifecycle before any real Pro task. Codex and Claude are the
first supported-agent targets, Goose next; these are goals, not certified integrations.

## Full synthetic reply and adapter login rejection (2026-09-28)

[Run 36501371837](https://github.com/randymy/omarchy-buzz/actions/runs/36501371837)
passes the actual patched ACP/relay lifecycle: owner mention, fresh read-only
session, synthetic tool permission rejection, thought exclusion, signed kind-9
reply to the admitted root, and a separate persisted event query. Messaging
conformance and owned-process cleanup also passed. [Evidence](evidence/harness-room-36501371837.json).
The initial run caught a Python path-type error before supervisor startup; fixed
and covered by a full mock startup check. No vendor model was used.

[Run 36501374029](https://github.com/randymy/omarchy-buzz/actions/runs/36501374029)
also passes actual Codex ACP 2.0.0 authentication rejection under native 0.158.0
ChatGPT-only startup config. The ACP SDK wraps the exact native policy denial
in error data; the test now verifies that exact wrapper using locked SDK source.
Native rejection, four vendor-discovery cases and previous ACP regressions also
pass. [Evidence](evidence/codex-adapter-subscription-36501374029.json).
No real account or paid task was used; effective routing/billing remains open.

A product gap was found: the installed panel queries top-level room messages,
so these thread replies are not visible there yet. An eight-row thread view,
signed thread-window reducer and scoped IPC are being implemented. See
[thread design](THREAD_REPLY_READINESS.md). Installed 0.0.6 remains unchanged.

## Native thread view validation (2026-09-28)

Source 0.0.7 implements an eight-reply, depth-one read-only thread pane, scoped
helper IPC, signed request-bound NIP-CW bounds, manual retry and automatic
refresh while the panel is open. It clears on root/room/access/generation loss
and hides with older helpers. Offscreen UI fixtures and manifest validation pass.
Request frames remain capped at 64 KiB; bounded status responses allow 96 KiB
for simultaneous room/thread projections. No agent execution or thread composer
is introduced.

Native ARM64 run `36502926877` passed 111 helper tests (two external fixtures
ignored) and the actual synthetic IPC/send/activation checks. Actual relay
thread validation initially failed because upstream ACP's signed kind-7
reactions have only an event target and no room tag. Sanitized diagnostic run
`36503438982` confirmed that exact shape. Commit `dc3776e` fixes both room and
thread readers, with SDK-built reaction regressions, and `5542ad1` additionally
checks that the root remains in room history while its reply stays in the
thread. An independent source review found no blocker. Corrected relay run
`36503855340` and ARM64 run `36503857736` both passed at source `5542ad1`.
The native run passed 114 helper tests (two external fixtures ignored), plus
synthetic IPC/send/activation. The downloaded artifact's SHA-256, exact source,
architecture and version were checked, and all three isolated process checks
also passed locally outside the socket-restricted sandbox. See
[thread evidence](evidence/thread-room-36503855340.json) and
[native evidence](evidence/helper-arm64-36503857736.json).

Installed helper and plugin are now 0.0.7. The plugin checkout is `4c0a7bc`
(runtime sources match tested `5542ad1`; subsequent commits only updated docs).
Native plugin update, helper service restart, shell restart and panel summon
succeeded. The helper is active/running with core dumps disabled. Shell logs
after restart show no Buzz load error. `plugin list`'s `active` flag only applies
to whole-bar alternatives, not Buzz's service/widget/panel; it is not a panel
health signal. Physical keyboard/display interaction remains unverified.

Rollback is preserved at
`~/.cache/omarchy-buzz/rollback/0.0.6-before-36503857736/` with the previous
binary, hash and plugin revision (`c70d33b`). Configuration, identity, delivery
ledger and relay deployment were preserved. No real agent/model task was run.

## Normal desktop window (2026-09-28)

User requested a normal window alongside the overlay. Installed plugin
`b929332` adds a **Window / Overlay** switch and direct summon payload
`{"mode":"window"}`. One PanelContent instance moves between native
FloatingWindow and PanelWindow, preserving the shared service/draft/selection;
only one presentation is shown at a time. Helper remains the validated 0.0.7
artifact above. Native Wayland synthetic switching/host-close test and the
existing offscreen content check passed. Native update/restart/summon passed;
Hyprland reported a mapped, visible, tiled `Buzz for Omarchy` application window
(836 × 986 on this session). No QML load/type/reference errors were found after
restart. Normal window size/presentation are not persisted, and the existing
bar/Super+B toggle closes an already-open view. Physical keyboard and
multi-monitor interaction remain unverified. See [native behavior](NATIVE.md).

## Subscription routing characterization (2026-09-28)

Manual run [36506274738](https://github.com/randymy/omarchy-buzz/actions/runs/36506274738)
passed both new account-free probes at `ce4a0ca`, with 11 synthetic fixture
tests. Actual Codex ACP 2.0.0 accepted a provider mutation and sent the custom
provider/URL to a scripted native peer; the peer rejected thread creation.
Actual Claude ACP 0.82.0 / SDK 0.3.280 native auth status reports the synthetic
API key even when the disposable home sets `forceLoginMethod: claudeai`.
The first run caught an unrecognized `api_key` status label in our parser;
corrected without treating any status as subscription approval. Both probes
run non-root without an external network interface and with fresh homes.
No login, real credential or model turn was used.

Evidence: [Codex routing](evidence/codex-routing-36506274738.json) and
[Claude native status](evidence/claude-subscription-36506274738.json).
These characterize the controls we need; they are not real Pro acceptance.
The contract is subscription authentication with no automatic API/provider
fallback, not unlimited plan usage or per-turn billing attestation.

A fresh-session-only Codex subscription policy is being prepared as a staged,
uninstalled upstream proposal, using native startup restrictions and supported
cwd-aware configuration/account checks. The corresponding
[Claude proposal](upstream/CLAUDE_SUBSCRIPTION_POLICY.md) is design-only.
Installed plugin/helper remain 0.0.7 with the normal-window addition; no user
configuration, provider credentials, relay or production agent changed.

## GitHub notification pause (2026-09-28)

The user reported excessive failed-run emails. These came from manually
dispatched development checks, including failures corrected in later runs;
they do not report failures of the installed desktop app. New remote workflow
dispatches and reruns are paused. Continue local source work and feasible
checks; do not silently resume remote CI. Account notification preferences
have not been changed. The staged Codex policy proposal is uninstalled and
its build/typecheck/runtime tests remain pending; saving a manual workflow
does not constitute validation.

## Local subscription policy work (2026-09-28)

Saved an uninstalled [Codex adapter proposal](upstream/CODEX_SUBSCRIPTION_POLICY.md)
against the exact pinned upstream revision, with native ChatGPT-only startup,
controlled child environment, per-prompt account/config checks and explicit
denial of provider mutation and imported sessions. Dependency-free execution
of its policy module passed locally on Node 26.9.0, along with 20 existing
Python authentication/routing fixture tests. These checks use synthetic data
and establish neither adapter integration nor actual Pro-account acceptance.

The manual `codex-policy.yml` workflow is saved for future typecheck/build and
adapter tests, but has not been dispatched. Remote CI remains paused. Local
disk has about 160 MB free (and `/tmp` about 19 MB), so dependency installation
and full builds remain deferred; no user files were removed. The installed
plugin/helper, relay and provider accounts are unchanged.
