# Isolated real-relay validation route

Research only, 2026-09-26. No services started, packages installed, images pulled,
production configuration read, or real identities used. Source baseline:
Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`.

This machine currently has no `docker`, `podman`, `postgres`, `initdb`, `psql`,
`redis-server`, `buzz`, or `buzz-relay` executable on the inspected PATH, and no
Docker/Podman socket at the usual inspected locations. PostgreSQL, Redis,
Docker, and Podman packages were not present in the package query. No relay
binary was found in the inspected temporary build or upstream target paths.
At inspection the home filesystem had about 332 MiB free; `/tmp` had about
2.6 GiB free. The existing helper target and relocated registry alone occupied
about 945 MiB and 411 MiB of temporary storage. A full relay build/image stack
has not been sized and cannot be assumed to fit.

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
IDs and bounded state transitions. Sending validation requires a separately
implemented helper send path; successful upstream test-client sends do not
establish that the helper can send.

This document is a concrete preparation route, not evidence of successful
real-relay runtime validation. The helper's existing synthetic HTTP/WS tests
remain a different verification layer.
