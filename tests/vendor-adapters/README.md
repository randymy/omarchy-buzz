# Disposable adapter discovery artifacts

This private npm package is a test fixture, not a plugin dependency. Run `npm ci
--ignore-scripts --omit=dev --no-audit --no-fund` only on a disposable runner.
The lock pins artifacts and transitive/native dependencies by npm integrity;
there are no account credentials or production installations in this fixture.

Reviewed registry metadata on 2026-09-28:

| Package | Version | Registry gitHead / reviewed source |
| --- | --- | --- |
| @agentclientprotocol/codex-acp | 2.0.0 | 2eebebc35441e03cd466003b40a75953b221b886 |
| @agentclientprotocol/claude-agent-acp | 0.82.0 | 18de37624071b48e95aed9ec5382823e2d72cd39 |
| @openai/codex | 0.158.0 | Native dependency pinned separately |
| @anthropic-ai/claude-agent-sdk | 0.3.280 | Native dependency pinned separately |

Registry gitHead links the published version to the reviewed source; it is not
an independent reproducible-build attestation. Exact artifact URLs and SHA-512
integrity values are in package-lock.json.

The manual ACP integration workflow optionally installs these packages, then
runs discovery as a non-root user in separate network and PID namespaces, with
no network interfaces except loopback and empty per-case profiles. Only ACP
initialization/auth-method discovery is requested. No authentication, session,
prompt, relay, or account interaction is requested. Discovered method IDs/types
and sanitized failure categories are the only saved adapter output. Timeout,
output bounds, and namespace teardown limit hung processes. This is network
isolation, not a filesystem sandbox: adapters can read the disposable runner
workspace. No saved account is provisioned there. The clean environment expects
Claude's local subscription and Console choices, not its remote-only login TUI.

Linux x86_64 discovery does not establish ARM64 runtime support, successful
subscription login, effective billing mode, or safe agent execution. Updates
require a new source review, lock update and explicit manual conformance run;
this fixture never upgrades installed agents automatically.
