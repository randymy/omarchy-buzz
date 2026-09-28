# Membership-bound room replies

Unsubmitted proposal against Buzz `781d39510cf23cfe224e8f521ae06a23377e06de`;
not installed on any relay. Apply after the six ACP/resource-limit proposals.
This adds a **proposed**, not currently supported, `/events/member-bound` endpoint.

The ordinary `/events` contract allows open-room nonmember posts. A preflight
query or optional header cannot establish the stronger contract our agent replies
need. The separate endpoint requires exact-route NIP-98 authentication and body
binding, even when ordinary bridge development authentication is enabled. It only
accepts kind-9 room messages and reuses upstream validation and signed events.

The database obtains the existing per-community/channel membership advisory lock
as the transaction's first SQL statement. It reads authoritative membership and
stores the event and thread metadata in that transaction. Membership removal
uses the same lock. Removal therefore commits before publication (publication
rejects) or after publication commits. No membership cache or open-room exception
can grant this write. This orders against membership changes, **not channel
deletion or a future authority engine**. Relay compromise remains outside the
client's ability to enforce this guarantee.

The staged ACP reply transport uses only this endpoint. It never falls back to
ordinary `/events`, and a missing endpoint remains a delivery failure. Existing
relays—including hosted relays—remain usable for normal human messaging; this
agent-reply feature requires upstream support before it can be enabled there.
No relay fork or deployment is part of this contribution.

Manual CI compiles the real relay/ACP and their test targets. Disposable
Postgres/Redis fixtures test open-room nonmembers, active/removed membership,
tenant isolation, duplicates after removal, removal-held-lock ordering, and the
actual router's authentication/kind/member checks. A client fixture proves an
older relay's ordinary endpoint is never used as fallback.

[Manual run 36493588467](https://github.com/randymy/omarchy-buzz/actions/runs/36493588467)
passed the complete relay/ACP build, two database tests, the actual router test
and the transport test (eight cases). See [evidence](../evidence/member-bound-36493588467.json).
The complete unit suites were compiled but not run. This validates the proposed
endpoint in disposable Postgres/Redis fixtures, not deployment or a complete
prompt-to-room lifecycle. The first run caught missing test imports, now corrected.
