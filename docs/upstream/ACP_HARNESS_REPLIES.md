# Harness-owned room replies — draft contribution

Status: **unsubmitted, not installed, not production-ready**. Applies to Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de` after the resource-limit, permission-mode,
interactive-login and key-isolation proposals. This restores a candidate reply
path removed by key isolation; it does not enable agents in the installed plugin.

`--harness-replies` requires Linux FD identity input, thread session policy,
queued event handling, no initial prompt and no heartbeat. The harness captures
only ACP user-facing text notifications for the current fresh session, at most
64 KiB, and publishes only after an `end_turn` result. Thought/tool notifications,
wrong-session chunks, malformed or oversized text, cancelled and failed turns
cannot become completed replies. Each turn uses a fresh session; a 128-session
identity guard forces process recycling through existing protocol-error recovery.
Preparation failure retains its input batch for the existing bounded retry path.

Destination comes from the admitted incoming thread, never from generated text.
The existing Buzz SDK builds/signs kind-9 replies with the thread root as parent.
The harness uses upstream NIP-98 signing and `/events`. Its dedicated HTTP client
has no proxy, redirects or retries, a five-second total deadline, and an 8 KiB
acknowledgment bound. Only an exact event-ID acknowledgment reports acceptance.
A missing, malformed, mismatched or ambiguous response remains delivery-unknown.
Eight concurrent publications are permitted; overload reports not-sent. There is
no durable outbox: process exit can interrupt publication. Agent completion and
reply delivery are separate outcomes.

## Release blockers and deliberate limits

- **Removal race:** the main loop suppresses replies for removals already received,
  but the detached submission is not atomically fenced against later membership
  changes. Buzz upstream permits nonmember posting to open rooms. Relay acceptance
  does not prove current room membership. Resolve this with an upstream authority
  check/fence or an explicitly narrower supported room policy before release.
- No guarantee against a same-UID agent reading harness memory/configuration.
  These proposals prevent known credential handoff, not local privilege isolation.
- Existing upstream agent wire/observer logs are not globally redacted. Do not
  interpret the new static delivery diagnostics as full telemetry sanitization.
- No DM, channel-wide session, steering, heartbeat or unsolicited initial reply.
  No attachments or automatic mentions in generated replies. Session continuity
  relies on upstream room/thread history rather than reusing adapter sessions.
- Tool auto-approval/default bypass remains unresolved. This proposal neither
  grants policy authority nor proves safe agent execution.
- Pro subscription is the required real-account acceptance path. This change does
  not select or verify billing. Actual Pro login and no-API-fallback enforcement
  remain unverified; offline authentication discovery is a separate result.

## Validation scope

Manual CI compiles the complete patched upstream crate and its unit-test target,
then exercises bounded capture, SDK routing/signing and a loopback HTTP fixture
that verifies event and NIP-98 signatures and body digest. HTTP cases cover exact
acceptance, mismatched ID, malformed/oversized response, forbidden response,
redirect and service failure; request counts assert no retry/redirect following.
These are synthetic transport checks, not a real-relay membership proof or a
full ACP prompt-to-room acceptance test. Both are still required. Earlier auth
and key-boundary tests remain in the manual workflow. No automatic triggers.
