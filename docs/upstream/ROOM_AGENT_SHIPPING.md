# Shipping subscription room agents through upstream Buzz

Status: contribution plan, **not submitted, installed, or enabled**. Source base
for the staged patches is Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`.
The locally available later upstream commit
`12670bd0f037c66a682272bb81c46c3f254fad74` still has the open-channel
nonmember fallback in `check_channel_membership` and no member-bound publication
route. Its ACP files are unchanged from the patch base; its relay ingest and router
have other changes, so the relay patch needs a fresh rebase if pursued. Source
inspection is not runtime acceptance. No Buzz, relay, adapter, or installed plugin
is changed by this document.

## First-release publication contract approved

The product owner approved ordinary Buzz room permissions for the first release
on September 29, 2026. The requested product outcome is a
subscription-authenticated agent that receives an admitted room mention and sends
a signed reply as its own identity, without a private Buzz fork or sending the
relay key to the adapter. The original requirement did **not** explicitly require
the relay to reject an already-generated reply if room membership changes during
publication. The atomic member-bound rule arose later from review of the reply
prototype. It remains an optional stronger future extension, not a first-release
gate. This decision selects the existing `POST /events` route; it does not enable
agents or waive the other gates below.

| Contract | Smallest upstream contribution | Honest removal behavior |
| --- | --- | --- |
| Ordinary Buzz permissions | Publish signed kind-9 replies through existing `POST /events` and its NIP-98 admission. No relay API patch. Require managed room membership at launch and suppress when a removal is already observed. | A reply racing membership removal can still be accepted in an open room. Membership at send time is not guaranteed, and an HTTP acceptance does not prove it. A closed room's ordinary rejection is useful but is not an atomic membership contract for all room types. |
| Atomic member-bound replies | Add the proposed `POST /events/member-bound` route and transaction fence, then send agent replies only there. No fallback to `/events`. | Publication and membership removal are ordered by the same lock. This covers the stated membership race, subject to the proposal's channel-deletion and future-authority limits. Relays without the route cannot accept these agent replies. |

The approved ordinary contract accepts the open-room race and describes
revocation as best effort. A preflight query,
client-side membership cache, or added optional HTTP header cannot make that
contract atomic. The existing [member-bound proposal](MEMBER_BOUND_EVENTS.md)
is a reviewed stronger future option; it is not an existing upstream API.

## Manual publication-contract conformance

The manual-only `acp-room-conformance.yml` workflow has a
`publication_contract` choice. Its default, `ordinary`, applies the six staged
Buzz patches for the approved first-release contract and omits
`member-bound-events.patch`. Selecting `member-bound` applies all seven patches,
including the stronger optional endpoint proposal. The ordinary choice leaves
the harness reply on upstream's existing `POST /events` route. Both
choices build a disposable relay and run the same synthetic ACP reply fixture
through signed receipt, persistence and thread projection. Neither choice
changes a production relay or enables a room agent.

Run either choice explicitly from the Actions workflow dispatch UI, or with
`gh workflow run acp-room-conformance.yml --ref <test-branch> -f publication_contract=member-bound`
and the same command with `publication_contract=ordinary`. Use a branch that
contains the workflow and patches under review. When the runner generates a
sanitized `summary.json`, the uploaded copy records `publicationContractSelected`,
`publicationRouteSelected`, `ordinaryPrivateRoomMembership`, and
`membershipRemovalConformance: concurrent_removal_not_tested`;
its stage and pass/fail fields still determine whether conformance actually
ran. A pass establishes the signed reply and persisted-thread path for that
selected disposable build. The messaging fixture additionally tests ordinary
private-room HTTP acceptance before removal and rejection after owner-verified
removal. It does not test an open-room publication race or atomic revocation.

Run [36587419111](https://github.com/randymy/omarchy-buzz/actions/runs/36587419111)
passed the ordinary variant at `5122750`: real relay messaging, synthetic ACP
routing, signed reply receipt, persistence and plugin thread projection.
[Recorded evidence](../evidence/ordinary-replies-36587419111.json) preserves
the exact build and limits. The later [run 36611201645](https://github.com/randymy/omarchy-buzz/actions/runs/36611201645)
passed sequential private-room HTTP admission and post-removal rejection; see
[its evidence](../evidence/private-room-revocation-36611201645.json). Open-room
and concurrent-removal semantics remain separate.

## Smallest upstream patch series for ordinary room replies

Contribute the independent, reviewable Buzz patches in this order, rebased and
tested against the upstream target revision: [WebSocket resource limits](WS_RESOURCE_LIMITS.md),
[explicit permission mode](ACP_PERMISSION_MODE.md),
[interactive login](ACP_INTERACTIVE_LOGIN.md),
[harness key isolation](ACP_KEY_ISOLATION.md),
[harness-owned replies](ACP_HARNESS_REPLIES.md), and
[permission-request denial](ACP_DENY_TOOL_REQUESTS.md). The optional
[authentication timeout](ACP_AUTH_TIMEOUT.md) applies to ACP-owned interactive
login; the current native Codex login path does not depend on it. The
`member-bound-events.patch` is **not** part of this ordinary series. Keep the
subscription policy contributions for the Codex and Claude adapters separate
from Buzz, since they are adapter changes, not relay changes.

The reply patch already targets `/events`. Retain its active-session text capture,
64 KiB cap, end-turn requirement, admitted thread destination, upstream SDK
signing, one-shot NIP-98 POST, exact event-ID acknowledgment, bounded HTTP
response/time, and distinct `not_sent`, `rejected`, and `delivery_unknown`
outcomes. Retain the already-observed removal suppression in
`crates/buzz-acp/src/lib.rs::handle_prompt_result`, but document its race limit.
Do not apply the `/events/member-bound` URL substitution from
`member-bound-events.patch`, nor the relay/database/router additions in that
patch. If the patch series is split for upstream review, keep the reply transport
and its synthetic tests with the harness reply change; keep key isolation and
permission denial as prerequisites for the supported room-agent configuration.

Before merging the ordinary path, add a relay-backed test using the real
`POST /events` route: active member publishes the correct signed thread reply;
closed-room removal rejects; open-room removal may accept under current rules;
wrong author/room and malformed or mismatched acknowledgment do not report
success. Cover an observed removal before `handle_prompt_result` suppressing the
reply, and a deliberately raced removal after that check to pin the documented
limit. Existing loopback HTTP and synthetic ACP tests verify signing, routing,
capture and delivery classification, but do not establish these relay semantics.
Run the full rebased ACP tests and isolated prompt-to-persisted-thread acceptance
against a disposable relay. No production message or model turn is needed for
these conformance checks.

If the stronger contract is pursued later, separately rebase
`member-bound-events.patch` on the selected upstream relay revision and retain
its exact-route NIP-98, kind-9, authoritative membership lock, transaction, and
no-fallback client tests. The already-passing disposable Postgres/Redis and router
checks in [MEMBER_BOUND_EVENTS.md](MEMBER_BOUND_EVENTS.md) are useful evidence at
the old base, not proof that the rebased code is accepted or deployed upstream.

## Existing upstream keyless work

Before proposing a second long-term signing transport, align with open Buzz
PRs #6922/#6967. The [pinned review](KEYLESS_UPSTREAM_REVIEW.md) finds a broker
mode with tool-directed `message.reply`, but no completed-turn harness reply.
The broker credential is deliberately provisioned to adapter/MCP processes;
its host policy and credential scope therefore need validation. Permission
handling and subscription routing are still separate. This unmerged stack
changes the preferred upstream discussion, not our installed interfaces or
production authorization.

## Gates outside publication policy

Neither publication choice permits an unattended agent launch by itself. The
original required outcome includes first-class Pro subscription sign-in for
Codex and Claude, with no automatic API billing or provider switch; a real model
turn must confirm the effective route before claiming that outcome. September 29
fixed-response model tests now pass for Codex's separate ChatGPT profile and
Claude's existing native subscription profile, after guarded API-auth rejection.
These establish the tested route and response, not a billing-receipt audit.
The ACP permission mode must be confirmed for each session, permission requests
must be denied unless a separate authority is deliberately designed, and native
tools that never request ACP permission still need a limited workspace, process,
filesystem and network policy. The harness FD key path removes known relay-key
handoffs; it does not isolate same-UID processes. A supervisor must own the
agent identity, room membership, native profile, artifact pins, resource bounds,
start/stop and cleanup. The user-facing plugin remains a messaging client; it
does not gain secrets or an approval surface from these contributions.

Before normal use, record upstream review/merge and a new pinned dependency,
ARM64 build and adapter compatibility, a full isolated mention-to-signed-reply
run, ordinary revocation behavior and its documented race limit, and a
deliberately authorized real subscription acceptance run. Existing synthetic
acceptance and status evidence does not certify a paid/model turn, effective
billing, same-UID isolation, or deployed relay support. Keep automatic triggers
disabled until these gates are resolved.

## Draft upstream issue (do not post)

**Title:** ACP harness should publish completed room replies without exposing the relay identity to adapters

**Body:** `buzz-acp` currently relies on agent-side Buzz CLI signing for normal
room replies. A harness-only identity input can remove that credential handoff,
but then a completed ACP turn has no room reply path. Please accept a scoped,
opt-in harness publisher: capture bounded user-facing text only from the active
session, sign a kind-9 reply to the admitted thread using existing Buzz SDK
primitives, and submit once through the existing NIP-98 `POST /events` bridge.
Report exact acceptance, rejection, not-sent and delivery-unknown separately;
never retry with a new event ID. Gate the mode on key isolation, queued thread
sessions and explicit permission denial. This proposal uses ordinary Buzz room
permissions, whose open-room removal race must be documented. If upstream wants
atomic membership at publication, the separate member-bound endpoint proposal
can be reviewed independently. Synthetic ACP/HTTP cases and a disposable-relay
prompt-to-persisted-thread test should accompany the PR. No production relay or
agent deployment is requested.

## Draft upstream PR description (do not post)

**Title:** Add opt-in harness-owned ACP room replies with bounded publication

**Body:** With an FD-fed relay identity, the ACP adapter no longer receives a
Buzz signing key, so its completed text cannot be posted through the current
agent-side CLI path. This patch binds active-session text to the admitted thread,
signs a kind-9 reply in the harness, and makes one bounded NIP-98 `POST /events`
request. It publishes only after `end_turn`, excludes thought/tool and stale
session output, and reports ambiguous delivery without resending. Configuration
requires the existing key-isolation and safe permission prerequisites. Planned validation covers text/session limits, cancellation, signatures, wrong-room routing,
acknowledgment classification, and real ordinary relay permission behavior.
Open-room membership removal can race publication under the existing relay
contract; that limit is explicit. The stronger atomic publication API is a
separate optional PR. Required validation before submission: full rebased ACP build/tests and isolated
prompt-to-persisted-thread fixture on the target Buzz revision; no real account
or production relay operation.
