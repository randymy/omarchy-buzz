# Isolated real-relay validation route

Messaging component conformance passed on 2026-09-26 in
[run 36280427402](https://github.com/randymy/omarchy-buzz/actions/runs/36280427402).
Evidence: [sanitized summary](evidence/relay-messaging-36280427402.json).
Full daemon/UI, ACP and Git/media conformance are separate gates.
No local services or production identities are used. Source baseline:
Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`.

This machine currently has no `docker`, `podman`, `postgres`, `initdb`, `psql`,
`redis-server`, `buzz`, or `buzz-relay` executable on the inspected PATH, and no
Docker/Podman socket at the usual inspected locations. PostgreSQL, Redis,
Docker, and Podman packages were not present in the package query. No relay
binary was found in the inspected temporary build or upstream target paths.
At the follow-up inspection the home filesystem had about 230 MiB free; `/tmp`
had about 2.1 GiB free. The existing helper target and relocated registry alone
occupied about 1.5 GiB and 416 MiB of temporary storage. A full relay build/image stack
has not been sized and cannot be assumed to fit. Hermit package definitions
for container tooling are cached, but these are not installed executables or
a running container engine. No local stack will be started on this constrained
host as part of this preparation.

## Recommended route

Use a disposable Linux VM or CI runner with Docker Compose, adequate disk,
Rust 1.95, and the pinned upstream checkout. Keep this desktop's production
services and keyring out of the experiment. Build only the relay and focused
Rust test clients; no desktop GUI, agent execution, or web build is required.
Do not infer that a published image corresponds to the inspected commit:
verify its source revision/digest first, or build the pinned source on the runner.

The upstream starting point is `scripts/start-isolated-test-relay.sh`, using
`docker-compose.harness.yml`: separate `buzz-harness` project, PostgreSQL 17,
Redis 7, MinIO and bucket initialization. The ordinary
`start-relay-for-tests.sh` uses the default development stack and is unsuitable
for an isolation guarantee on a workstation.

Do not run the isolated script unchanged as a general-purpose supervisor:
it uses a fixed tmux session/log path and resets its dedicated database on
each invocation. A runner wrapper should own one unique project, temporary
HOME/XDG/cache/build directories, private logs, and bounded child-process
cleanup. Bind published ports to `127.0.0.1`; the upstream compose file publishes
them without an explicit loopback address, and the script binds relay main to
`0.0.0.0`. Use a clean environment rather than inheriting `.env`, cloud
credentials, or host keyring/session bus variables.

Apply `schema/schema.sql` using the pinned `bin/pgschema` and then
`scripts/reconcile-schema-after-pgschema.sql`, as the upstream launcher does.
The schema requires `pgcrypto`; the reconciliation repairs required schema
state. Seed a synthetic community whose `host` exactly matches the chosen
relay authority including its non-default port. Relay tenancy fails closed
when that host mapping is absent. Generate synthetic relay and human identities
in test memory. Use NIP-98/NIP-42 directly; no Secret Service is needed for these
relay-facing tests. A later helper-process test can use the separately isolated
keyring smoke pattern if enrollment itself needs coverage.

## Required services and proof sequence

The standalone relay requires a PostgreSQL schema and Redis pub/sub at startup
(`crates/buzz-relay/src/main.rs`, database initialization and lines 452–467).
Media/S3 objects are initialized too. The default git object-store conformance
probe performs real object-store operations and fails startup on probe failure
(main.rs lines 599–633). Keep MinIO in the test stack instead of silently
bypassing that deployment gate. For this source, search is PostgreSQL full-text
search; no additional search service is needed.

A smaller, explicitly message-only experiment can use the supported
`BUZZ_GIT_CONFORMANCE_PROBE=false` setting to omit MinIO. The media client
constructor only configures the S3 client (`crates/buzz-media/src/storage.rs:220`);
provide fixed synthetic static S3 credentials so the AWS credential chain is
never consulted, and point the unused endpoint at a private loopback address.
Do not exercise media or git operations in this variant. PostgreSQL and Redis
remain required. This skips the object-store startup admission gate in
`crates/buzz-relay/src/main.rs:603`; it cannot establish full deployment, media,
or git validation. The default MinIO route above remains the complete startup
route. The messaging-only variant passed in the run linked above.

Use the pinned tests as conformance witnesses before adding helper-specific
integration:

- `e2e_relay::test_nip29_standard_client_flow`: synthetic group create/join,
  message and deletion workflow.
- `e2e_nostr_interop::test_channel_window_rows_overlays_and_exact_multiple_exhaustion`
  and `test_channel_window_rejects_half_cursor_and_client_overlay_kinds`:
  actual NIP-CW rows, auxiliary closure, signed bounds, exhaustion and relay-only
  overlay admission.
- Focused tests in `e2e_human_edit_agent_content`: author/owner edits,
  authorized and rejected deletions, and removed-member edit/delete denial.
  Those tests establish agent ownership with signed NIP-OA; they do not need
  an agent process or cloud model.

For example, on that prepared isolated runner, use the explicit disposable
relay origin with `cargo test -p buzz-test-client --test e2e_nostr_interop
 test_channel_window -- --ignored --test-threads=1` (a single shell argument
line when executed). These upstream tests default to localhost:3000 if
`RELAY_URL` is absent; the wrapper must explicitly set the isolated origin.

Then exercise the helper's catalog/history projection against the same real
relay using only generated fixture keys: trusted `self`, signed 39002/39000
membership discovery, an author edit, deletion of an edit, deletion of a row,
member removal, and revoked-access clearing. Confirm the exact request/event
IDs and bounded state transitions. The helper now implements scope-checked
sends with durable request binding and exact-key recipient selection. Validate that path too: generated fixture identities, signed kind-0
self-asserted names, room roster membership, explicit exact mentions, an accepted
send, a rejected send, an ambiguous lost receipt, and member removal before
sending. Isolate `XDG_STATE_HOME` as well as config/runtime/keyring directories:
the helper's metadata ledger must not share the user's state. Successful
upstream test-client sends do not establish that the helper can send, and
synthetic names are not proof of human or agent identity. Do not invoke an agent
in these message-delivery tests.

The broader test plan above exceeds the component coverage executed so far.
The helper's synthetic HTTP/WS tests remain a different verification layer;
the linked runtime evidence records the smaller real-relay test.

## WebSocket resource-bound integration seam

At the same pinned Buzz revision, `buzz-ws-client` has no supported caller
options for transport frame/message budgets, replay-buffer budgets, stream
injection, or active ping/pong probes. `NostrWsConnection` stores its socket and
`VecDeque` privately (`crates/buzz-ws-client/src/connection.rs:26`);
`connect_authenticated` delegates to `connect`, which calls `connect_async`
without a supplied config (lines 37–55). `next_event` receives already-parsed
messages; a caller's later size check cannot impose pre-parse transport limits.

The underlying pinned Tungstenite 0.29 defaults are finite but large: 16 MiB
frames and 64 MiB messages. Unrelated parsed messages still accumulate without
an aggregate budget while awaiting AUTH or a matching OK (lines 205, 257–259).
The existing wait loops use absolute deadlines; deadlines bound duration but
not the bytes accumulated during that duration. Replies to incoming Ping are
implemented (lines 148, 208, 262); no public API exposes a correlated active
probe. The helper already uses supported exact-ID NIP-45 COUNT for freshness.

The smallest change preserving upstream ownership is an upstream additive
connection-options API. Its implementation can use the already available
`tokio_tungstenite::connect_async_with_config`, plus frame-count/serialized-byte
accounting at every replay-buffer insertion, removal and drain. Keep the
existing entry points with their current defaults; let the helper opt into
stricter budgets after pinning the reviewed upstream revision. Include typed
resource-limit failures and small-frame flood/oversize tests on AUTH and OK
paths. Public `send_raw` or an outer timeout cannot implement this today, and
copying the private connection into the helper would violate the no-fork
boundary. See `helper/WS_UPSTREAM.md` for the proposed contract. Process memory
limits remain mitigation, not proof of strict pre-auth memory bounds.

## Executable disposable-runner test

The manual `Isolated Buzz messaging conformance` GitHub Actions workflow builds the
exact pinned upstream relay and compiles the helper's ignored
`real_relay_messaging_conformance` test. It invokes
`scripts/relay-conformance-runner` only on a GitHub-hosted Linux runner, with a
fresh Compose project, a fresh PostgreSQL volume, an internal network and
loopback-only published PostgreSQL and relay-main ports. The relay runs in a
restricted container because upstream health and metrics listeners bind all
container interfaces; those ports are not published.

The fixture uses an explicitly public synthetic owner and an in-memory member
identity. It checks NIP-42, exact COUNT freshness, signed helper HTTP
catalog/roster/history queries, durable SDK sending, exact real acknowledgement,
persisted mentions, membership removal and denied subsequent writes. This is
helper-component conformance, not the full QML/daemon/ACP acceptance scenario.
Real edit/delete overlays, reconnect/lost receipts and full daemon coverage
remain follow-up work. No workstation or production stack is accepted.

Run `gh workflow run relay-conformance.yml --repo randymy/omarchy-buzz` after
pushing the reviewed workflow. The named ignored test must actually pass;
zero matching tests fails. Offline mock tests cover isolation and scoped
cleanup. Only a sanitized summary is uploaded; private synthetic logs stay on
the disposable runner. Generated fixture secrets do not change the production
helper's no-secret-argv/environment boundary. See CHECKPOINT.md for run results.

The workflow explicitly selects `--messaging-only`: upstream
`BUZZ_GIT_CONFORMANCE_PROBE=false` skips the object-store startup probe and no
MinIO is started. The pinned MinIO image rejected anonymous pulls in run
36279475453; this mode uses upstream configuration without source patches.
Readiness still checks PostgreSQL, Redis and the deletion serving catalog.
Evidence records `validationScope: messaging-only` and `objectStoreProbe: false`.
This does not certify Git, media, or object-store deployment. The optional runner
mode without this flag retains the object-store probe when its images are
accessible; even that probe is not full Git/media conformance.

The runner attaches PostgreSQL and the relay to a second dedicated bridge for
loopback port publication. Docker does not publish ports for containers attached
only to an internal network; that caused schema connection refusal in runs
36279871986 and 36280066547. Redis/object storage stay on the internal network.
The second bridge permits outbound traffic from the relay/database; evidence
records `relayEgressBlocked: false`. This fixture isolates data and inbound
ports, not all egress. It runs only on a disposable CI host with synthetic
identities and no provider credentials. This topology is not a sandbox claim
for a real agent. See [Docker networking](https://docs.docker.com/engine/network/)
and [the upstream report](https://github.com/moby/moby/discussions/53256).
