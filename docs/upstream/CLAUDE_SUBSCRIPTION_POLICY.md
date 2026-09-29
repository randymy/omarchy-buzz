# Proposed Claude ACP subscription-only launch policy

Source basis: `@agentclientprotocol/claude-agent-acp` 0.82.0 at
`18de37624071b48e95aed9ec5382823e2d72cd39` (local checkout
`/tmp/claude-agent-acp-source-review`), which pins Claude Agent SDK 0.3.280.
The [adapter patch](claude-subscription-policy.patch) is an **uninstalled
upstream proposal** against that exact revision, not a native CLI patch, login
result, model run, or billing attestation. No account or local Claude settings
were read. The patch adds `--require-claude-subscription` and synthetic tests;
the ordinary adapter mode is unchanged. Build and tests still need to run on
the patched checkout.
The synthetic tests currently cover policy predicates and direct method denials;
fake SDK tests for successful creation, lost account at prompt, and cleanup
after admission failure remain an acceptance gate.
The adapter's separate informational `auth status` probe and `auth logout`
subprocesses still inherit the parent environment; their scoped-mode child
environment needs review before deployment. Neither admits a model turn.

## Narrow objective

Add an explicit adapter flag such as `--require-claude-subscription`. A launch
with this flag must use the native first-party Claude account route, with a
current subscription identity and without a known API-key, token, gateway, or
cloud-provider override. Missing evidence refuses the session/turn. It does
not promise that a vendor plan has no overage or other charge, or identify a
per-turn payer from a receipt; those are outside the SDK/ACP evidence here.
The flag is mutually exclusive with the existing `--hide-claude-auth`, whose
policy is the inverse and whose unreadable-account path fails open
(`src/hide-claude-auth.ts`, `src/acp-agent.ts:8850-8874`).

The smallest adapter change is a policy branch around existing boundaries,
not a new ACP protocol or a replacement native CLI:

1. In `src/acp-agent.ts::initialize`, advertise only the local
   `claude-ai-login` terminal method (`--cli auth login --claudeai`). Omit
   `console-login`, remote ambiguous `claude-login`, gateway methods and
   provider capability in this mode. If terminal auth is unavailable, report
   unavailable rather than offering a different login. The existing terminal
   descriptor's `_meta` copies `process.argv` into `--cli` arguments; strip the
   new policy flag from native `--cli` forwarding in `src/index.ts` (or from
   those descriptor args), so subscription login remains executable. Login
   method selection is a UX gate, not proof of the later session route.
2. Reject `authenticate` gateway, `providers/set`, and `providers/disable`
   before mutation. The first two currently install process-wide provider
   state (`src/acp-agent.ts:2622-2664,2877-2980`); provider changes recreate
   queries on later turns (`src/acp-agent.ts:9098-9145`). Check that no
   provider/gateway override is already active at every create/resume/load and
   prompt boundary. Do not silently ignore a mutation request.
3. In `createSession`, reject client `_meta.claudeCode.options` fields that can
   alter auth/routing or the native launch: at least `env`, `settings`,
   `settingSources`, `extraArgs`, and `pathToClaudeCodeExecutable`. Resolve
   the effective settings and launch environment *after* managed policy env
   has been applied (`src/index.ts:46-50`, `src/managed-policy.ts`) and before
   `query()` spawns the native process. Fail on unreadable settings in policy
   mode: `SettingsManager.loadAllSettings` normally logs the error and
   substitutes `{}` (`src/settings.ts:91-106`). Use an isolated/explicit
   settings source set for this launch; reject `apiKeyHelper`, credential or
   alternate-route `env`, and provider settings in the effective managed and
   admitted settings. Reject nonempty known routing/credential variables in
   the actual SDK env, including `ANTHROPIC_API_KEY`,
   `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` or its FD variant,
   `ANTHROPIC_BASE_URL`, custom headers, and Bedrock/Vertex switches. The
   existing provider-cache variable list at `src/acp-agent.ts:10625-10652`
   is a useful starting inventory, not a complete allowlist. Pin the bundled
   native executable or an explicitly verified equivalent; the adapter's
   `CLAUDE_CODE_EXECUTABLE` override otherwise changes the binary
   (`src/acp-agent.ts:1716-1761,8690-8702`). An external launcher should
   supply the clean environment and controlled config roots; the adapter must
   validate rather than assume that launch contract.
4. Use the SDK's supported `initializationResult.account` before registering
   every new/resumed/recreated session (`src/acp-agent.ts:8825-8874`) and
   `query.accountInfo()` before admitting each prompt
   (`src/hide-claude-auth.ts:192-294`, `src/acp-agent.ts:3030-3045`). Accept
   only `apiProvider: firstParty`, nonempty `subscriptionType`, absent
   `tokenSource`, and an explicitly allowed no-key `apiKeySource` such as
   absent/`none`. Reject *unknown* `apiKeySource` values as well as the known
   `ANTHROPIC_API_KEY`, `apiKeyHelper`, and `/login managed key` values. Do not
   invert `billsClaudeSubscription()` without tightening its negative
   API-key test: it currently returns true for an unrecognized key source.
   Reject missing `initializationResult.account`, missing/throwing
   `query.accountInfo`, and any mismatch. Existing `_auth/status_update` and
   `claude auth status --json` are asynchronous, connection-scoped display
   information; they are not the admission check (`src/auth-status.ts:1-47`,
   `src/acp-agent.ts:2698-2827`).

The first patch refuses load/resume/fork and automatic query recreation, so it
checks fresh creation and each prompt rather than importing an existing query.
It refuses client MCP servers, additional roots, and all
`_meta.claudeCode.options`; uses SDK `settingSources: []` for managed-only
settings; and validates both resolved settings and the exact child environment.
The normal adapter's `settings` and `env` assembly
merges process, client, and provider inputs and enables user/project/local
setting sources (`src/acp-agent.ts:8531-8703`); a launch-only check is
insufficient. The native SDK account read is the strongest supported local
signal found here, but its account is cached at query initialization. The
existing guard documents that an external credential change may be noticed by
an asynchronous CLI probe only after a turn boundary
(`src/hide-claude-auth.ts:1-39`). Thus this proposal gives a fail-closed
**admission policy for known inputs and observed account state**. It cannot
prove that arbitrary out-of-band credential-store changes or native CLI
precedence changes are impossible during a live query. A strict guarantee
against that residual case needs a supported native fresh-effective-credential
check or query recreation before every turn, with the resulting behavior
validated against the pinned native binary. No `forceLoginMethod` setting or
equivalent guarantee was found in the pinned adapter source; no unpacked SDK
package was available locally during that source review. A subsequent native
status probe in [run 36506274738](https://github.com/randymy/omarchy-buzz/actions/runs/36506274738)
reports the synthetic API key as active even with `forceLoginMethod: claudeai`
in the disposable home settings. That status-only result does not test a model
turn, but confirms that setting is not a substitute for validating credentials
and route inputs. [Evidence](../evidence/claude-subscription-36506274738.json).

## Offline acceptance fixtures

- Pure account table: first-party subscription with absent/`none` key source
  passes; absent account, absent plan, key helper, env/API key, managed Console
  key, bearer/OAuth token, external provider, gateway, and unknown key source
  all refuse before a prompt is released. Simulate `accountInfo()` absent and
  throwing; both refuse. Use `src/tests/authorization.test.ts`'s existing fake
  query/account fixtures and extend its session-creation and per-turn cases.
- Fake ACP client: initialize advertises only the local subscription terminal
  method; Console, ambiguous remote login, gateway and provider mutation are
  rejected. Exercise `session/new`, `load`, `resume`, fork and prompt; denied
  requests leave no live query/turn. Test `--cli` argument forwarding with the
  policy flag so the login command receives only native arguments.
- Fake SDK query inspection: reject ambient and managed-policy credential/env
  values, unreadable or changed settings, and `_meta` override attempts;
  confirm the accepted query receives a fixed executable and no alternate
  route. A later attempted `providers/set` or gateway `authenticate` must not
  alter the next prompt. These tests make the adapter policy reviewable
  without a real account, provider call, or cost claim.

No billing receipt is required to satisfy the proposed *route-selection*
product requirement. An authorized account smoke test is still needed before
claiming the native binary actually accepts the subscription login and refuses
an API-key fallback in its current release.
