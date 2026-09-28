# Published ACP adapter discovery

On 2026-09-28, [manual run 36478940003](https://github.com/randymy/omarchy-buzz/actions/runs/36478940003)
passed in 2m58s at plugin commit `4b28fb659d9df5c5a3e8da1a1c868e95e02b729c`.
The complete patched Buzz ACP crate built, three targeted upstream unit tests,
nine real-client synthetic authentication tests and five discovery boundary
tests passed. All four published-adapter discovery cases passed.

This adds runtime evidence to the [Codex](CODEX_ADAPTER_SOURCE.md) and
[Claude](CLAUDE_ADAPTER_SOURCE.md) source assessments. It does not establish
successful sign-in or billing choice for a real account.

| Installed artifact | Version |
| --- | --- |
| @agentclientprotocol/codex-acp | 2.0.0 |
| @openai/codex | 0.158.0 |
| @agentclientprotocol/claude-agent-acp | 0.82.0 |
| @anthropic-ai/claude-agent-sdk | 0.3.280 |
| Node | 22.23.3 |

The exact npm artifact URLs, SHA-512 integrity and dependency graph are pinned
in [package-lock.json](../tests/vendor-adapters/package-lock.json). Registry
metadata's gitHead matches the reviewed source commits; this is not a
reproducible-build attestation. Package installation used `npm ci` with lifecycle
scripts disabled on the disposable Ubuntu 24.04 x86_64 runner.

## Observed methods

| Adapter | Default discovery | With explicit terminal capability |
| --- | --- | --- |
| Codex | `api-key` (agent), `chat-gpt` (agent) | Same two methods |
| Claude | Empty list | `claude-ai-login` (terminal), `console-login` (terminal) |

The terminal capability must not be advertised by a UI/client unable to execute
the corresponding flow. The [interactive-login proposal](upstream/ACP_INTERACTIVE_LOGIN.md)
provides that missing standalone harness path. This run initialized real
adapters but did **not** invoke either adapter's authentication method or
terminal login command. Claude's remote-only login variant was not tested.

## Isolation and evidence

Discovery ran non-root in separate network and PID namespaces after dropping
capabilities, with empty per-case HOME/XDG directories, no inherited credentials,
no production configuration and no external network interfaces. It requested
only `auth-methods`; no session, prompt, account sign-in or relay connection.
The subprocess output was bounded and reduced to method IDs/types and failure
categories. Namespace teardown cleans remaining child processes. This is not a
filesystem sandbox: adapters retain access to the disposable runner workspace.

[Sanitized evidence](evidence/vendor-adapters-36478940003.json) includes the lock
hash, package versions, observed methods, source revision and limits. No user
account, installed plugin, relay or native credential store was changed.

## Remaining checks

- Linux ARM64 runtime and native-binary packaging, beyond source/registry entries.
- Deliberately authorized subscription and API sign-in through exact adapters.
- Effective billing mode, expiry/allowance handling and refusal of fallback.
- Buzz relay-key input/child propagation, tool decisions and workspace isolation.
- Upstream acceptance of local harness changes before depending on them publicly.

Updates are explicit: inspect source changes, update the lock, then dispatch the
manual workflow with `vendor_discovery=true`. Automatic triggers remain disabled.
