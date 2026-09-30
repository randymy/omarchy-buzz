# Isolated Codex room agent

On September 29, a real owner mention reached unmodified Buzz ACP and Codex,
using the existing separate ChatGPT login. Codex read a harmless workspace file
and published the exact expected reply in the same Buzz thread as its dedicated
identity. The installed Omarchy helper verified the reply. See
[evidence](evidence/stock-codex-room-2026-09-29.json).

## Current operator setup

The separately supervised `omarchy-buzz-codex.service` is running manually.
It is not enabled at login. In the configured room, type `@codex` in the composer and press Tab/Enter or click
**Codex (isolated)** in the suggestions to attach its exact public-key mention. Only the configured owner's messages trigger it.

The writable task workspace is
`~/.local/state/omarchy-buzz-room-workspaces/codex`, visible inside as `/workspace`.
The normal home directory and another project's checkout are not mounted. No desktop D-Bus,
display socket or provider API environment variable is passed into the sandbox.
The dedicated existing ChatGPT profile is mounted; credentials are not copied
into QML or the task workspace. Native Codex owns login and token refresh.

```sh
systemctl --user status omarchy-buzz-codex.service
systemctl --user stop omarchy-buzz-codex.service
systemctl --user start omarchy-buzz-codex.service
```

This is a manually configured operator preview, not yet a community agent setup
wizard. `scripts/room-sandbox` constructs the filesystem view. `scripts/room-agent`
loads only the dedicated agent identity from Secret Service, passes it through
in-memory Bubblewrap options, then starts unchanged Buzz ACP. The installed bundle
contains official Buzz binaries and npm Codex ACP/native Codex packages plus thin
launch scripts. `room-codex` adds the supported `forced_login_method=chatgpt`
command-line setting. There is no automatic API-auth fallback. This does not
independently certify a plan tier or billing receipt.

## Execution and trust limits

- Stock Buzz automatically permits ACP tool requests. The outer filesystem
  sandbox provides the accepted local boundary; there is no human approval UI.
- The agent can modify its dedicated workspace and profile. Runtime files and
  bundle are read-only. This is not isolation from malicious host processes
  running as the same user or from kernel vulnerabilities.
- Network access is shared for relay and provider connectivity. This also permits
  reaching other network services; no egress allowlist is claimed.
- Stock Buzz gives the dedicated agent's signing key to its tools so the upstream
  CLI can post replies. It never receives the human or relay signing key.
  Owner/room flags filter harness input; they are not a cryptographic limitation
  on the agent identity's relay permissions. Public-room rules still apply.
- Each task has a 180-second wall limit and a 60-second idle limit. The service
  caps memory at 2 GiB and tasks at 128. Stopping it kills the whole process group
  through systemd's cgroup. Core dumps and raw stdout/stderr logging are disabled.
- Plugin messages are available; detailed agent execution states are not yet
  integrated. A profile or presence indication is not proof of current work.

Claude's subscription smoke test is separate; no Claude room service is installed.
Future setup needs reviewed packaging, explicit workspace selection and native
start/stop controls. This implementation remains generic; a downstream integration
can supply an intentionally chosen workspace later without changing the plugin.
