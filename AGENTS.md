# Project instructions

- Read DESIGN.md before implementation and docs/CHECKPOINT.md for current capabilities and remaining gates. Historical milestone evidence is not a claim about the current installed build. Do not treat proposed interfaces as existing upstream APIs.
- Keep the public plugin generic. Downstream trading, registry, credential, deployment, and authority integrations belong outside the base plugin.
- Do not fork Buzz or Omarchy, modify their installed sources, or modify vPerps as part of base plugin work.
- QML is a presentation/control surface. Never put identity keys, bearer tokens, provider credentials, raw agent telemetry, or arbitrary command execution into its data model.
- Use upstream signing, event, and agent implementations. Pin inspected dependency revisions and distinguish source compatibility from runtime verification.
- Preserve unknown, stale, partial, and delivery-unknown states. Never invent agent work states, exact unread counts, successful sends, or security approval guarantees.
- Use synthetic fixtures and isolated test relays. Never send messages, invoke agents, place orders, or alter production systems merely to validate a UI.
- Keep changes small and reviewable. Document material compatibility changes, test the actual boundaries, and never include secrets in diagnostics or commits.
