# Consuming Buzz from vPerps

Buzz for Omarchy is an independent community plugin. The vPerps overlay should
install a reviewed release of this repository and its matching helper binary,
using the same plugin ID, source and update path as other Omarchy users. No
vPerps code or credentials belong in the base plugin. This is an integration
plan; no vPerps repository has been modified and the end-to-end ACP demo has not
been performed.

## Available integration today

The plugin provides a native bar, panel, room discovery, recent conversation
snapshots, message composition and exact-key recipient selection. The helper
supports either a Buzz-hosted community URL or a self-hosted relay. Configure the
same existing human Buzz identity through hidden terminal input and Secret
Service, following the main README. The overlay must never embed identity keys
in its files, environment variables or command arguments.

A vPerps menu item can invoke the existing fixed command:

```sh
omarchy-shell shell summon community.buzz '{}'
```

This opens the generic panel. It does not select a room, inject a message,
execute an agent, or accept arbitrary navigation targets. The optional Super+B
installer is conflict-aware and reversible; do not duplicate its binding in the
overlay. Use the plugin's documented installation and cleanup ownership:
Omarchy manages QML; the helper binary, user units and optional shortcut are
separate. Preserve the identity and delivery ledger on ordinary upgrades and
uninstall.

Create and manage development, operations or research rooms through upstream
Buzz. Rooms appear only after the enrolled identity has membership. Labels such
as `#development` or `#risk` are configuration choices, not built-in plugin
semantics. Typing `@codex` is not sufficient to address an agent: select its exact
public key from the verified room roster. Agent processes and their routing
must already be independently configured.

## Separate integrations to add later

- **ACP supervision:** run upstream Buzz ACP and supported adapters in separate
  processes with explicit owner/routing, workspace and tool policies. Review
  upstream permission defaults before enabling them. The current plugin does
  not launch ACP, attach to an existing Codex session or override repository
  instructions.
- **Activity publishers:** vPerps services can publish supported signed Buzz
  events under independently managed identities. Keep exchange credentials,
  wallets, deployment credentials and local data-plane access inside their
  existing authority boundaries. Do not forward raw logs or credential-bearing
  error output into a room.
- **Navigation:** propose a generic typed resource-handler interface with
  locally registered handlers and validated identifiers before adding links to
  a repository, PR or application. No such handler registry is implemented yet;
  event text must never become a shell command or an unrestricted URL scheme.
- **Approvals:** display a validated request only when its owning authority can
  bind a response to the exact action, actor, scope, expiry and replay rules.
  Posting approval text to Buzz does not itself authorize a merge, deployment,
  collateral transfer or trade.

These additions should be configuration, upstream contributions or separate
packages. They must not require a branded fork of the plugin. Preferred-room,
notification and agent-dashboard configuration remain future work; no
undocumented manifest fields or IPC commands should be assumed.

## Acceptance sequence

First certify messaging against an isolated relay using synthetic identities.
Then configure a supported ACP adapter in an isolated development workspace and
prove exact-identity task routing, response history and evidence-backed status.
Only then repeat the observe-only service investigation in the vPerps Omarchy
environment. Keep production trading, deployment and credential actions outside
that demo. The same unchanged plugin must run in both environments.

Buzz remains the coordination and evidence plane. A future vMachine runtime is
the authority plane: it owns capability leases, credential access, risk limits,
attestation and action enforcement. A valid Buzz signature establishes who
signed an event; it does not replace any of those decisions. See
[DESIGN.md](../DESIGN.md) for the inspected vPerps source baseline and interfaces.
