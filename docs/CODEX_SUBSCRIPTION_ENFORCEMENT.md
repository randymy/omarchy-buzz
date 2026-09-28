# Codex ACP subscription-only enforcement assessment

Reviewed 2026-09-28. The adapter is
[`agentclientprotocol/codex-acp` `2eebebc35441e03cd466003b40a75953b221b886`](https://github.com/agentclientprotocol/codex-acp/tree/2eebebc35441e03cd466003b40a75953b221b886),
version 2.0.0, whose lockfile pins `@openai/codex` 0.158.0. The matching native
source is [`openai/codex` `54e1bd264b4122fe9471ee7d54c4d021a76bb8ff`](https://github.com/openai/codex/tree/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff)
(`rust-v0.158.0`). This note is source inspection and an account-free test plan,
not proof of a Pro account, active entitlement, or billing route.

[OpenAI's authentication documentation](https://learn.chatgpt.com/docs/auth)
distinguishes ChatGPT subscription access from usage-billed API-key access and
documents `forced_login_method = "chatgpt"`. The adapter itself still advertises
and accepts an ACP `api-key` method unless the native app-server rejects it
([adapter methods](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAuthMethod.ts),
[adapter authenticate](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts)).

## Where the login restriction must be applied

The adapter reads `CODEX_CONFIG` at startup but passes it as a **thread config**
when creating or resuming sessions
([`index.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/index.ts),
[`CodexAcpClient.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexAcpClient.ts)).
Native app-server creates its shared `AuthManager` from **startup config** before
any thread exists
([`app-server/src/lib.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/app-server/src/lib.rs#L514-L578)).
Thread config is loaded separately
([`thread_processor.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/app-server/src/request_processors/thread_processor.rs#L1321-L1340)).
Therefore `CODEX_CONFIG` alone is not an API-login enforcement control. Use an
explicit startup `config.toml` in an isolated `CODEX_HOME`, or a reviewed launch
path that supplies native `-c forced_login_method=chatgpt` before `app-server`.
The stock adapter launches bundled `codex.js app-server`, or a `CODEX_PATH`
executable plus `app-server`, without injecting that flag
([`CodexJsonRpcConnection.ts`](https://github.com/agentclientprotocol/codex-acp/blob/2eebebc35441e03cd466003b40a75953b221b886/src/CodexJsonRpcConnection.ts#L15-L32)).
The native CLI forwards root `-c` overrides to app-server
([`cli/src/main.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/cli/src/main.rs#L1219-L1270)).

When the shared startup policy is ChatGPT-only, native `account/login/start`
rejects an API key with `API key login is disabled. Use ChatGPT login instead.`
([`account_processor.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/app-server/src/request_processors/account_processor.rs#L423-L440)).
The auth manager also filters disallowed stored credentials instead of selecting
them as active auth
([`login/src/auth/manager.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/login/src/auth/manager.rs#L1184-L1256)).
The app-server builds that manager with `enable_codex_api_key_env = false`, so
`CODEX_API_KEY` or `OPENAI_API_KEY` cannot silently replace its OpenAI login.
The adapter's explicit ACP `api-key` method can read those variables and request
API login, which the startup policy must reject. Do not infer that app-server
logs out and exits: the separate `enforce_login_restrictions` helper is not
called in the inspected app-server startup path.

## Account-free conformance check

On disposable CI, use the actual pinned native artifact and an empty temporary
`CODEX_HOME` containing startup `config.toml` with
`forced_login_method = "chatgpt"`. Initialize app-server and send
`account/login/start` with `type: "apiKey"` and a synthetic key. Assert the
specific rejection above and no accepted API login. Do not send a model turn.
The native repository already tests this exact denial with a temporary home
([`app-server/tests/suite/auth.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/app-server/tests/suite/auth.rs#L565-L590)).
An additional adapter test can verify that a thread `CODEX_CONFIG` value cannot
loosen the startup login policy. Keep `APP_SERVER_LOGS` disabled because the
adapter can log raw API-login requests. No account or paid request is needed for
these denial checks.

## Routing boundary and remaining proof

`forced_login_method` constrains the OpenAI login method; it does not pin a
thread to the official OpenAI provider. Native thread startup accepts
`model_provider`, and its provider selection can take a thread override over
ordinary config
([`core/src/config/mod.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/core/src/config/mod.rs#L3795-L3816)).
Custom providers can use `env_key` or require no OpenAI authentication
([`model-provider-info/src/lib.rs`](https://github.com/openai/codex/blob/54e1bd264b4122fe9471ee7d54c4d021a76bb8ff/codex-rs/model-provider-info/src/lib.rs#L135-L189));
`openai_base_url` can also change the built-in provider endpoint. The adapter
offers `MODEL_PROVIDER`, `CODEX_CONFIG`, gateway authentication, and ACP
`providers/set` routes. A subscription-only integration must constrain these
routes, inspect the effective provider and endpoint before turns, and fail
closed on unknown or changed routing. An informational `account/read` or
`_auth/status_update` alone cannot prove the next turn's payer.

The account-free check can establish API-login rejection and routing guard
behavior. Only a separately authorized real-account check could establish
ChatGPT login, Pro entitlement, and the effective billing path; none was done
for this assessment.
