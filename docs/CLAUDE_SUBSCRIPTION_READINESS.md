# Claude ACP subscription-only readiness

Reviewed 2026-09-28 against `claude-agent-acp` 0.82.0 source at
[`18de37624071b48e95aed9ec5382823e2d72cd39`](https://github.com/agentclientprotocol/claude-agent-acp/tree/18de37624071b48e95aed9ec5382823e2d72cd39),
which pins `@anthropic-ai/claude-agent-sdk` 0.3.280 in
[`package.json`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/package.json#L1-L72).
This is a source review, not a login, model call, installed-native test, or proof
of Claude subscription entitlement or effective billing. See the broader
[authentication contract](AGENT_AUTH_COMPATIBILITY.md) and
[adapter assessment](CLAUDE_ADAPTER_SOURCE.md).

## What the stock adapter provides

With local terminal capability, `initialize` advertises `claude-ai-login`
(`--cli auth login --claudeai`) separately from `console-login`
(`--cli auth login --console`, API usage billing). In a remote environment it
offers the less specific `claude-login` TUI method instead. These are ACP
**terminal** methods: the client runs the advertised adapter command and then
reinitializes; ACP `authenticate` handles only legacy gateway methods
([`initialize`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2381-L2509),
[`authenticate`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2622-L2664)).
The SDK session and terminal wrapper use the bundled Claude executable unless
`CLAUDE_CODE_EXECUTABLE` overrides it
([`claudeCliPath`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L1716-L1761),
[`createSession`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8690-L8702)).
Pin and inspect that native artifact; a separate `claude` on PATH does not
identify what ACP will run.

There is no subscription-only selector for a normal session. The stock
`--hide-claude-auth` flag enforces the **opposite** policy, refusing subscription
use where detected. Its source documents a credential-change detection window;
it cannot be repurposed as a subscription guard
([`hide-claude-auth.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/hide-claude-auth.ts#L1-L105)).

## Required boundary before a subscription turn

1. Store the user's subscription intent outside ACP prompt metadata. Permit only
   the exact pinned adapter/native executable and an explicitly selected
   `claude-ai-login` terminal flow for local login. Do not treat a Console or
   gateway login as equivalent. Remote `claude-login` is ambiguous until the
   native outcome can be checked.
2. Launch with a controlled child environment and known configuration roots.
   Reject inherited API/token and alternate-route inputs, including
   `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`,
   `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK`,
   `CLAUDE_CODE_USE_VERTEX`, and related endpoint/header/cloud variables.
   Control `CLAUDE_CODE_EXECUTABLE`. Inspect or isolate user, project, and local
   settings, including `apiKeyHelper` and `env`; a saved subscription can
   coexist with a higher-precedence API key
   ([`createSession`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8602-L8665),
   [`auth-status.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/auth-status.ts#L210-L249)).
3. At the ACP boundary, reject provider mutation (`providers/set` and gateway
   `authenticate`) and untrusted session `_meta.claudeCode.options` that can
   change `env`, `settings`, `settingSources`, or native startup arguments. The
   adapter accepts provider overrides without a client prerequisite and
   recreates loaded queries for subsequent turns; its session environment
   merges process env, client options, then provider env
   ([`providers/set`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2877-L2980),
   [`createSession`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8531-L8538),
   [`query options`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L8602-L8703),
   [`provider update`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L9098-L9145)).
   Check the policy again on session creation, load/resume, and before every
   prompt. A clean launch alone cannot fence later changes.

The adapter's `_auth/status_update` is useful identity information, **not** an
effective-payer gate. Its CLI probe is asynchronous and can be absent or stale;
the adapter intentionally keeps reporting its own login after `providers/set`
reroutes traffic. `kind: account` therefore cannot certify that a particular
turn used subscription billing
([`auth-status.ts`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/auth-status.ts#L1-L47),
[`publishSessionAccountIdentity`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2698-L2737),
[`probeCliAuthStatus`](https://github.com/agentclientprotocol/claude-agent-acp/blob/18de37624071b48e95aed9ec5382823e2d72cd39/src/acp-agent.ts#L2750-L2827)).
Thus stock adapter status plus launch filtering cannot establish a fail-closed
subscription-only guarantee. That claim needs an authoritative check of the
effective credential and route at each turn boundary, or an upstream native
enforcement feature, followed by authorized account testing.

Without an account, a bounded fake ACP peer can verify the client selects the
terminal method, rejects Console/gateway/provider/session overrides, and
preserves a clean launch environment across new/load/resume/prompt requests.
Isolated adapter initialization and synthetic settings can check advertised
methods and status mapping. Neither test proves real OAuth reuse, entitlement,
credential precedence inside the native CLI, or who pays for a model turn.
