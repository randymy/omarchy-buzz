# ACP readiness at the pinned Buzz revision

Status: source review on 2026-09-28, not a real-agent run. Buzz is pinned at
`781d39510cf23cfe224e8f521ae06a23377e06de` in the helper. This note uses
that local checkout and the evidence in [ACP_VALIDATION.md](ACP_VALIDATION.md).
It does not certify a newer Buzz build, an installed adapter, provider login,
or the deployed relay's ACP behavior.
Unqualified `src/` references below are within `crates/buzz-acp/`.

## Initial presentation increment (implemented in 0.0.5)

Keep agent execution in independently supervised upstream `buzz-acp` processes.
The next safe plugin increment is **read-only, evidence-labelled agent
discovery**, behind a new optional helper capability. It can be built and
tested entirely with signed synthetic relay events. It must not launch an
adapter, handle agent or provider secrets, offer tool approvals, or infer a run
state from room messages. Exact-key mentions and room responses already work as
ordinary messaging; the plugin does not need to become an ACP controller for
those paths.

Concretely, add a bounded `agents` view to the helper's versioned status after
its existing authenticated room and roster checks. A self-authored,
signature-verified kind `10100` may label a roster key as a *self-described
agent*. Missing profile evidence leaves the person as “participant”; even an
advertised `online` value does not prove a running process. Clear this view on
disconnect, room revocation, and generation change. The QML bridge receives
only public keys, bounded display fields, event IDs, and stable categories.
Add synthetic positive, forged-author, wrong-room, revocation, and disconnect
fixtures at the helper boundary. Do not put raw events, observer frames, or
tool content in QML. A later, separately verified kind `20002` room subscription
could supply an expiring “typing” hint; silence must not mean idle.

This is an M3/v0.1 presentation increment, not completion of M4. The design
above is retained as implementation context; see the 0.0.5 follow-through below
and `docs/CHECKPOINT.md` for current 0.0.6 messaging/activity behavior. ACP
execution and approvals remain unfinished.

## What is established

| Claim | Evidence and limit |
| --- | --- |
| Synthetic ACP routing works through an isolated relay | `docs/ACP_VALIDATION.md`, `tests/acp_fixture.py`, `scripts/acp-fixture`, `helper/src/acp_relay_tests.rs`, and sanitized `docs/evidence/relay-acp-36280910334.json`. The pinned fake peer is `desktop/tests/e2e/fixtures/fake-acp-agent.mjs`. It proves a signed exact-mention route and same-room reply, not model behavior or a permanent exclusion guarantee. |
| The harness supports ACP stdio peers | `crates/buzz-acp/README.md` and `src/config.rs` default to `goose acp`; the desktop runtime catalog at `desktop/src-tauri/src/managed_agents/discovery/catalog.rs` names Goose, Claude Code via `claude-agent-acp` (legacy `claude-code-acp`), Codex via `codex-acp`, and Buzz Agent via `buzz-agent`. This is a source support list, not local installation, login, architecture, or permission verification. Raw `codex`/`claude` CLI presence is insufficient. |
| Routing has an owner policy, separate from tool approval | `src/config.rs` defaults to `respond-to=owner-only`; `src/lib.rs::resolve_agent_owner` and `filter.rs::match_event` enforce exact signed identities. As `ACP_VALIDATION.md` records, verified same-owner NIP-OA siblings can also qualify. The receiving agent must have room membership. |
| Tool requests are automatically decided by the harness | `src/config.rs:461–472` defaults to `bypass-permissions`. `src/pool.rs` applies a nondefault mode only when the peer advertises it. `src/acp.rs::handle_permission_request` selects `allow_once` when offered, otherwise `reject_once`. A UI approval card would not interpose on this path. |
| Persistent relay key handling violates the strict launch boundary | `src/config.rs:250–251` takes `--private-key` or `BUZZ_PRIVATE_KEY`; no inspected keyring, FD, or signer interface exists. `src/git.rs::GitEnvironment::install` writes a 0600 `.nostr-key` and exports `BUZZ_PRIVATE_KEY` to the child; `src/acp.rs::spawn_with_env` inherits the parent's environment and applies the launch environment. Parsing zeroizes one Rust string best-effort but does not remove argv/environment copies or child access. |
| Codex launch widens network access | `src/config.rs::codex_network_env` injects `CODEX_CONFIG` with `sandbox_workspace_write.network_access=true`; `src/acp.rs::spawn_with_env` merges it into the child configuration. That grants general outbound access under the described sandbox mode, not relay-only egress. |
| Current plugin is a messaging client | `helper/src/protocol.rs::envelope` advertises connection, catalog, history, send, recipients, and auto-refresh capabilities; `plugin/Service.qml` accepts those. `helper/src/recipients.rs` deliberately treats profile names as unverified. No ACP process or secret crosses the helper IPC. |

The synthetic test is already the correct baseline for changes to routing. Do
not repeat it against the operator's relay or identity merely to validate UI.
The local ACP fixture uses disposable keys in an isolated process and relay;
that exception does not justify persistent keys in environment variables.

## Remaining gates before a real-agent demo

1. **Upstream secret input and propagation.** Provide and inspect a supported
   `buzz-acp` input/signing route that keeps a persistent relay secret out of
   argv, environment, agent child, and reusable temporary files. Test child
   environment and Git bootstrap behavior, including crash/cleanup paths.
   A private systemd unit alone does not change `src/git.rs` propagation.
2. **Permission policy.** Upstream must expose a fail-closed tool policy with
   an explicit decision path. Test an offered `allow_once` request, unsupported
   mode, timeout, cancellation, and adapter-specific tool calls. Do not equate
   `--permission-mode default`, `plan`, or `dont-ask` with a review gate while
   the current auto-approval and unsupported-mode fallback remain.
3. **Adapter-by-adapter availability and authentication.** Pin exact adapter
   artifact/version and verify the target Linux architecture, stdio handshake,
   login route, effective sandbox, workspace, network boundary, and bounded
   resource/cost behavior. The installed `codex-cli 0.155.1` observation in
   `ACP_VALIDATION.md` is not a `codex-acp` check. Desktop discovery declares
   a 1.10.0 minimum in `desktop/src-tauri/src/managed_agents/discovery.rs`,
   and the catalog uses `codex login status`; the older ACP README's API-key
   fallback text is not
   proof of current credential behavior. Apply the same scrutiny to Goose,
   Claude Code, and Buzz Agent before naming any one ready.
4. **Scope and telemetry.** An approved demo needs a disposable room and agent
   identity, exact owner/room admission, and a deliberately limited workspace.
   `ObserverHandle` in `src/observer.rs` is process-local. Encrypted kinds
   `24200`/`44200` are owner-scoped; they require a separate authorized,
   gap-aware design before a detailed run dashboard. Presence/typing kinds
   `20001`/`20002` and availability snapshot `40902` do not certify execution.
5. **Transport and compatibility.** Resolve the upstream WebSocket resource
   bounds documented in `helper/WS_UPSTREAM.md`, then test the exact pinned
   harness/adapter/relay combination on isolated infrastructure. Deployed
   relay transport and human room access in `docs/CHECKPOINT.md` are messaging
   observations, not ACP compatibility evidence.

Until these gates pass, no production agent launch, approval UI, agent-control
IPC, or claim of “working/thinking/awaiting approval” belongs in the base
plugin. The first real-model step should be a separately supervised,
observe-only task in an isolated environment, with the actual adapter and
permission behavior recorded. The plugin can consume its verified room reply
through the existing message path.

## 0.0.5 implementation follow-through

The plugin now queries exact verified roster authors for signed kind-10100
profiles and exposes at most ten sanitized hints in the selected room. A card
shows a self-described agent identity and **execution unknown**. It does not
infer a human from the absence of a profile. The room recipient picker remains
the way to construct exact-key mentions. Activity counters measure new observed
messages; they do not mean an agent is working. Typing subscriptions, process
supervision, tool approvals and encrypted run telemetry remain unimplemented.

### Concrete upstream changes needed for real ACP execution

These are proposed upstream contributions, not local patches or filed issues:

- Support an explicit per-agent choice between subscription sign-in and
  usage-based API billing, through the provider's supported agent/ACP adapter
  authentication flow. Subscription support for Claude Code and Codex is a
  first-class product requirement, not an optional API-key-only follow-up.
  Acceptance and credential boundaries are specified below. This requirement
  is not a claim that every current adapter supports both modes.
- Add an explicit credential source (inherited descriptor or external signer)
  to `buzz-acp` and separate relay signing from Git bootstrap. Default agent
  child environments must exclude relay/provider secrets. An opt-in Git bridge
  must use bounded credentials or its own authority, not the human relay key.
  Acceptance: inspect argv, child environment and temporary files under a fake
  ACP peer; no persistent relay key appears, including after a failed launch.
- Replace implicit `allow_once` with an explicit permission policy and a
  fail-closed decision channel. Unsupported requested modes must stop launch.
  Acceptance: fake peers cover unsupported modes, offered allow/reject options,
  response expiry, cancellation, peer exit and replay; none silently grants.
- Expose bounded public run-state events or a stable local observer interface,
  with documented scope, freshness and cancellation semantics. Keep tool
  payloads and encrypted owner telemetry separate. Acceptance: stale or missing
  events produce unknown, never fabricated thinking/idle/completed states.
- Bound WebSocket frame/message sizes and pre-authentication queues in the
  reusable upstream client. Acceptance: oversized frames and a flood before
  authentication terminate with category-only errors and bounded memory.

Until those interfaces are available and tested, launching real agents or
adding an approval button would bypass the project's stated security boundary.
The community plugin can still collaborate with independently configured agent
identities through normal messages; that does not certify those agents' runtime
permissions or credential handling.

## Required subscription and usage-based authentication workflow

For Claude Code and Codex, users should be able to choose **Subscription sign-in**
or **Usage-based API billing** when configuring an agent. The integration must
preserve that explicit choice for each agent. Where the provider and adapter
support it, reuse the user's existing native signed-in session; otherwise invoke
the provider's supported browser or terminal login flow. Do not require an API
key merely because the same agent is being used through Buzz ACP.

This is a required integration outcome, subject to provider entitlements and
documented adapter support. Verify each mode through the actual ACP adapter;
successful authentication in a standalone CLI alone is insufficient. If a mode
is unsupported, show that limitation and track the upstream work rather than
silently substituting another authentication route.

- Keep account login, token refresh and API credential storage in the native
  agent/provider mechanism or a dedicated local process. QML may display the
  selected mode and sanitized authentication status, never passwords, tokens,
  API keys or credential-store contents. Buzz relay identity remains separate.
- Subscription expiry, exhausted allowances or rate limits must produce an
  actionable paused/unavailable state. Never fall back automatically to API
  billing, another account or another provider. A billing-mode change requires
  an explicit user action before the next task runs.
- Show which authentication/billing mode an agent is configured to use, and
  distinguish configured mode from verified effective mode. Do not promise
  unlimited subscription usage, display invented balances or infer costs from
  message counts. Show provider-reported limits only where supported.
- Mode selection must not alter workspace permissions, tool-approval policy,
  sandboxing or credential isolation. Subscription login is not authorization
  for the agent to perform privileged actions.

Before claiming Claude or Codex subscription support, record a compatibility
matrix for the exact agent, ACP adapter and harness versions. Test subscription
sign-in/session reuse, usage-based setup, effective-mode confirmation, logout,
expiry, exhausted allowance, revoked API credentials and explicit mode changes.
Use mocked authentication for routine conformance; real paid/subscription runs
need a deliberately authorized limited demo. Confirm that no test or failure
silently changes the payer or leaks credentials into relay events, QML, logs,
argv, unrelated child processes or temporary files.

## Authentication compatibility follow-up

See [AGENT_AUTH_COMPATIBILITY.md](AGENT_AUTH_COMPATIBILITY.md) for the native
provider versus ACP verification matrix and the discovered terminal-auth
capability mismatch in the Buzz harness. Source refresh to official Buzz
`ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43` did not change these ACP paths.
Subscription support is still required; no real subscription/paid agent was
launched to establish it in this pass.

## Explicit permission-mode follow-up

A [staged upstream proposal](upstream/ACP_PERMISSION_MODE.md) rejects unsupported
nondefault modes and propagates setter errors instead of silently continuing.
Its four focused tests pass with transport doubles. It does not resolve the
harness's default bypass mode, automatic tool approval, credential propagation
or effective-mode verification. No installed ACP behavior changes.

## Reuse and terminal execution follow-through

Buzz Desktop's existing account workflow is now explicitly mapped in
[ACP_INTERACTIVE_LOGIN.md](upstream/ACP_INTERACTIVE_LOGIN.md). An alternative
upstream patch adds the missing standalone terminal execution path while
reusing adapter-native login. Two descriptor tests and twelve real PTY fixture
cases pass; this is not full-harness or provider-login certification. The
[Codex source assessment](CODEX_ADAPTER_SOURCE.md) identifies its different
agent-owned ChatGPT login path. Credentials remain native; no subscription
support claim is inferred from synthetic tests.
