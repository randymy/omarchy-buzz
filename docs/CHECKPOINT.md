# Development checkpoint — 2026-09-27

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
Five cached packages lack notice text. An exact-commit nostr notice and its
verified provenance are separately saved under ignored
`artifacts/supplemental-notices/`; it has not yet been integrated into the
archive inventory. Four Bitcoin packages lack exact source revision metadata,
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
