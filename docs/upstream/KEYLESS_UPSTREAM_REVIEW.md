# Upstream keyless broker review — September 29, 2026

Status: source review only. No upstream branch, local helper, relay, agent, or
production identity was changed or run. This records [Buzz PR #6967](https://github.com/block/buzz/pull/6967)
at exact head [`3d2ea5b89adccf980879b9841e9a6b517b588b57`](https://github.com/block/buzz/tree/3d2ea5b89adccf980879b9841e9a6b517b588b57).
At review time, #6967 and its exact base [contract PR #6922](https://github.com/block/buzz/pull/6922)
(`115e7975a11d7d4d95043cb847abd13c1a32f370`) were open. The
[broker-spec PR #6790](https://github.com/block/buzz/pull/6790) was closed
without merge, and [RFC #6467](https://github.com/block/buzz/issues/6467)
remained open. These are proposals, not an installed or stable Buzz interface.

## What the head supplies

- The [config path](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/config.rs#L925)
  makes `broker` an explicit mode, rejects a private key, requires broker URL,
  credential, canonical relay identity, explicit channel UUIDs and owner key,
  and currently restricts inbound authors to `owner-only`. Local mode still
  requires a private key. Broker mode uses a generated placeholder key to keep
  existing concrete types; it is not exported as the agent identity.
- The [runtime transport](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/runtime_transport.rs#L240)
  derives the agent public key from `storage.address`, polls `channel.read`,
  verifies returned event signatures/channel tags, and routes storage and live
  signals through broker actions. Direct relay publishing is disabled in broker
  mode; unavailable channel metadata is treated as unknown, and relay-only
  facilities are omitted. The [broker CLI path](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-cli/src/commands/messages.rs#L914)
  offers `message.post` and `message.reply`, with the host deriving thread roots
  and applying policy. That is a real keyless route for a tool-directed reply
  if an authorized broker host implements the contract.
- The [child spawn path](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/acp.rs#L496)
  force-overrides inherited Buzz mode, relay, key and broker variables for
  configured children, removing empty tombstones. In broker mode the
  [persona environment](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/config.rs#L1223)
  deliberately gives the adapter `BUZZ_BROKER_CREDENTIAL`; the
  [configured MCP server](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/lib.rs#L5249)
  receives that credential too. PR #6967 says observer and ACP wire logs redact
  MCP environment values. Redaction does not remove the credential from the
  child. Broker authorization, credential scope and child/tool isolation remain
  the host/operator's enforcement boundary.

## Relation to our unsubmitted proposals

The [FD key-isolation proposal](ACP_KEY_ISOLATION.md) keeps a signing key only
in the harness and scrubs known child key/Git paths. PR #6967 offers a stronger
upstream direction for *Buzz signing-key custody*: broker mode has no agent key
and no direct relay route, while the broker holds signing authority. The two
approaches should not be maintained as parallel long-term transports without a
specific missing capability. Broker mode is not credential-free: its bearer-like
broker credential reaches the adapter and MCP tool. The FD proposal's local
signing path may remain useful for isolated tests or a short transition, but it
does not replace the broker's audience and action policy. Neither path is a
same-user hostile-process sandbox.

The [harness-owned replies proposal](ACP_HARNESS_REPLIES.md) remains distinct.
At this head, [ACP user-facing chunks](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/acp.rs#L1805)
are still logged, and the [pool's visible failure notice](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/pool.rs#L4848)
still uses the local signed relay path. #6967 supplies `message.reply` to the
agent's Buzz CLI, not bounded capture of a completed ACP turn followed by a
destination chosen by the harness from its admitted trigger. If tool-directed
broker replies meet the product need, prefer that upstream route and verify the
host's per-action and per-audience constraints. If a guaranteed automatic room
reply is required, a narrow upstream harness-to-broker action remains to be
designed and tested; the old direct-signing patch should not simply be layered
onto broker mode.

The upstream permission and subscription gates remain separate. The
[permission-mode default](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/config.rs#L498)
is `bypass-permissions`, and the [ACP permission handler](https://github.com/block/buzz/blob/3d2ea5b89adccf980879b9841e9a6b517b588b57/crates/buzz-acp/src/acp.rs#L1983)
still selects `allow_once` when offered. The broker limits Buzz actions only to
the extent its host implements and enforces the contract; it does not constrain
native agent tools or provider billing. #6967 changes no Codex/Claude adapter
authentication policy. Preserve our restrictive permission-request and
subscription-route acceptance work independently of the transport choice.

For the no-fork path, track the upstream stack and review its host
implementation, broker credential issuance/scope, signed reply behavior and
real permission/subscription acceptance before adopting it. The exact PR head
is source compatibility evidence only; no local broker host, deployed relay,
adapter session or model turn was exercised in this review.
