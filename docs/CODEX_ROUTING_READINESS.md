# Codex ACP subscription routing readiness

An uninstalled [adapter policy proposal](upstream/CODEX_SUBSCRIPTION_POLICY.md)
now implements the narrower controlled-profile approach below. Remote run
[36508544990](https://github.com/randymy/omarchy-buzz/actions/runs/36508544990)
passed its build, typecheck and 42 focused tests. This does not change the
stock-adapter findings or certify a real Pro subscription. The installed Buzz
plugin has not enabled this proposed agent launch mode.

Runtime follow-through: [manual run 36506274738](https://github.com/randymy/omarchy-buzz/actions/runs/36506274738)
passed against actual adapter 2.0.0 and a scripted native app-server peer in a
non-root network namespace with no external interface. It observed plain
`thread/start`, accepted ACP `providers/set`, and then observed
`modelProvider: custom-gateway` with the supplied synthetic URL in native
thread configuration. The peer rejected thread creation, so no prompt or model
turn occurred. This proves the adapter mutation path, not native routing or
subscription entitlement. [Sanitized evidence](evidence/codex-routing-36506274738.json).
The probe and seven synthetic boundary tests are in `tests/codex_routing_probe.py`
and `tests/test_codex_routing_probe.py`.

Reviewed 2026-09-28 against
[`codex-acp` `2eebebc35441e03cd466003b40a75953b221b886`](https://github.com/agentclientprotocol/codex-acp/tree/2eebebc35441e03cd466003b40a75953b221b886)
and its locked native Codex 0.158.0 source,
[`openai/codex` `54e1bd264b4122fe9471ee7d54c4d021a76bb8ff`](https://github.com/openai/codex/tree/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff).
See [the startup login assessment](CODEX_SUBSCRIPTION_ENFORCEMENT.md) for the
account-free API-key rejection result. No account, credential store, model turn,
or provider endpoint was used for this routing assessment.

## Required route and source behavior

The subscription-only route requires active ChatGPT authentication, native
provider ID `openai`, and the built-in provider with no inference URL override.
For ChatGPT auth the pinned native provider chooses
`https://chatgpt.com/backend-api/codex`; for non-ChatGPT auth it chooses
`https://api.openai.com/v1`. A configured provider `base_url` overrides either
default ([`model-provider-info/src/lib.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/model-provider-info/src/lib.rs#L415-L432)).
Native config can build the OpenAI provider from `openai_base_url` and choose a
provider from managed requirements, a per-thread override, or ordinary config
([`core/src/config/mod.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/core/src/config/mod.rs#L3795-L3816)).
Custom providers can take a key from `env_key`, a configured bearer token, or
require no OpenAI auth
([`model-provider-info/src/lib.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/model-provider-info/src/lib.rs#L135-L189)).
The ChatGPT-only login policy does not prohibit these routes.

The adapter reads `MODEL_PROVIDER` and `CODEX_CONFIG` from its environment
([`index.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/index.ts#L77-L103)).
It sends `modelProvider` and merged `CODEX_CONFIG` to native `thread/start`, and
passes corresponding overrides on resume, load, and fork
([`CodexAcpClient.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L587-L720)).
Native app-server appends thread request config after startup CLI overrides
([`config_manager.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/app-server/src/config_manager.rs#L429-L460));
do not assume startup `-c model_provider=...` alone pins a later thread.

ACP `providers/set` for the OpenAI slot creates a `custom-gateway` provider
with caller-supplied URL and headers; ACP `authenticate` with method `gateway`
uses the same route
([`CodexAcpClient.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L365-L463),
[`providers.test.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/__tests__/CodexACPAgent/providers.test.ts#L145-L195)).
When sessions exist, `providers/set` restarts native Codex and resumes them on
the new route
([`CodexAcpServer.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1091-L1166)).
A route check only at initial session creation is insufficient.

## Implementable integration constraints

At launch, use verified pinned artifacts, a dedicated controlled `CODEX_HOME`
and project configuration, and a child environment assembled from an allowlist.
Remove inherited `MODEL_PROVIDER`, `CODEX_CONFIG`, `DEFAULT_AUTH_REQUEST`,
`CODEX_PATH`, API/provider keys, and route-related URL overrides unless the
integration sets reviewed values itself. Apply `forced_login_method="chatgpt"`
at **native startup**. Ensure the effective project config has no custom
provider selection, `openai_base_url`, or provider URL override. The present
Buzz spawn inherits ambient variables unless explicitly replaced or removed
([`AcpClient::spawn_with_policy`](https://github.com/block/buzz/blob/781d39510cf23cfe224e8f521ae06a23377e06de/crates/buzz-acp/src/acp.rs#L512-L608));
the generic base plugin has no subscription route allowlist today.

Before each prompt, a subscription-specific ACP boundary should reject
`providers/set`, gateway `authenticate`, unexpected provider/config changes,
and sessions whose effective route cannot be established. Recheck after
session resume, fork, replacement, and any provider update. Buzz's current
runtime client does not send ACP `providers/set`, but the stock adapter exposes
it; this absence is a constraint of the present client, not an adapter policy
([`AcpClient` requests](https://github.com/block/buzz/blob/781d39510cf23cfe224e8f521ae06a23377e06de/crates/buzz-acp/src/acp.rs#L675-L804)).

**Blocker:** stock ACP `session/new` returns session ID, models, modes, and
config options, but omits native `response.modelProvider`
([`CodexAcpServer.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1016-L1037),
[`CodexAcpClient.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L699-L720)).
It also omits the effective native provider URL. `providers/list` supplies a
fallback display URL of `https://api.openai.com/v1`, even though ChatGPT native
requests use the ChatGPT backend; `_auth/status_update` deliberately ignores
client-driven provider routing
([`CodexAcpClient.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts#L428-L443),
[`CodexAcpServer.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpServer.ts#L1281-L1330)).
Neither surface proves the effective payer. A strict per-prompt route guarantee
therefore needs a reviewed ACP/native boundary that exposes and validates the
effective thread provider and URL, or a narrower locked-down deployment whose
configuration and provider mutation paths are independently constrained.
The stock adapter alone cannot attest this guarantee without such a boundary.

## Smallest account-free conformance check

Use the pinned adapter with a scripted native app-server peer and no real
credentials. Before any prompt, capture the native `thread/start` request for
a plain session and for a session after ACP `providers/set`; assert that the
latter has `modelProvider: "custom-gateway"` and the supplied URL. The pinned
adapter's own [`providers.test.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/__tests__/CodexACPAgent/providers.test.ts#L145-L195)
already checks this behavior with mocks. A host-side fixture should then assert
that the subscription boundary rejects the mutation and emits no
`session/prompt`. This proves fail-closed routing behavior for the tested path,
not a real Pro entitlement or billed request.
