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
it does not log in. It validates installed package versions and native package
hashes recorded by the ARM64 runner. It is not a continuous integrity monitor
for every transitive JavaScript file. `status` reports runtime readiness and
explicitly leaves authentication `not_checked`.

Each agent gets an owner-only profile beneath
`~/.local/state/omarchy-buzz-agent-preview/`. The launcher creates separate HOME,
provider, XDG and working directories. Reused configuration trees containing
symlinks or other owners are rejected. Existing native credentials and user
project settings are not imported. The launch environment is allowlisted;
provider API keys, proxy overrides, Node options and Buzz credentials are not
forwarded. Profile locations, not tokens, pass through the Buzz auth subprocess.

## User-controlled login

Only after target validation, run `login` with the same arguments from an
interactive local terminal. This delegates to the native subscription login:
ChatGPT for Codex or Claude subscription login for Claude. No API-login choice
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
