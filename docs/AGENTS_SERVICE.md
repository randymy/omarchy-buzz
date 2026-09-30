# Agent manager — decision and interface

Decided by the owner on September 30, 2026: the plugin gains an **Agents**
section like Buzz Desktop's, for single agents (personas) that run on this
machine. Teams, sharing and import are on hold. Harnesses: Claude Code and
Codex, each with one shared provider login per harness (as Desktop shares the
vendor CLI login), used by every agent of that harness. Remote deployment is out
of scope. This supersedes DESIGN.md's first-release exclusion of agent
installation and identity administration for this one component.

## Components

- **Agent service** (`omarchy-buzz agents-daemon`, unit `omarchy-buzz-agents.service`
  + `omarchy-buzz-agents.socket`, socket `$XDG_RUNTIME_DIR/omarchy-buzz/agents.sock`).
  The only privileged component: it stores persona definitions, generates and
  enrolls agent identities, publishes owner-signed persona and managed-agent
  records, writes per-agent systemd user units from a fixed reviewed template,
  starts/stops/inspects them, and launches provider sign-in. It reuses the
  helper crate's config, keyring, relay auth and event code. It never accepts a
  command line, a path outside the rules below, a key or a token from IPC.
- **Panel**: an `Agents` section in the left column and an agent editor in the
  middle column. QML sends the structured requests below and renders status. It
  never holds keys or runs commands.
- **Harness bundles** under `~/.local/share/omarchy-buzz/agent-<harness>/`:
  official `buzz-acp`, the harness ACP adapter and vendor CLI, and the launch
  scripts (`room-agent`, `room-sandbox`, `room-agent-entry`), assembled by a
  reviewed script. Shared provider profile per harness under
  `~/.local/state/omarchy-buzz-agent-preview/<harness>/` (the existing Codex
  profile is `…/codex`). Each agent gets its own workspace and sandbox.

## Persona record (service store, `~/.local/state/omarchy-buzz/agents/personas.json`, mode 0600)

| field | rule |
| --- | --- |
| `id` | UUID v4 assigned by the service |
| `name` | 1–64 chars, no control or bidi characters |
| `description` | ≤ 256 chars, same sanitizing; shown on the agent's public profile |
| `instructions` | ≤ 16 KiB UTF-8 text |
| `harness` | `claude-code` or `codex` |
| `model` | ≤ 64 chars matching `[A-Za-z0-9._:-]+`, or empty for the harness default |
| `acpCommand` | `buzz-acp` only (reserved for parity) |
| `rooms` | 1–8 channel UUIDs from the helper's verified joined-room list |
| `respondTo` | `owner-only` (default) or `mentions` (any member's mention) |
| `workspace` | absolute existing directory owned by the user; default `~/.local/state/omarchy-buzz-room-workspaces/<id>`; never `$HOME` itself, never under `~/.config`, `~/.ssh`, `~/.gnupg`, `~/.local/state/omarchy-buzz*` or another agent's workspace/profile |
| `identity` | agent public key once enrolled; the private key lives only in Secret Service (`omarchy-buzz.room-agent.v1` / account = public key) |
| `startAtLogin` | boolean, maps to `systemctl --user enable` |

## IPC (line-delimited JSON, same envelope style as the helper)

Refined on September 30 with the panel and bundle work; these are the exact
shapes the service emits and accepts. The panel connects through
`omarchy-buzz agents-bridge` (stdio, like `ui-bridge`); the first frame is
`hello` (within 5 s), then the panel sends `subscribe`.

Frames from the service have exactly the keys
`{"version":1,"type":"hello"|"status","id":…,"instanceId":…,"capabilities":["agent_manager"],"status":{…}}`
(no `generation`). `id` is `null` or the UUID of the request being answered;
`instanceId` matches `[A-Za-z0-9_-]{1,128}`; `capabilities` is exactly
`["agent_manager"]`. `status` has exactly:

```
status: {
  harnesses: [{id:"claude-code"|"codex", bundle:"ready"|"missing", signedIn:true|false|null}],
  agents: [{id,name,description,instructions,harness,model,acpCommand,rooms,respondTo,workspace,
            identity|null, enrolled:bool, unit:"active"|"inactive"|"failed"|"unknown",
            startAtLogin:bool, published:bool, lastError:string|null}],
  pending: {requestId,type,state:"working"|"done"|"failed",category:string|null} | null
}
```

Each agent has exactly those 16 keys; `id` is a lowercase UUID v4; `enrolled`
is true only with a non-null `identity`; at most 16 agents. `pending.type` is
one of the eight mutating types; `category` is non-null exactly when `state` is
`failed`. Error frames have exactly `{"version":1,"type":"error","id":<request
UUID>,"instanceId":…,"category":…}`.

Requests carry `version:1`, a UUID `id`, the daemon's `instanceId` and `type`.
The target agent is `agentId` because `id` is the request UUID:

| type | other keys |
| --- | --- |
| `subscribe` | none |
| `create_agent` | `fields:{name, description, instructions, harness, model, rooms, respondTo, workspace, startAtLogin, acpCommand:"buzz-acp"}` (all present; empty `workspace` = service default) |
| `update_agent` | `agentId`, `fields` with only the changed persona fields (never `startAtLogin`) |
| `delete_agent` | `agentId`, `forget` (optional, default false) |
| `enroll_agent`, `start_agent`, `stop_agent` | `agentId` |
| `set_start_at_login` | `agentId`, `enabled` |
| `sign_in` | `harness` |

`update_agent` stops a running agent first only when `harness`, `workspace`,
`rooms` or `respondTo` change; other edits republish and take effect on next
start. A room dropped from an enrolled agent is left on the relay (the owner's
kind 9001 remove-member) during that republication. `delete_agent` removes an
enrolled agent from all its rooms, then stops, disables, removes the unit,
keeps the identity in Secret Service unless `forget: true`. `enroll_agent` generates the identity,
signs the NIP-OA attestation with the owner key, publishes kind 30175 persona
and kind 30177 managed-agent records, and adds the agent to each room as the
owner does in Desktop. `sign_in` opens a terminal running the vendor CLI login
against the shared harness profile.

The outcome of a request is either an error frame with its `id` (malformed or
refused before it runs, `agent_busy`) or `status.pending` with its `requestId`
reaching `done`/`failed`; the service then also answers with a status frame
carrying the request `id`. The panel waits up to 60 s. Errors use the fixed
categories `agent_invalid`, `agent_busy`, `agent_limit`, `harness_missing`,
`not_signed_in`, `enroll_failed`, `unit_failed`, `workspace_refused`,
`relay_unavailable`. At most 16 personas. One mutating request at a time. Rooms
are stream-room UUIDs from the helper's verified catalog.

## Units

`~/.config/systemd/user/omarchy-buzz-agent-<id>.service`, generated from the
template in `service/agent.service.in` (same limits as the existing Codex unit:
`MemoryMax=2G`, `TasksMax=128`, `LimitCORE=0`, `KillMode=control-group`,
`Restart=no`, no stdout/stderr). `ExecStart` is exactly (paths expanded, one
argv array, no shell):

```
~/.local/share/omarchy-buzz/agent-<harness>/launcher/room-agent --harness <claude-code|codex>
  --profile ~/.local/state/omarchy-buzz-agent-preview/<harness> --workspace <ws>
  --bundle ~/.local/share/omarchy-buzz/agent-<harness> --relay <ws(s)://origin>
  --room <uuid> [--room <uuid> … up to 8] --owner <hex> --identity <hex>
  --respond-to <owner-only|mentions> --auth-tag <file> [--instructions <file>]
  [--model <name>]
```

`mentions` is passed literally (the launcher maps it to upstream `anyone`).
`--auth-tag` (added September 30 with the integration fixes) is always present
for an enrolled agent: `<agent dir>/auth-tag.json`, 0600 in the same private
directory, holding the owner's NIP-OA attestation for `--identity`; the service
refuses to render the unit when that attestation is not the current owner's.
`--instructions` is omitted when the instructions are empty; the file is 0600,
at most 16 KiB, inside the service's private 0700 per-agent directory (not
under the profile, workspace or bundle). `--model` only when set. Status is
read with `systemctl --user show`.

## Implementation notes (September 30, branch `agentsvc`; not installed)

Code: `helper/src/agents_service/` (`omarchy-buzz agents-daemon [--keep-running]`,
`omarchy-buzz agents-bridge`). The daemon listens on
`$XDG_RUNTIME_DIR/omarchy-buzz/agents.sock` through the helper's own IPC code
(`ipc.rs`: private 0700 runtime directory, 0600 socket or checked socket
activation, peer uid, 64 KiB request / 1 MiB response framing, at most 8
clients, idle exit after 30 s unless a request is still running).

**What is checked.**
- Requests: unknown keys, `null` values, keys a type does not take, missing
  required keys, a non-UUID `id`/`agentId`, `version` ≠ 1 and a foreign
  `instanceId` are refused (`agent_invalid`); a frame that is not a request
  object with a UUID `id` ends the session. The bridge refuses only non-request
  frames locally and forwards everything else.
- Persona fields, on every create/update and again on every load of the store:
  `id` lowercase UUID v4; `name` 1–64 characters, not blank; `description` ≤ 256
  characters; both without control, bidi (U+061C, U+200E/F, U+202A–E,
  U+2066–9), zero-width (U+200B–D, U+2060) or BOM characters; `instructions`
  ≤ 16384 UTF-8 bytes with no control characters except newline, CR and tab;
  `harness`; `model`; `acpCommand` = `buzz-acp`; 1–8 unique canonical room
  UUIDs, all present in the helper's verified catalog (read from the helper's
  own `control.sock`: authenticated, catalog `partial`/`ready`, same relay and
  owner, stream rooms only; otherwise `relay_unavailable`); `respondTo`.
- Store: `$XDG_STATE_HOME/omarchy-buzz/agents/personas.json`, 0600 in a 0700
  directory, replaced atomically (temporary file, fsync, rename, directory
  fsync), ≤ 1 MiB, unknown keys refused, a group/other-readable or invalid file
  refused rather than repaired. Unique ids and identities, at most 16.
- Workspace: absolute and normalized (no `.`, `..`, repeated or trailing
  separators, control characters), an existing directory with no symlinked
  component, owned by the user, with no group/other permission bits (the same
  test as `scripts/room-sandbox` `private_directory`; a 0755 project directory
  is refused rather than changed). Refused when equal to, inside or containing:
  `$HOME` (or any ancestor), `~/.config`/`$XDG_CONFIG_HOME`, `~/.ssh`,
  `~/.gnupg`, `~/.local/share/omarchy-buzz` (bundles), either harness profile,
  another agent's workspace; and anything under `~/.local/state/omarchy-buzz*`
  (and `$XDG_STATE_HOME/omarchy-buzz*`) except this agent's own default
  `…/omarchy-buzz-room-workspaces/<id>`. The default is created at 0700; its
  existing parent is never changed but must be an unlinked user directory that
  others cannot write. Workspaces are rechecked before every start.
- Unit files: every word is double-quoted with `\`, `"`, `%` and `$` escaped,
  control characters refused; relay, owner and agent keys, room UUIDs and paths
  are revalidated when rendering. Unit control uses only
  `/usr/bin/systemctl --user start|stop|enable|disable <unit>`,
  `daemon-reload` and `show --property=ActiveState <unit>`, for unit names
  `omarchy-buzz-agent-<uuid>.service` only, with a 30 s bound and no shell.
  `ActiveState` `active`/`inactive`/`failed` map directly; any other state or
  failure is `unknown`. Agents without an identity report `inactive`.

**Enrollment** (`enroll.rs`). A new identity is `nostr::Keys::generate()`; its
64-hex secret goes only to Secret Service (`keyring` crate, service
`omarchy-buzz.room-agent.v1`, user = public key, plus an `account` = public key
attribute so `secret-tool lookup service … account …` in the launcher finds it).
The owner key is read with the helper's `auth::read_keys`. The attestation is
`buzz_sdk::nip_oa::compute_auth_tag(owner, agent, "")` as Desktop does; it is
stored in the persona file and written to `<agent dir>/auth-tag.json` (0600).
On an owner-authenticated connection (`auth::connect_identity`) the service
publishes, each counted only after the relay's `OK` for its id (15 s bound):
1. kind 30175, `["d", <persona id>]`, no `shared` tag, content in NIP-AP order
   `{"display_name","system_prompt","acp_command":"buzz-acp","runtime":<harness>,
   ["model"],"respond_to":"owner-only"|"anyone",["description"]}`;
2. kind 30177, `["d", <agent pubkey>]`, content
   `{"name","persona_id","parallelism":1,"respond_to"}` (never a secret, the
   attestation, environment or runtime fields);
3. kind 9000 per room not yet acknowledged, `["h",room],["p",agent],["role","bot"]`
   (`buzz_sdk::build_add_member`, as `buzz-cli` and Desktop's
   `attachManagedAgentToChannel`), so repeated enrollment does not re-add rooms;
4. kind 9001 per room whose membership was acknowledged but which is no longer
   in `rooms`, `["h",room],["p",agent]`, empty content (`buzz_sdk::build_remove_member`,
   next to `build_add_member` in `crates/buzz-sdk/src/builders.rs`; no role
   tag). The relay (`crates/buzz-relay/src/handlers/side_effects.rs`, 9001
   branch and `handle_remove_user`) lets a channel owner or admin remove any
   member, and a plain member remove an agent it owns (`is_agent_owner`, set
   when the agent authenticated with the owner's attestation);
then, on a connection authenticated as the agent with the attestation in its
AUTH event (`auth::connect_attested`), kind 0 `{"about","display_name"}` with the
`auth` tag, as Desktop's `build_profile_event`.
`member_rooms` (service-private) holds rooms whose add was acknowledged and
whose removal was not, so a dropped room stays there until its 9001 is
acknowledged and is retried on the next publication (any republishing edit or
`enroll_agent`); taking such a room back before that neither adds nor removes
it, and an acknowledged room is never added again. At most 64 entries. 30175/30177 use a monotonic
`created_at` (`max(now, previous + 1)`). `published` is true only after all of
these; a rejection is `enroll_failed`, a missing answer, closed socket or
unreachable relay `relay_unavailable`; relay text is never reported. A retry
reuses the stored identity; if the owner identity changed, the attestation is
reissued and rooms are added again.

**Harness scripts** (through a spawner trait, argv only, 15 s bound):
`<bundle>/launcher/agent-bundle --check <harness>` (exit 0 and `ready` → ready,
else `missing`), `~/.local/share/omarchy-buzz/scripts/agent-login --status
<harness>` (`signed-in`/`signed-out`, anything else → `null`) and
`~/.local/share/omarchy-buzz/scripts/agent-login <harness>` for `sign_in`
(started without waiting, never read). `sign_in` gives the script a cleared
environment holding only `PATH`, `HOME`, `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`,
`DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `HYPRLAND_INSTANCE_SIGNATURE` and
`XDG_*` from the service's own environment. The socket-activated unit gets
those from the user manager, which the session fills through `systemctl --user
import-environment` / `dbus-update-activation-environment --systemd` (uwsm
does this on Omarchy); `omarchy-buzz-agents.service` names the ones it needs
with `PassEnvironment=` (`XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`,
`WAYLAND_DISPLAY`, `DISPLAY`, `HYPRLAND_INSTANCE_SIGNATURE`,
`XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`, `XDG_DATA_DIRS`,
`XDG_CONFIG_DIRS`; `PATH` and `HOME` are always set for user units). A script must be an unlinked regular file owned
by the user, executable and not group/other-writable; otherwise `missing`/`null`
and `sign_in` answers `harness_missing`. `start_agent` rechecks both and refuses
with `harness_missing`/`not_signed_in`.

**Other behaviour.** `start_agent` requires an enrolled, published agent,
writes the instructions and attestation files, renders and writes the unit
(0600), runs `daemon-reload`, `enable` when `startAtLogin`, then `start`.
`set_start_at_login` on an enrolled agent installs the unit and enables or
disables it; otherwise it only records the choice. `delete_agent` first
removes an enrolled agent from every room in `member_rooms` (kind 9001 per
room on one owner-authenticated connection, each counted only after its `OK`,
same categories as enrollment); if that fails, the acknowledged removals are
recorded, `lastError` is set and nothing else changes, so the delete can be
retried. Memberships attested by a previous owner identity are skipped (the
current owner has no authority over them). Only then does it stop and remove
the unit and files. It keeps the default workspace and does not publish
deletions of the 30175/30177 records.
Internal failures without a contract category (for example a store write
failure) are reported as `agent_invalid`. Unit states are refreshed on
`subscribe`, after unit operations and every 15 s while a client is connected;
harness readiness on `subscribe`, before `start_agent` and every 60 s.

`OMARCHY_BUZZ_AGENTS_FAKE_CONTROL=1` makes unit control, the keyring, the
spawner and the room source in-memory fakes (rooms from
`OMARCHY_BUZZ_AGENTS_FAKE_ROOMS`, comma-separated). It exists for
`tests/agents_smoke.py` and development only.

**Tests.** `helper/src/agents_service/*_tests.rs` and the unit tests in
`request.rs`/`rooms.rs`: every field and workspace rule, the 16 limit, store
round trip and refusals, request shapes, status/error key sets and the 1 MiB
worst case, golden unit file (`testdata/agent.service`), argv and state mapping,
fake start/stop/enable/status, enrollment against a loopback relay (exact
events and tags, no secret material, OK/rejection/closed/timeout/refused-AUTH),
`sign_in` refusals, and the socket protocol over a socket pair.
`tests/agents_smoke.py <binary>` runs the real daemon and bridge in fake mode in
private directories (use a short `TMPDIR` such as `/tmp`: Unix socket paths are
limited to 108 bytes).

**Installation steps for later (human-reviewed, not performed).**
1. Review this branch, the unit files and the bundle/panel work together; build
   and package the helper as in `service/README.md`.
2. Extend `scripts/helper-install` (separately reviewed) to install
   `omarchy-buzz-agents.service` and `omarchy-buzz-agents.socket` into
   `~/.config/systemd/user/` and to place the reviewed `agent-login` script in
   `~/.local/share/omarchy-buzz/scripts/` (owner-only writable, executable).
3. Install the harness bundles under `~/.local/share/omarchy-buzz/agent-<harness>/`
   (with `launcher/room-agent` and `launcher/agent-bundle`).
4. `systemctl --user daemon-reload` and
   `systemctl --user enable --now omarchy-buzz-agents.socket`; then enable the
   panel's Agents section.
5. Create one agent against an isolated test relay first; check the published
   events, the Secret Service item (`secret-tool search service
   omarchy-buzz.room-agent.v1` shows the account attribute, never print the
   secret), the generated unit and `systemctl --user show`.

**Unverified / left for others.**
- No real systemd, Secret Service, relay or script was used. Real
  `systemctl --user` behaviour of generated units, keyring access from the
  service unit and the `account` attribute lookup are unverified.
- `ExecStart` passes `--auth-tag <agent dir>/auth-tag.json`; the launcher
  hands it to `buzz-acp` as `BUZZ_AUTH_TAG` (see Bundles and sign-in). A real
  `buzz-acp` run with it is unverified.
- `agent-login` detaches the terminal into its own transient user scope
  (see Sign-in and status), so it outlives the service. A real
  `systemd-run --user --scope` launch from the socket-activated service, and
  whether the user manager's environment carries the display variables on a
  given login, are unverified.
- The relay's acceptance of kind 30175/30177/9000/9001 over WebSocket with these
  exact contents, the owner's permission to add members to each room, and the
  kind-0 publication through an owner attestation were checked against the
  pinned source only, not a live relay.
- Deletion of the 30175/30177 relay records on `delete_agent` is not
  implemented. A delete needs the relay and the owner key whenever the agent
  still has memberships.

## Bundles and sign-in

Implemented on branch `agentbundle` (September 30, 2026). Scripts:
`scripts/agent-bundle`, `scripts/agent-login`, `scripts/room-agent`,
`scripts/room-sandbox`, `scripts/room-claude`, `scripts/room-claude-acp`.

### Launch argv

The unit's `ExecStart` is exactly (one `--room` per room, 1–8, canonical
lowercase UUIDs, no duplicates):

```
<bundle>/launcher/room-agent --harness claude-code|codex
  --profile ~/.local/state/omarchy-buzz-agent-preview/<harness>
  --workspace <workspace> --bundle ~/.local/share/omarchy-buzz/agent-<harness>
  --relay <ws(s)://origin> --room <uuid> [--room <uuid> …]
  --owner <hex> --identity <hex> --respond-to owner-only|mentions
  [--auth-tag <file>] [--instructions <file>] [--model <name>]
```

`--harness` and `--model` are additions to the Units list above. Defaults
(`--harness codex`, `--respond-to owner-only`, no instructions, no model, one
room) render the same `buzz-acp` command as the September 29 Codex agent.
Every check fails with exit 2 and a category on stderr before Secret Service
is read: `separate_agent_and_owner_required`, `room_count_invalid`,
`duplicate_room`, `canonical_room_required`, `model_invalid` (not
`[A-Za-z0-9._:-]{1,64}`; an empty value means the harness default),
`bundle_harness_mismatch`, the existing `room-sandbox` path categories, and
`instructions_*`, `auth_tag_*` / `provider_settings_review_required` below.

Inside the sandbox `buzz-acp` runs with `--agent-command codex-acp` or
`claude-agent-acp` (both names are upstream standard adapters,
`crates/buzz-acp/src/acp.rs`), `--channels <rooms joined by commas>`,
`--subscribe mentions` and `--respond-to owner-only` or, for `mentions`,
`--respond-to anyone` (upstream values: `owner-only|allowlist|anyone|nobody`,
`crates/buzz-acp/src/config.rs`). Limits, permission mode and the memfd key
handoff are unchanged.

- **Instructions.** The file must be absolute, link-free, a regular 0600 file
  owned by the user, at most 16 KiB of UTF-8, in a 0700 user-owned directory,
  and not inside the profile, workspace or bundle. It is bound read-only at
  `/run/agent/instructions` and passed as `buzz-acp --system-prompt-file`. That
  is the file form of the `system_prompt` argument Desktop sets through
  `BUZZ_ACP_SYSTEM_PROMPT` (`desktop/src-tauri/src/managed_agents/runtime.rs`);
  it keeps the text out of process arguments.
- **Owner attestation.** `--auth-tag <file>` passes the same file checks as
  the instructions (categories `absolute_auth_tag_file_required`,
  `linked_auth_tag_path`, `auth_tag_path_overlap`, `auth_tag_file_missing`,
  `auth_tag_file_permissions_unsafe`, `auth_tag_file_too_large`, and the
  `private_directory` ones for its directory) with a 4 KiB limit; the file is
  opened once with `O_NOFOLLOW` and checked through that descriptor. Its
  content must be the NIP-OA tag `["auth","<owner hex>","<conditions>","<sig
  hex>"]`: exactly four strings, 64 and 128 lowercase hex characters,
  conditions empty or `&`-joined `kind=`/`created_at<`/`created_at>` clauses in
  canonical decimal (`buzz-sdk` `nip_oa.rs` `parse_auth_tag_fields` and
  `validate_conditions`), otherwise `auth_tag_file_invalid`; the owner must be
  `--owner` (`auth_tag_owner_mismatch`). The launcher does not verify the
  signature (`buzz-acp` does, and the service did when it wrote the file). The
  tag is re-encoded as compact JSON, exactly as `compute_auth_tag` emits it, and
  reaches `buzz-acp` as `BUZZ_AUTH_TAG` only through the memfd options on fd 3
  (`--setenv BUZZ_AUTH_TAG <json>` after `--setenv BUZZ_PRIVATE_KEY <key>`),
  never as a process argument or a mount. `buzz-acp` reads that variable to
  resolve its owner (`crates/buzz-acp/src/lib.rs` `resolve_agent_owner`), to
  put the tag in its relay AUTH (`HarnessRelay::connect`) and for its REST
  client (`run_task.rs`), and forwards it to its MCP tools.
- **Model.** Codex: `buzz-acp --model <name>` (the argument behind Desktop's
  `BUZZ_ACP_MODEL`). Claude Code: `ANTHROPIC_MODEL=<name>` in the sandbox
  environment and no `--model`, following Desktop's single startup model
  authority for Claude (`managed_agents/claude_config/mod.rs`).

### Bundle layout

`agent-bundle <harness> [--output DIR]` builds a new directory (default
`~/.local/share/omarchy-buzz/agent-<harness>`) and refuses to replace an
existing one. It stages next to the output and renames at the end.

| Path | Codex | Claude Code |
| --- | --- | --- |
| `bin/buzz-acp`, `bin/buzz`, `bin/buzz-admin` | official Buzz `781d395`, run 36639519388 hashes, or a CI `build.json` at that revision with no patches | same |
| `bin/node` | Node v22.23.3 ARM64 (version checked) | same |
| `bin/room-agent-entry` | unchanged | unchanged |
| `bin/<adapter command>` | `codex-acp` ← `room-codex-acp` | `claude-agent-acp` ← `room-claude-acp` |
| `bin/<cli wrapper>` | `codex` ← `room-codex` (`-c forced_login_method=chatgpt`) | `claude` ← `room-claude` (`--settings claude/subscription-settings.json`) |
| `adapter/` | `@agentclientprotocol/codex-acp` 2.0.0, `@openai/codex` 0.158.0 and its linux-arm64 native binary (hash pinned) | `@agentclientprotocol/claude-agent-acp` 0.82.0, `@anthropic-ai/claude-agent-sdk` 0.3.280, without the SDK's native CLI packages |
| `claude/claude`, `claude/subscription-settings.json` | — | Claude Code CLI 2.1.280 (SHA-256 `92f2b4fd…45a2`) and `{"forceLoginMethod": "claudeai"}` |
| `launcher/` | `room-agent`, `room-sandbox`, `agent-login`, `agent-bundle` | same |
| `licenses/`, `bundle.json` | plugin license (and Buzz's from a CI artifact) | same |

The adapter tree comes from `--adapter DIR` (a prepared npm tree; Codex
defaults to the installed stock bundle's `adapter/`) or, only with the opt-in
`--npm-ci --npm-cli FILE`, a locked `npm ci --ignore-scripts` of
`packaging/agent-<harness>/package-lock.json`. Those locks are dependency
subsets of the reviewed `tests/vendor-adapters/package-lock.json`. Either
way, pinned package versions and npm integrity values are checked against
`node_modules/.package-lock.json`, and links are refused.

Everything the agent runs is **copied**, nothing is bound from outside the
bundle. The Claude CLI is copied from the mise install (default
`~/.local/share/mise/installs/claude/latest/claude`, resolved) and must match
the pinned hash, which is byte-identical to the SDK 0.3.280 native CLI. Binding
the mise path would let `mise upgrade`/`prune` change or delete the binary
under a running agent and would add a host path to the sandbox. The CLI's
updater is off (`DISABLE_AUTOUPDATER=1`); the bundle is read-only inside.
Updating any pin is a reviewed source change followed by a new bundle.

`bundle.json` records `harness`, plugin revision, Buzz revision and
provenance, `versions`, `entrypoints`, and `files` (SHA-256, size and mode of
every file). `agent-bundle <harness> --check [--output DIR]` prints `ready`
(exit 0) or `missing` (exit 1, `{"error": category}` on stderr:
`bundle_missing`, `bundle_manifest_invalid`, `bundle_harness_mismatch`,
`bundle_file_missing`, `bundle_hash_mismatch`, `bundle_unexpected_file`,
`bundle_link_refused`); unknown harness exits 2 with `harness_unknown`. It
hashes the whole bundle (about 0.5 s warm for 600 MB). The service calls
`<bundle>/launcher/agent-bundle <harness> --check` for `harnesses[].bundle`
(a missing directory is `missing` without calling it). The manifest is
self-recorded: it detects changes, not a forged bundle.

### Subscription login

Codex is unchanged: the native CLI always runs with
`-c forced_login_method=chatgpt`. Claude Code: the adapter spawns the CLI named
by `CLAUDE_CODE_EXECUTABLE=/opt/agent/bin/claude`, which adds flag settings
`{"forceLoginMethod": "claudeai"}` to every invocation. An offline run in the
sandbox reported `"forcedLoginMethod": "claudeai"` and `configDirectory:
/profile/provider`. `forceLoginMethod` alone does not stop an API key in the
environment (see [CLAUDE_SUBSCRIPTION_READINESS.md](CLAUDE_SUBSCRIPTION_READINESS.md)),
so the other half is the environment: `bwrap --clearenv` passes no
`ANTHROPIC_*`, `CLAUDE_CODE_*` token, cloud-provider or proxy variable. Before
launch, `room-agent` refuses a profile `provider/settings.json` that is a link
or sets `apiKeyHelper`, `env`, `awsAuthRefresh`, `awsCredentialExport`,
`gcpAuthRefresh`, `modelOverrides` or a `forceLoginMethod` other than
`claudeai` (`provider_settings_review_required`). The stock adapter still
advertises its Console method; nothing in the service path invokes it, and the
forced setting is intended to make the CLI refuse it (not tested). This is not
a per-turn billing proof.

### Shared profiles

`~/.local/state/omarchy-buzz-agent-preview/<harness>/` with the 0700
`home`, `provider`, `config`, `data`, `cache`, `state` directories
`room-sandbox` requires. Inside the sandbox `HOME=/profile/home`,
`CODEX_HOME=/profile/provider` and, for Claude Code,
`CLAUDE_CONFIG_DIR=/profile/provider`. The existing Codex profile is reused
as is. The earlier `…/claude` preview profile is not used.

### Sign-in and status

`agent-login <harness>` validates the harness (`harness_unknown`, exit 2),
requires the bundle's native CLI (`harness_missing`), creates the profile
directories (`profile_unsafe` if they are links or not 0700), and opens a
terminal: `omarchy-launch-floating-terminal-with-presentation` from `PATH` or
`~/.local/share/omarchy/bin`, else `xdg-terminal-exec` (`terminal_unavailable`
otherwise). The terminal argv is run as `systemd-run --user --scope --collect
--quiet -- <terminal argv>` when `systemd-run` is on `PATH` (its own transient
user scope, outside the agent service's cgroup, so it survives the service's
idle exit or stop), else as `setsid -f <terminal argv>`. The Omarchy wrapper
itself execs `setsid uwsm-app -- xdg-terminal-exec …`, which reaches an app
scope only through uwsm's app daemon, and `xdg-terminal-exec` alone never
leaves the caller's cgroup, so the scope is always added. `--dry-run` prints
the final argv. It prints `launched` and does not wait. The terminal runs
`agent-login --in-terminal`, which requires a TTY and execs, outside any
sandbox, `codex -c forced_login_method="chatgpt" login` or `claude --settings
<subscription-settings.json> auth login --claudeai`, with `HOME`, `XDG_*_HOME`
and `CODEX_HOME`/`CLAUDE_CONFIG_DIR` in the shared profile, plus only
`DISPLAY`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`
and `TERM` from the caller. Vendor output stays in that terminal. The browser
opened for sign-in sees the profile's `HOME`, as with the earlier preview.

`agent-login --status <harness>` prints one word and exits 0. It never opens
the credential file; it uses `lstat` on `provider/auth.json` (Codex) or
`provider/.credentials.json` (Claude Code). A regular, user-owned, non-empty
file is `signed-in`; a missing profile, provider directory or file, or an empty
file, is `signed-out`; a link, another owner, a non-file or an unreadable path
is `unknown`. The service maps these to `true`/`false`/`null`. Presence does
not prove the login is valid, unexpired or a subscription: an expired or
revoked token, or a Codex API-key `auth.json`, still reads `signed-in` (the
forced login method stops the latter from being used).
