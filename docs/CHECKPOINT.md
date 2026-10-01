# Development checkpoint — 2026-09-29

## Latest verified state

- Installed messaging preview is 0.0.8. Native rooms, sends, usernames,
  thread reading and reply composition, overlay, normal window and application-search launcher are available. A separate isolated Codex room service now passes real subscription
  acceptance and is running manually; no public release or marketplace listing exists.
- Both Codex/ChatGPT and Claude native-subscription fixed-response model tests
  pass through guarded ACP adapters, including session-correlated responses and
  explicit API-auth rejection. This is separate from room-agent acceptance.
- ARM64 helper package run 36584446129 passed; archive digest and all notice
  texts were checked. The artifact's helper smoke, send-scope and inherited-socket
  tests pass on this Omarchy machine without credentials or a relay.
- The current package has 252 dependency records, 192 unique notice texts and
  no package without notice text. Distribution review flags remain explicit.
- The approved stock Codex path now works without upstream patches. Broader agent UI, Claude room acceptance and distribution remain unfinished. See
  [isolated room agents](ROOM_AGENTS.md).

The dated sections below retain historical evidence; older installed-version
and pending-test statements are superseded by this summary and later entries.

Current source version: **0.0.8 thread-composition development preview**, not a community release.
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

## Next release gates (updated September 28)

1. Resolve upstream WebSocket resource bounds described in
   `helper/WS_UPSTREAM.md`. Isolated relay tests and authenticated production
   messaging/thread reads have passed; they do not establish hostile-load bounds.
2. Review dependency notices before public binary distribution. Source remains
   available with manual helper build/setup; experimental archives are not releases.
3. Finish target desktop acceptance (physical shortcut, multi-monitor, lock/DND
   and failure states) and target architecture coverage. Local activity badges
   and optional content-free notifications are implemented.
4. Review and approve the prepared messaging-only community submission in
   [MARKETPLACE_SUBMISSION.md](MARKETPLACE_SUBMISSION.md). The repository is public;
   a marketplace listing has not been submitted.
5. Separately advance the ACP preview through upstream contributions, real
   subscription acceptance and supervised room-agent validation. These are not
   prerequisites for a messaging-only listing. No Buzz/Omarchy fork or legacy
   publication fallback is authorized.

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
Upstream Buzz, Omarchy and downstream project source files remain unmodified.

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
Omarchy fork, downstream-project modification, fake approval mechanism or live agent launch
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
Buzz synchronized unread state. Native messaging and the generic downstream-project
desktop consumption path are operational without a fork. GitHub workflows stay manual.

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

Focused static review found and prompted fixes for a configuration-context
change (the adapter automatically trusted the workspace after the precheck)
and client-supplied MCP processes. The revised proposal preserves existing
workspace trust and rejects client MCP servers/additional directories in its
initial subscription mode. Its advertised login/session/MCP capabilities now
match those restrictions. These fixes still need the full adapter and pinned
native configuration fixtures; static review and policy-module checks are not
a substitute for those gates.

## Remote validation resumed (2026-09-28)

The user explicitly authorized remote tests again. The earlier dispatch pause
is lifted; workflows remain manual-only and account notification settings are
unchanged. First gate: build/typecheck and isolated tests of the Codex policy
proposal against the pinned adapter. No real account or model task is implied.

Codex policy run [36508544990](https://github.com/randymy/omarchy-buzz/actions/runs/36508544990)
passed typecheck, build, module smoke and 42 tests across three selected adapter
files at `c2e2307`. All tests ran in an isolated network namespace without real
accounts or model turns. [Evidence](evidence/codex-policy-36508544990.json).
The next focused run adds actual native config/read compatibility and compiled
adapter denial checks; these are still account-free conformance checks.

## Codex native policy conformance (2026-09-28)

Run [36509294886](https://github.com/randymy/omarchy-buzz/actions/runs/36509294886)
passed build/typecheck, focused tests, actual native config compatibility and
compiled ACP policy denials at `e733acf`. The preceding run caught the native
default ChatGPT URL being rejected; the corrected guard admits only its exact
pinned value. API login, provider mutation and imported-session requests are
rejected through the compiled adapter. Evidence is saved under
`docs/evidence/codex-policy-{native,adapter}-36509294886.json`. No real account,
model turn or installed agent was used. Next: equivalent Claude policy work,
then the controlled harness/adapter integration; real Pro acceptance remains
unverified. Installed plugin/helper remain unchanged.

## Claude subscription policy proposal (2026-09-28)

Saved an uninstalled adapter proposal against Claude ACP 0.82.0 / SDK 0.3.280.
It admits subscription accounts only, removes provider/API login paths, limits
the first version to fresh sessions and rejects steering, imported sessions and
client routing overrides. Independent review caught and closed the direct
steering input path. Native login uses a restricted child environment.

Run [36510233749](https://github.com/randymy/omarchy-buzz/actions/runs/36510233749)
passed TypeScript build and 163 focused tests at `bd43cd4`. These were isolated,
account-free tests, not real Pro acceptance. Additional guarded session
creation/prompt tests and native status/logout environment checks are underway.
The SDK account read is cached per query; the policy requires controlled profile
configuration and does not prove credentials stayed unchanged externally.
[Evidence](evidence/claude-policy-36510233749.json). Nothing is installed.

## Claude session lifecycle validation passed (2026-09-28)

Run [36510783911](https://github.com/randymy/omarchy-buzz/actions/runs/36510783911)
passed the build and all 172 tests at `81eb6aa`, including 13 policy tests.
Fake SDK fixtures now cover accepted session creation, rejection cleanup,
account change/read failure before prompt enqueue, and client overrides denied
before query creation. Native auth status/logout use the restricted environment
in guarded mode; normal logout retains its original call signature after the
preceding regression test caught the changed argument shape.
[Evidence](evidence/claude-policy-36510783911.json).

Both subscription guard proposals remain uninstalled and unsubmitted. Remote
checks are authorized and manual-only. No production relay, identity, provider
account or installed plugin/helper changed. Next concrete gates:

1. Validate the compiled Claude adapter and native login interface account-free.
2. Connect the guarded adapters to the isolated Buzz ACP harness using the
   reviewed permission, identity-isolation and room-membership proposals.
3. Validate target ARM64 artifacts and controlled profile/service lifecycle.
4. Perform an authorized real subscription acceptance flow before claiming Pro
   support or enabling normal room-agent launches. Preserve cached-account and
   externally mutable configuration limitations in the deployment contract.

## Username-first message headers (2026-09-28)

User requested usernames instead of abbreviated keys plus Participant. Messages
and thread replies now use verified current-room recipient profile names. Full
keys remain available on hover; exact-key mentions are unchanged. Unknown or
blank names fall back to a short key. Explicit recipient room scope prevents
labels carrying into another room. The synthetic author-name test and full
offscreen panel check passed. This UI-only change does not require a new helper
binary. The helper still limits recipient profiles to 20 current members.

## Installed reply styling and confirmed relay blocker (2026-09-28)

Installed plugin `665603d` uses native `Ui.Button` for View/Hide replies and
Refresh replies. Thread and full-panel offscreen checks passed. Username lookup
on the actual selected room returned its signed profile name; neither names nor
keys are hard-coded in the UI.

ARM64 workflow [36511830911](https://github.com/randymy/omarchy-buzz/actions/runs/36511830911)
passed formatting, release tests, build and isolated IPC/send/activation checks.
Its exact `665603d5158e2a94e44851ca4e9e679ca1e157da` helper was verified against
artifact provenance and SHA-256
`d51d64cbbe86a0b9104e551b38313c1a2693ab541abcfbbb38484eb28d8afa7f`,
installed, and the service restarted. Previous binary is preserved locally under
`~/.cache/omarchy-buzz/rollback/before-36511830911/`. Version remains 0.0.7;
this is a diagnostic development build, not a release.

An authenticated read of an existing thread now confirms the exact static error
`thread_missing_bounds`. No message or agent task was sent. Deployed Buzz
`8342dfc` has no NIP-CW thread-window implementation: it interprets the query as
a legacy depth-limited query and never emits signed kind-39007 bounds. The
plugin must not turn this into an empty thread or silently weaken validation.
Replies remain unavailable on this deployment.

Next: prepare and rehearse the upstream relay upgrade described in
[RELAY_UPGRADE.md](RELAY_UPGRADE.md). The source gap contains 22 migrations, so
production must not be upgraded by blindly pulling the mutable main image.
Only container names/images/health were inspected remotely; no relay, database,
identity, environment or proxy configuration was changed.

## Relay upgraded; live replies verified (2026-09-28)

Completed the reviewed relay upgrade to upstream `781d395` using its immutable
ARM64 image. Database migrations and both target startup and old-image restore
were rehearsed privately first. Production cutover used a consistent backup and
separate copied volumes, preserving the community URL and relay identity.
Backup checksums, all four service health checks and 49 successful migrations
passed. The original project remains stopped with its data retained.

The installed helper now reads both recent thread roots successfully: snapshots
contain zero and two replies, replacing `thread_missing_bounds`. These reads
sent no messages or agent tasks. `thread_completeness_unknown` remains honest;
thread composition, pagination and nested-reply coverage are not implemented.
Native themed reply buttons and verified profile-name labels remain installed.

Active deployment, private backup locations and recovery constraints are in
[RELAY_UPGRADE.md](RELAY_UPGRADE.md); sanitized production evidence is in
[evidence/relay-cutover-2026-09-28.json](evidence/relay-cutover-2026-09-28.json).
Do not restart the old deployment or discard the newer data during rollback.
Subscription guard proposals remain uninstalled; the pending ACP gates above
are unchanged.

## Compiled subscription adapters and Buzz discovery passed (2026-09-28)

Run `36516177558` passed the compiled Claude policy checks, native CLI
subscription-login help check, build and focused tests at `acab26f`. SDK 1.5.0's
auth error prefix was corrected in the probe after the first run exposed the
assertion mismatch. Native CLI is 2.1.280; SDK is 0.3.280. No login was started.

Run `36516179113` passed full proposed Buzz ACP compilation/unit checks, all 10
terminal-auth subprocess tests, and real compiled guarded-adapter discovery.
The integration review caught missing `CODEX_HOME` / `CLAUDE_CONFIG_DIR` in the
auth subprocess allowlist. The updated proposal preserves these controlled
paths across discovery/login/reconnect without forwarding credential markers.
Codex offers only ChatGPT login; Claude offers only subscription terminal login
when requested. No session or model turn was created.

Sanitized evidence is saved under `docs/evidence/{claude-policy-adapter,
claude-policy-native}-36516177558.json`, `guarded-harness-36516179113.json`,
and `acp-profile-auth-36516179113.json`. Proposals remain uninstalled and
unsubmitted; production is unchanged. Next gates: target ARM64 artifacts,
controlled profile/service lifecycle and then real subscription acceptance.
Do not claim discovery alone completes agent room collaboration. Local disk is
still constrained; use remote builds. The new failed local fixture build was
removed without deleting prior caches or user data.

## ARM64 authentication preview staged (September 28)

Manual workflow `36518401395` passed at
`043668cd35f77ff7fe004d16dc7043d9a42ac598`: patched Buzz ACP unit checks,
compiled Codex/Claude guarded discovery and the actual ARM64 Claude native
subscription-login help check. All seven Buzz proposals are included, including
membership-bound publication; there is no legacy endpoint fallback. The bundle
contains notices and provenance and omits vendor runtimes pending locked setup.

Downloaded and verified archive/manifest digests, source revision and all nine
proposal hashes against this checkout. Bundle is staged beneath
`~/.cache/omarchy-buzz/agent-previews/36518401395/agent-preview/agent-preview-arm64`.
Both private profiles were prepared with the launcher on this ARM64 machine.
No credentials were imported, login started, account verified, session created,
model invoked or room-agent service enabled. Production messaging is unchanged.
Sanitized evidence: [agent-arm64-preview-36518401395.json](evidence/agent-arm64-preview-36518401395.json).

Target hydration is blocked by approximately 300 MiB free disk. Added a tested
preflight floor (1 GiB Codex / 2 GiB Claude on bundle/profile filesystems) before
npm can replace a runtime. The actual target invocation safely refused with
`runtime_setup_insufficient_disk`. Do not repeatedly retry or fill the disk.
GitHub download needed a home-cache TMPDIR because /tmp is also full. No further
user files or caches were removed in this staging pass.

Fresh source checkout and non-overwriting helper/unit installation are now
explicit in the public documentation. The current release-gate list distinguishes
messaging from later ACP work. Nine launcher tests, four packaging tests, native
manifest validation and shell syntax validation passed. Marketplace submission
remains prepared but unsubmitted; resource-bound, notice and desktop acceptance
gates remain. Next: obtain disk headroom, hydrate/verify target runtimes, then
interactive subscription acceptance; independently finish messaging release gates.

## Target runtimes ready; native sign-in opened (September 29)

Try Omarchy enlarged the virtual disk to 64 GiB and expanded ext4 on boot.
After setup, approximately 38 GiB remains available. Both vendor runtimes were
hydrated from the staged bundle's original locks with lifecycle scripts disabled.
Package versions and native runtime hashes passed against the ARM64 build report.
Restricted-network attempts failed DNS; explicit network-enabled retries passed.

On this machine, the compiled Buzz harness and adapters expose only `chat-gpt`
for Codex and `claude-ai-login` for Claude. Authentication-method validation now
rejects incompatible types, duplicate IDs and discovery lists lacking the method
used by the login command. npm retries/timeouts are bounded. Eleven launcher
boundary tests pass; no provider credentials were inspected or imported.

A native Omarchy terminal was launched with a private local wrapper for sequential
ChatGPT and Claude subscription sign-in. Completion is not observed or certified;
do not infer a logged-in Pro account or successful model execution. Its local
wrapper is beneath the staged run directory (`sign-in.sh`); no credentials or
login output are stored in the repository. The installed messaging socket is
active. No room agent, production relay change or model task was initiated.
Evidence: [agent-target-setup-2026-09-29.json](evidence/agent-target-setup-2026-09-29.json).

Independent notice review confirmed the staged Buzz workspace crate manifests,
lock and root license match pinned source. Three exact registry notice sources
remain unavailable locally: nostr 0.44.7, bitcoin-io 0.1.4, bitcoin_hashes 0.14.1.
Older cached versions cannot supply their provenance. Preserve inventory review
flags; the bundle is still experimental. Next: observe user-controlled sign-in,
verify subscription admission without API fallback, and advance upstream gates
before any supervised room-agent acceptance. Marketplace remains unsubmitted.

## Codex generated profile links fixed (September 29)

The first interactive login stopped before authentication with
`profile_tree_unsafe`. Actual discovery had created four native command shims
in `provider/tmp/arg0/codex-arg0*/`; the blanket link prohibition rejected its
own runtime output. Added a narrow exception for those four exact names/paths,
owned by the current user and pointing directly to the recorded native Codex
binary with a matching SHA-256. Arbitrary targets, modified binaries and
configuration links remain rejected. No existing profile files were removed.

Twelve launcher tests pass, including rejection of alternate targets/tampering.
Actual target discovery followed by another status/profile check passes. Reopened
the native sign-in terminal. Provider login completion remains unverified; no
agent task or API-billed operation was started.

## Native Codex login handoff (September 29)

Browser sign-in opened but Buzz exited `auth_authenticate_failed`; the subsequent
localhost callback could not connect. Source inspection found a generic 60-second
ACP request deadline inside the nominal ten-minute authenticate operation. That
is a likely cause, not a timed trace of the user's failure. No callback URLs or
authorization codes were saved. The preview now invokes the verified native Codex
binary's subscription-only login directly in the same isolated profile; installed
help confirms syntax. Claude retains the terminal-auth handoff. The upstream ACP
timeout remains unresolved and should be corrected separately.

Browser startup also creates legitimate fontconfig and Chromium singleton links.
Recursive profile validation now excludes only the contents of real current-user
home/.cache and config/chromium directories; their roots cannot be links. Provider
configuration links remain rejected. This is not isolation from same-user code.
Fifteen tests and target profile/status recheck pass. A fresh native sign-in
terminal was opened. Login completion/account entitlement still unverified.

## Codex ChatGPT login confirmed (September 29)

User completed native sign-in. The verified bundled Codex binary, run with the
same dedicated CODEX_HOME and forced ChatGPT login setting, returned success
and the native ChatGPT-login status. Only the classified boolean was reported;
no credential files or raw account output were inspected or saved. This confirms
the authentication route, not a specific Pro tier or successful model execution.
Claude sign-in and room-agent acceptance remain unverified.

## Existing native profile selection (September 29)

Preview CLI now supports explicit --profile-mode existing and optional absolute
--provider-directory alongside the preserved separate-profile default. Clean
HOME/work/cache and provider environment filtering remain. Credentials are never
copied; native tools resolve them through CODEX_HOME/CLAUDE_CONFIG_DIR. Added
bounded auth-status classification without exposing raw account output.

The default existing Codex profile reports ChatGPT login through the verified
native CLI. Existing Claude native status did not confirm subscription login;
review identified native settings could invoke credential helpers. Added a
preflight rejecting hooks/env/provider/helper overrides and unsafe paths before
future native probes. The existing Claude profile now correctly reports
existing_settings_review_required; no settings were changed. Do not claim native
status guarantees zero provider-managed side effects or establishes model billing.

Eighteen launcher tests pass; model/room agents remain disabled. User-facing
selection is CLI-only for this experimental stage; no QML account selector or
saved default was added. See AGENT_PREVIEW_SETUP.md and sanitized evidence
existing-profile-check-2026-09-29.json. Next: review Claude settings compatibility
without weakening subscription guards, then actual guarded session acceptance
once upstream room-agent gates are resolved.

## Both existing native subscription routes confirmed (September 29)

Claude's compatibility gate was an existing SessionStart hook. No hook command
or user settings were changed. Pinned native CLI help exposes --setting-sources
and --settings. Added a settings-free, hooks-disabled auth-status command and
an account-free fixture containing synthetic SessionStart/apiKeyHelper commands;
neither executed. Scoped the user-hook exception to auth-status only; managed
hooks and API/provider/helper overrides remain blocked. Existing-hook discovery
and login are not thereby enabled.

Native Claude status now reports loggedIn, firstParty, nonempty subscriptionType
and no API key/token source. Combined with the prior Codex ChatGPT check, both
existing native profiles support subscription-status reuse without new login or
credential copying. No raw account output, credential content or tier was saved.
Nineteen unit tests and the pinned native status fixture pass. These are native
status reports, not model-turn billing or room-agent acceptance. Remaining work:
validate existing-profile adapter/session behavior, upstream publication and
permission gates, then the room-agent demo; no agent services are enabled.

## Existing profiles pass adapter discovery (September 29)

Both compiled guarded adapters now pass Buzz auth-methods discovery with the
existing native profiles: chat-gpt for Codex and claude-ai-login for Claude.
Codex requires normal host profile access for its runtime files; read-only
sandbox startup failed and host execution passed. No config file was rewritten.
The native config/read probe additionally confirms forced ChatGPT, default/OpenAI
provider and absence of custom provider definitions or endpoint overrides in the
controlled work directory. These checks send no prompt and create no session.

Claude discovery with synthetic existing SessionStart hooks passed inside a
loopback-only network namespace without executing the hook. The user-hook
exception is now scoped to auth-status and auth-methods; login/session acceptance
is still separate. The real existing-profile discovery then passed. Tests and
reproduction probes are checked in. Evidence: existing-adapter-check-2026-09-29.json.

Prepared an independent upstream auth-timeout patch so ordinary agent-owned
browser login can use its advertised ten-minute window instead of the generic
60-second transport timeout. It is uninstalled/unsubmitted; apply checks passed,
but Rust compilation and delayed-process acceptance remain unrun. Our verified
native-login path remains installed. No production relay or room-agent service
changed. Next gates are real session permission/subscription admission and the
unmerged relay publication interface; discovery alone does not complete them.

## Codex live session admission passed (September 29)

Added a bounded no-prompt admission probe and three synthetic-peer tests. It
verifies bundle/runtime, reads effective native config, refuses custom routing
and configured MCP/hooks/plugins/projects/skills, initializes ACP, requires API
login rejection, then requests a session in an empty private temporary workspace.
All ACP client permission requests are cancelled and other client operations
refused. Payloads, account output and session IDs are not logged. Child process
groups are terminated after the result; native empty-session metadata may persist.

The dedicated signed-in Codex profile passed on the actual host: routing/integration
preflight, initialization, API auth rejection and session/new all succeeded.
Sandboxed attempt rejected admission; normal-access retry passed. No model prompt,
relay event, native tool action or room-agent service was requested. The native
process can perform account/config operations during admission; do not call this
read-only or proof of billing for a model turn. Evidence is saved in
codex-session-admission-2026-09-29.json. Existing Codex discovery remains verified,
but its session-level config has not been certified by this separate-profile test.

Claude session/new remains deferred: SDK query spawns native CLI before final
account admission, and user/managed hooks need session-level validation. Discovery
hook tests alone are insufficient. The new probe refuses Claude sessions outright
until that gate is addressed. Three admission fixture tests, 19 launcher tests
and four native-config tests pass. Upstream relay publication remains unresolved.

## Live subscription responses and authentication deadline (September 29)

Codex's separate ChatGPT profile and Claude's existing native subscription profile
both passed actual guarded model smoke tests: initialize, reject API login,
create session, return exact fixed response, finish normally. No room was used,
no agent service enabled, no client permission request arrived. These results
prove the tested subscription route/model response, not independently audited
native tool behavior, plan tier or billing receipts. Evidence lives in the
respective `*-subscription-smoke-2026-09-29.json` files.

Claude session-start hooks were tested in a loopback-only network namespace:
ordinary adapter ran the synthetic hook; guarded adapter rejected the missing
subscription without running it. This does not isolate suppression from account
rejection or independently prove hook behavior in an admitted session. Source
inspection separately establishes guarded SDK `settingSources: []`. The admission probe now supports Claude
subject to managed-hook/provider/MCP/plugin preflight. This supersedes the earlier
Claude-session deferral. Five synthetic admission tests exercise the probe, including rejection of
early and cross-session response text. Both live fixed-response checks were
repeated successfully after this independent-review correction.

Manual ARM64 run 36582892134 at 21b36af passed all eight proposed Buzz patches,
both guarded adapter builds and native packaging. An actual synthetic ACP login
lasting 65 seconds passed, closing the generic 60-second timeout regression.
The downloaded archive digest matches its sidecar; no replacement was installed.

Remaining room-agent blockers include unmerged harness key isolation and tool
permission changes, lifecycle/room acceptance, and the publication contract.
The user has been asked whether ordinary Buzz room permissions suffice or the
stronger atomic membership-removal guarantee is required. No answer yet; no
legacy fallback or patched production relay has been deployed. Release notice
evidence and packaging validation are being completed independently.

## Upstream alignment and ordinary relay conformance (September 29)

Refreshed official Buzz main; it remains 12670bd. Prepared a focused WebSocket
PR packet at `docs/upstream/WS_PR_SUBMISSION.md`, validated on that exact target
with 21 focused Rust tests and formatting. Existing upstream PRs 4212 and 3964
address adjacent surfaces rather than these shared-client queue bounds.
Permission to send the draft using a contribution-only fork is pending; no
upstream message, fork or PR has been created.

Source inspection also found the open keyless broker stack #6922/#6967. The
review is in `docs/upstream/KEYLESS_UPSTREAM_REVIEW.md`. It may supersede a
long-term local signing-key path, but still passes scoped broker credentials
to children and lacks a completed-turn harness reply. It does not close native
permission or subscription-policy requirements and is not merged.

Manual conformance run 36587419111 at 5122750 is testing ordinary `POST /events`
with synthetic agents and a disposable relay. The workflow retains member-bound
as its default and labels selected-contract evidence explicitly. Neither mode
changes the product policy or production relay. Membership removal is not
covered by this reply-persistence scenario.

## Pause checkpoint — September 29

User requested a pause for a couple of hours. Working tree was clean before
this checkpoint; all implementation, evidence and contribution plans are saved.
The installed messaging plugin/helper and Mac mini relay were not changed in
this work period. No local agent service or long-running build must remain up.

GitHub run [36587419111](https://github.com/randymy/omarchy-buzz/actions/runs/36587419111)
is still in progress independently of this workstation. It tests ordinary
publication with synthetic credentials/agents; no real subscription usage or
production relay is involved. Leave it running. On resume, inspect its result,
download sanitized evidence, and address any concrete failure before broadening
tests. ARM64 package run 36584446129 and auth-deadline run 36582892134 passed.
All 71 Python regression tests passed locally, as did five session-correlation
tests and target packaged-helper IPC checks. Both live subscription fixed-response
checks passed after session-correlation review.

Two requested decisions remain unanswered: ordinary Buzz versus stronger atomic
publication semantics for the product, and permission to send the prepared
WebSocket draft PR using a contribution-only fork and signed-off commits. Do not
interpret elapsed time as approval. No upstream fork, PR, comment, marketplace
submission, release tag or room-agent enablement occurred. Resume with the saved
WS_PR_SUBMISSION and KEYLESS_UPSTREAM_REVIEW plans rather than rediscovering them.

## Resumed: ordinary signed replies passed

Run 36587419111 completed successfully at 5122750. The isolated real-relay
fixture accepted a synthetic mention, ran the proposed six-patch ACP harness,
verified its signed reply, queried the persisted event and projected it through
the plugin's thread reducer. It used existing upstream `POST /events`, with no
member-bound relay patch. Fixture cleanup completed. Evidence is in
`evidence/ordinary-replies-36587419111.json`. This closes ordinary-route
reply-persistence conformance, not removal semantics, real-provider integration
with a room, or upstream adoption. No production changes were made.

## Ordinary private-room HTTP revocation passed

Run 36611201645 at fde56e6 passed both messaging and synthetic harness-reply
conformance. A current private-room member received HTTP 200 with the exact
accepted event ID. After owner-visible removal, a freshly signed request received
HTTP 400 with the exact membership rejection. Transport errors cannot satisfy
the negative assertion. Signed reply persistence/thread projection and cleanup
also passed. See `evidence/private-room-revocation-36611201645.json`.

This is sequential ordinary membership enforcement, not a concurrent-removal
guarantee or open-room revocation test. No product contract/default, installed
helper, production relay or room-agent service changed. Upstream main refreshed
to 8519db1; its new NIP-FI authentication behavior is under source review before
any dependency update. The shared WS client and ACP paths are unchanged.

## Current upstream admission review complete

Source review of 8519db1 finds plain NIP-42/NIP-98 compatible in the default
FI-off mode; FI-enforce requires an assertion the helper does not support.
Deny-protected intentionally blocks messaging. The helper currently reads only
the NIP-11 signer, so it does not pre-detect FI enforcement. This is not runtime
certification or an adopted dependency update. Exact sources and limits are in
`upstream/BUZZ_8519DB1_COMPATIBILITY.md`. Shared WS/client and ACP files remain
unchanged, preserving the focused contribution's source basis.

No GitHub test remains running from this work period. Both ordinary reply
conformance and subsequent private-room revocation conformance passed, and the
working changes/evidence are committed. The requested publication-contract and
upstream contribution-fork/sign-off decisions are still pending; no upstream
submission or production agent deployment occurred.

## Approved contract and upstream draft submitted

The user approved ordinary Buzz room permissions for the first release and
the contribution-only fork/signed-off draft submission. These decisions are
no longer pending. Stronger atomic member-bound publication is optional future
work; no silent route fallback or weakening of the other agent boundaries is
authorized.

[Upstream draft PR #7976](https://github.com/block/buzz/pull/7976) is open from
`randymy/buzz:omarchy-buzz/ws-resource-limits`, head 7c75297 on base 8519db1.
DCO passed. Independent source review found no blockers. Full workspace/native
acceptance and human test confirmation remain outstanding; the PR stays draft
and carries no review-completed marker. The plugin/helper still pins official
upstream, and no production relay, helper or room-agent service was changed.

## Ordinary ARM64 bundle and packaging follow-through

Manual ARM64 agent run 36619625452 at 0991c2f passed. The build omits the
member-bound patch, retains subscription/key/permission/auth-timeout guards,
and tests the ordinary single-attempt reply transport. New manifests label
ordinary `/events` explicitly. Artifact download/digest review remains next;
no installed bundle or agent service has changed.

Helper packaging now selects system D-Bus with Rust crypto instead of keyring's
vendored feature. Cargo's resolver removed seven unused packages, including
OpenSSL source/bindings and foreign-types; no package version/checksum was
added or changed. Local dependency download then hit the existing broken Hermit
registry cache, so compilation is delegated to the fresh CI environment. The
ARM64 workflow now checks actual libdbus linkage and isolated Secret Service
enrollment before producing a new archive. Runtime verification remains pending
and the installed helper is unchanged.

## Verified ordinary bundle and system D-Bus helper

Ordinary agent bundle 36619625452 has verified archive/manifest checksums and
explicit `/events` metadata; evidence is saved in
`evidence/ordinary-agent-arm64-36619625452.json`. It remains a synthetic
experimental build, not an installed production agent.

Helper ARM64 run 36621010979 at 8928adc passed all build, IPC, private keyring
and system D-Bus checks. Its downloaded artifact passed version, actual
libdbus linkage, IPC, inherited-socket restart and private Secret Service
enrollment/retrieval tests on Omarchy. Tests used temporary resources and
synthetic identities. Archive and all notice-text hashes match; inventory is
245 packages/14 review flags, down from 252/19. See
`evidence/helper-package-36621010979.json`. The installed helper is unchanged.
The dangling Hermit registry cache was restored from the saved September 26
archive using safe tar extraction; no dependency upgrade was performed.

Manual run 36621707596 tests the exact submitted WS contribution 7c75297 in
Buzz's actual workspace. Check its result before claiming workspace validation.
The upstream PR remains draft pending its documented review gates.

## Workspace and x86-64 checks passed

Run 36621707596 passed all 21 affected library tests in the real locked Buzz
workspace at submitted contribution head 7c75297. The draft upstream PR body
now links this evidence; full workspace CI/native/human confirmation remain
outstanding. Run 36622093137 at b96d140 passed all three validation jobs,
including x86-64 helper Rust/IPC/private keyring tests with the system D-Bus
change, native manifest validation and isolated WS/ACP policy/terminal fixtures.
Neither run used real provider credentials or production relays. Evidence is
in `evidence/ws-workspace-36621707596.json` and
`evidence/x86-validation-36622093137.json`.

## Observed-removal regression passed; work saved

Added a handler-level completed-reply suppression test to the staged harness
reply contribution. Initial run 36623181009 caught an invalid fixture missing
its in-flight TaskMeta; correction d736929 registers the task and scope owner.
Run 36623681719 passed the corrected regression, existing security/transport
checks, auth deadlines, adapter probes and preview packaging. It uses the
approved ordinary seven-patch series; no member-bound endpoint. Evidence is
`evidence/observed-removal-36623681719.json`. Five package-preview tests pass
locally. No new production relay/helper/agent installation or model use occurred.

All manual runs from this work period have finished. Current upstream draft
is https://github.com/block/buzz/pull/7976, with exact workspace evidence in
its description. Remaining product work: upstream review/adoption of WS and
ACP/adapter interfaces; supervised room-agent lifecycle and execution scope;
integrated subscription room acceptance; distribution review and remaining
desktop acceptance. Normal room permissions and contribution-only fork/sign-off
are approved and must not be asked again. The existing installed messaging
client remains available. Do not claim the full agent product is released.

## 0.0.8 installed — September 29 afternoon

ARM64 run 36627997783 and x86-64 validation 36627997557 passed at
31c6fe649f61a4d234836a2b06ca2561c902c21f. The isolated real-relay messaging
run 36627466468 passed at fd0b77a, including signed thread replies, exact ACK,
idempotent replay and persisted thread history. No production test messages
were sent. See the corresponding JSON in `docs/evidence/`.

The downloaded ARM64 package passed checksum, package metadata and version
checks, then actual local IPC, send-scope, inherited-socket lifecycle and private
Secret Service tests. The native UI was updated first, then `scripts/helper-install`
installed the helper with a backup at
`~/.local/share/omarchy-buzz/backups/20260929T204731.462590Z`.
The live helper subsequently reported authenticated with `thread_send` capability.
MemoryMax remains 256 MiB and core dumps are disabled. Identity and relay config
were preserved. The installed optional desktop entry passed actual GIO argument
parsing and launch; Hyprland confirmed the Buzz normal window was mapped.

Thread composition keeps independent room/thread drafts. Delivery receipts identify
the original scope; discarding an uncertain draft clears only that draft, even
when another room is selected. Unavailable roots disable replies without changing
the destination. New thread ledger entries require 0.0.8; preserve the ledger and
do not downgrade to 0.0.7 after sending thread replies.

Usable today: the messaging development preview. Still not shipped: supervised
subscription room agents, a public release, or a community marketplace listing.
The upstream WS draft is PR 7976. The permission contribution now includes four
passing real ACP session-path fake-peer tests; the seven staged patches apply in
order, but they are contribution prototypes, not installed production dependencies.

## Stock Codex room acceptance passed — September 29 evening

The owner approved an isolated-workspace first agent. Build 36639519388 passed
for unchanged Buzz 781d395 (buzz-acp, buzz CLI and buzz-admin). Official npm
Codex ACP 2.0.0 and Codex 0.158.0 were installed with scripts disabled. The native
wrapper forces ChatGPT login and reuses the already signed-in separate profile.
No provider API credentials or normal HOME/desktop session enter the sandbox.

A dedicated Buzz identity was generated using upstream buzz-admin, stored in
Secret Service and added to the approved private room. One startup attempt failed
before agent execution because this bwrap lacks --preserve-fds. The corrected
launcher uses a memfd containing --args options; synthetic checks cover the
key handoff without putting its value in process arguments or disk files.

The successful owner mention asked the agent to read a harmless workspace marker.
It returned the exact marker in a signed reply to the same thread. The installed
0.0.8 helper verified and projected that exact reply. Evidence is
`evidence/stock-codex-room-2026-09-29.json`. This is actual subscription room
acceptance, not the earlier independent provider and synthetic-relay checks.

The transient test unit was stopped and replaced with the manually started
`omarchy-buzz-codex.service`, memory-limited to 2 GiB and 128 tasks, with core
dumps disabled and whole-cgroup cleanup. It is not enabled at login. Per-turn
limits are 180 seconds absolute and 60 seconds idle. It has its own workspace,
not another project's checkout. Stock agent tools hold their dedicated Buzz signing key;
room/owner filters route input and do not restrict all possible signed events.
Shared network and provider credentials inside the dedicated profile remain
explicit trust limits. Do not present this as a hostile-code or network sandbox,
a future vMachine authority layer, human approval UI, or full agent-state dashboard.

## Running UI upgrade corrected

The operator reported `Incompatible helper` despite matching 0.0.8 files.
`omarchy restart shell` followed by normal-window summon resolved it. A targeted
window capture confirmed authenticated rooms and messages in the approved private
room. The
earlier helper/socket verification did not establish that old compiled QML had
unloaded. Future upgrades must include the actual rendered-window check.

## Usernames and inline mentions corrected

A room-open burst sends history and recipients into a one-slot helper command
queue. Recipient rejection as request_busy previously left names and mentions
unavailable permanently. The UI now retries that read at most twice, scoped to
room, helper instance and generation. The actual Process fixture rejects the
first lookup and verifies recovery without duplicate message sends.

Composer @ completion uses only the current verified room roster. Click or
Tab/Enter attaches an exact key and inserts its readable token. Duplicate names
receive a distinguishing key suffix. Inline targets are removed when their tokens
are deleted or an unchanged draft is acknowledged; explicit picker selections
remain independent. Rendered tests cover duplicate names, exact submission keys,
deleted tokens, acknowledgment cleanup, emails, missing/foreign recipients and
room switching. No model task was used to test these UI changes.

## Codex reply visibility and native reaction follow-through

The user's September 29 18:16:45 CDT message carried the correct Codex `p`
mention. The stock agent replied 18 seconds later in the correct thread:
root `c6ac01443861699fff2693f415343cf139e8414c1fe8d8383f44227ec255c9b6`,
reply `ec38ad8828267776880f4166d33e6c67b288567948b0cb65bf9b141dad657de1`.
Both the upstream CLI and installed helper's verified thread projection returned
the response. No replacement task or test message was sent. The room service
was active. The UI could expand a bottommost thread below its visible viewport.

Upstream `crates/buzz-acp/src/pool.rs` uses signed kind-7 👀 for queued and 💬
for actively prompting; the cleanup guard removes both when the turn ends.
These are cosmetic reactions, not proof of successful completion or reply counts.
The helper now projects observed distinct-author counts from its existing bounded,
verified history auxiliary set, applies reaction deletions, and suppresses uncertain
authority. It does not introduce a separate reaction polling CLI, raw events in QML,
or a new Buzz protocol. Thread rows currently omit reaction metadata. Empty counts
are not a completeness claim; absent metadata remains compatible with older helpers.

ARM64 run 36646298882 passed native tests, IPC and private keyring checks.
The downloaded package passed its digest/version checks and isolated socket
checks on this machine. It was installed with backup
`~/.local/share/omarchy-buzz/backups/20260930T003325.465724Z`.
The installed binary SHA256 is
`f6b8f6d6c870828152b26110984456a4023f6e53530719a1cf31cabedf30eda7`.
A fresh authenticated helper read returned the user's exact reply and observed
zero remaining 👀/💬 reactions for the completed turn. The shell was restarted;
the normal window was visually checked authenticated. The expanded last-message
viewport and snapshot reaction rendering were validated in a rendered synthetic
fixture; no new production turn was generated for these tests.

## Quiet background updates

The displayed lists were replaced on every status frame, including identical
five-second history snapshots. The eight-second thread timer also called the
initial-open path, clearing replies and toggling loading on every poll.
Same-scope refreshes now retain the validated snapshot while loading; errors,
revocation, room/identity changes and unavailable results still clear it.
Unchanged catalog/history/thread/recipient projections retain their model identity.
The message view uses keyed rows so actual additions and reaction changes update
existing delegates without rebuilding the conversation. Background updates never
invoke the explicit thread-reveal scroll action.

The rendered thread regression verifies repeated snapshot identity, quiet loading,
new-message delegate retention, unchanged scroll position and open-thread retention.
Mention, thread-send and author-name checks passed. No helper or relay change and
no production message/model turn was needed.

## Periodic room check no longer blanks the panel — September 29 night

The operator still saw the current page flicker after `388d3d6`. A passive
`ui-bridge` subscriber recorded the installed 0.0.8 helper for 75 seconds:
every ~31 seconds `auth.rs` starts its joined-room check and publishes
`catalog=loading` with no rooms, `history=unavailable` and
`recipients=unavailable`, restoring them over the next ~2 seconds. The panel
mirrored each step: room list, messages, names, the open thread and the
composer disappeared, and the composer stayed in a stale reply mode. `388d3d6`
covered the 5-second history and 8-second thread polls, not this path.

`d0e5299` keeps the displayed snapshots within one authenticated helper scope
until the helper's views are re-established in order: catalog, history,
roster, open thread. Removal from the room list, failed checks, read errors,
disconnects and scope changes still clear. An unfinished check ends after 20
seconds. A submission made during the check is held and written once after
the helper can validate it; if the check fails it becomes a known failure with
the draft retained, never an uncertain send. Busy refreshes of an already
displayed history or roster no longer hide it.

Three earlier expectations changed deliberately: the default preview, activity
and room-activity fixtures asserted that `catalog=loading` empties the
conversation, roster and bar count. They now assert retention.

New checks: `scripts/preview --catalog-refresh` (rendered, replays the recorded
frame sequence plus removal, failure, read error, refused thread and
disconnect) and `scripts/preview --catalog-refresh-send` (Process fixture;
held submission order). All twelve preview modes pass.

Installed at `d0e5299` with a shell restart. Twelve captures of the real window
across two live room checks were pixel-identical. That live check ran in an
empty room, so it proves stable rooms, header and composer; message, scroll and
open-thread retention are proven by the synthetic rendered test only. No
message was sent and no helper, relay or agent change was made.

This is a UI-side accommodation. The helper should stop tearing down its views
for a background room check; until then sends and thread reads are unavailable
in the helper for the ~2 seconds of each check.

## Thread panel and denser layout — September 29 night

The operator confirmed the reference behavior: threads open to the right in
both Buzz Desktop and Slack, which keeps the room followable. They also asked
to keep this plugin's density rather than Slack's spacing, and to make refresh
actions small controls instead of rows.

`4b14cda` replaces the inline reply accordion with a right-hand thread panel
(root, reply count, replies, its own composer). At 1100 units and wider the
room list, room and thread show together; below that an open thread hides the
room list; below 640 the thread replaces the room with a back control. The
presentation is split into `BuzzMessage.qml`, `BuzzComposer.qml` and
`BuzzScroll.qml`.

The service still has one active destination. `composeScope`, `updateDraftFor`,
`canSendFor` and `submitFor` select it from whichever composer is used, so the
delivery ledger, scope fences and uncertain-send lock are unchanged. Closing a
thread from the panel returns the destination to the room; an involuntary loss
of the root still leaves the reply destination in place and unsendable.

Also changed: Enter sends and Shift+Enter adds a line; the conversation opens
at and follows the newest message unless the reader scrolled up; short times
with day dividers and grouped headers; `↻` header icons for refresh; status
rows reduced to one header line, captions and tooltips. The recipient picker
moved behind the composer's `@` control.

All twelve preview modes pass with no scene warnings. `--thread-replies` now
checks the panel position, follow-newest and reader-position rules, and the
close control. Installed at `4b14cda` with a shell restart; the real window
was captured rendering the new layout in an empty room. Opening a thread and
sending a reply against the real relay were not exercised by the maintainer:
no production message was sent and the operator's window was not driven.

Still helper-bound: reply counts on room messages (every message offers
`Thread ›` because the helper does not project thread summaries), more than
eight replies, older history, live updates, and direct messages. DMs remain
excluded by DESIGN.md; the catalog admits only `stream` rooms. The selected
room is not remembered across shell restarts.

## Helper re-checks joined rooms without blanking — September 29 night

Branch `quiet-room-check`, not installed. The helper's periodic joined-room check
now keeps the published catalog, history, roster, open thread and activity, and
their polls, while discovery runs; sends and thread reads stay available. Only
the first check after authentication publishes `catalog=loading`. On success a
room that left the joined set is handled like an access denial: its history,
roster and thread become `*_access_denied`, its jobs stop and a pending send in
it becomes delivery-unknown; other rooms keep everything. Tradeoff: a removed
room used to vanish the moment a check started (as part of the blank); it now
stays listed until the check completes, at most the 15-second discovery timeout.
A failed or timed-out check still clears every dependent view. The panel's own
retention logic is unchanged.

## Reply counts on room messages — September 29 night

Room history now requests NIP-CW `kind:39005` thread summaries. The helper
accepts one only when it is signed by the pinned relay identity, has exactly one
`e`, `d` and `h` tag (`e` = `d`, this room) and typed content, and names a row on
the same page; anything else rejects the whole page, as unmatched edits and
reactions already do. The newest summary per row is projected as a bounded reply
count (direct replies, matching the depth-1 thread panel), last reply time and
up to ten participants, behind the new `thread_summaries` capability. Room
messages then read `N replies ›` or `Reply ›`; an older helper keeps `Thread ›`.
Verified only with synthetic Rust fixtures and all preview modes; no live relay
response was fetched and the installed helper and plugin were not updated.

## Existing direct messages, synthetic only — September 29

Branch `direct-messages`, phase (a) of [DM_MAP.md](DM_MAP.md). The catalog admits `t=dm`
channels with 2–9 participant keys including self (pinned `dm.rs:109-118`; otherwise the
catalog is rejected). The DM `hidden` tag is an always-present list hint
(`side_effects.rs:1215-1219`), so hide state comes only from the NIP-DV snapshot; a
snapshot not relay-signed or not exactly shaped fails the catalog as `catalog_invalid_shape`.
DM names come from participant profiles with key-prefix fallback. The panel lists visible
DMs under Direct messages, without `#`; reading, threads and sending were unchanged.
Verified with locked helper tests and every `scripts/preview` mode only. No live relay
verification: no real DM was listed, read or sent, and nothing was installed.

## September 29, 2026 — threads read like Buzz Desktop

Branch `desktop-thread-mode`; not merged, installed or run against a relay.
The helper reads a thread with Desktop's legacy oldest-first filter
(`depth_limit` 64, pages of 50 with the composite `thread_cursor`) until a
short page or 200 replies. Each row carries `depth` and `parent`; the panel
indents nested replies (at most three steps) under a "↳ replying to" caption
and rejects any frame whose reply tree does not chain to the root through
earlier rows. Replies with an unknown parent are hidden and disclosed ("some
hidden"); a capped thread reads "First 200 replies · more exist". Completeness
is a heuristic in this mode: there is no signed bounds event. The IPC response
bound rose from 96 KiB to 512 KiB (worst case measured at ~428 KiB). Replies
are still sent to the root. Details: [THREAD_REPLY_READINESS.md](THREAD_REPLY_READINESS.md).
Evidence is synthetic only: helper unit tests and the offscreen preview modes.

## Starting a direct message, synthetic only — September 29

Branch `new-dm`, phase (b) of [DM_MAP.md](DM_MAP.md). New `open_dm` request
(1-8 distinct canonical keys, request UUID, generation and instance) and
`dm_open` capability. The helper signs kind 41010 as pinned `build_dm_open`
does and publishes it on the authenticated socket outside the send ledger
(why that is safe: [SENDING.md](SENDING.md)). `status.dmOpen` reports `idle`,
`sending`, `acknowledged` (with the relay's `channel_id` and `created`),
`rejected` or `unknown`; one open at a time. Allowed people are the verified
roster of the room on screen or participants of a DM already in the catalog,
never the viewer. Desktop searches the whole community directory; the helper
has no verified directory, so this is the smallest safe rule. An acknowledged
open re-checks joined rooms at once; the panel selects the DM once it is
listed. The panel's `+ New message` control under Direct messages opens a
picker of the current room's members (up to 8) with a Start action and one
status caption; it is hidden without the capability.
Verified with locked helper tests (loopback relay fixture: event shape and
signature, roster restriction, busy, OK acknowledged/rejected, timeout,
immediate re-check) and every `scripts/preview` mode, including the new
`--new-dm`. No live relay verification: no DM was opened on a real relay and
nothing was installed. Not verified: whether the relay's discovery events for a
new DM are visible to the immediate re-check (they are emitted best-effort after
commit); a slower emission is picked up by the regular 30-second check.

## Older room history — September 29, 2026

Branch `older-history`, section A of [PAGING_LIVE_MAP.md](PAGING_LIVE_MAP.md); not
merged, installed or run against a relay. The helper keeps the head page's signed
`next_cursor`. A new `fetch_older` command (room id only; the panel never names a
position) resends the same window filter with `until` = `next_cursor.created_at` and
`before_id` = `next_cursor.id`. The continued page is verified exactly like the head
page, plus: its `kind:39006` `d` tag must be `<room>:<until>:<before_id>` for that
request (the head binding is refused), every row must lie strictly past the request
cursor in `created_at DESC, id ASC` order, the new cursor must move past it, and no row
may lie past the page's own cursor (the head page is held to that too). Pages repeating
a held id are refused whole rather than merged. The room holds at most 100 rows: the
head plus four older pages of 20, within 200 events / 512 KiB per page and 800 events /
2 MiB across older pages. At the cap the view reports `history_older_unheld` and no
cursor. The 5-second head refresh replaces the head and keeps older pages: held rows
inside the new head's range are dropped (on the head, or deleted since), rows an earlier
head showed that newer messages pushed off it stay held with their last verified
projection, and deletion markers on any later page apply to held rows. A stale cursor
discards the page; room change, `fetch_recent`, re-authentication, revocation, a new
generation or connection, and any error that clears history drop all older pages. The
panel shows a muted "Load older messages" control while a cursor exists and keeps the
reader's top visible row in place when rows arrive above it. Status frames with 100
worst-case rows and 200 thread replies measure 818,238 bytes, under the 1 MiB bound.
Evidence is synthetic only: locked helper tests (unit, loopback NIP-98 and observer
fixtures) and every `scripts/preview` mode, including the new `--older-history`.
Helpers without the new `older_history` capability are read as having no cursor.

## Live updates for the open room — September 30, 2026

Branch `live-updates`, section C of [PAGING_LIVE_MAP.md](PAGING_LIVE_MAP.md); not
merged, installed or run against a relay. After a verified head page for the selected
room the helper sends one `["REQ", "omarchy-buzz-live-<uuid>", {"kinds":[9,40002,40003,5,9005,7,39005],
"#h":[room], "since": now}]` on its authenticated socket. The pinned relay requires
authentication and `messages:read` scope, caps a connection at 1024 subscriptions and
checks `#h` membership (`handlers/req.rs:52-210`); `kinds` is not required for an
`#h`-scoped WebSocket REQ (the p-gated check only guards global filters, `req.rs:232`;
the kindless rejection Desktop notes is for its `/query` thread read) but it is sent to
narrow traffic. Activity sampling of other rooms is unchanged.

Live events are triggers, never content. An event counts only if it is signed, of a
subscribed kind, under 64 KiB, and carries exactly one `h` equal to the room (a
deletion or reaction without `h` must reference a row the helper holds; a thread
summary must be signed by the pinned relay). It schedules a head refetch through the
verified `history::fetch` path, at most one per 300 ms (2 s after more than 20 events
in 10 s), and, if a thread is open and the event has an `e` tag, a thread refetch
through `thread::fetch` with the `fetch_thread` scope checks. Anything else is ignored
and counted; more than 200 in 60 s closes the subscription for five minutes. `EOSE`
primes it; relay `CLOSED` falls back to polling and re-arms after 5 s, 30 s, then
5 min; `NOTICE` is ignored. Before re-authenticating on a relay `AUTH` the helper sends
`CLOSE` first (see [WS_UPSTREAM.md](../helper/WS_UPSTREAM.md)) and re-arms only after
freshness and a new head page. Room change, `fetch_recent`, Retry, shutdown and any
connection exit close it.

While primed the head poll runs every 30 s instead of 5 s and the helper refreshes an
open thread every 30 s itself; unprimed, the old cadence applies. `history.live` is
true only while primed, behind the new `live_updates` capability (14 capabilities).
The panel then reads `Live · …` and slows its thread refresh from 8 to 30 s; a
refresh or loss of the subscription returns the old label.

Evidence is synthetic only: locked helper tests (unit tests of the trigger rules and
six loopback observer fixtures: exact REQ, one refetch per event and per burst,
rejected frames and the flood close, CLOSE before AUTH and re-arm, CLOSED fallback and
backoff, room change and Retry, the 30 s primed cadence, thread refetch on `e` tags and
the periodic thread refresh), `tests/helper_smoke.py`, and every `scripts/preview`
mode including the new `--live-updates`. Not verified: live delivery timing and
`CLOSED` reasons of a real relay, and behaviour under a real burst.

## Harness bundles and provider sign-in — September 30, 2026

Branch `agentbundle`; not merged, installed or run as an agent. Implements the
bundle and sign-in part of [AGENTS_SERVICE.md](AGENTS_SERVICE.md#bundles-and-sign-in).

`room-agent` accepts `--harness claude-code|codex`, 1–8 `--room`, `--respond-to
owner-only|mentions` (upstream `owner-only`/`anyone`), `--instructions <0600
file>` (bound read-only, `buzz-acp --system-prompt-file`) and `--model`
(`buzz-acp --model` for Codex, `ANTHROPIC_MODEL` for Claude Code, as Desktop).
The defaults render the September 29 Codex command byte for byte; a golden test
compares them. New `room-claude`/`room-claude-acp` wrappers point the stock
Claude adapter at the pinned CLI with `{"forceLoginMethod": "claudeai"}` flag
settings. `agent-bundle` assembles and checks `agent-<harness>` bundles;
`agent-login` opens the vendor login and reports `--status` from credential
file metadata.

Evidence: 45 synthetic unit tests pass (`python3 -B -m unittest
tests.test_room_sandbox tests.test_agent_bundle tests.test_agent_preview
tests.test_package_agent_preview`), including a real bubblewrap run proving the
instructions file is readable and not writable inside. A real Codex bundle
was assembled into an untracked directory from the installed stock bundle
(read-only source; pinned hashes matched, 1333 files, 608 MB, `--check` ready
in 0.5 s). A real Claude Code bundle was assembled with the opt-in locked
`npm ci` (public npm registry, scripts disabled) and the mise CLI 2.1.280,
whose hash equals the SDK 0.3.280 native CLI (6566 files, 490 MB, ready).
Inside the sandbox, with the network unshared and an empty synthetic
profile, the bundled CLI reported `2.1.280`, `auth status` reported logged out
with `forcedLoginMethod: "claudeai"` and `configDirectory: /profile/provider`,
and `buzz-acp auth-methods` initialized both stock adapters through the
wrappers. Both trial bundles were deleted. `scripts/preview` default passed.

Not verified: a real sign-in, a model turn, a relay connection, agent
execution, the browser hand-off from the login terminal, the Omarchy floating
terminal launch, and whether the Claude CLI refuses a Console login under the
forced setting. No installed bundle, profile, unit, keyring entry or login was
created or changed.

## Agents section in the panel — September 30, 2026

Branch `agentsui`; not merged, installed or run against the agent service, which is
being built separately. The panel implements its side of
[AGENTS_SERVICE.md](AGENTS_SERVICE.md). `plugin/AgentService.qml` is owned by
`Service.qml` (the manifest allows one service entry point) and exposed as
`service.agents`. It runs `<helper> agents-bridge` through `Process`, exactly as
`ui-bridge` is run, with the same 5-second handshake, and connects only where the
main service auto-connects (again on Retry or when the panel opens after a lost
session; nothing polls). Every frame is checked exactly: envelope keys, instance,
capabilities (only `agent_manager` is known), `harnesses`, `agents` (all 16 persona
and state fields with the contract's rules, UUID v4 ids, at most 16) and `pending`;
errors must carry a known category and this session's instance. Anything else ends
the session. A service without `agent_manager` is accepted but offers nothing.

Requests are validated locally with the contract's rules before they are written:
rooms must be 1–8 of the main helper's verified joined rooms (streams, not DMs), the
workspace absolute (empty on create for the service default), and only one mutating
request may be in flight, from this panel or reported `working` by the service. Each
carries a fresh UUID and is correlated by it; no answer within 60 s is shown as an
unknown outcome, never a failure. The panel sends no key, token or command line.

The left column shows `Agents` under Direct messages, one row per agent (`running`,
`stopped`, `failed`, `not enrolled` or `unknown`, and the harness), and `+ New agent`.
The editor replaces the room view (an open thread is hidden, not closed) and offers
the persona fields, start at login, Save/Create, Enroll, Start/Stop, Delete with a
confirming second click (the identity is kept), and `Sign in to <harness>` when the
service reports it signed out. Service categories are shown as short sentences. With
the helper connected but no usable agent service the sidebar says `Agent manager
unavailable`.

Interface choices the service must honour (the contract leaves them open): the
bridge subcommand is `agents-bridge`; persona fields travel as `fields` on
`create_agent` (all fields plus `startAtLogin` and `acpCommand`) and on
`update_agent` (only changed persona fields); the target agent is `agentId`, since
`id` is the request's UUID; `delete_agent` always carries `forget`; status frames
carry `id` as `null` or the UUID of the request they answer; `pending.category` is
non-null exactly when `state` is `failed`; `enrolled` implies a non-null `identity`.

Evidence is synthetic only: `scripts/preview --agents` (new) and every other
`scripts/preview` mode, including `--bridge` with a locally built helper. Not
verified: the real service, real sign-in, enrollment or units.

## Agent manager service — September 30, 2026

Branch `agentsvc`; not merged, installed or run against systemd, Secret Service
or a relay. `omarchy-buzz agents-daemon`/`agents-bridge` implement the service
side of [AGENTS_SERVICE.md](AGENTS_SERVICE.md) with the IPC shapes refined with
the panel and bundle work that day: a persona store at
`$XDG_STATE_HOME/omarchy-buzz/agents/personas.json` (0600, atomic, strictly
validated, at most 16), workspace refusals modeled on `room-sandbox`, rooms
checked against the helper's own verified catalog over `control.sock`, per-agent
units rendered from `service/agent.service.in` with a quoted argv `ExecStart`
and driven by fixed `systemctl --user` argv arrays, and enrollment that stores
the new agent key only in Secret Service, signs the owner's NIP-OA attestation
with the pinned SDK and publishes kind 30175, a secret-free kind 30177, kind
9000 `role=bot` per room and the agent's attested kind 0, each counted only
after the relay's `OK`. The service and socket unit files are source only.

Evidence is synthetic only: locked helper tests (store and workspace rules,
request and frame shapes, golden unit file, fake unit control, enrollment
against a loopback relay with OK/rejection/closed/timeout/refused-AUTH, sign-in
refusals, socket protocol) and `tests/agents_smoke.py` in fake-control mode.
Not verified: real unit control, keyring access and `secret-tool` lookup of the
stored key, relay acceptance of the published events, harness scripts. Open:
the launcher has no argument for the attestation (written to
`<agent dir>/auth-tag.json`), `agent-login` must detach from the service's
cgroup, and dropped rooms and deleted agents are not removed from the relay.

## Agent manager integration fixes — September 30, 2026

Branch `agent-integration` (from `5f33d47`); not installed or run against systemd,
Secret Service, a relay or a vendor CLI. `room-agent --auth-tag` checks the
owner attestation like `--instructions` and hands it to `buzz-acp` as
`BUZZ_AUTH_TAG` through the memfd options; `ExecStart` always passes it for an
enrolled agent. `agent-login` detaches the terminal with `systemd-run --user
--scope` (else `setsid -f`) and gets only session variables from the service.
Dropped rooms are left with the owner's kind 9001 and `delete_agent` leaves all
rooms first. Evidence is synthetic (helper tests, smoke, Python tests with a
real bubblewrap memfd run, every preview mode). Not verified: `buzz-acp` with
the tag, a real detached login, relay acceptance of 9001.

## ASCII avatars — September 30, 2026

Branch `ascii-avatars` (from `7e780f9`); QML and tests only, not installed. Every
person and agent gets a deterministic block-character identicon
(`plugin/Identicon.js`, `plugin/BuzzAvatar.qml`): a mirrored 5×3 glyph folded
from the public key's own hex digits (a persona id for agents not yet enrolled)
and a hue from the key, with lightness solved per hue to one relative luminance
so it reads on light and dark themes. Other values get a fixed neutral glyph in
the foreground color. Avatars appear on lead message rows and thread roots
(grouped rows keep the slot so text aligns), in the DM list (first participant
other than this identity), the Agents list and the agent editor header.

Agents may carry pasted ASCII art (at most 6 lines × 12 columns, control and
direction characters removed) edited in the agent editor with a live preview.
It is stored only on this machine in `$XDG_STATE_HOME/omarchy-buzz/avatars.json`
(atomic, keyed by persona id, damaged or out-of-contract files ignored) and never
sent to the agent service, whose persona record has no avatar field yet; it moves
there when the service gains one. No third-party art is bundled.

Evidence is synthetic: `scripts/preview --identicon`, the avatar checks added to
`--thread-replies` and `--agents`, and every other preview mode. Not verified:
rendering in the installed shell under real themes and fonts.

## First agent created from the panel — September 30 morning

With 0.0.13 installed, the owner signed in to Claude Code from the panel
(detached terminal, browser login; `agent-login --status` flipped to
`signed-in` about a minute later), created the agent `vClaude` (Claude Code
harness, one room, answers only the owner), enrolled it (identity in Secret
Service, kind 30175/30177/0 acknowledged, kind 9000 membership) and started it.
The unit came up as `buzz-acp → claude-agent-acp` inside the bubblewrap sandbox
(about 170 MB) and the agent answered the owner's exact-key mention in the
room. No maintainer message was sent.

Two things went wrong on the way and are now follow-ups:

- The helper's relay session reported `auth_rejected` on a mid-session
  re-authentication while the sign-in was in progress and stayed disconnected
  until a `retry_connection` was sent; the panel showed "Rooms unavailable"
  and the agent form could not be submitted. Follow-up: retry a rejected
  re-authentication automatically with backoff, and offer Reconnect in every
  view, not only the room view.
- A hand-typed `@vclaude` did not reach the agent because only the picker
  attaches the exact key. Follow-up: on send, resolve `@name` tokens that
  match exactly one roster name to that key, as Desktop does.
- Also: `harnesses[].signedIn` refreshes only every 60 s; re-check every
  5 s for two minutes after a `sign_in` request completes.

## Agent run follow-ups — September 30, 2026

Branch `agent-followups` (from `bef49ed`); not merged or installed. A rejected
mid-session re-authentication now reconnects on the network budget (1/2/4/8/16 s),
shown as `connecting`/`auth_rejected`, then waits for Retry; a rejected first
authentication still stops at once. On send, a typed `@name` adds the one key whose
name in this room's verified roster it matches (longest case-insensitive match,
spaces kept, removed or dashed, word boundaries), within 20 keys; ambiguous names
resolve nothing and the composer shows `Notifies: <names>`. After `sign_in` the
agent service re-reads harness status every 5 s for two minutes. Evidence is
synthetic (loopback and fake-spawner tests, `tests/Mentions.qml`, every preview).

## Onboarding step one: relay and identity from the panel — September 30

Branch `onboarding-identity`. A new user can now set the relay and create a
Buzz identity from the setup panel, without a terminal. Joining a community
or room is the next, separate step and is not built.

- Helper: `set_relay {url}` and `create_identity {}` on `control.sock`,
  capability `setup_assist`. Accepted only while `unconfigured`,
  `disconnected` or `unavailable`; refused while authenticated
  (`setup_not_allowed`) or connecting/unlocking (`setup_busy`). `set_relay`
  uses `config::canonical_relay` and `config::with_relay` (shared with
  `setup relay`). `create_identity` generates a key, runs
  `enrollment::check_then_store` against `catalog::relay_signer`, stores the
  secret through `enrollment::store_in` (Secret Service
  `omarchy-buzz.identity.v1`, account `relay|identity`) and saves the public
  key; the connection actor then publishes the new configuration and
  reconnects. Fixed error categories as listed in `docs/SECURITY.md`.
- Panel: relay field with **Use this relay**; with a saved relay and no
  identity, an "I already have a Buzz identity" note (terminal enrollment)
  and **Create a new identity on this device**; the public key short with the
  full key on hover and **Copy public key**; a notice that a new identity
  belongs to no community yet. Helpers without `setup_assist` keep the
  terminal instructions.
- Evidence (synthetic only): helper unit tests for request shapes, relay
  persistence, identity refusal/creation through a fake secret store,
  relay-signer refusal and discovery failures against loopback NIP-11
  fixtures, refusal while authenticated, and an IPC test asserting the
  secret never appears in any frame; `tests/helper_smoke.py` (bad relay
  refused, relay saved, creation fails at discovery on a closed port);
  `scripts/preview` (setup controls only with the capability) and
  `scripts/preview --onboarding`.
- Not verified: a real Secret Service write from the panel, a real relay's
  NIP-11 response, and whether a real relay admits a new identity that has no
  membership (it may report `auth_rejected` until invited). With
  `identity_missing` (identity configured, secret absent) the panel shows only
  the enrollment note; `create_identity` would answer `identity_exists`.

## ANSI art avatars — September 30, 2026

Branch `ansi-avatars` (from `404db2f`); QML and tests only, not installed.
Avatar art can now be true-color ANSI art (`.ans`) as well as the 6 × 12
pasted text.

- `plugin/AnsiArt.js` parses text into a grid of `{ch, fg, bg}` cells. It
  keeps SGR `0`, `39`, `49`, `30–37`/`90–97` and `40–47`/`100–107` (fixed
  mid-tone palette), `38;5;n`/`48;5;n` (xterm 256 table) and
  `38;2;r;g;b`/`48;2;r;g;b`, and ignores bold. Every other
  escape sequence (cursor movement, `ESC[K`, OSC, private modes, truncated
  sequences), `\r`, control, C1 and direction characters, and anything after
  the DOS end-of-file byte (SAUCE) is removed. Input is clipped to 256 KiB, 60
  rows and 120 columns. The stored form is the grid re-serialized with
  truecolor SGR only (`sanitize`, idempotent). A loaded plain `.txt` file is
  stored the same way, so a leading reset marks grid art.
- `BuzzAvatar` draws grid art as a `Canvas` thumbnail (`thumbnail(grid, 6,
  12)`: for each block the centre cell, or the nearest non-blank cell when
  the centre is blank, color kept, never averaged). Each cell is a filled
  block of 1:2 aspect inside the identicon's own 5 × 3 footprint, filled with
  a character's foreground or a blank's background (`blockColor`), so message
  slots keep one width. Message-row avatars are clickable and open a
  profile card (`AvatarCard.qml`, `buzzAvatarCard`) that draws the full grid,
  backgrounds first, in its own characters and colors at the largest cell size that fits (4–12
  px wide, at most 100 × 50 cells), with the display name and key prefix.
  Escape or a click outside closes it.
- **Set my avatar** in the sidebar (under the relay and room-list captions,
  shown once the helper reports an identity) takes an absolute path to a
  `.ans` or `.txt` file. `AvatarFileLoader.qml` refuses relative paths, `..`
  and other extensions, runs `stat` to require a regular file of at most 256
  KiB, reads it once with `FileView`, and checks the size again. It stores
  only the sanitized art, never the path, in the existing `avatars.json`
  keyed by the user's 64-hex public key. **Clear** removes it. This is local
  only until profile avatars are published to the relay: other people still
  see the identicon. The agent editor's **Load from file…** uses the same
  loader for an agent's art. The store now accepts persona ids and 64-hex keys
  (at most 32 entries) and writes synchronously.
- Brightness: `adjust(grid, {brightness})` auto-levels (the brightest
  foreground reaches lightness 0.95), then lifts lightness by the gamma
  1 / brightness, scaling each color as a whole so its hue is kept and
  clamping channels; backgrounds get the same curve. Grid art is stored as
  `{art, brightness}` (0.5–3 in steps of 0.25; new file art starts at 1.5);
  a plain string entry is still read and shown as stored. **Brightness −/+**
  sits under Set my avatar (applied at once, the sidebar thumbnail is the
  preview) and under the agent editor's avatar (a draft until Save). The
  thumbnail and the profile card use the same adjusted colors.
- Evidence (synthetic only): `scripts/preview --ansi-art`, which covers the
  parser, clipping, thumbnail determinism, `toArt`, the stored form, slot width
  with a colored avatar, the card opening on a real click and closing on
  Escape or an outside click, and my avatar loaded from a fixture file,
  persisted, restored by a fresh service, cleared, and refused on unsafe paths,
  an oversized or missing file, and damaged stored art; backgrounds in the
  parser, thumbnail and stored form; `adjust` monotonic, clamped, hue-keeping
  and auto-leveling a synthetic dark grid; brightness stepped, saved and
  restored, with damaged brightness entries refused. `--agents` covers
  colored art and its brightness in the editor. All other preview modes pass.
- Not verified: rendering in the installed shell under real themes and
  fonts; CP437-encoded `.ans` files (read as UTF-8, so their block
  characters are not converted); wide (double-column) characters, which count
  as one cell. The thumbnail of a 100 × 50 portrait at 12 × 6 cells reads
  as a head and shoulders with the right colors, not as a recognisable face.
  In art with a dark background behind every character, brightness 1.5 lifts
  the card's face only a little: the background fills most of each cell.

## Onboarding step two and agent direct messages — September 30

Branch `join-and-agent-dms` (from `404db2f`); not merged, installed or run
against a relay, systemd, Secret Service or an agent.

- Joining (capability `community_join`, details in [JOIN_MAP.md](JOIN_MAP.md)):
  `claim_invite {input}` and `accept_invite {code, policyVersion|null}` redeem
  an invite through the relay's HTTP API (join policy, acceptance receipt,
  NIP-98 claim), allowed while authenticated or `disconnected` with a relay and
  identity (a non-member cannot complete NIP-42, so a new identity redeems
  while refused and reconnects after the claim). An invite naming another relay
  is refused; the relay is never switched. `open_rooms` lists up to 50 open
  stream rooms from a signer-pinned unscoped 39000 read; `join_room` /
  `leave_room` publish kind 9021 / 9022 and resolve only on the exact `OK`.
  New status views `setup`, `openRooms`, `roomAction`; fixed categories
  `invite_invalid`, `invite_relay_mismatch`, `invite_rejected`,
  `invite_rate_limited`, `policy_required`, `relay_unavailable`,
  `setup_busy`, `setup_not_allowed`, `room_not_open`, `join_rejected`,
  `leave_rejected`. Panel: invite field and **Redeem**, the terms with
  **I accept**, **Open rooms** with **Join** and a no-approval note,
  **Leave** (second click confirms) beside the room's ↻.
- Direct messages now carry a `p` tag for every other participant, as in
  Desktop; `buzz-acp`'s mention subscription filters on `#p`.
- Agents: persona field `answersDms` (17th agent key; a store without it loads
  false). On, `ExecStart` carries `--answers-dms` instead of `--room`, and
  `room-agent` omits `--channels`, so the agent's memberships (its rooms and
  DMs opened with it) define the scope; upstream answers only the owner in a
  DM. Changing it stops a running agent like a rooms change. Editor toggle
  **Answers direct messages** with a one-line explanation.
- Evidence (synthetic): locked helper tests (invite grammar and refusals,
  loopback join-policy/accept/claim fixtures including 403/429/malformed
  answers and the NIP-98 proof, strict open-room pages and signer pin, 9021/9022
  through the production observer with OK, rejection and timeout, IPC gating
  and an offline redemption, unit rendering both ways, store migration),
  `tests/helper_smoke.py`, `tests/agents_smoke.py`, the Python launcher tests
  (argv golden without `--channels`), and every `scripts/preview` mode,
  including the extended `--onboarding` (invite → terms → claim → open room
  joined and left) and `--agents` (toggle round trip).
- Not verified: a real relay's invite endpoints, policy text, rate limit and
  9021/9022 handling; that `buzz-acp` without `--channels` picks up a DM opened
  after it started; an agent answering a real DM.

## 0.0.15 installed; agent answered a direct message — September 30 afternoon

0.0.15 (helper SHA256 `13533201810076b552de65d13b295644a5b51d4e9704879cd1571af5227c6ef5`,
ARM64 run 36748809963) is installed. Turning on `Answers direct messages` for
`vClaude` first failed: the unit exited with status 2 because the harness
bundles assembled under 0.0.13 carry their own copy of `room-agent`, which did
not know `--answers-dms`; `agent-bundle --check` compares a bundle only with
its own `bundle.json`, so it still reported `ready`. Both bundles were
reassembled from the current scripts with the already-present adapters (no
network) and swapped in; the agent started without `--channels` and answered
the owner's direct message with no mention. Follow-up: install the launcher
scripts with the helper package, make `--check` report `stale` when a bundle's
launcher differs from the installed scripts, and offer a refresh from the panel.

## Account menu and settings view — September 30

- Account control (`buzzAccount`) pinned to the bottom of the sidebar, as in
  Buzz Desktop: my avatar (identicon, or my local ANSI art), my roster name
  when a verified roster lists me (else `Me`; `Not connected` when not
  online), key prefix in the tooltip, and a dot: green authenticated, amber
  connecting, red disconnected/unavailable/locked, grey unconfigured. Shown
  whenever the service exists, including before a helper session, so Settings
  stays reachable when the helper is missing.
- Account menu (`buzzAccountMenu`), a bordered popover above the control:
  name with an `Online`/`Connecting`/`Offline`/`Not set up` pill, the relay
  host as **Community** (not interactive; switching communities is not
  available yet), **Send feedback** (opens the fixed
  `https://github.com/randymy/omarchy-buzz/issues/new` only on click) and
  **Settings · Ctrl+,**. Escape, an outside click or a choice closes it.
- Settings view (`buzzSettingsView`) replaces the room view like the agent
  editor; **‹ Back to rooms** returns with drafts and selection unchanged.
  Sections: Avatar (the loader, Clear and Brightness − / +, moved out of the
  sidebar; same object names), Notifications (**Alerts**, same
  `notificationsEnabled` preference), Window (**Overlay**/**Window**, shown only
  with the host switch), Shortcut (a note: QML cannot see Hyprland binds, so it
  names `scripts/desktop-shortcut install` instead of claiming a state), About
  (plugin version from the host-provided manifest, relay host, public key with
  **Copy public key**). The helper reports no version, so none is shown. The
  header keeps only the title, status and **Close · Esc**.
- Evidence (synthetic): new `scripts/preview --settings` (dot per state, menu
  open/Escape/outside click/keyboard, feedback URL recorded not opened,
  Settings and Back, Alerts both ways, presentation requests, 64-hex clipboard
  copy, Ctrl+,); `--ansi-art` now opens Settings from the account menu;
  `--presentation` switches from Settings on Wayland; every other mode and
  `--bridge` pass unchanged.
- Not verified: the menu and settings in the live shell; the `Qt.openUrlExternally`
  hand-off to a real browser.

## Stale harness bundles: detection and refresh — September 30 evening

Branch `bundle-refresh` (not installed). Follow-up to the entry above: a bundle
assembled before a launcher change kept running the old `room-agent` while
`--check` said `ready`.

- `agent-bundle <harness> --check [--scripts DIR]` also compares every
  `launcher/` file with the same-named script (default the installed
  `~/.local/share/omarchy-buzz/scripts`, else the checkout's `scripts/`) and
  prints `stale` (exit 3, `launcher_outdated`) on any difference or a file
  missing on either side; a `launcher/` file that differs from `bundle.json` is
  also `stale`, anything else still `missing`. Assembly records the source
  script hashes in `bundle.json` (`scriptSources`).
- `agent-bundle <harness> --refresh-launcher` replaces only `launcher/`
  (`room-agent`, `room-sandbox`, `agent-login`, `agent-bundle`): staged inside
  the bundle, swapped in with `renameat2(RENAME_EXCHANGE)`, then `bundle.json`
  replaced with one rename. It refuses a missing bundle, any mismatch outside
  `launcher/`, and a linked, foreign-owned or group/other-writable script
  source or scripts directory.
- Service: `harnesses[].bundle` is `ready|stale|missing`, read with the
  **installed** `agent-bundle` (no longer the bundle's own copy, which cannot
  know the comparison); `refresh_bundle {harness}` runs the refresh and
  re-reads readiness; `start_agent` refuses a stale bundle with `bundle_stale`.
- Panel: the agent editor shows `Harness bundle needs a refresh` with a
  `Refresh bundle` control and holds Start (tooltip gives the reason); the
  harness chooser labels it `· needs refresh`.

Not verified: a refresh of a real installed bundle, the service reading
`stale` from real scripts, and the panel against the real service. The service
now needs `agent-bundle` in `~/.local/share/omarchy-buzz/scripts/` (the
installer change is the maintainer's); until it is there every bundle reads
`missing`. `bin/room-agent-entry` and the `bin/` wrapper copies are not
compared or refreshed; a change to them still needs a reassembled bundle.

## 0.0.16 installed; the `auth_rejected` episodes were clock drift — September 30 evening

Both `auth_rejected` disconnections today (morning, and again after the owner's
afternoon absence) had the same cause: the VM's system clock stops while the VM
is suspended and `systemd-timesyncd` does not step a large offset, so NIP-42
AUTH events were signed 73 minutes in the past and the relay refused them. The
RTC was correct. `pkexec systemctl restart systemd-timesyncd` stepped the clock
and the helper authenticated at once. 0.0.16 (helper SHA256
`305d45fbbc6b6bffbf6bab649661aa3ad1626034f82fcbaa332c22fa19783449`, ARM64 run
36754066288) is installed; the launcher scripts are now in
`~/.local/share/omarchy-buzz/scripts/`, both bundles reported `stale` and were
refreshed in place with `agent-bundle --refresh-launcher`. Follow-up: the helper
should detect a clock offset (compare the relay's HTTP `Date` header or NIP-11
against local time) and report `clock_skew` instead of `auth_rejected`.

## Invite people from Settings — September 30

Branch `invite-people` (not merged or installed; no real relay contacted).
Upstream references are to the pinned Buzz `781d3951`.

- Relay contract (`crates/buzz-relay/src/api/invites.rs:48-83,278-412`,
  `router.rs:337`; Desktop `desktop/src/shared/api/invites.ts` `mintInvite`):
  `POST /api/invites`, NIP-98 with a payload hash, callable only by the
  tenant's `owner` or `admin` (403 `only relay owners and admins can create
  invites`). Body `{ttl_secs?, max_uses?}` (60 s–30 days, 1–10 000); there is
  no role field and every claim grants `member`. Answer `{code, expires_at,
  max_uses, uses_remaining, url}` with a `v2.` code and
  `url = http(s)://<tenant host>/invite/<code>`. No list or revoke route
  exists at this revision, so the helper has none.
- Helper (`helper/src/invites.rs`, capability `invite_mint`): `mint_invite
  {maxUses 1–100, expiresInHours 1–720}` with a UUID id, only while
  authenticated (`relay_unavailable` otherwise, `setup_busy` while one is
  running). It sends exactly `{"max_uses":n,"ttl_secs":h*3600}` and accepts
  only the five documented keys, a canonical v2 code, the requested
  `max_uses` and `uses_remaining`, an expiry within 15 minutes of the one
  requested and the landing URL on the configured host.
  `status.invites = {state: idle|minting|minted|failed, code, expiresAt,
  maxUses, role, category}`; categories `invite_forbidden` (403),
  `invite_rejected` (400/401), `invite_rate_limited` (429),
  `relay_unavailable`, `setup_busy`. The code is never logged; a relay or
  identity change clears the view.
- Panel: Settings → **Invite people** (`buzzInviteSection`, only with the
  capability): 1/5/25 people, 1/7/30 days, **Create invite**
  (`buzzMintInvite`), then the `buzz://join?relay=<wss://host>&code=<code>`
  link (the form Buzz's web invite page opens, `web/src/features/invite/ui/InvitePage.tsx:248`),
  the `https://<host>/invite/<code>` link and a message for newcomers
  (`buzzInviteBlurb`), each with **Copy**. `invite_forbidden` reads "Only the
  relay's owner or admins can create invites."
- Evidence (synthetic): Rust loopback tests for the signed request and exact
  body, 403/429/400/401/404/5xx/302 and malformed answers, strict parsing and
  IPC gating; `scripts/preview --invites` (refusal sentence, both links, the
  message with code and host, the three copies, exact requests); `--settings`
  checks the section is hidden without the capability.
- Not verified: minting on a real relay (owner and non-owner), the relay's
  `url` host behind a proxy (a different host is refused as malformed), and
  opening the `buzz://` link in Buzz Desktop. A panel older than this one
  refuses a helper announcing 17 capabilities: update both together.

## File attachments: show, download and upload — September 30

Branch `attachments` (not merged or installed; no real relay contacted).
Upstream references are to the pinned Buzz `781d3951`; the research map is
[ATTACHMENTS_MAP.md](ATTACHMENTS_MAP.md).

- Rows (`history`, older pages, thread replies) carry `attachments` (always
  present, at most four, from the event supplying the content, so an edit's
  tags replace the original's) and `attachmentsUnavailable`. Each is
  `{name, mime, size, url, hash, dim, kind}` parsed strictly from `imeta`
  (`crates/buzz-relay/src/handlers/imeta.rs:10-200`): relay keys only, `url, m,
  x, size` required, `url` exactly `<relay http origin>/media/<hash>.<ext>` with
  the hash equal to `x`, size 1 B–1 GiB, `dim` ≤ 16384², the relay's filename
  rule; `kind` `image` only for JPEG/PNG/GIF/WebP. A malformed tag makes that
  row's list empty and unavailable, never the page. Desktop's redundant last
  markdown lines (`![image](url)`, `![video](url)`, `[label](url)`,
  `imetaMediaMarkdown.ts:282-312`) are removed from the shown text.
- Frame budget: four attachments on each of 300 rows would not fit the 1 MiB
  status frame (the baseline worst case was already ≈1.0 MB). At most 48
  attachments are projected across held channel rows and 48 across thread
  replies, newest rows first; later ones read `attachmentsUnavailable`. The
  largest-frame test now measures 996 642 bytes (thread replies in that test no
  longer carry the reaction/summary fields `auth` never gives them).
- Capability `attachments` (18th). Requests `download_attachment {eventId,
  hash}`, `thumbnail_attachment {eventId, hash}`, `open_download {path}`,
  `upload_attachment {roomId, rootId?, path}` (the draft scope needs the room
  and thread, both composers being visible), `remove_pending_attachment {hash}`.
  Status `download {state, eventId, hash, path, received, size, category}`,
  `thumbnails [{hash, path}]` (≤ 64), `pendingAttachments [{scope, name, mime,
  size, url, hash, dim}]` (scope `<room>` or `<room>:<root>`, ≤ 4 per scope,
  ≤ 16), `upload {state, scope, name, category}` (added for progress captions).
  Categories `attachment_unknown`, `attachment_forbidden`, `attachment_mismatch`,
  `attachment_too_large`, `attachment_invalid`, `attachment_type_refused`,
  `attachment_storage_unavailable` (local disk; added), `relay_unavailable`,
  `setup_busy`. One download, one upload, one preview at a time (preview queue
  16); all need an authenticated session and end with the connection. A relay or
  identity change clears every attachment view.
- Tokens (`helper/src/media.rs`): kind 24242, content `Get buzz-media` /
  `Upload buzz-media`, tags `t`, `x`, `expiration` (now + 600 for get, + 300 or
  + 3600 for video uploads, Desktop's values:
  `desktop/src-tauri/src/commands/media.rs:300,311-339,362-381,420-424`),
  `server` = `host[:port]` (`extract_server_authority`, `media.rs:50-57`),
  `Authorization: Nostr <base64url, no padding>` (`media.rs:330-333,430-433`;
  the relay also accepts standard base64, `api/media.rs:1093`). Desktop's get
  token is server-scoped only; ours adds `x` (the relay accepts `x` or
  `server`, `crates/buzz-media/src/auth.rs:207-239`). Download: `GET` the URL,
  200 only; 401/403 `attachment_forbidden`; any other status or redirect
  `relay_unavailable`. Upload: `PUT <origin>/upload` with `X-SHA-256`,
  `Content-Type` (advisory, by extension), `Content-Length`, the file streamed
  (`api/media.rs:248-343`); 401/403 forbidden, 413 too large, 415/422 type
  refused (`crates/buzz-media/src/error.rs:112-167`); the `BlobDescriptor` must
  have known keys only, the same `sha256` and `size`, a media URL on the origin
  with that hash, and for images the declared type (`types.rs:1-29`). No
  `/media/upload` fallback.
- Where files land: partial downloads `$XDG_STATE_HOME/omarchy-buzz/downloads/
  <uuid>.part` (0700 directory, 0600 files, removed on any failure); verified
  saves `~/Downloads/<name>` (hard link, or copy across file systems; 0600);
  previews `$XDG_STATE_HOME/omarchy-buzz/thumbs/<hash>.<ext>`.
- `send_message` adds the draft's pending attachments as `imeta` tags in
  Desktop's order `url, m, x, size, dim, filename` (`buildImetaTags`,
  `imetaMediaMarkdown.ts:89-107`; passed to `buzz_sdk::build_message`'s
  `media_tags`, `builders.rs:208-213,240-263`) and appends Desktop's markdown
  line per attachment (`buildOutgoingMessage`, `:326-341`). The text may be
  empty when attachments are pending. The relay's exact-ID acceptance removes
  exactly those attachments from the draft; rejection or unknown keeps them.
- Panel: cards under the text (`buzzAttachment`: icon, name, `12.3 KB`,
  **Download**, **Open** with the saved path as tooltip after a verified save,
  progress and refusal captions), the verified preview at most `Style.space(180)`
  high (click downloads), `attachment unavailable` for malformed metadata; the
  composer's 📎 next to `@` reveals a path field and **Attach**, lists pending
  files with ×, and Send includes them. Service validates every new field
  exactly (URL against the configured relay's origin, names, scopes, paths).
  Attach/Download stay enabled while busy (the service refuses): an `Ui.Button`
  disabled under the pointer keeps its tooltip over the next control.
- Evidence (synthetic): Rust `attachments_tests` (valid, missing keys, off-relay
  URLs, hash mismatch, sizes/dims/types/keys, fifth ignored, names, markdown
  stripping, reducer rows, frame budget), `media_tests` (token shape, saved
  download with unique name and 0600, mismatch/403/401/cut-off/short/redirect/
  404 leave nothing, oversized length, save-name refusals, preview cache and
  LRU, upload candidate checks, descriptor strictness, exact upload request and
  status mapping, replaced file refused), sending (exact `imeta` tags and
  content lines, blank text only with attachments, acceptance releases exactly
  the draft's), IPC gating, largest frame; `scripts/preview --attachments`;
  extended `Preview.qml` validation; every preview mode, `--bridge` and
  `tests/helper_smoke.py` pass.
- Not verified: any transfer against a real relay (token acceptance, NIP-43
  membership on media routes, the descriptor URL host behind a proxy — a
  different host is refused), relay acceptance of our `imeta` tags and of images
  carrying metadata (Desktop re-encodes images to strip it; the helper does not,
  so such an image may be refused as `attachment_type_refused`), `xdg-open`
  from the systemd user service (it needs `WAYLAND_DISPLAY` etc. in that
  environment), the observer wiring end to end (covered by unit, IPC and
  fixture tests only), and the panel in the live shell. A panel older than this
  one refuses a helper announcing 18 capabilities: update both together.

## File chooser for attachments and avatars — September 30

- Branch `file-chooser`. QML only; no helper, service or protocol change.
- `plugin/BuzzFileChooser.qml`: a `Ui.Button` (**Browse…**, `objectName`
  `buzzBrowse`) that opens `QtQuick.Dialogs` `FileDialog` (`OpenFile`) with a
  `title` and `nameFilters`, starting in `folder` (default `file://$HOME` from
  `Quickshell.env`, then the folder of the last file chosen with that button,
  for the session). The dialog is created lazily from a `Component` on the first
  real click (`dialog` stays `null` until then), so nothing is made or shown in
  offscreen tests. On accept it emits `chosen(path)` with a plain absolute path:
  `file://` (and `localhost`) stripped, percent-escapes decoded; other schemes,
  relative paths, `..` segments, control characters and bad escapes are refused
  with `refused(reason)` (and `problem`). Cancel emits `canceled()`. `stub`
  (null in production) replaces the dialog: `open()` calls `stub.open()` and the
  stub's `accepted(path)`/`rejected()` drive the same handling.
- Composer: **Browse…** sits after **Attach** in the 📎 row (filters `All files`
  and `Images (*.png *.jpg *.jpeg *.gif *.webp)`); a chosen file fills the path
  field and runs the same attach as **Attach**; a refusal shows
  `buzzComposerBrowseProblem` until the field changes. The typed field stays.
- Avatar loader (Settings and the agent editor): **Browse…** beside **Apply**
  (filter `ANSI or text art (*.ans *.txt)`); a chosen file fills the field and
  runs Apply; a refusal fails the loader closed with its reason. The field stays
  the loader's direct child, so existing lookups are unchanged.
- Evidence (synthetic, stub chooser): `--attachments` (Browse opens the stub and
  makes no dialog; cancel, relative, `..`, `%2E%2E` and non-file URLs leave the
  field and send nothing; `file:///home/fixture/shot%202.png` fills the field
  with `/home/fixture/shot 2.png` and uploads it) and `--ansi-art` (the same
  refusals leave the field and avatar; a chosen `%61vatar.ans` URL fills the
  decoded fixture path and loads it). Every preview mode, including
  `--presentation` and `--bridge` with a freshly built helper, passes.
- Not verified here: the real dialog. The maintainer opened one once from a
  Quickshell `FloatingWindow` on Wayland through xdg-desktop-portal (a harmless
  "Failed to register with host portal … app ID" warning is printed); this
  branch was not tried in the live shell, including from the overlay
  (`PanelWindow`) presentation and the returned URL form for names with
  non-ASCII characters.

## File chooser moved out of the shell (`scripts/pick-file`) — September 30

- Live result of the section above: clicking **Browse…** brought omarchy-shell
  down (SIGABRT in a GLib "dconf worker" thread, `coredumpctl` on the
  maintainer's machine, twice). A `QtQuick.Dialogs` `FileDialog` inside the
  shell process is not viable; a dialog from a standalone `qs` window had
  worked, which is why it was not caught earlier.
- Replacement: `scripts/pick-file` (Python, `Gio` from python-gobject, which
  every Omarchy install has through `uwsm`) asks xdg-desktop-portal
  `org.freedesktop.portal.FileChooser.OpenFile` for one file, with a fresh
  `handle_token`, subscribes to the request's `Response` (re-subscribing when
  an older portal picks its own handle), passes `current_folder` and the
  button's `nameFilters` as portal `filters` (`"Label (*.a *.b)"` parsed to
  glob patterns; the first is `current_filter`), and prints the chosen file's
  absolute path (`file://` and `localhost` URLs only, no `..`, no controls).
  Exit 0 chosen, 1 cancelled (also after a 15-minute timeout, when it asks the
  portal to `Close` the dialog), 2 portal missing or failed (a reason on
  stderr). The dialog is the portal's own process (xdg-desktop-portal-gtk on
  Omarchy); killing the script closes it.
- `plugin/BuzzFileChooser.qml` now runs that script through `Quickshell.Io`
  `Process` (`/usr/bin/python3 <plugin>/../scripts/pick-file TITLE FOLDER
  FILTER...`, path from `Qt.resolvedUrl`) and reads stdout with a
  `StdioCollector`; `picking` is true while the process runs (button reads
  "Choosing…" and is disabled); exit 0 → `take(path)`, 1 → `canceled()`, else
  `problem`/`refused` with the first stderr line in the console log. `folder`
  is now a plain path. The `stub` path, `localPath`/`take`, signals and object
  name are unchanged; tests assert `!chooser.picking` instead of
  `dialog === null`. `tests/pick_file.py` (in `validate.yml`) covers filter
  parsing and URL acceptance without a bus.
- Verified: the script alone opens a portal dialog titled as asked, in
  `~/Downloads`, with filters; `--attachments` and `--ansi-art` pass; the live
  shell check follows the plugin update.

## Clock skew detection (`clock_skew`) — September 30

Branch `clock-skew` (not merged or installed; no real relay contacted, the
system clock was not changed). Follows up the evening's `auth_rejected`
episodes, which were a suspended VM's clock 73 minutes behind.

- Measurement (`helper/src/clock.rs`): the relay's HTTP `Date` header, parsed
  strictly as an RFC 7231 IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`, weekday
  checked; obsolete forms, other zones, a repeated header refused), minus the
  local clock, round trip ignored, clamped to ±10 years. Read in two places
  only: `catalog::relay_info` (the NIP-11 `GET /info` catalog discovery already
  makes for the signer pin; no extra request) and `auth::explain_rejection`
  (one `HEAD /info`, 5 s deadline, after each rejected first or
  re-authentication; any status carries `Date`).
- `status.clockSkewSeconds: integer|null` (signed, `relay − local`) keeps the
  last measurement; a failed measurement keeps the previous one, a relay change
  clears it. Informational only (see SECURITY.md).
- A rejected authentication whose last measurement is at least 120 s either way
  publishes `clock_skew` instead of `auth_rejected`, first or re-authentication.
  It retries on the network budget (1/2/4/8/16 units, shared): `connecting`/
  `clock_skew` while retrying, `disconnected`/`clock_skew` once spent, until
  Retry (which starts a fresh budget). A small offset keeps `auth_rejected`
  with its previous behaviour (a first-authentication rejection waits for Retry).
- `protocol::CONNECTION_CATEGORIES` lists every connection category; tests
  assert each published one is listed and that the panel's `acceptFrame` list
  is identical. **Co-update:** a panel older than this one refuses a frame with
  `clock_skew` (it fails the session as `invalid_response`); install the plugin
  and helper together. The new panel accepts helpers without
  `clockSkewSeconds`.
- Panel: `statusLabel` `Clock is off by 73 min behind` (`ahead` when the local
  clock is fast; hours from 120 min; `Clock is off` when unknown), bar `Buzz ·
  Clock` / `!`, and setup instructions: the clock disagrees with the relay's,
  often after suspend; on Omarchy `sudo systemctl restart systemd-timesyncd`
  (or check `timedatectl`), then Retry.
- Evidence (synthetic): `clock` unit tests (valid, malformed and other-form
  dates, missing/repeated headers, signed bounded skew, threshold);
  `catalog_tests` reads `Date` from the NIP-11 response; `auth_reauth_tests`
  with a loopback relay that answers HTTP with a configurable `Date`: skewed
  first authentication reads `clock_skew` and recovers without a command,
  skewed re-authentication (local clock ahead) likewise, a 30 s offset keeps
  `auth_rejected` and is not retried, five skewed retries stop at
  `disconnected`/`clock_skew` with exactly one `HEAD` per rejection and Retry
  reconnects; no `Date` leaves the offset null. `Preview.qml` covers the
  labels, instructions, bar, invalid values and old helpers. `test --locked`
  (290 passed, 2 ignored), `fmt --check`, `tests/helper_smoke.py` and every
  `scripts/preview` mode pass.
- Not verified: a real relay's `Date` header and its answer to `HEAD /info`
  (any status with a `Date` suffices; a proxy may strip or rewrite it), how far the pinned relay
  tolerates AUTH `created_at` drift (the 120 s threshold is ours), the catalog
  path publishing a measurement end to end (unit-tested at `relay_info` only),
  and the panel in the live shell.

## Model check: harness model names and a live probe — September 30

Branch `model-check` (not merged or installed; no vendor CLI turn, provider or
relay was contacted). Until now any `Model` string was saved and a wrong or
unavailable model failed silently on the agent's first turn. Contract in
[AGENTS_SERVICE.md](AGENTS_SERVICE.md#model-check-added-september-30-branch-model-check).

- Static check (`helper/src/agents_service/models.rs`): Claude Code takes the
  aliases `opus`, `sonnet`, `haiku`, `fable` or `claude-[a-z0-9-]+`; Codex
  `gpt-[a-z0-9.-]+`, `o[0-9][a-z0-9-]*` or `codex-[a-z0-9.-]+`; empty is the
  harness default. `create_agent`, and `update_agent` when `model` or `harness`
  changes, refuse anything else with `agent_invalid` and the new
  `pending.detail` `model_not_for_harness`. Stored legacy models load and stay.
- Live probe: `probe_model {agentId}` runs the harness launcher's new
  `room-agent --probe-model <model>` in a transient `systemd-run --user
  --scope` (agent unit limits), 90 s, 4 KiB of merged output, classified into
  `status.modelProbe` (`idle|running|ok|unavailable|not_signed_in|failed`)
  with one fixed sentence; the output is never kept. Gated like `start_agent`
  (`harness_missing`, `bundle_stale`, `not_signed_in`), one mutation at a time.
- `room-agent --probe-model`: the agent's `bwrap` view with a throwaway 0700
  workspace, no relay, no fd 3 key or attestation, no `buzz-acp`, no Secret
  Service lookup; `claude -p "Reply with exactly OK" --model <m> --max-turns 1
  --output-format text --tools "" --no-session-persistence` or `codex exec
  --model <m> --sandbox read-only --skip-git-repo-check --ephemeral --color
  never "Reply with exactly OK"` through the bundle's forced-subscription
  wrappers; its own 80 s deadline (exit 124) and cleanup. Flags were read from
  `claude --help` (2.1.280; `--max-turns` is a hidden option in that build,
  found in the CLI's option table) and `codex exec --help` (0.158.0).
- Panel: alias chips under Model for Claude Code (a click fills the field), a
  hint for Codex, inline refusal of the other harness's models, and `Test
  model` (saved model, signed-in harness, ready bundle) with "A real model
  turn: it may count toward the harness's usage." and the result sentence.
- **Co-update:** status frames gain `modelProbe` and `pending.detail`; an
  older panel refuses them (`invalid_response`), and this panel refuses an
  older service. Install helper and plugin together. The launcher changed, so
  installed bundles read `stale` until `Refresh bundle`.
- Evidence (synthetic): Rust `models` tests (patterns per harness, panel
  constants identical, classification of ok/unavailable/not signed in/busy/
  timeout/refusal/garbage, bounded merged output and deadline kill, probe
  environment), service tests (save-time refusal with detail, legacy model
  kept, probe gating, exact argv, each outcome through the spawner fake,
  `running` blocks other mutations, reset on edit/delete), request shapes;
  Python: probe argv golden for both harnesses against the agent's own view,
  a real `bwrap` run of a synthetic CLI (argv, `/workspace`, no key/relay
  variables, merged stderr, workspace removed), bad model and extra-argument
  refusals, deadline; `tests/Agents.qml` + fixture (chips, refused models,
  probe states and fixed sentences, malformed probe frames); `agents_smoke.py`.
- Not verified: a real probe turn with either CLI (so the exact vendor error
  texts behind the patterns, whether `--max-turns 1` and `--tools ""` combine as
  intended in 2.1.280, and Codex's `exec` output inside the sandbox),
  `systemd-run --user --scope` from the socket-activated service, and the panel
  against the real service.

## Installer records what it installed — September 30

- `scripts/helper-install install` refused the 0.0.20 upgrade with "modified
  or unrecognized unit: …/scripts/room-agent": `inspect_existing` compared
  every installed reviewed file with the current checkout's copy, so the first
  release to change a shipped script could not be installed over the previous
  one (earlier upgrades only added files, which the check allowed).
- Now the installer writes `~/.local/share/omarchy-buzz/installed.json`
  (mode 600; `{name: sha256}` for every reviewed file) after putting the
  files, backs it up beside the files, restores or removes it with them, and
  accepts an installed file that matches either the checkout's copy or its
  recorded hash. A damaged or oversized record reads as empty; an edit nobody
  recorded is still refused. `tests/helper_install.py` gains
  `test_upgrade_accepts_its_own_recorded_copy_but_not_an_edit` (12 tests).
- This machine had no record: it was bootstrapped after verifying all 13
  installed reviewed files byte-for-byte against commit `01e5a74` (0.0.19),
  then 0.0.20 installed normally.

## Default presentation is the window — September 30

- Randy: "change the default of super+B to window not overlay." `Panel.qml`
  and `PanelContent.qml` start with `windowMode: true`, so a bare summon (the
  Super+B toggle, `'{}'`) opens the normal resizable window; `{"mode":"overlay"}`
  or **Overlay** in Settings switches for the session, as before. No shortcut,
  helper or service change. `--presentation` (a bare open is a window; a
  `window` re-summon keeps it) and `--settings` (Window shown as current, both
  switches still requested) updated and green; default, `--onboarding` and
  `--bridge` pass. README and `docs/NATIVE.md` say the default.

## The overlay steps aside for the file chooser — September 30

- Randy: "the file selection crashed again" after the portal picker shipped.
  The journal shows no coredump and the shell alive across four portal
  dialogs (17:57–18:00; xdg-desktop-portal-gtk logs only "Unhandled parent
  window type", from the empty parent handle). Buzz was in the overlay then:
  the portal's dialog is a normal window, so the Overlay-layer Buzz surface
  (exclusive keyboard focus) covered it, and closing Buzz with Escape unloaded
  the panel, killed the picker process and so the dialog — nothing visible,
  everything gone. The last attempt (18:00:35) also coincided with the
  maintainer's shell restart for the window-default change.
- Fix: `Service.filePickersOpen` counts choosers with a dialog up
  (`BuzzFileChooser.service`, wired from the composer and both avatar loaders
  through `AvatarFileLoader.service` and `AgentEditor.service`; decremented on
  finish and on destruction). `Panel.qml` hides the overlay window while it is
  above zero (`overlayShown` alias for tests) and shows it again after; the
  window presentation, now the default, is unaffected. `--presentation`
  checks the step-aside and return; `--attachments`, `--ansi-art`, `--agents`,
  `--settings` pass.

- Live verdict (maintainer, 18:20 CDT): Browse… from the window opened the
  portal dialog and the chosen file attached — "it worked".

## Bounded WebSocket client adopted (`relay_resource_limit`) — September 30

- Marketplace issue omacom/omarchy-plugin-marketplace#9414: the reviewer asked
  for a dependency revision with enforced frame/message and replay-buffer
  limits and a demonstration of bounded rejection of pre-authentication
  floods, since the pinned `buzz-ws-client` buffers unrelated relay messages
  without bound while it waits for `AUTH`/`OK` (`helper/WS_UPSTREAM.md`).
- `helper/Cargo.toml` takes `buzz-ws-client` from the maintainer's fork at
  the reviewed commit behind draft PR block/buzz#7976 (same manifest and
  dependency versions as the pin; `buzz-sdk` unchanged).
  `auth::connection_options` sets the budgets (256 KiB frame/message, 128
  messages / 2 MiB buffered, 20/40/10/5 s deadlines);
  `WsClientError::ResourceLimit` maps to the new connection category
  `relay_resource_limit` (13 categories; retried on the network budget; the
  panel's category list and setup text updated; the Rust/QML parity test
  covers the list).
- `auth_wire_tests.rs`: three loopback flood tests through `connect_identity`
  (small-frame flood before AUTH, oversized frame, 100 KiB flood during the OK
  wait) assert the category, rejection under five seconds and the relay's
  writes stopping within budget plus `SOCKET_SLACK` (8 MiB of loopback socket
  buffering, stated in the test).
- The maintainer ran `cargo update -p buzz-ws-client` (one package changed,
  ten unchanged). Measured on this machine (`--nocapture`): small-frame flood
  — the relay wrote 254 frames / 257 302 bytes before the socket closed, in
  1.9 ms (budget 128 messages); byte flood during the OK wait — 24 frames /
  2 457 912 bytes in 16.5 ms (budget 2 MiB); oversized frame — 125 small
  frames absorbed by socket buffers after it, then closed. Full helper suite
  301 passed (`RUST_TEST_THREADS=1`), `cargo fmt --check` clean, helper and
  agents smoke tests, `--bridge` and default previews pass, read-only relay
  check unchanged. Released as 0.0.21.

- Installed 0.0.21 on the maintainer's machine (helper SHA256
  `a4017c4b…`, run 36798105802); the panel authenticated through the bounded
  client at once, bundles `ready`, one shell restart.
