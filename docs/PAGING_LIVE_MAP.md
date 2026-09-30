# Older history, longer threads and live updates — implementation map

Research pass on September 29, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. File:line references are to that
checkout. This is a map for implementation, not verified behavior of the
installed helper; confirm each reference before relying on it.

## A. Older room history

Desktop adds `top_level:true`, `until` and `before_id` to the `#h`+`kinds`
window filter (`desktop/src-tauri/src/commands/channel_window.rs:21-38`).
NIP-CW requires both-or-neither and never demotes a malformed cursor to a head
request (`docs/nips/NIP-CW.md:74-78`; relay `crates/buzz-relay/src/api/bridge.rs:692-721`).
Continue by reading `kind:39006`: if `has_more`, resend with
`until=next_cursor.created_at`, `before_id=next_cursor.id`; row count is never
the exhaustion signal (`NIP-CW.md:166-171`). Window `limit` default 50, max 200
(`bridge.rs:571-572,723-727`); aux closure pages capped by `AUX_PAGE_LIMIT` for
up to `AUX_MAX_PAGES=64` (`bridge.rs:587-593,642`). The `39006` `d` tag is
`<channel>:<until>:<before_id>` or `<channel>:head` (`NIP-CW.md:154-162`);
`helper/src/history.rs:167` verifies only the head binding today because
`query.rs::RoomHistory` never sends a cursor. Edits, deletions and reactions
for an older page arrive in that page's own aux closure (`NIP-CW.md:98-101`).

Helper: cursor variant of `QueryRequest::RoomHistory` (`query.rs:49`, limit
currently `1..=20`); continuation `d` binding in `history.rs::reduce`; a
`fetch_older` IPC command (`protocol.rs::request`, `ipc.rs`) patterned after
`FetchThread`; per-room accumulated pages in `auth.rs::observe_inner` (rows
fully replace on each poll today, `auth.rs:719-722`); `nextCursor` on
`protocol::History`. Keep 200 events / 512 KiB per page (`history.rs:7-8`),
bound cumulative rows (100–200) and respect `RESPONSE_LIMIT` (`protocol.rs:5`).

## B. Longer and deeper threads

Two server modes. Newest-first `thread_window:true` with `kind:39007` bounds,
`limit` 1–200 (default 50), `depth_limit` 1–100 (`NIP-CW.md:230-247`), relay
limits 64 aux scans / 8192 rows / 8 MiB / 8 s (`NIP-CW.md:285-288`). NIP-CW
states Desktop ships no client for this mode (`NIP-CW.md:333`); Desktop uses
the legacy oldest-first path: `depth_limit` 64, `limit` 200 capped 500,
ascending order, continued with `thread_cursor`/`thread_cursor_id` from the
last loaded reply, no signed bounds (`desktop/src-tauri/src/commands/messages.rs:245-315`,
`NIP-CW.md:174-209`); `desktop/src/features/messages/useThreadReplies.ts:40-89`
pages until a short page. Nested replies are laid out client-side from the
flat list (`lib/threadTreeLayout.ts`, not read in depth).

The helper uses the `39007` mode with `depth_limit:1, limit:8`
(`query.rs:52-53`, `thread.rs:96-144,193-231`), which has the strictest
verification NIP-CW defines but caps depth at 1. Smallest change: raise
`limit` to 50 and `thread.rs::ROWS`, add an `until`/`before_id` cursor with
its binding, a `fetch_older_thread` command and `nextCursor` on
`protocol::Thread`; keep 200 events / 512 KiB per page (`thread.rs:12-13`)
plus a cumulative cap (200). Depth > 1 requires walking ancestry in
`thread.rs::reduce` (`thread.rs:247-267`) or switching to Desktop's legacy
mode; that is a design decision for the owner.

## C. Live updates instead of polling

Desktop opens one REQ per visible channel:
`{kinds: CHANNEL_EVENT_KINDS + 39005, "#h": [channel], since: now}`
(`desktop/src/shared/api/relayClientSession.ts:337-349`; kinds in
`desktop/src/shared/constants/kinds.ts:101-113`: messages 9/40002/40001,
edits 40003, deletions 5/9005, reactions 7, diffs 40008, system 40099,
huddles). Threads ride the same channel feed. On reconnect Desktop refetches
the head page and re-arms the REQ (`desktop/src/features/messages/hooks.ts:407-462`,
`NIP-CW.md:170`); the live feed never fills gaps. Relay allows 1024
subscriptions per connection (`crates/buzz-relay/src/handlers/req.rs:26,74`).

Implemented on branch `live-updates` (September 30, 2026; not merged or
installed) with a narrower design than first mapped here: one subscription for the
selected room only (no thread subscription), and live events are refetch triggers,
never content, so no event queue is needed. A verified event (signed, subscribed
kind, one `h` equal to the room, under 64 KiB) schedules a debounced head refetch
through `history::fetch` and, for an open thread and an `e`-tagged event, a
`thread::fetch`. The subscription is closed before every mid-session
`authenticate`, on room change, `fetch_recent`, Retry and every connection exit,
and after more than 200 unverifiable frames per minute; `CLOSED` falls back to
polling with 5 s/30 s/5 min re-arm. While primed, polling slows to 30 s but
remains the safety net. The owner requested this design with that mitigation;
the upstream pre-authentication buffer itself stays unbounded and its release
gate stays open (`helper/WS_UPSTREAM.md`, September 30 checkpoint entry).

## Order of work

A first (command plumbing already patterned after `FetchThread`), then B
(owner decision on depth), then C (changes the connection's trust boundary;
depends on the WebSocket limits). A and B are independent of each other and
of C.

Not independently re-opened in this pass: `applyEditTagOverlay.ts`,
`renderScopedReactions.ts`, `threadTreeLayout.ts`, `formatTimelineMessages.ts:235`,
`bridge.rs:755`.
