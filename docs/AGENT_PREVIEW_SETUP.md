# Subscription authentication preview

This is an opt-in development tool, not an enabled room-agent service. It uses
unsubmitted Buzz/adapter contributions. Account-free tests do not establish a
Pro entitlement, successful model turn, effective billing, or safe unattended
agent execution. The normal messaging plugin remains independent.

## Bundle and profiles

The manual ARM64 workflow builds the exact pinned harness and adapters and
checks them without accounts. The archive includes Node/npm, code, provenance,
licenses and dependency notices. It omits both vendor native runtimes; setup
retrieves their original locked npm packages with lifecycle scripts disabled.
No provider account is copied from an existing Codex or Claude installation.

Before extracting an artifact, verify its workflow repository, full source
commit, successful checks and archive digest. Record `manifestSha256` from that
verified workflow's checksum report outside the extracted bundle. The launcher's
file hashes detect changed files; a self-authored manifest is not proof of origin.
Only use artifacts produced by a trusted workflow and source review.

From a checked-out copy of this repository, with an absolute extracted bundle
path and its independently verified manifest digest:

```bash
scripts/agent-preview prepare --agent codex --bundle /absolute/bundle \
  --manifest-sha256 VERIFIED_MANIFEST_DIGEST
scripts/agent-preview hydrate --agent codex --bundle /absolute/bundle \
  --manifest-sha256 VERIFIED_MANIFEST_DIGEST
scripts/agent-preview auth-methods --agent codex --bundle /absolute/bundle \
  --manifest-sha256 VERIFIED_MANIFEST_DIGEST
```

Use `--agent claude` for Claude. Hydration performs a locked upstream download;
setup requires at least 1 GiB free for Codex or 2 GiB for Claude on both the
bundle and profile filesystems. These are conservative working-space floors,
not promises about future package sizes. Low space is rejected before npm can
replace an existing runtime. Free space or move the bundle before retrying.
Hydration does not log in. It validates installed package versions and native package
hashes recorded by the ARM64 runner. It is not a continuous integrity monitor
for every transitive JavaScript file. `status` reports runtime readiness and
explicitly leaves authentication `not_checked`.

Each agent gets an owner-only profile beneath
`~/.local/state/omarchy-buzz-agent-preview/`. The launcher creates separate HOME,
provider, XDG and working directories. Reused configuration trees containing
unexpected symlinks or other owners are rejected. Browser-owned contents under the real, current-user directories `home/.cache`
and `config/chromium` are excluded from recursive link inspection; their roots
must not be symlinks. These directories are not sandboxed. The provider-tree link exception is
Codex's four known temporary command shims under `provider/tmp/arg0/`: each must
point directly to the exact native binary whose digest matches the verified
bundle. Configuration and credential symlinks remain rejected. Existing native credentials and user
project settings are not imported. The launch environment is allowlisted;
provider API keys, proxy overrides, Node options and Buzz credentials are not
forwarded. Profile locations, not tokens, pass through the Buzz auth subprocess.

## Reuse an existing native login

The preview CLI accepts `--profile-mode existing` for each action. It references
`~/.codex` or `~/.claude` by default, or an absolute `--provider-directory` for a
custom native profile. Selection is explicit per invocation, not saved in QML.
The default remains `--profile-mode separate`, preserving the Buzz profile.

```bash
scripts/agent-preview auth-status --agent codex --profile-mode existing \
  --bundle /absolute/bundle --manifest-sha256 VERIFIED_MANIFEST_DIGEST
```

Use `--agent claude` for Claude. No credential file is copied or parsed by the
launcher. Native tools own credential retrieval, refresh and sign-in. Provider
paths and known configuration/credential paths are checked for unexpected links,
ownership and write permissions. Native status output is bounded and reduced to
an authentication category; account details and raw output are not exposed.
Status does not certify a Pro tier, billing or a successful model turn.

Private HOME/work/cache directories and the environment allowlist remain in use;
only CODEX_HOME/CLAUDE_CONFIG_DIR references the existing profile. Claude state
stored directly in the original HOME may therefore be unavailable. Existing
Claude settings with hooks, environment/provider overrides or credential helper
commands require review before native status/discovery/login; the launcher does
not execute those configured commands to decide whether they are safe. It never
rewrites existing settings to make them pass. Managed Claude settings are checked
as well. For `auth-status`, ordinary user hooks are permitted because the verified
native status command uses `--setting-sources "" --settings '{"disableAllHooks":true}'`.
An account-free fixture verifies that configured hooks/helper commands do not
run on that path. Managed hooks and provider/helper overrides still require
review. Guarded `auth-methods` discovery also permits user hooks after a separate
isolated synthetic-hook probe passed. Login with existing hooks remains outside that validation. A separate
network-isolated session-start test observed no ordinary user SessionStart hook
execution before the guarded adapter rejected an account-free session. This
does not isolate hook behavior in an admitted session; source inspection confirms
the guarded SDK uses `settingSources: []` to omit user settings, and
managed hooks remain blocked. If a check fails, retain the separate profile pending review.

Reusing a provider directory shares its settings and native credential lifecycle;
it is not isolation from the other agent processes using it. The preview still
has no room-agent launch action. Existing-profile status/discovery is not agent
execution approval; guarded subscription admission remains necessary for every
future session/task.

## User-controlled login

Only after target validation, run `login` with the same arguments from an
interactive local terminal. This delegates to the native subscription login:
ChatGPT for Codex or Claude subscription login for Claude. Codex executes the
verified native binary with `-c forced_login_method="chatgpt" login`, using the
same dedicated CODEX_HOME as the adapter. This avoids the Buzz transport's
60-second generic RPC timeout during interactive ACP authentication. Claude
continues using the terminal-auth handoff. The Buzz timeout proposal now passes compiled unit tests and a real 65-second
authentication probe in ARM64 run 36582892134. That newer bundle has not replaced
the installed preview; the launcher does not weaken subscription admission or
enable API login.
See [official authentication guidance](https://developers.openai.com/codex/auth/). No API-login choice
is exposed by this preview. Native output stays in the terminal; the plugin
never receives it, credentials or callback data. The tool does not report a
verified Pro account merely because login exits successfully.

Native credential storage remains vendor-owned. A dedicated profile and filtered
environment are not isolation from malicious processes running as the same user.
Session/display handles may reach desktop services. Do not grant untrusted tools
write access to these profiles or the installed bundle. Live credential changes
remain subject to the documented adapter/native limitations.

## Removal and remaining gates

No systemd agent service, autostart, room subscription, agent identity or relay
membership is created. Removing the extracted bundle does not delete native
login credentials. Retain the profiles by default; use the vendor's account
revocation/logout procedure before intentionally deleting account data. Removing
the QML plugin also does not remove these separately created profiles.

Real room agents additionally require tested launch isolation, verified native
subscription admission, permission behavior, controlled agent identity and room
membership, and the proposed membership-bound relay publication endpoint. The
installed relay does not implement that endpoint. Current upstream Buzz at
`12670bd0f037c66a682272bb81c46c3f254fad74` was checked on September 28, 2026 and
still lacks it. Do not enable a legacy publication fallback or deploy a relay
fork to conceal this dependency. See [ACP readiness](ACP_READINESS.md).

## Session admission evidence

The dedicated Codex profile passed a live no-prompt `session/new` check through
its guarded adapter on September 29. The native effective configuration first
passed routing and integration preflight; API-key authentication was rejected.
The probe denied any client-side permissions/tools and then terminated the
adapter process group. No prompt or relay operation was sent. Native empty-session
metadata may remain in the selected provider profile; this test is not read-only.
It does not verify model billing, agent tool execution or relay publication.

`tests/session_admission_probe.py` records only booleans/categories, bounds all
output, and never exposes session IDs or provider output. Its dedicated-profile
default avoids the normal user's plugins/hooks. It rejects configured Codex
MCP/hooks/plugins/projects/skills. Claude admission rejects managed hooks,
provider overrides, configured MCP servers and installed plugins; ordinary user
SessionStart hooks are excluded by the guarded adapter's inspected
`settingSources: []` configuration. The account-free hook test does not independently
prove this for an admitted session. Do not disable these checks merely to
make an existing profile pass. Existing-profile discovery and auth-status remain
available independently.

## Live subscription smoke tests (September 29, 2026)

Both guarded adapters created a session, rejected API-key authentication and
returned the exact requested fixed response in a real model turn. Codex used
the separate signed-in ChatGPT profile; Claude used the existing native
subscription profile. The probe used an empty private workspace and requested
no tools. No ACP client permission request arrived, no Buzz room was involved,
and no room-agent service was enabled.

The optional `--prompt-smoke` flag on `tests/session_admission_probe.py` makes
this one model request explicitly; default execution remains a no-prompt
admission check. Outputs contain booleans/categories only. Evidence is recorded
in `evidence/codex-subscription-smoke-2026-09-29.json` and
`evidence/claude-subscription-smoke-2026-09-29.json`. These checks establish
successful guarded subscription routing and model responses for the tested
profiles/versions. They do not independently certify a plan tier, inspect a
billing receipt, audit every native tool action or prove room-agent operation.
