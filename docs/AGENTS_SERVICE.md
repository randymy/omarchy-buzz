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
  --instructions <file> [--model <name>]
```

`--harness` and `--model` are additions to the Units list above. Defaults
(`--harness codex`, `--respond-to owner-only`, no instructions, no model, one
room) render the same `buzz-acp` command as the September 29 Codex agent.
Every check fails with exit 2 and a category on stderr before Secret Service
is read: `separate_agent_and_owner_required`, `room_count_invalid`,
`duplicate_room`, `canonical_room_required`, `model_invalid` (not
`[A-Za-z0-9._:-]{1,64}`; an empty value means the harness default),
`bundle_harness_mismatch`, the existing `room-sandbox` path categories, and
`instructions_*` / `provider_settings_review_required` below.

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
otherwise). It prints `launched` and does not wait. The terminal runs
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
