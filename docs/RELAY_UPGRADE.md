# Relay upgrade required for validated thread reads

Status: preparation only. Production has not been changed.

The inspected deployment runs Buzz
`8342dfcc5890b81a269a8ec3db73a8a56f76ce79`; the helper and isolated conformance
tests target `781d39510cf23cfe224e8f521ae06a23377e06de`. An authenticated read
confirmed `thread_missing_bounds` on September 28, 2026.

## Why replies fail

The old `crates/buzz-relay/src/api/bridge.rs` handles the request as a legacy
`depth_limit` / `#e` query, ignoring `thread_window` and the requested kind filter
in that branch. It returns events without the signed kind-39007 bounds required
by the helper. The pinned source implements the explicit thread-window path.
This is not evidence that the room has no replies. The button styling and
username presentation are independent and are already installed.

## Safe upgrade sequence

1. Inspect the actual Compose project paths, image digest, mounts and database
   migration ledger. Emit only selected non-secret fields; never dump container
   environment, rendered Compose configuration or database event contents.
2. Resolve and verify the upstream image for the exact tested revision and
   target architecture. `.github/workflows/docker.yml` publishes SHA tags and
   digests; verify availability rather than assuming a tag exists. Preserve the
   running image. Do not pull mutable `main` into production.
3. Prepare owner-only, verified backups of PostgreSQL and associated object/Git
   storage plus existing configuration and relay signing identity, without
   exporting credentials into logs or this repository. Establish a consistent
   backup window and restore procedure.
4. Restore into an isolated disposable deployment and rehearse migrations,
   startup and signed thread conformance. Do not replay production events into
   a public relay or invoke agents. Keep restored data private and network
   isolated. Measure migration downtime and verify restore before rollout.
5. Account for all migrations 0028–0049. Migration 0029 adds deletion write
   fences; 0030 takes exclusive table locks; 0032 validates newly written signed
   roster snapshots. Existing malformed signed rosters are not repaired merely
   by upgrading. Migration 0044 removes interim ledger tables.
6. Follow upstream `docs/thread-window-deployment.md` for the concurrent
   prebuild of `idx_thread_metadata_window` before migration 0049. Verify valid,
   ready and live flags and exact index definition. Do not substitute a blocking
   index build or blindly drop an existing index.
7. Produce a concrete rollout/rollback procedure with measured downtime and
   restore results before changing production. Retain origin, relay identity,
   volumes, memberships and existing environment settings.
8. Apply the rehearsed migrations and pinned relay image, then check readiness,
   signer continuity, authenticated rooms/history and signed thread reads.
   A read failure must remain visible; do not claim success from readiness alone.

## Upstream operational constraints

`crates/buzz-db/src/runtime/migration.rs` runs embedded migrations under a
schema/destruction advisory lock. `buzz-admin migrate` is an explicit supported
entry point. Automatic migration requires `BUZZ_AUTO_MIGRATE`; startup also
validates the deletion-serving catalog, so swapping the binary alone is not
sufficient. Review the actual deployment configuration rather than copying new
example environment files.

The Compose upgrade wrapper pulls/recreates before its backup reminder; it is
not a safe backup-first procedure. A binary-only rollback is not sufficient:
the old relay can conflict with newer roster-write constraints. A rollback plan
must include restoring the corresponding database/storage snapshot and account
for any writes since that snapshot.

No legacy fallback that discards signed bounds is proposed. Nested reply
coverage, thread pagination and composing replies remain separate product work;
the current UI requests at most eight direct kind-9 replies.
