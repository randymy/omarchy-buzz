# Buzz for Omarchy — engineering handoff

Last verified: September 29, 2026 (America/Chicago). This is a point-in-time
handoff; verify the checkout and installed state before making changes.

## Start here

The project is a generic Omarchy community plugin for upstream Block Buzz.
Messaging and one isolated Codex subscription room agent work on the operator's
self-hosted relay. This is still a development preview, not a finished agent
product or a published community release.

Read, in order:

1. [AGENTS.md](../AGENTS.md): binding project instructions and approved decisions.
2. [DESIGN.md](../DESIGN.md): architecture, source references and trust boundaries.
   Its opening status describes the original design stage, not current capability.
3. This handoff, then the **latest sections** of [CHECKPOINT.md](CHECKPOINT.md).
   Earlier checkpoint entries record historical blockers that may be resolved.
4. [ROOM_AGENTS.md](ROOM_AGENTS.md), [DEVELOPMENT.md](DEVELOPMENT.md), and
   [helper installation](../service/README.md) for the area being changed.

Do not reconstruct the project from the chat history. Credentials were pasted
in that history; do not reproduce or reuse them. Current source, installed
metadata, and dated test evidence are the working references. Reconcile any
conflict explicitly rather than assuming an old milestone is current.

## Verified baseline

| Component | State at handoff |
| --- | --- |
| Repository | `https://github.com/randymy/omarchy-buzz`, branch `main`; local `~/Projects/omarchy-buzz` |
| Latest implementation commit | `44c3e2c` — release 0.0.12 (live updates for the open room and thread through a relay subscription that only triggers verified refetches; on top of 0.0.11's older history, new-DM open, reply counts, quiet room check, existing DMs, Desktop-mode threads and last-room memory); pushed to `origin/main` |
| Installed UI | `~/.config/omarchy/plugins/community.buzz`, verified at the same implementation commit |
| Plugin/helper version | `0.0.12` (installed September 30, 2026, 07:14 CDT); 0.0.9–0.0.11 were installed briefly the previous evening |
| Helper binary | `~/.local/bin/omarchy-buzz`; SHA256 `15408fe56624fcd2b16f7c23ed1085cba3dca072a78a5e6e63313b66488996de` |
| Helper build | [ARM64 run 36712850154](https://github.com/randymy/omarchy-buzz/actions/runs/36712850154), source `44c3e2c589e4ad868df6c599a8d262b77c59fdf6`; after installation the live subscription was observed primed on the real relay (`history.live=true` for the selected room); each new binary is first run read-only in a private short-path runtime directory against the real relay (`scratchpad/check010.py` pattern: authenticated, 4 stream rooms, no rejection) before installation; rollback backup `~/.local/share/omarchy-buzz/backups/20260930T121404.583817Z` |
| Buzz dependency | Official upstream `781d39510cf23cfe224e8f521ae06a23377e06de`; do not silently advance the pin |
| Agent runtime | Stock Buzz ACP, Codex ACP `2.0.0`, native Codex `0.158.0`; [stock build 36639519388](https://github.com/randymy/omarchy-buzz/actions/runs/36639519388) |
| Host | ARM64 Omarchy VM; installed Omarchy package reported `4.0.3-1`; see DESIGN for source/package distinction |
| Supervision | `omarchy-buzz.service` active/running, socket active/enabled; `omarchy-buzz-codex.service` active/running, static and manually started |

The helper may legitimately exit when idle and reactivate from its socket.
Codex is **not enabled at login**; do not promise it survives reboot automatically.
Artifact downloads expire; the local verified helper artifacts are under
`~/.cache/omarchy-buzz/helper/<run id>/` while retained.

Local helper builds work without root: the wrapper
`/tmp/claude-1000/…/scratchpad/tools/helper-cargo` (session scratch, recreate
if missing: pinned toolchain `~/.rustup/toolchains/1.95.0-aarch64-unknown-linux-gnu`,
a read-only Python `pkg-config` shim over `/usr/lib/pkgconfig`, per-worktree
`CARGO_TARGET_DIR`). Never share one target directory between worktrees: cargo
ran another worktree's test binary. Never use `git stash` with several
worktrees: the stash list is shared. The `auth::send_integration_tests::observer_*`
tests use 1-second timeouts and flake when the load average exceeds ~5.

## What works, and what remains limited

- Native bar integration, normal Buzz window and optional overlay; hosted or
  custom relay configuration. The operator currently uses a Mac mini relay.
- Joined-room snapshots, recent messages plus older pages on request (100 held),
  sending, threads in a right-hand panel with their own composer (oldest-first,
  nested, up to 200 replies, Desktop's mode), relay reply counts on room
  messages, existing direct messages named by participant, starting a direct
  message with a verified member, room-scoped display names, exact-key mention
  completion, last-room memory and optional alerts. Everything since 0.0.8 is
  verified with synthetic fixtures and read-only relay checks only: no thread
  reply, older page, DM or new-DM open has been exercised against the real relay
  by the maintainer.
- Type `@codex`, then choose the actual roster suggestion with Tab/Enter/click.
  Text resembling a mention alone is not proof of an attached routing key.
- The dedicated **Codex (isolated)** agent uses the existing separate ChatGPT
  login, with forced subscription authentication and no automatic API fallback.
  It accepts only its configured owner's mentions in the configured room.
- Real subscription acceptance passed. A later user message also received a
  signed Codex reply after 18 seconds. See [acceptance evidence](evidence/stock-codex-room-2026-09-29.json)
  and the checkpoint's reply-visibility entry. Do not send another production
  task merely to prove the UI works.
- 👀 and 💬 are observed, verified reactions: queued and actively prompting
  in stock ACP. They are removed after completion. They are cosmetic evidence,
  not authoritative agent state or reply counts. Refresh is periodic, not live
  streaming; short-lived reactions can occur between snapshots.
- Room/message completeness and activity remain explicitly partial/local.
  There is no synchronized unread counter, full history pagination, attachment
  support, full agent dashboard or security approval UI.
- **No Claude room service is installed.** Separate subscription smoke evidence
  is not room acceptance. Goose is prospective, not a verified supported runtime.
- Public release packages, community listing and general agent setup/lifecycle
  UX remain unfinished. [SHARING.md](SHARING.md) describes preview sharing.

## Most recent bug fixes: preserve these

1. Names/mentions could stay unavailable after a busy recipient lookup. There
   are bounded, scope-checked read retries. Never retry uncertain message sends.
2. Thread expansion could leave replies outside the scrollable viewport. The
   rendered test now checks a bottommost message's expanded reply is visible.
3. Every status frame replaced UI arrays; the thread timer cleared replies every
   eight seconds. Commit `388d3d6` retains identical projections, keeps same-scope
   snapshots visible during loading, and updates message delegates by event ID.
   The rendered regression checks delegate and scroll retention on new messages.

4. The helper's joined-room check blanked the whole panel for about two seconds
   every ~31 seconds. Commit `d0e5299` keeps same-scope snapshots until the
   helper's views are re-established and holds submissions made meanwhile. See
   the checkpoint's periodic room check entry before changing `resyncStage`.

Both refresh fixes are installed and have synthetic rendered test coverage.
There is no subsequent user confirmation of the improved experience yet.
Do not claim full live desktop acceptance solely from helper/socket tests.

## In flight at the September 30 evening stop (read before continuing)

**Installed:** helper 0.0.19 (`01e5a74`; clock-skew detection and the file
chooser; SHA256 `f3e223730432f5ff53d4ff4d9b1f29a2c74fb64f4a04ba7b65433e44ac15cd78`,
ARM64 run 36781835423, rollback backup
`~/.local/share/omarchy-buzz/backups/20260930T220104.787912Z`) with both
harness bundles `ready`, and the plugin at `216bc24` (`main`), which moves the
file chooser out of the shell: **Browse…** now runs `scripts/pick-file`
(python-gobject → xdg-desktop-portal `FileChooser.OpenFile`) in its own
process, because the earlier in-shell `QtQuick.Dialogs` dialog aborted
omarchy-shell (two SIGABRT coredumps, GLib "dconf worker"). The script alone
was verified to open the portal's dialog; the click from the live panel was
handed to the maintainer to try and had no verdict at this line's writing. If
it still misbehaves: `journalctl --user -f | grep -i buzz` shows the picker's
first stderr line as `Buzz: pick-file: …`.

If the helper shows `auth_rejected` after the VM was suspended, check the clock
first (0.0.19 reports `clock_skew`; see the checkpoint entry on clock drift).

Verified live: agent `vClaude` (Claude Code) answers owner mentions and DMs;
copy, live updates, reply counts, the yellow ANSI portrait avatar, Settings,
Invite people and attachment cards (a real 47 KB row) are in daily use. No
real upload or download had been exercised at this line's writing.

Branch in progress: `model-check` (worktree `~/Projects/omarchy-buzz-model`,
Opus builder): a `probe_model` request on the agent manager so a Claude Code
agent's model name is checked before it is saved. Read its checkpoint entry and
run the suites before merging; release 0.0.20 after the merge.

Resume order: hear the Browse… verdict (fix if needed) → merge `model-check`
when green and release 0.0.20 (bump `helper/Cargo.toml`, `Cargo.lock`,
`manifest.json`; `tests/package_helper.py`; push; `gh workflow run
helper-arm64.yml --ref main`; verify → read-only relay check → `helper-install`
→ plugin update → shell restart) → presence/"Update your status" → community
distribution (x86-64 package, notices, listing). Teams, sharing and import stay
on hold. Git history is never rewritten.

## Shell restarts

Before `omarchy restart shell`, check `pgrep -fa '^/usr/bin/quickshell'`. An
orphaned second shell instance (seen September 30, left by an earlier restart)
causes "omarchy-shell is not responding", duplicate D-Bus registrations and a
SIGABRT core dump of the restarting instance; kill the instance that owns no
layers (`hyprctl layers -j` shows the owner's pid) and restart once. After a
helper upgrade a second restart is sometimes needed before the new panel code
loads ("Incompatible helper" until then).

## Next three priorities

1. **Stabilize daily messaging UX.** Observe the installed quiet-refresh behavior;
   preserve scrolling, drafts, focus and open threads through updates. Check
   reconnect/error paths and reply discoverability. Use synthetic data for
   repeatable tests; never populate the real room just to exercise UI states.
2. **Make agent setup reproducible.** Package the approved stock Codex path with
   explicit workspace selection and supervised start/stop/status. Then complete
   Claude subscription room acceptance as a separate milestone. Reuse native
   login/refresh mechanisms; do not copy provider credentials into plugin state.
3. **Prepare community distribution.** Review ARM64 and x86-64 packages, notices,
   install/update/uninstall and version compatibility; clean up stale docs and
   prepare release/listing. Downstream projects consume this same generic plugin
   through configuration or separate extensions, never a product-specific fork.

These priorities are recommendations, not authorization to expand an agent's
workspace, start arbitrary model work, invite users, or publish externally.

## Runtime locations and credential boundaries

| Location | Purpose |
| --- | --- |
| `~/.config/omarchy-buzz/config.toml` | Public relay and human identity metadata; read this for the current relay rather than hard-coding operator infrastructure |
| `$XDG_RUNTIME_DIR/omarchy-buzz/control.sock` | Local bounded helper IPC; sanitized presentation data only |
| `~/.local/state/omarchy-buzz/delivery/ledger.json` | Durable send deduplication state; preserve across upgrades/uninstall |
| `~/.local/share/omarchy-buzz/stock-agent-codex` | Installed stock agent bundle and launcher copies |
| `~/.local/state/omarchy-buzz-agent-preview/codex` | Dedicated provider profile; native Codex owns its credentials and refresh |
| `~/.local/state/omarchy-buzz-room-workspaces/codex` | Only the explicitly selected task workspace; mounted as `/workspace` |
| `~/.local/state/omarchy-buzz-room-workspaces/codex-identity.json` | Public agent identity/room metadata |
| `~/.config/systemd/user/omarchy-buzz-codex.service` | Operator-specific, separately supervised room agent |

Human and dedicated agent Buzz signing identities are in Linux Secret Service.
The launcher uses the agent's `omarchy-buzz.room-agent.v1` service/account lookup;
the helper uses `omarchy-buzz.identity.v1`. Inspect the existing implementation
for access; do not dump keyring values, auth files, environments or raw agent logs.
QML must never own keys, provider tokens, capability credentials or authority.

The approved stock runtime automatically permits tools **inside its filesystem
sandbox**. It has shared network access, its dedicated provider profile and its
own Buzz signing key so the stock CLI can publish replies. It does not mount
normal HOME, desktop D-Bus/display or another project's checkout. Room/owner filters are
routing controls, not cryptographic restrictions on everything that key can sign.
The service has 2 GiB/128-task bounds, 180-second turn/60-second idle limits and
core dumps disabled. Raw stdout/stderr are suppressed; absence of journal text
is not proof that no task ran. Read ROOM_AGENTS.md before changing this boundary.

## Verification and deployment commands

Start read-only from `~/Projects/omarchy-buzz`:

```sh
git status --short
git log -5 --oneline
git -C ~/.config/omarchy/plugins/community.buzz rev-parse HEAD
~/.local/bin/omarchy-buzz --version
sha256sum ~/.local/bin/omarchy-buzz
systemctl --user show omarchy-buzz.service omarchy-buzz.socket omarchy-buzz-codex.service -p Id -p ActiveState -p SubState -p UnitFileState
```

Focused UI checks, already passed for the recent changes:

```sh
scripts/preview --thread-replies
scripts/preview --catalog-refresh
scripts/preview --catalog-refresh-send
scripts/preview --mentions
scripts/preview --thread-send
scripts/preview --author-names
scripts/preview --send-bridge
```

The preview uses the pinned sibling Omarchy checkout. Offscreen window-mask and
sandbox-denied test IPC warnings can be expected; inspect the actual PASS/error
and process result. Real helper socket tests require socket access. A failed
local helper build recently lacked `pkg-config`; do not weaken dependencies to
hide that. The existing manual ARM64 workflow passed with native prerequisites.
For Rust changes use locked tests and the relevant manual CI workflow; avoid
triggering the entire workflow matrix for a UI-only fix.

After testing a UI change and ensuring the installed checkout has no user edits:

```sh
omarchy plugin update community.buzz --yes
omarchy restart shell
omarchy-shell shell summon community.buzz '{"mode":"window"}'
```

Use the Omarchy skill for desktop configuration/install work. Updating/rescanning
alone has previously left old compiled QML running; verify the rendered window
after reload. Reload disrupts the shell and can discard in-memory drafts, so
minimize it. The installed UI checkout currently follows the local source repo;
check its origin before assuming a public fetch/deploy relationship.

Helper upgrades are separate. Follow [service/README.md](../service/README.md),
verify the artifact source and hash, run installer `--dry-run`, then install.
The installer preserves config, identities and ledger, and automatically restores
prior files if activation fails. Latest upgrade backup:
`~/.local/share/omarchy-buzz/backups/20260930T003325.465724Z`.
There is no public `rollback` subcommand: review backup contents and compatibility
before deliberate restoration/repackaging. Do not downgrade below 0.0.8 after
thread sends. For UI rollback, revert the specific source change in a new commit
and deploy it normally; do not reset unrelated work or delete the installed tree.

## Upstream and collaboration expectations

- Keep Buzz/Omarchy upstream unmodified in deployed dependencies. The authorized
  contribution-only [Buzz draft PR #7976](https://github.com/block/buzz/pull/7976)
  concerns WebSocket resource limits; it is not an adopted production dependency.
  Staged ACP patches/experimental bundles are not the running stock agent.
- Track upstream changes and inspect candidate interfaces before upgrading.
  A moving `main`, app version string or green unrelated workflow is insufficient.
- Subscription authentication is a product requirement. Native ChatGPT login was
  verified; do not claim a specific subscription tier or billing audit from that.
- Work autonomously on authorized engineering. Ask only for material product,
  authority or scope decisions. Do not ask the user to run routine commands the
  agent can execute. Respect the execution environment's approval requirements;
  do not promise a bypass mode.
- The user permits selective inexpensive subagents. Delegate bounded independent
  tasks with explicit file ownership; do not spawn several agents just to reread
  the same history. Check current tool/model availability and budget guidance.
- The user dislikes repeated GitHub failure emails. Keep automatic workflows
  disabled as currently configured; use focused manual runs and inspect failures.
  Account-wide notification settings were not changed.
- Do not reopen settled decisions: generic community plugin; no product forks;
  hosted or self-hosted relays; ordinary Buzz permissions for first-release room
  publication; approved stock Codex isolation described above. Optional stronger
  future authority/signing architecture is not an implicit blocker to this path.

At the next handoff, update this baseline, exact deployed evidence, unresolved
issues and next priorities. Leave a clean, reviewable commit history and identify
any work that is saved locally but not pushed or installed.
