# Explicit rejection of ACP tool permission requests

Unsubmitted proposal, applied after the five ACP/resource-limit proposals at
Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`. Not installed or a sandbox.

`--deny-tool-requests --permission-mode default` removes automatic approval for
that harness. Each requested permission selects the adapter's unique, valid
`reject_once` option. If no such option exists, the harness returns the ACP
`cancelled` outcome. Missing, malformed, duplicate or excessively large option
sets cannot fall back to an allow choice. Diagnostics contain a static denial
reason, not request content or proposed commands.

This flag requires an explicit default mode. Session setup must advertise that
mode and acknowledge the mode-setting request before any prompt is sent. The
policy passes through conversation/task startup and all recovery launches. It
is enforced again at runtime preparation for callers that bypass CLI parsing.
Existing launches without the flag retain upstream behavior.

This is a narrow protocol boundary: **an adapter may execute actions that never
request permission**, and its default mode may allow tools. Same-UID process
access and provider-side sandbox settings are separate concerns. This does not
make a real-agent demo safe by itself, does not add an approval UI, and does not
authorize privileged actions. A future authority integration must own any grant.

Manual CI covers the actual ACP client's normal and idle-aware readers using
synthetic stdio peers, including offered allow choices, absent rejection choices,
malformed options and colliding option IDs; runtime tests check policy propagation
and rejection of bypass mode. No real agent, account, shell tool action or relay
is used by those tests. Full pinned-adapter execution isolation remains a gate.
