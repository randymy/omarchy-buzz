# Claude Code and Codex authentication compatibility for Buzz ACP

Reviewed 2026-09-28. The pinned Buzz checkout is
`781d39510cf23cfe224e8f521ae06a23377e06de`. This is a source and
interface assessment, **not** an authenticated ACP run. The user's requirement
is an explicit per-agent choice between subscription sign-in and usage-based
API billing, with no automatic change of payer. See
[ACP_READINESS.md](ACP_READINESS.md#required-subscription-and-usage-based-authentication-workflow).

## Current conclusion

Both native products support subscription and API-key modes. [Official OpenAI
Docs](https://learn.chatgpt.com/docs/auth) distinguish ChatGPT sign-in for
Codex subscription access from API-key sign-in for usage-based access. [Claude
Code authentication](https://code.claude.com/docs/en/authentication) documents
subscription OAuth as the default for Pro, Max, Team and Enterprise users and
separate API credential sources. This establishes product capability, **not**
that the installed Buzz ACP path selects, preserves, or reports either mode.

The two native CLIs resolve on this host (`codex`, `claude`). Neither
`codex-acp`, `claude-agent-acp`, `claude-code-acp`, nor `buzz-acp` resolves on
the inspected PATH. No version, login status, settings, credential file,
environment value, provider request, ACP agent, or real relay was inspected or
run. Therefore neither adapter, architecture packaging, subscription reuse,
nor effective billing mode is locally verified.

| Layer | Codex | Claude Code |
| --- | --- | --- |
| Native product | [OpenAI Docs](https://learn.chatgpt.com/docs/auth) say `codex login` starts ChatGPT browser sign-in and `codex login status` reports the active method. API-key sign-in is separately billed. | [Claude Code docs](https://code.claude.com/docs/en/authentication) describe `/login` subscription OAuth, `/status` for the active credential, and separate Console/API credentials. |
| ACP adapter source | The current [codex-acp README](https://github.com/agentclientprotocol/codex-acp/blob/main/README.md#authentication) advertises ChatGPT login and API key methods through ACP `initialize`; `CODEX_API_KEY` or `OPENAI_API_KEY` is used when the API-key method is selected. It can run its bundled Codex dependency, so the standalone `codex` binary is not necessarily the one used by the adapter. | The [claude-agent-acp README](https://github.com/agentclientprotocol/claude-agent-acp) identifies a Claude Agent SDK adapter, but its inspected overview does not specify a subscription/API method contract or effective billing evidence. Native Claude login does not prove adapter behavior. |
| Buzz integration | `desktop/src-tauri/src/managed_agents/discovery/catalog.rs:86–120` selects `codex-acp`, names `codex login` and probes `codex login status`. | The same catalog at lines 51–84 selects `claude-agent-acp` (legacy `claude-code-acp`) and probes `claude auth status`. |
| Verification here | CLI path only. ACP adapter absent and no auth probe run. | CLI path only. ACP adapter absent and no auth probe run. |

The pinned `crates/buzz-acp/README.md` examples explicitly require
`OPENAI_API_KEY` and `ANTHROPIC_API_KEY`, even saying a ChatGPT subscription is
not usable. That text conflicts with the current Codex adapter's advertised
ChatGPT method, the newer Buzz catalog, and the native product documentation.
It cannot be the final authority for a subscription workflow. Conversely, an
adapter README claim is not proof that this Buzz harness completes the flow or
uses the intended payer. Pin exact adapter releases before making a support
claim.

## Buzz's current ACP authentication boundary

The pinned harness has a useful read-only `buzz-acp auth-methods --json`
subcommand (`crates/buzz-acp/src/lib.rs::run_auth_methods`): it spawns the
configured adapter, calls ACP `initialize`, prints its advertised methods and
shuts it down. It needs no Buzz relay key. Its `authenticate` subcommand calls
the adapter's ACP `authenticate(methodId)` after checking the ID was advertised.
Neither command was run here because there is no installed adapter; even
`initialize` may read a native login store or start adapter-owned processes.

There is a specific terminal-login mismatch. The harness advertises
`clientCapabilities.auth.terminal=true` and `_meta.terminal-auth=true` in
`crates/buzz-acp/src/acp.rs::build_client_capabilities`, but
`run_authenticate` does not branch on the method's `type`. The [ACP
authentication draft](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/draft/authentication.mdx)
requires a `terminal` method to run a separate interactive copy of the agent
command with the advertised args/environment, then reconnect and initialize.
It expressly forbids sending ACP `authenticate` for that method type. Current
Buzz can list such a method, but the inspected subcommand is not a compatible
terminal-login executor. Buzz currently requests an ahead-of-spec ACP protocol
version 2 in `src/acp.rs`, so pinning the final negotiated authentication
contract is also necessary. An upstream fix must implement the terminal flow
or stop advertising terminal support. Treat unknown method types as
unavailable.

The conversational launch is a different path. `crates/buzz-acp/src/config.rs`
chooses the agent command and constructs Codex `CODEX_CONFIG` with unrestricted
`sandbox_workspace_write.network_access=true`; `src/acp.rs::spawn_with_env`
inherits the parent environment, and `src/git.rs::GitEnvironment::install`
exports `BUZZ_PRIVATE_KEY` into the child. These are separate launch-security
gates documented in [ACP_READINESS.md](ACP_READINESS.md). A successful native
login would not resolve them.

## Required no-fallback contract

For each configured agent, store a non-secret intent of `subscription` or
`usage_api` outside QML's command surface. Before a turn starts, the supervisor
must identify the exact adapter artifact/version, binary path and native CLI it
launches; read its actual ACP `authMethods`; perform only the explicitly chosen
supported flow; and verify the *effective* provider account/billing mode from a
provider or adapter-supported status surface. A generic successful
`initialize`, `session/new`, `codex login status` from a different binary, or
the presence of a credential cannot substitute for that evidence.

For subscription intent, fail closed if the adapter cannot prove subscription
mode, if an inherited API credential or settings file would take precedence,
or if login/refresh/allowance fails. For usage-based intent, fail closed if the
chosen API credential is unavailable or rejected. Never retry through the
other mode, account, provider, or key without a new explicit selection. Do not
show “subscription active” from a configured preference alone. Status should
distinguish `configured`, `effective_verified`, and `unknown/unavailable`.

This matters particularly for Claude Code: its [environment reference](https://code.claude.com/docs/en/env-vars)
says `ANTHROPIC_API_KEY` takes precedence over a logged-in subscription in
non-interactive mode, and settings can rewrite the process environment. Its
[authentication guide](https://code.claude.com/docs/en/authentication)
documents additional credential precedence. For Codex, the adapter README
distinguishes ChatGPT and key methods, while OpenAI Docs state that API-key
sign-in uses usage-based billing. The harness must check what the adapter
actually selected rather than assume a clean inherited environment means a
subscription. A deliberately isolated launch environment is necessary but is
not itself evidence of billing mode.

## Reviewable conformance sequence before implementation claims

1. Pin each adapter package and transitive native CLI artifact, verify Linux
   ARM64 packaging, and record the exact hash/version. Keep source behavior
   separate from runtime results.
2. With a fake ACP peer and no provider account, test `auth-methods` parsing,
   method IDs/types, timeouts, unknown types and bounded output. Test an
   agent-owned method separately from a terminal method. A terminal method
   must launch the advertised interactive args and reinitialize; it must never
   receive ACP `authenticate`. Test cancellation and nonzero exits.
3. In isolated provider-specific workspaces, deliberately authorized by the
   account holder, test both modes per adapter. Confirm the selected mode
   before and after an ACP prompt using the adapter's actual native process and
   an authoritative status signal. Record only redacted method/category and
   exact versions, never token contents or account identifiers.
4. Test wrong-mode pressure: inherited `OPENAI_API_KEY`, `CODEX_API_KEY`,
   `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, provider settings, stale stored
   login, expired subscription, exhausted allowance, revoked key and logout.
   Every case must stop before a paid task if effective mode differs from the
   configured choice. Ensure errors cannot trigger an API-key fallback.
5. Only after the relay-key and permission gates in ACP_READINESS pass, run one
   bounded observe-only task through the pinned harness and isolated relay.
   Record separately that ACP routing, provider authentication, billing mode,
   sandbox and tool policy behaved as claimed.

No synthetic adapter-auth executable was added in this pass. The adapters are
absent locally, and a fake server could test only Buzz's ACP framing, not real
subscription reuse or billing choice. The sequence above is the acceptance
contract for the upstream authentication work and a later adapter-by-adapter
compatibility fixture.
