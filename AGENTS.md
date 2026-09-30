# Project instructions

- For an incoming maintainer, start with [docs/HANDOFF.md](docs/HANDOFF.md); verify its dated baseline against the checkout and installed state.

- Read DESIGN.md before implementation and docs/CHECKPOINT.md for current capabilities and remaining gates. Historical milestone evidence is not a claim about the current installed build. Do not treat proposed interfaces as existing upstream APIs.
- Keep the public plugin generic. Downstream trading, registry, credential, deployment, and authority integrations belong outside the base plugin.
- Do not maintain a Buzz/Omarchy product fork, modify installed upstream sources, or modify vPerps as part of base plugin work. The user approved a contribution-only Buzz fork and signed-off draft WebSocket PR on September 29; this does not authorize deploying patched upstream as a production dependency.
- First-release room publication uses ordinary Buzz permissions, explicitly approved September 29. Atomic member-bound publication is an optional future extension, not a required relay API. Preserve the documented open-room removal race and all credential, permission, subscription and lifecycle checks.
- QML is a presentation/control surface. Never put identity keys, bearer tokens, provider credentials, raw agent telemetry, or arbitrary command execution into its data model.
- Use upstream signing, event, and agent implementations. Pin inspected dependency revisions and distinguish source compatibility from runtime verification.
- Preserve unknown, stale, partial, and delivery-unknown states. Never invent agent work states, exact unread counts, successful sends, or security approval guarantees.
- Use synthetic fixtures and isolated test relays. Never send messages, invoke agents, place orders, or alter production systems merely to validate a UI.
- Keep changes small and reviewable. Document material compatibility changes, test the actual boundaries, and never include secrets in diagnostics or commits.
- The user explicitly resumed remote tests on 2026-09-28 after the notification pause. Run focused manual workflows as needed; keep automatic triggers disabled. Account notification preferences have not been changed.

- September 29: the user explicitly approved the first stock Codex subscription room agent in an isolated workspace. This authorizes the bounded real acceptance task and dedicated identity, not unrestricted host access or provider API fallback. Stock signing with the agent’s own key and auto tool decisions are confined to the documented filesystem sandbox; harness room/owner filters are routing controls, not a signing authority. Do not reintroduce optional harness-only signing as an implicit blocker for this approved path.

- September 30: the user approved an Agents section for single agents (Claude Code and Codex harnesses, one shared provider login per harness, this machine only; teams, sharing and import on hold). This authorizes the separate privileged agent service and per-agent sandboxed units described in [docs/AGENTS_SERVICE.md](docs/AGENTS_SERVICE.md). QML still never holds keys, tokens or command lines; the agent service accepts only the structured requests in that document.
