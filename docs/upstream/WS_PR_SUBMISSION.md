# Buzz WebSocket resource limits: upstream PR packet

Status: **draft submission packet only**. No Buzz source, installed helper,
production relay, remote branch, issue, or PR was changed. Target the locally
available Buzz `block/buzz` commit
`12670bd0f037c66a682272bb81c46c3f254fad74` (2026-09-28). The patch was
written at `781d39510cf23cfe224e8f521ae06a23377e06de`; the four existing source
files it touches are byte identical between those revisions, and the new
`resource_tests.rs` file is absent at both revisions. A fresh archive of
the target revision accepted `git apply --check` and `git apply` without edits.
Recheck the actual upstream PR base before submission because `main` can move.

September 29 refresh: upstream main advanced to
`8519db1532efd6cda8f72bb6454c00fc3f87cfba`. Git comparison confirms the shared
WS client and test-client source files remain unchanged from `12670bd`; the
focused test evidence therefore covers the same source bytes. This is not a
full workspace build at the new revision. New relay FI admission changes are
tracked [separately](BUZZ_8519DB1_COMPATIBILITY.md).

## Proposed PR

**Title:** `fix(ws-client): bound connection frames, replay queues, and deadlines`

**Body:**

> The shared NIP-42 client can accumulate unrelated relay messages while it
> waits for AUTH or a publish receipt, and a peer can extend a receive by
> sending repeated pings. This change gives connections finite frame, message,
> replay-queue and operation budgets. Exceeding a budget closes the socket and
> returns a typed resource error; existing connect entry points keep finite
> defaults, while callers can supply explicit `ConnectionOptions`.
>
> The patch also bounds outbound JSON and AUTH challenges, and consumes a
> challenge buffered during an OK wait exactly once. The test client preserves
> the new error categories. The defaults are 1 MiB per frame/message, 256
> queued messages, 8 MiB queued wire text, 20 seconds to connect, 40 seconds
> for authentication, 10 seconds to write and 5 seconds to close. Publish
> retains its 30-second deadline and now includes the write.
>
> Validation: the patch applies cleanly to Buzz `12670bd0`; an isolated Rust
> 1.95.0 test crate using the patched upstream source passed 16 client tests,
> 5 test-client consumer tests, and doc tests on Linux ARM64. `cargo fmt --check`
> passed in that staging tree. Full Buzz `just ci` and a native client flow
> remain for the upstream PR branch. Byte accounting measures UTF-8 wire text,
> not exact heap use; new error variants can require downstream exhaustive
> match updates.
>
> Related work: [#4212](https://github.com/block/buzz/pull/4212) bounds
> desktop/ACP publish waits and improves rate-limit rejection handling; it
> does not change `buzz-ws-client`. [#3964](https://github.com/block/buzz/pull/3964)
> adds verified, bounded CLI listening. This PR covers the shared client's
> frame, replay-queue and deadline limits.

Do not add `buzz-review-completed` to this body yet. Buzz's
[agent contributor guide](https://github.com/block/buzz/blob/12670bd0f037c66a682272bb81c46c3f254fad74/AGENTS.md) reserves that marker for a
completed agent review, exercised flow, and **human-confirmed** test. Do not
claim those steps from the isolated tests.

## Exact contribution

Apply [ws-resource-limits.patch](ws-resource-limits.patch), SHA-256
`e781a5b984ed3895ddd5ca0f893918f35028753f79b6c0a159d56f88c09bf24c`.
The PR contains exactly these five Buzz files:

| File | Change |
| --- | --- |
| `crates/buzz-ws-client/src/connection.rs` | `ConnectionOptions`, finite defaults and validation, Tungstenite frame/message limits, bounded queue/deadlines/writes, and connection close on overflow. |
| `crates/buzz-ws-client/src/error.rs` | Typed `ResourceLimit` and `InvalidConnectionOptions`. |
| `crates/buzz-ws-client/src/lib.rs` | Export the new options type. |
| `crates/buzz-ws-client/src/resource_tests.rs` | Production-seam loopback regression cases for AUTH/OK floods, frame and fragmented-message bounds, challenge handling and deadlines. |
| `crates/buzz-test-client/src/lib.rs` | Preserve the two new errors in exhaustive conversion and test the mapping. |

The patch has 732 insertions and 85 deletions. It adds no relay endpoint,
database migration, adapter policy, or plugin code. Keep it separate from the
ACP room-agent series. The public API defaults and typed errors deserve explicit
maintainer review; large legitimate payloads may require configured larger
limits, while exhaustive downstream matches may need updates.

## Reproduction and evidence

For this packet, `git archive 12670bd...` populated a new `/tmp` tree containing
the WebSocket crate and test-client library. The patch applied cleanly there.
The existing [isolated test manifest and lock](../../tests/upstream-ws/Cargo.toml)
were copied only into that temporary tree, along with its consumer shim.
Rust 1.95.0 on Linux ARM64 then passed the 16 crate tests, 5 consumer tests,
and 0 doc tests with no failures. `cargo fmt --check` passed. The tests use
disposable loopback sockets and synthetic identities; no production relay or
user account was contacted. A private temporary Cargo cache held downloaded
locked dependencies. Earlier 21-test evidence at the original base is in
[ws-contribution.json](../evidence/ws-contribution.json); this packet's target
revision was tested again rather than inferred solely from that record.

The standalone manifest compiles the real patched source and consumer library,
but is not the full Buzz workspace. It does not establish native desktop
behavior, other downstream exhaustive matches, live-relay interoperability,
the full `just ci` gate, or adoption by upstream. The proposed bounds cap wire
data and queue count, not precise heap allocation after parsing or the memory
of a caller-created outbound value. These limits must be reviewed against
legitimate upstream message sizes.

## Current open-PR overlap (read-only check)

The open [PR #4212](https://github.com/block/buzz/pull/4212) changes
`crates/buzz-relay/src/connection.rs`, `crates/buzz-acp/src/relay.rs`, and
`desktop/src/shared/api/relayClientSession.ts`. Its body addresses
rate-limited EVENT rejection and bounded desktop/ACP publish waits. It shares
the deadline and pending-send concern with this proposal, but it does not touch
the shared `buzz-ws-client` or its test client. Keep this patch focused on the
shared transport's frame, replay-queue and operation limits. Compare caller
behavior again if #4212 merges before submission so the PR does not imply it
introduced every publish deadline.

The open draft [PR #3964](https://github.com/block/buzz/pull/3964) changes
`buzz-cli` commands, client code, tests and CLI docs. Its bounded catch-up,
verified filtering and reconnect behavior serve long-running CLI listeners;
none of its listed files are in this five-file patch. It is complementary, not
a substitute for bounding the shared client's pre-authentication and OK-wait
buffers. Link both PRs as related work and ask maintainers whether the proposed
defaults should align with those callers. The public bodies and changed-file
lists support a separate focused PR; no author contact or comment was made.

## Submission readiness and required sign-off

- [x] Read Buzz `AGENTS.md`, `CONTRIBUTING.md`, `VISION.md`, and the relevant
  `TESTING.md` guidance. The change supports Buzz's Nostr conversation transport
  without changing protocol events or access control.
- [x] Confirm the five source paths are unchanged at target `12670bd`; apply
  the patch cleanly in an isolated target tree; pass focused tests and formatting.
- [x] Inspect the two closest open PRs' public bodies and changed-file lists:
  #4212 and #3964, as summarized above. No changed-file overlap was found.
  A September 29 title scan of the first 100 open PRs/issues found no matching
  resource-bound contribution.
  [PR 7852](https://github.com/block/buzz/pull/7852) concerns thread windows over
  WebSocket REQ, not these client queue limits. This bounded title scan does not
  establish that the full remote queue has no duplicate; recheck before sending.
- [ ] Review the patch against current upstream `main`, including default limits,
  API compatibility, and the regression tests' production seams. Refresh the
  target and rerun checks if any affected files change.
- [ ] On a contribution branch, run Buzz's required `just ci`; resolve any
  workspace formatting, Clippy, test, build or downstream-match failures.
- [ ] Run the affected native client flow against a disposable local relay,
  then have the human author test it and explicitly confirm the result. Per
  upstream `AGENTS.md`, an agent test does not replace human confirmation.
- [ ] Obtain agent review; fix concrete blockers or record the human author's
  explicit decision to decline them. Add `buzz-review-completed` only after
  agent review, exercised flow and human test all hold; return to draft after
  behavior-changing edits until they are repeated.
- [ ] Create all contribution commits with `git commit -s` so the DCO Check
  sees a `Signed-off-by` trailer. The human contributor must confirm their
  right to license the patch under Apache-2.0 and obtain employer IP sign-off
  if applicable. The PR title uses upstream's Conventional Commit format.
- [ ] Open as a **draft PR** first. Buzz's `CONTRIBUTING.md` tells external
  contributors to use a fork. This packet creates no fork or remote artifact;
  if the contributor lacks direct branch access, the upstream PR workflow will
  require a contribution fork despite the goal of avoiding a maintained Buzz
  fork. A fork for submission would not be a deployed product dependency.

The only requested external action after this packet is reviewed is opening
the focused draft contribution through the permitted upstream workflow. Do not
mark it ready for review or merge before the upstream checklist is complete.
