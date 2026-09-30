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
start. `delete_agent` stops, disables, removes the unit, keeps the identity in
Secret Service unless `forget: true`. `enroll_agent` generates the identity,
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
  --respond-to <owner-only|mentions> [--instructions <file>] [--model <name>]
```

`mentions` is passed literally (the launcher maps it to upstream `anyone`).
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
then, on a connection authenticated as the agent with the attestation in its
AUTH event (`auth::connect_attested`), kind 0 `{"about","display_name"}` with the
`auth` tag, as Desktop's `build_profile_event`. 30175/30177 use a monotonic
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
(started detached, never read). A script must be an unlinked regular file owned
by the user, executable and not group/other-writable; otherwise `missing`/`null`
and `sign_in` answers `harness_missing`. `start_agent` rechecks both and refuses
with `harness_missing`/`not_signed_in`.

**Other behaviour.** `start_agent` requires an enrolled, published agent,
writes the instructions and attestation files, renders and writes the unit
(0600), runs `daemon-reload`, `enable` when `startAtLogin`, then `start`.
`set_start_at_login` on an enrolled agent installs the unit and enables or
disables it; otherwise it only records the choice. `delete_agent` keeps the
default workspace and does not publish deletions or remove relay membership.
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
- The launcher does not yet receive the attestation: it is written to
  `<agent dir>/auth-tag.json` next to the instructions file, but `ExecStart`
  has no argument for it. A relay that admits agents only through their owner
  (NIP-OA in AUTH, `BUZZ_AUTH_TAG` in `buzz-acp`) needs the bundle launcher to
  pass it (for example an `--auth-tag <file>` argument); that is a contract
  change to agree on.
- `sign_in` starts `agent-login` in the service's cgroup: the script must
  detach the terminal into its own scope, or it ends when the service exits or
  stops. Whether the service's environment carries the display variables is
  unverified.
- The relay's acceptance of kind 30175/30177/9000 over WebSocket with these
  exact contents, the owner's permission to add members to each room, and the
  kind-0 publication through an owner attestation were checked against the
  pinned source only, not a live relay.
- Room removal (kind 9001) when rooms are dropped, and deletion of relay
  records on `delete_agent`, are not implemented.
