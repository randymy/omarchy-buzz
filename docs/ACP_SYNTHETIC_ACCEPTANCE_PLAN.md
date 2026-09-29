# Next synthetic ACP acceptance gate

Status: proposed test plan, not an executed result. Buzz base is
`781d39510cf23cfe224e8f521ae06a23377e06de` with these seven local patches,
in order: `ws-resource-limits`, `acp-permission-mode`,
`acp-interactive-login`, `acp-key-isolation`, `acp-harness-replies`,
`acp-deny-tool-requests`, `member-bound-events`. The targeted DB, HTTP, and ACP
transport checks passed in manual run `36493588467`. They do not yet connect an
actual relay, ACP subprocess, harness-owned publisher, and observed room reply
in one test.

## Smallest full path

Extend the existing manual `member-bound-conformance.yml` job with one ignored
synthetic test after its targeted checks. Build the seven-patch `buzz-relay` and
`buzz-acp` in its disposable checkout. Reuse the isolated
`scripts/relay-conformance-runner` topology (Postgres 17, Redis 7, loopback
relay, private fixture workspace, scoped cleanup) and the signed room setup in
`helper/src/acp_relay_tests.rs`. The runner currently rejects a modified Buzz
source tree, so pass it a second, clean checkout at the pinned revision for
schema and pinned fixture verification, while passing the **patched** relay and
ACP binary paths separately. Do not weaken its source-cleanliness check.

Add a new helper test, for example `acp_harness_replies_synthetic`, selected
explicitly by the runner. It should create a fresh **open** stream room, join a
disposable owner and agent, register the agent's exact NIP-OA ownership, and
subscribe to agent-authored kind-9 events before publishing the trigger. A
stranger may be prepared for a later routing check. The test sends one
owner-signed `AE-ID:<random>` message with an exact agent `p` mention. It
accepts only a verified agent-signed kind-9 event in the same room, containing
`AE-ACK:<same random token>`, with its NIP-10 parent/root pointing to the
trigger. An ACP `end_turn` alone is not acceptance: confirm that a room history
read finds the same signed event. The targeted transport test separately proves
exact event-ID acknowledgment handling. Then stop the supervisor and assert its
process-group cleanup.

Use a new small fake stdio ACP peer based on
`desktop/tests/e2e/fixtures/fake-acp-agent.mjs`. It should answer
`initialize`, `session/new`, `session/set_config_option`, and `session/prompt`.
Advertise `read-only` in session modes and return exactly one `mode` option
with `currentValue: read-only` after the setter. On a prompt containing the
random token, send one `agent_message_chunk` text update for the current
session with `AE-ACK:<token>`, then return `stopReason: end_turn`. It must never
invoke `buzz`, set `BUZZ_E2E_CLI_BIN`, make HTTP requests, execute a tool, or access a model. The implemented peer
sends a synthetic permission request without executing anything; it withholds
the reply until the harness selects rejection. It also emits a thought sentinel
that must be absent from the published message. The pinned fake peer **does** invoke the CLI when that variable
is present, which would prove a different publisher.

Adapt `scripts/acp-fixture` for a separate reply-only mode, retaining its clean
environment, loopback URL validation, fixed synthetic identities, deadline,
bounded private diagnostics, and descendant process-group cleanup. Feed the
disposable agent scalar through one inherited anonymous pipe via
`--private-key-fd <fd>` (`pass_fds` in Python); close the write end after
writing the fixed scalar and never put the scalar in argv, environment, or logs. In this mode do
not require or expose the `buzz`/Git helper binaries. Use the patched harness
with the following relevant arguments (plus the fixture's absolute Python peer,
agent owner, room, relay URL, and existing bounded timeouts):

```text
--private-key-fd <inherited-read-fd> --harness-replies
--session-policy thread --multiple-event-handling queue
--deny-tool-requests --permission-mode read-only
--respond-to owner-only --allowed-respond-to owner-only
--subscribe mentions --channels <room-uuid> --kinds 9
--mcp-command "" --no-memory --no-presence --no-typing
--heartbeat-interval 0
```

`--harness-replies` requires the FD identity, thread sessions, queued events,
no initial message, and zero heartbeat. The denial proposal requires an
advertised and confirmed mode; the existing fake peer does not provide that
contract. Keep the agent key joined throughout the positive turn so this gate
tests the full success path. A follow-up deterministic test can pause the peer
before `end_turn`, remove the agent, release the peer, and assert no guarded
reply is stored. Do not use a timed preflight membership check as a race proof.

Record only synthetic public keys, event IDs, room ID, bounded stage names,
patch digests, and pass/fail counts. Keep raw prompt text, FD contents, private
logs, and credentials out of uploaded artifacts. This gate proves synthetic
prompt-to-room publication by the patched harness and relay. It does not prove
real adapter behavior, provider login or Pro billing, tool safety, same-UID
isolation, or production relay availability; those remain separate gates in
`CHECKPOINT.md` and `ACP_READINESS.md`.
