# Buzz for Omarchy 0.0.7 — messaging preview candidate

Status: **candidate notes for a source-only development preview; no public release or tag has been made**. A source checkout alone does not provide a working installation: users must build and install the matching Rust helper and user socket. This candidate is not a complete agent product.

## What works

- The Omarchy bar, overlay and resizable desktop window show authenticated joined rooms and conservative recent-message snapshots. The window and overlay share one view, selected room and in-memory draft.
- A bounded, read-only thread pane shows up to eight depth-one replies for a selected root. It is not a full thread timeline and the composer does not send replies or attachments.
- Plain-text sending uses exact public-key mentions selected from a verified, partial room roster. A matching positive relay acknowledgement is shown as accepted; lost acknowledgement remains delivery-unknown and is never retried automatically.
- Local activity badges and optional generic notifications report newly observed activity. They are session-only hints, not Buzz-synchronized unread counts. Verified profile names are self-asserted display labels; a self-described agent profile does not establish execution or authority.

## Installation and compatibility

Use the [source installation steps](../README.md#try-the-local-development-preview), [matching helper build](../README.md#install-the-development-helper), and [user socket setup](../service/README.md). The helper needs Rust 1.95 to build, Linux Secret Service to enroll an existing human identity, and a configured Buzz hosted or custom relay. Hosted account and identity handoff remains manual. Update the plugin and helper together; plugin updates do not replace the helper binary. Preserve the helper's delivery ledger during updates.

The inspected Omarchy shell source is [`7b336b1`](https://github.com/omacom/omarchy/tree/7b336b1b0da722e7bb864a7136f91e784ef731bf), with the built-in bar. The helper pins Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`. ARM64 helper, synthetic IPC/UI and isolated relay checks, plus native window and installed-panel checks, are recorded in the [checkpoint](CHECKPOINT.md). This does not establish support for all Omarchy 4.x builds, replacement bars, x86_64 binaries, or every desktop layout. Physical shortcut, multi-monitor, lock/DND and failure-state acceptance remain incomplete.

## Limits and release gates

Recent history is a bounded, partial snapshot. It cannot prove that a relay disclosed every edit or deletion, and the UI does not provide full live synchronization. The helper does not launch room agents, approve actions or claim an agent's work state. Experimental ACP and subscription work is separate from this messaging preview.

Before a public binary package or claim of a fully supported release, finish the upstream WebSocket resource-bound review, target desktop and architecture acceptance, and the actual binary's third-party notice and distribution review. The [notice inventory process](DEPENDENCY_NOTICES.md) now has pinned CC0 text evidence for four previously missing Bitcoin crate notices, while retaining manual review flags; a fresh inventory must be generated against the build's exact lock, target and features. No dependency inventory or checksum file is a publisher signature or legal clearance. The [packaging procedure](PACKAGING.md) remains a local preview workflow.

A source-only prerelease tag could describe this **development preview** if the reviewed source and these limits are kept together. It would not ship a ready-to-run helper, close the resource-bound or desktop gates, or satisfy the goal of a complete working product on both target architectures.
