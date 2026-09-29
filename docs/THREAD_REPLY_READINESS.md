# Room replies: read-path readiness

Source preview 0.0.7 now implements the bounded read-only thread view described
below. Helper unit tests and offscreen QML checks pass. The synthetic ACP
acceptance proves a signed reply is persisted, but the subsequent actual
thread-window read caught a `query_invalid_scope` compatibility error.
[Diagnostic run 36503438982](https://github.com/randymy/omarchy-buzz/actions/runs/36503438982)
identified a signed kind-7 reaction without an `h` tag, matching upstream
`buzz_sdk::build_reaction`. Commit `dc3776e` allows those auxiliary reactions
with validated event targets while rejecting explicit wrong-room tags and
forged events; reactions never become rows or execution-state evidence.
The corrected actual-relay test is pending. Installed 0.0.6
is unchanged. The following design describes the intended contract, not a
claim that the live-relay thread read has passed.

The installed 0.0.6 helper asks
`POST /query` for `top_level: true` in `helper/src/query.rs::RoomHistory`.
`helper/src/history.rs` materializes only that channel-window response and
requires a signed `39006` head bounds event. `helper/src/protocol.rs::History`
exports those rows, and `plugin/Service.qml::messages` passes them directly to
`plugin/PanelContent.qml`. A NIP-10 reply beneath a prompt is excluded from the
top-level page, even if it is persisted and correctly signed. The current
activity observer likewise sees only top-level rows. An ACK in a relay query or
the conformance test is therefore not yet a visible plugin reply.

## Smallest useful increment

Add an explicit **View replies** action on a displayed top-level root. The
current `HistoryRow` omits event kind; either expose a validated `kind` field
(`9` or `40002`) or offer the action for both supported root kinds. Do not
infer kind from the text or author.
Keep the existing channel window, its `39006` check, and its 20-row snapshot
unchanged. The helper should issue one separate, bounded NIP-CW thread-window
query for the selected room/root:

The thread page is capped at eight rows so a simultaneous 20-row channel
snapshot and thread snapshot fit the helper's bounded IPC status frame.

```json
[{"thread_window":true,"#h":["<canonical room UUID>"],"#e":["<lowercase root event ID>"],"kinds":[9],"depth_limit":1,"limit":8,"include_aux":true}]
```

Depth one is enough for a harness reply directly under the human prompt and
avoids representing nested conversations before the UI has a thread model. Do
not broaden `RoomHistory` to `top_level: false` or use the older `thread_cursor`
query: either would mix different page/cursor semantics into the existing
signed channel snapshot. The thread-window root itself is not a returned row.

Implement this as a separate `QueryRequest::ThreadReplies` in
`helper/src/query.rs`, a dedicated bounded reducer in `helper/src/thread.rs`
(sharing the conservative text/author/edit/delete helpers from `history.rs` if
useful), and a distinct `thread` view model/`fetch_thread` IPC command in
`helper/src/protocol.rs` and `helper/src/auth.rs`. The command must accept only
a canonical room UUID and lowercase 64-hex root ID that is currently present
in the selected, validated channel snapshot. The status should carry both
room and root, loading/snapshot/unavailable state, at most 8 rows, `hasMore`,
and a completeness-unknown category. Expose this via a new advertised helper
capability; older helpers should simply leave the action hidden. The QML
should validate all fields, clear thread state on root/room change, catalog
loss, disconnect, generation change, and unsupported capability, and show the
replies in a separate bounded pane under that root. Do not present a thread
reply as another top-level room message.

The reducer must verify every event signature and room scope, and require
exactly one fresh relay-signed kind-39007 bounds event. Verify its sole `d`,
`h`, and `e` tags; `d` must equal the NIP-CW `tw:1:` SHA-256 binding over the
normalized request, trusted relay authority, enrolled reader key, room, root,
limit 8, depth 1, kinds `[9]`, null cursor, and `include_aux: true`. Check
`version:1`, `direction:"older"`, and cursor/`has_more` consistency. Reject
missing, stale, duplicate, forged, or mismatched bounds; an empty array is
unavailable, not signed exhaustion. Use the configured canonical relay
authority as the binding input and test it with non-default ports. No legacy
fallback should turn an unverified response into a plausible thread.

Accept only signed kind-9 reply rows directly tied to the requested root
(NIP-10 `e` reply marker) and within the room, with unique IDs and a bounded
event/byte budget. For each returned reply, apply the existing conservative
edit/delete rules: same original author for edits, direct original-author
deletion, and `unavailable` when authority or competing edits are uncertain.
The thread response may contain auxiliary events for the root and for replies
outside the selected page. Validate their signatures and shape, but do not
mistake them for reply rows or apply them to an unseen original. Keep the
existing top-level root projection authoritative for the root display. A root
removed from the refreshed channel snapshot should close its thread pane.
`hasMore` means only an older thread page exists; the first increment need not
offer pagination. Preserve “completeness unknown” because auxiliary closure
and snapshots do not prove a complete global history.

Reuse the selected-history task cancellation/ticket/generation and joined-room
checks in `helper/src/auth.rs`; a late result must not cross a room/root switch.
On query access denial, clear the thread together with the selected room and
delivery state, as the history path already does. Do not let thread fetches
exhaust the query semaphore or silently stop the five-second top-level
refresh. A separate, user-triggered fetch is sufficient for this increment.

## Tests that make the claim reviewable

- `helper/src/query_tests.rs`: exact thread filter and NIP-98 request; reject
  wrong room/root, unsupported row/overlay kinds, and oversized responses.
- New thread reducer tests: valid direct signed ACK survives projection;
  missing/wrong-signer/wrong-request/duplicate/stale `39007` fails; forged or
  cross-room rows fail; root-target auxiliary events do not become rows;
  edits, deletions, uncertain authority, and event/byte caps retain current
  conservative behavior.
- `helper/src/auth_history_tests.rs` or a sibling integration test: switch
  rooms/roots during an in-flight fetch, remove membership, or change helper
  generation and prove no stale thread rows reach status. Confirm the normal
  channel snapshot keeps refreshing.
- QML service tests: capability gating, exact room/root matching, strict row
  validation, and clearing on selection or access loss. An end-to-end
  disposable-relay test should send a signed prompt, observe the real signed
  ACP reply in a thread-window result, then assert the helper thread status
  and panel model expose `AE-ACK:<positive>` while the top-level snapshot still
  contains only the prompt.

The initial ARM64 build passed 111 tests plus synthetic IPC checks in
[run 36502926877](https://github.com/randymy/omarchy-buzz/actions/runs/36502926877).
The reaction correction is undergoing a fresh native build.

The implementation targets pinned Buzz revision
`781d39510cf23cfe224e8f521ae06a23377e06de`. Release gates remain the actual
relay thread-read conformance and a passing native ARM64 helper build/IPC check.
