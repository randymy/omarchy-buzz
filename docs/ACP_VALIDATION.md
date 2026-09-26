# Isolated ACP validation before a Codex demo

Read-only research at Buzz commit
`781d39510cf23cfe224e8f521ae06a23377e06de`. The pinned synthetic harness passed in
[run 36280557661](https://github.com/randymy/omarchy-buzz/actions/runs/36280557661)
against the actual isolated relay at plugin commit `f276997`.
[Sanitized evidence](evidence/relay-acp-36280557661.json) records both messaging
and ACP success with cleanup. This is not model-backed agent validation.
No cloud agent, provider login,
package installation, or real identity was used. A version-only check found
`codex-cli 0.155.1` on this `aarch64` Linux host. `codex-acp`, `buzz-acp`, and
`buzz-agent` were absent from the inspected PATH and helper build directory.
CLI presence does not establish adapter compatibility or account availability.

## First demo: synthetic ACP peer through the real isolated relay

**Go after the isolated relay, roster, and helper messaging checks pass.**
The smallest supported peer is the pinned fixture
`desktop/tests/e2e/fixtures/fake-acp-agent.mjs`. It implements ACP initialize,
session creation, and prompt completion without model access. For a prompt
containing `AE-ID:SYNTHETIC1`, it emits an `AE-ACK:SYNTHETIC1` assistant chunk
and, when `BUZZ_E2E_CLI_BIN` is configured, invokes that fixed Buzz CLI to post
the acknowledgment in the prompt's channel (lines 47–91). It invokes no cloud
provider or general-purpose tool command. This proves the harness/stdio/event
route; it does not prove the Codex adapter or a model works.

Build the necessary pinned tools on the prepared isolated runner:

```sh
"$BUZZ_SOURCE/bin/cargo" build --locked -p buzz-acp -p buzz-cli -p git-sign-nostr -p git-credential-nostr
```

The CLI binary is named `buzz` (`crates/buzz-cli/Cargo.toml:15`). Configure the
fixture launcher with a temporary workspace, HOME, XDG directories, private
logs, and a clean environment. PATH must include the newly built tools and
Node. Set `BUZZ_E2E_CLI_BIN` to the absolute built `buzz` path. Set
`BUZZ_RELAY_URL` to the explicitly isolated WebSocket authority and
`BUZZ_PRIVATE_KEY` to a **generated disposable agent key only**. Do not inherit
provider variables, session buses, production auth tags, cloud credentials,
user Git configuration, or the real Codex home. The environment requirement
here is acceptable only because the identity is synthetic and disposable.
Do not use the task-agent Python fixture for this relay demo: its private test
log intentionally records the generated test key.

Prepare one synthetic owner, a different synthetic agent, and a stranger;
add the owner and agent to one isolated stream room. The agent must have relay
and room membership before the harness can discover or subscribe to it.
NIP-OA ownership registration, when used, must be signed by the synthetic
owner for the exact agent key; no host identity is reused.

From the private workspace, with those prepared variables, the inspected CLI
supports this launch:

```sh
"$BUZZ_BIN_DIR/buzz-acp" \
  --agent-command node \
  --agent-args "$BUZZ_SOURCE/desktop/tests/e2e/fixtures/fake-acp-agent.mjs" \
  --agent-owner "$FIXTURE_OWNER_PUBLIC_KEY" \
  --respond-to owner-only --allowed-respond-to owner-only \
  --subscribe mentions --channels "$FIXTURE_ROOM_ID" --kinds 9 \
  --permission-mode default --mcp-command "" \
  --no-memory --no-presence --no-typing \
  --idle-timeout 10 --max-turn-duration 30
```

These arguments are now runtime-verified with the pinned synthetic peer.
Own its process group and impose an external overall deadline and cleanup.
Use the helper's exact recipient picker to send the synthetic owner's
`AE-ID:SYNTHETIC1` message mentioning the agent key. Verify the correlated ACP
prompt, assistant chunk, and signed ACK message in the same room. An owner
message without the mention and a stranger's message with the mention must not
wake it. Remove membership and verify access/routing fails closed. Record
signed event IDs and public keys, never private-key logs. Fixture acknowledgments
are synthetic outputs, not evidence of model reasoning or execution approval.

Useful existing witnesses are `crates/buzz-acp/tests/run_task.rs` and
`tests/stdio_contract.rs`; the former covers real process/stdio task boundaries,
not relay mention routing. Its prepared-test command is:

```sh
"$BUZZ_SOURCE/bin/cargo" test --locked -p buzz-acp --test run_task --test stdio_contract
```

## Routing and permission boundaries that the demo must preserve

The default is `owner-only` (`src/config.rs:476`), and an explicit
`allowed-respond-to=owner-only` restricts permitted startup policy. Owner
resolution first verifies `BUZZ_AUTH_TAG`, then falls back to the explicit
public owner key (`src/lib.rs:150`). The policy admits the owner **and agents
with verified same-owner NIP-OA sibling attribution** (`src/lib.rs:380–397`);
it is not a promise that only one human key can ever wake the harness. The
fixture community should contain no unrelated sibling agents.

Do not describe this author gate as tool approval. The conversational harness
defaults to `bypass-permissions` (`src/config.rs:460–472`). Even after explicitly
selecting `default`, `src/acp.rs:1982` auto-approves an ACP permission request
when it offers `allow_once`. `plan` and `dont-ask` are defined modes, but
`src/pool.rs:1748` applies a nondefault mode only if the adapter advertises it;
unsupported modes are skipped. They do not provide a fail-closed permission
boundary by themselves. The synthetic peer performs no permissioned model tools.

## Real Codex adapter demo: separate conditional gate

Buzz's desktop catalog names the executable `codex-acp` and npm package
`@agentclientprotocol/codex-acp` (`desktop/src-tauri/src/managed_agents/discovery/catalog.rs:86`).
Discovery requires version **1.10.0 or later** (`discovery.rs:793`); this is an
adapter version, not the installed Codex CLI version. The inspected Buzz source
does not vendor an ARM64 Linux adapter artifact or a pinned package integrity
record. ARM64 adapter packaging and its dependencies therefore remain
unverified here. Do not use the unpinned auto-installer as reproducible evidence;
select and verify a concrete adapter version/artifact before installing it.

The supported conversational launcher selects `--agent-command codex-acp`;
`normalize_agent_args` in `src/config.rs:872` normalizes the generic default
`acp` argument for standard adapters. The desktop's auth probe is `codex login
status` and its setup copy is `codex login` (catalog lines 117–119). The ACP
README's older API-key-only/fallback description disagrees with that newer
catalog behavior; it is not sufficient evidence for current adapter credential
handling. No login status or user auth file was inspected. A live demo needs
an independently verified adapter authentication route and deliberately
provisioned demo credentials; do not copy the desktop's private auth files or
place provider credentials in QML, commands, logs, or repository files.

There is also an upstream key-input limitation: conversational `buzz-acp`
accepts its relay private key via `--private-key` or `BUZZ_PRIVATE_KEY`
(`src/config.rs:250`). It has no inspected key-file, hidden-input, inherited-FD,
or OS-keyring interface for that service. Its Git bootstrap creates a private
key file but also deliberately exports `BUZZ_PRIVATE_KEY` to the agent child
(`src/git.rs:78`); avoiding the CLI argument does not eliminate environment
exposure. Under a strict no-private-key-in-argv-or-environment requirement,
**a real persistent relay identity demo is no-go with this pinned harness**.
Use disposable fixture relay identities for initial adapter validation, or
wait for a supported secure input/signing interface.

Finally, the Codex-specific launcher injects `CODEX_CONFIG` with
`sandbox_workspace_write.network_access=true` (`src/config.rs:826–868,1145`).
It does not constrain egress to the relay alone. For a real provider-backed
step, isolate the process and workspace at the OS/container level, expose no
production files/keys or broad MCP tools, verify supported restrictive adapter
settings at runtime, and impose explicit resource/cost limits. Until that
boundary and adapter credentials are prepared, the safe first demo is the
synthetic peer above. Approval behavior, adapter availability, provider access,
and agent execution remain separate claims from successful Buzz message delivery.

## Executable fixture preparation

`scripts/acp-fixture` now supplies the bounded supervisor, and the helper has an
ignored `acp_relay_synthetic_routing` test. The supervisor accepts only the
fixed public fixture agent, a verified owner-signed fixture auth tag, the pinned
Node peer, an explicit loopback relay and canonical room. It clears inherited
credentials and configuration, bounds runtime/log size, and cleans up adapter
process groups as well as the harness. Its `started` event is process creation,
not readiness; the Rust driver requires a verified same-room agent ACK.

The driver prepares membership and NIP-OA using upstream SDK builders. It checks
an exact owner mention followed by bounded no-mention/stranger observations.
Those quiet intervals are not proof of permanent exclusion. The fake peer can
see historical trigger tokens, so this fixture must not be interpreted as a
security proof or a model evaluation. Local compilation and supervisor process fixtures passed. The actual upstream
ACP harness also passed the optional stage in run 36280557661. Messaging-only
runs select `real_relay_`; the opt-in stage separately selects `acp_relay_`.

The optional runner stage explicitly sets `OMARCHY_BUZZ_TEST_ACP_BUZZ_SOURCE`,
`OMARCHY_BUZZ_TEST_ACP_BIN_DIR`, and `OMARCHY_BUZZ_TEST_ACP_NODE`, alongside its
disposable `OMARCHY_BUZZ_TEST_RELAY_URL`, then run the ignored `acp_relay_` test.
Built binaries must include `buzz-acp`, `buzz`, `git-sign-nostr`, and
`git-credential-nostr`. No production identity or provider credentials are
accepted by this synthetic supervisor.

The manual relay workflow now offers `synthetic_acp=true` (default false).
It builds the four pinned tools, then passes paired `--acp-bin-dir` and
`--acp-node` arguments to the runner. The ACP test runs only after exact
messaging conformance passes. Evidence records messaging and ACP results
separately; an ACP failure cannot erase or imply the messaging result.
No real agent/provider is selected by this option.
