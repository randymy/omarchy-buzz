# Codex ACP: pinned authentication source assessment

Published artifact initialization has since passed on an isolated Linux x86_64
runner; see [runtime follow-up](VENDOR_ADAPTER_VALIDATION.md). The original
source assessment and its account/billing limits remain below.

Reviewed 2026-09-28 from public `agentclientprotocol/codex-acp` commit
[`2eebebc35441e03cd466003b40a75953b221b886`](https://github.com/agentclientprotocol/codex-acp/tree/2eebebc35441e03cd466003b40a75953b221b886).
Its source package declares version `2.0.0` and depends on `@openai/codex`
`^0.158.0` ([`package.json`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/package.json)).
This is source inspection, **not** an installed-package or account test. No
adapter, Codex CLI, account, local config, credential store, or provider call
was run or inspected. The package on npm and this host may differ from this
commit.

## ACP method selection and login

[`getCodexAuthMethods`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAuthMethod.ts#L1-L89)
always advertises `api-key`. It advertises browser `chat-gpt` unless
`NO_BROWSER` is set, device-code `chat-gpt-device-code` when the client supports
URL elicitation, and `gateway` only when the client opts in to that capability.
These methods omit `type`, so the [ACP authentication draft](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/draft/authentication.mdx)
defaults them to `agent`. Unlike Claude Agent ACP's terminal methods, the
ChatGPT and API-key methods **do** use ACP `authenticate`.

[`CodexAcpClient.authenticate`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L175-L296)
switches on the exact method ID. `api-key` supplies an explicitly provided
`_meta["api-key"].apiKey`, otherwise `CODEX_API_KEY`, then `OPENAI_API_KEY`, to
the native app-server's `account/login/start` as `type: "apiKey"`. Missing keys
error rather than choosing ChatGPT. `chat-gpt` reads the app-server account
first; if already signed in with ChatGPT, it returns success without a new
login. Otherwise it starts `type: "chatgpt"`, opens the returned browser URL,
and waits for `account/login/completed`. Device code starts
`type: "chatgptDeviceCode"` and requires ACP URL elicitation; declining it
cancels the login. A gateway method sets a separate model route. A successful
ACP authentication is then followed by an auth-state refresh in
[`CodexAcpServer.authenticate`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1039-L1055).

Official [OpenAI Docs for Codex app-server](https://learn.chatgpt.com/docs/app-server#auth-endpoints)
describe `account/read`, `account/updated.authMode`, and the distinct `apikey`
and `chatgpt` modes. [Official Codex CLI docs](https://learn.chatgpt.com/docs/developer-commands#codex-login)
also describe `codex login status` as reporting the active mode. Those are
Codex product surfaces; this assessment does not claim that a different local
CLI's status proves the adapter process's state.

The adapter has an optional `DEFAULT_AUTH_REQUEST` environment setting, used
when Codex reports authentication required
([`src/index.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/index.ts#L77-L101),
[`checkAuthorization`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L535-L550)).
A no-fallback integration must pin or exclude that setting: otherwise it is
another adapter-selected authentication request outside the user's explicit
per-agent choice. The adapter's method-selection code alone does not enforce
the requested subscription-versus-usage policy.

## Bundled native Codex and effective-mode evidence

By default the adapter starts the `@openai/codex` package's bundled
`bin/codex.js app-server`; `CODEX_PATH` replaces it
([`CodexJsonRpcConnection.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexJsonRpcConnection.ts#L15-L32)).
Its [lockfile](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/package-lock.json#L925-L995)
pins the package to `0.158.0` and lists an optional Linux ARM64 native package.
Source-level packaging exists, but the actual npm artifact, native binary,
runtime compatibility, and credential store used here are unverified. An
unrelated `codex` executable on PATH need not be this adapter's child.

The adapter maps app-server `account/read` and `account/updated` into a
push-only `_auth/status_update` with `account`, `api_key`, `gateway`, `external`,
or `none` ([`AuthStatusMeta.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/AuthStatusMeta.ts#L1-L210)).
It schedules its first account read **after** `initialize`, and later receives
account-change pushes
([`CodexAcpServer.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1249-L1368)).
Missing/unreadable state means no status push, not verified sign-out. The
extension intentionally reports the **agent-owned login**, while ignoring
client-driven `providers/set` routing; Codex's own configured model provider
can also select a gateway. A recent `kind: account` therefore cannot alone
prove that the next session or turn uses ChatGPT rather than an override
([`readAgentAuthIdentity`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1281-L1330),
[`getAgentConfiguredModelProvider`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L507-L535)).
The existing per-session auth state reads `account/read` for OpenAI routes but
treats other providers as configured without that account
([`getAuthStateForProvider`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L790-L808)).
This is useful evidence, not a per-turn payer guarantee or a no-fallback gate.

One source-level diagnostic concern matters before exercising credentials:
when `APP_SERVER_LOGS` is set, the adapter logs raw app-server input/output,
including the request that can carry an API key, and logs startup auth request
data ([`CodexJsonRpcConnection.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexJsonRpcConnection.ts#L35-L49),
[`src/index.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/index.ts#L83-L96),
[`Logger.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/Logger.ts#L7-L46)).
Keep this log option disabled for any auth test or integration until upstream
redacts those fields. No secret-bearing diagnostics were generated here.

## Subscription-only enforcement candidate

A further local inspection of the same pinned adapter found generated native
app-server types `src/app-server/ForcedLoginMethod.ts` (`chatgpt` or `api`),
`src/app-server/v2/Config.ts::forced_login_method`, and
`src/app-server/v2/ConfigRequirements.ts::allowedLoginMethods`. The adapter reads
`CODEX_CONFIG` in `src/index.ts` and passes its config into `thread/start` through
`CodexAcpClient.ts::createSessionConfig`. It does not itself validate effective
login restrictions; no non-generated use of those enforcement fields was found.

This is a candidate for enforcing subscription intent, **not yet a verified
control**. Inspect the matching native Codex 0.158.0 implementation to establish
whether startup versus thread config accepts the restriction, its precedence,
and its behavior with a stored API-key login, provider overrides or an expired
subscription. Test rejection before a paid task. Do not treat the field's
presence, successful ChatGPT login or informational auth status as proof of the
effective payer. No native account/configuration was inspected during this check.


Follow-up: the [pinned native source assessment and offline conformance](CODEX_SUBSCRIPTION_ENFORCEMENT.md)
now establish startup API-login rejection. Thread-level `CODEX_CONFIG` is too
late to set the shared native login policy. Provider/endpoint constraints and
real Pro-account verification remain separate gates.
