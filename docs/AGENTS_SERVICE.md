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

Frames from the service: `{"version":1,"type":"hello"|"status","id":…,"instanceId":…,"capabilities":["agent_manager"],"status":{…}}` with

```
status: {
  harnesses: [{id:"claude-code"|"codex", bundle:"ready"|"missing", signedIn:true|false|null}],
  agents: [{id,name,description,instructions,harness,model,acpCommand,rooms,respondTo,workspace,
            identity|null, enrolled:bool, unit:"active"|"inactive"|"failed"|"unknown",
            startAtLogin:bool, published:bool, lastError:string|null}],
  pending: {requestId,type,state:"working"|"done"|"failed",category:string|null} | null
}
```

Requests (each carries `version:1`, a UUID `id`, `instanceId`): `subscribe`,
`create_agent {persona fields}`, `update_agent {id, fields}` (stops a running
agent first only when `harness`, `workspace`, `rooms` or `respondTo` change; other
edits republish and take effect on next start), `delete_agent {id}` (stops,
disables, removes the unit, keeps the identity in Secret Service unless
`forget: true`), `enroll_agent {id}` (generate identity, sign NIP-OA
attestation with the owner key, publish kind 30175 persona and kind 30177
managed-agent record, add the agent to each room as the owner does in Desktop),
`start_agent {id}`, `stop_agent {id}`, `set_start_at_login {id, enabled}`,
`sign_in {harness}` (opens a terminal running the vendor CLI login against the
shared harness profile). Errors are `{"type":"error","id",…,"category"}` with
fixed categories (`agent_invalid`, `agent_busy`, `agent_limit`, `harness_missing`,
`not_signed_in`, `enroll_failed`, `unit_failed`, `workspace_refused`,
`relay_unavailable`). At most 16 personas. One mutating request at a time.

## Units

`~/.config/systemd/user/omarchy-buzz-agent-<id>.service`, generated from the
template in `service/agent.service.in` (same limits as the existing Codex unit:
`MemoryMax=2G`, `TasksMax=128`, `LimitCORE=0`, `KillMode=control-group`,
`Restart=no`, no stdout/stderr), `ExecStart` = the bundle's `room-agent` with
`--profile`, `--workspace`, `--bundle`, `--relay`, `--room` (repeated) ,
`--owner`, `--identity`, `--respond-to`, and `--instructions <file>` where the
file is written by the service into the agent's workspace-adjacent private
directory. Status is read with `systemctl --user show`.
