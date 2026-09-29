# Proposed Codex ACP subscription route policy

This is an **uninstalled upstream proposal** against
[`codex-acp` 2.0.0 at `2eebebc35441e03cd466003b40a75953b221b886`](https://github.com/agentclientprotocol/codex-acp/tree/2eebebc35441e03cd466003b40a75953b221b886),
whose bundled native Codex is
[`0.158.0` at `54e1bd264b4122fe9471ee7d54c4d021a76bb8ff`](https://github.com/openai/codex/tree/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff).
Apply [`codex-subscription-policy.patch`](codex-subscription-policy.patch) to that exact adapter revision. The normal adapter path remains unchanged unless launched with `--require-chatgpt-subscription`.
The patch also documents the flag in upstream `readme-dev.md`.

The opt-in mode requires an explicit absolute `CODEX_HOME` supplied by a launcher that owns a **dedicated clean profile**. It rejects ambient adapter provider/auth overrides and API keys, starts the bundled native app-server with `-c forced_login_method=chatgpt`, and passes native Codex only a short environment allowlist. It disables adapter file logging in this mode because the pinned logger records raw native JSON-RPC. The startup restriction is carried through the adapter's native restart path.

On fresh `session/new`, the policy reads native `config/read({cwd})`, requires ChatGPT-only login configuration, and rejects provider selection, custom provider definitions, and known URL/token overrides. The pinned native default `chatgpt_base_url` is accepted only when absent or exactly `https://chatgpt.com/backend-api/`; other URLs, aliases, and query strings are denied. It also requires native `account/read` to report an existing ChatGPT account. The adapter then explicitly sends `modelProvider: "openai"` on `thread/start` and checks that native's returned provider is `openai`. It does not inject the normal adapter's `projects.*.trust_level=trusted` overrides. Client-supplied MCP servers and additional directories are denied in this first version. It repeats config and account checks before each ACP prompt. API-key and gateway authentication, ACP provider mutation, resume/load/fork, and steering/goal extensions are denied and omitted from advertised capabilities/auth methods; `session/new` carrying `sessionId` is denied because this adapter otherwise treats it as a resume. No real account or model turn was used to develop or validate this proposal.

The guard uses supported native `config/read(cwd)` and `account/read` shapes, plus the adapter's existing `thread/start` response. `config/read` describes configuration before thread request overrides, so the proposal also controls the adapter's outgoing overrides and checks native's returned provider. These checks are not atomic with external file or policy changes; the launcher must control the profile and project config for the entire session. The guard does **not** prove a specific effective URL, ChatGPT Pro entitlement, billing route, or the behavior of preexisting native threads. A controlled launcher must make the dedicated profile and artifact pin real, and must not allow arbitrary code or clients to access the native app-server. Auth through the real ChatGPT flow is a separate user action; this patch does not perform it.

Review and CI validation: from a clean checkout of the exact adapter revision, apply the patch, install locked dependencies, run `npm run typecheck`, `npm run build`, and `npx vitest run src/__tests__/SubscriptionPolicy.test.ts src/__tests__/CodexACPAgent/providers.test.ts src/__tests__/CodexACPAgent/auth-status.test.ts`. A bounded account-free native `config/read` check is useful to verify that a clean profile plus startup override produces the expected fields; neither this check nor mock tests establish entitlement. Remote run [36508544990](https://github.com/randymy/omarchy-buzz/actions/runs/36508544990) passed typecheck, build, policy module smoke and 42 focused adapter tests (three files) at plugin revision `c2e2307`. Tests ran without an external network interface and without real accounts or model turns. Native default configuration compatibility and compiled ACP denials subsequently passed in run `36509294886`; real subscription acceptance remains a separate gate.

Dependency-free module validation passed locally on Node 26.9.0 using
`node tests/codex_policy_smoke.mjs /path/to/patched/codex-acp` from this plugin
repository. It executes the actual TypeScript policy module with types stripped,
using synthetic native responses. It checks routing overrides, environment
filtering, unknown sessions, account loss/change between prompts, and sanitized
failure responses. It does not exercise ACP dispatch, native processes, adapter
restarts, or a real subscription. The manual workflow has now passed; see the remote evidence above.

Runtime follow-through: [run 36509294886](https://github.com/randymy/omarchy-buzz/actions/runs/36509294886)
passed against the actual pinned native binary and compiled patched adapter.
The native default includes exactly `https://chatgpt.com/backend-api/`, which
the initial guard rejected; the corrected guard admits that exact value and
continues to reject other URLs. The compiled adapter advertises scoped login
options and rejects API authentication, provider mutation, and load/resume/fork
requests. No model turn or real account was used. Evidence:
[native config](../evidence/codex-policy-native-36509294886.json),
[compiled ACP](../evidence/codex-policy-adapter-36509294886.json).
