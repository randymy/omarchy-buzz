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
not the vPerps checkout. Stock agent tools hold their dedicated Buzz signing key;
room/owner filters route input and do not restrict all possible signed events.
Shared network and provider credentials inside the dedicated profile remain
explicit trust limits. Do not present this as a hostile-code or network sandbox,
a future vMachine authority layer, human approval UI, or full agent-state dashboard.

## Running UI upgrade corrected

The operator reported `Incompatible helper` despite matching 0.0.8 files.
`omarchy restart shell` followed by normal-window summon resolved it. A targeted
window capture confirmed authenticated rooms and VASSIVE DEV messages. The
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
