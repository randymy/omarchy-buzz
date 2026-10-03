# Direct messages — implementation map

Research pass on September 29, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. File:line references are to that
checkout unless prefixed `helper/`. This is a map for implementation, not a
statement about the installed helper; confirm each reference before relying
on it. DMs remain excluded from the first release by DESIGN.md until built.

## Wire representation

DMs are ordinary channels with `channel_type = 'dm'`, identified by a hash of
the sorted participant set, not by a name (`crates/buzz-db/src/store/dm.rs:50-60,103-181`).
Their discovery events (kinds 39000/39002) carry `["t","dm"]`, a `["hidden"]`
hint tag and one `["p", pubkey]` per participant
(`crates/buzz-relay/src/handlers/side_effects.rs:1215-1229`); stream channels
carry `["t","stream"]` and no `p` tags there. Group DMs have 2–9 participants
(`dm.rs:109-118`, `handlers/command_executor.rs:315-319,491-495`). The channel
`name` is literally `"DM"` or `"Group DM (N)"` (`dm.rs:160-164`).

## How Desktop lists and names them

`desktop/src-tauri/src/commands/channels/fetch.rs:161-197` uses the same
39002→39000 chain with no stream-only filter; `desktop/src-tauri/src/nostr_convert.rs:126-150`
derives `channel_type` from `t` (fallback: a `hidden` tag means dm) and
participants from `p` tags. The display name is built client-side from
participant profiles (`desktop/src/features/channels/lib/dmParticipantDisplay.ts:41-108`).
Hidden DMs come from NIP-DV: query `kinds:[30622], #p:[me], limit:1` and
collect its `h` tags (`desktop/src/features/channels/hooks.ts:24-48`,
`docs/nips/NIP-DV.md:104-110`).

## Messages inside a DM

Identical to rooms: kind 9 with only an `h` tag (`crates/buzz-sdk/src/builders.rs:240-263`),
no channel-type gate on ingest (`crates/buzz-relay/src/handlers/ingest.rs:3072-3101`;
the only `channel_type != "stream"` check is huddle-only at `ingest.rs:104`),
and NIP-CW windows key on `#h` alone (`docs/nips/NIP-CW.md:55-76`). Content is
plaintext to the relay at this revision: no NIP-44/NIP-04 wrapping in the send
path; `KIND_GIFT_WRAP` (`crates/buzz-core/src/kind.rs:59-60`) is unused by DMs.

## Opening, hiding, reopening

Open: sign kind 41010 with empty content, one `p` tag per other participant,
no `d` tag (`builders.rs:1869-1884`, `desktop/src-tauri/src/commands/dms.rs:36-84`).
Relay `handle_dm_open` (`command_executor.rs:297-429`) requires 1–8 others,
resolves or creates by participant hash (idempotent), answers with
`OK … {"channel_id", "created"}` (`command_executor.rs:417-428`); the client
then queries kind 39000 by `#d`. Scope `MessagesWrite` (`ingest.rs:604`); no
relay toggle for DMs was found. Hide: kind 41012 with an `h` tag; relay checks
membership and `channel_type == "dm"`, sets `hidden_at`, republishes the
caller's 30622 snapshot (`command_executor.rs:568-637`). Reopen with 41010 and
the same set clears it (`command_executor.rs:409-415`, `NIP-DV.md:93-98`).
Hiding never affects membership or delivery (`NIP-DV.md:23,31,115`).

## Helper changes

(a) Existing DMs — list, read, send: `helper/src/catalog.rs:219` drops every
channel where `kind != "stream" || hidden || archived`; DMs always carry the
`hidden` hint, so both conditions must change together. `Room`
(`catalog.rs:16-21`) needs a channel kind and participant keys; names must be
derived from participant profiles (kind 0 lookups already exist in
`query.rs:38`). `recipients.rs::roster`, `history.rs`, `query.rs::RoomHistory`
and `sending.rs` (gate at `sending.rs:88`, generic `build_event` at 254-289)
need no DM-specific change once the catalog admits them.
(b) New DM: a signed kind 41010 submission outside the kind 9 send ledger,
plus the 39000 follow-up query; new request kind in `protocol.rs:38-46`.
The people to choose from are described under "People picker".

## People picker (New message)

Desktop's `NewMessageScreen` lists the community's people, not one room's
members: `search_users` (`desktop/src-tauri/src/commands/profile.rs:265-330`)
sends a NIP-98 signed `POST /query`. An empty query is
`{"kinds":[0],"limit":50,"page":N}`; typed text is NIP-50
`{"kinds":[0],"search":q,"search_mode":"prefix","limit":50,"page":N}`. Results
are ranked locally (`nostr_convert/user_search.rs`: name, then nip05, then key
prefix; exact > prefix > substring; an empty query lists by name) and up to 8
become chips; the search box clears after each pick.

The helper does the same through its own signed `/query` path
(`QueryRequest::People`, one page of 50; no paging). `search_people`
(`{"query": "..."}`, at most 256 bytes on the wire, normalized to 64 bytes of
plain text) is answered in `status.people`: `state` (`unavailable`, `loading`,
`snapshot`), the echoed `requestId` and normalized `query`, `entries`
(`key`, sanitized `name`, at most 50) and a `people_*` failure category. The
panel shows a view only for the request it made. The capability is
`people_search`.

`recipients::people` rejects the whole read for a bad signature, another kind
or a future time; keeps the newest kind 0 per author (lower id on a tie);
leaves out this identity; sanitizes names like roster names. Keys it serves are
remembered per connection (`dm_open::Opener`, last 200) and, with the room
roster and existing DM participants, are the only keys `dm_open::allowed`
accepts; the panel's own check (`dmKeyAllowed`) mirrors it but is not the
authority. The list shows existing DM partners first, then the directory or
search, and the open room's members only while the read has not answered.

Not done: Desktop's paging past the first 50, agent filtering
(`getMentionableAgentPubkeys`), the archived-identity filter and typo-tolerant
ranking. A relay that does not honor `search`/`page` simply returns what it
returns; the helper still caps and verifies it.

## Risks

- `catalog.rs` rejects the whole catalog on one malformed room; a group DM
  with unexpected `p` tag shapes could hide every room. Needs a group-DM
  fixture before trusting the path.
- Participant-name resolution is a new lookup across all DMs, not only the
  selected room.
- Upstream acknowledges Desktop does not re-verify the relay signature on
  NIP-DV snapshots (`NIP-DV.md:122`); the helper must.
- Not read in depth: `desktop/src/features/channels/dmResurface.ts`,
  `hiddenDmResurfaceCoordinator.ts`.
