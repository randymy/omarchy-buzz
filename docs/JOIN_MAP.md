# Joining a community from the panel — implementation map

Research pass on September 30, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. File:line references are to that
checkout. Confirm each before relying on it; nothing here was checked live.

## What Desktop does

- Identity: a keypair is generated on first run with no user action and
  persisted to the OS keyring or a file (`desktop/src-tauri/src/app_state.rs:190-201,270-301`).
  No kind 0 profile is published by onboarding; that is a separate user action
  (`commands/profile.rs:446`).
- A community is a relay plus its NIP-29 groups. Hosted communities live at
  `*.communities.buzz.xyz` (`desktop/src/features/communities/hostedCommunityApi.ts:5`)
  and are created or listed through a browser OAuth login driven by Tauri
  commands; there is no public HTTP API a native client can call for that.
  DESIGN.md already decided to open buzz.xyz in the browser instead.
- Not yet in a community: Desktop's onboarding (`features/onboarding/ui/PendingInviteGate.tsx`,
  `MembershipDenied.tsx`, `InviteRedeemForm.tsx`, `WelcomeSetup.tsx`) asks for
  an invite, parsed by `desktop/src/shared/api/inviteHelpers.ts:26-77`:
  `https://<relay>/invite/<code>`, `buzz://join?relay=…&code=…`, or a bare code.

## The relay's join paths

1. **Invite claim (the real invite mechanism).** NIP-98-signed HTTP POSTs
   (`crates/buzz-relay/src/api/invites.rs:1-30,278-573`; client
   `desktop/src/shared/api/invites.ts`): `POST /api/invites` (mint, owner/admin
   only), `GET /api/join-policy` and `POST /api/invites/accept-policy`
   (terms/age attestation, `desktop/src-tauri/src/commands/join_policy.rs`),
   `POST /api/invites/claim`. Claiming grants **relay membership** (a kind 13534
   NIP-43 membership row, role member), not channel membership; the claim
   endpoint is exempt from the membership gate (`invites.rs:8-9`) and
   rate-limited 10/min per pubkey (`invites.rs:39-45`). The response is
   `{status, communityId, host, role}` (`useClaimInvite.ts`).
2. **Open channels.** For any authenticated reader, accessible channels are the
   member channels plus every `visibility = open` channel
   (`crates/buzz-db/src/store/channel_members.rs:974-1003`); an unscoped
   `{"kinds":[39000]}` REQ returns them (`desktop/src-tauri/src/commands/channels/fetch.rs:194-283`).
   Private channels stay invisible to non-members (`side_effects.rs:1175-1178`).
   A non-member self-joins an open channel with kind 9000 self-add
   (`channel_authz.rs:111-138`) or kind 9021 join request
   (`crates/buzz-sdk/src/builders.rs:908-911`; `side_effects.rs:2082-2121`,
   `ingest.rs:3000-3018` — refused for private channels, no approval step).
3. **Private channels**: only an active member can add someone (kind 9000);
   kind 9009 invite events are a no-op at this revision (`side_effects.rs:343-346`)
   — never build against them.
4. **Leaving**: kind 9022 (`builders.rs:800-803`, `side_effects.rs:2161-2178`),
   refused for the sole owner (`channel_authz.rs:65-73`). Removal by others is
   kind 9001 (`channel_authz.rs:175-186`).

## Panel design (step two of onboarding, after relay + identity)

- Not a member of anything: show an invite field accepting the three formats
  above; the helper claims through NIP-98 HTTP (accepting the join policy only
  after showing its text), then re-discovers the catalog.
- With relay membership: an `Open rooms` list from the unscoped 39000 query
  (relay-signed, pinned signer, strict shapes; rows not in the joined catalog and
  with `visibility = open` only) with a `Join` control sending kind 9021 or a
  9000 self-add; copy must say that open means no approval.
- A `Leave room` control sending kind 9022 (never offered to a sole owner).
- Every new inbound shape is validated like `catalog::validated`; the relay
  signer pin (`catalog.rs:60-64`, `relay_identity_changed`) applies to all new
  paths; the invite code and relay URL are the only things the user pastes.

## Risks to confirm live

Relay key rotation behaviour under the pin; whether the deployed relay exposes
`/api/invites/*` and rate-limits as documented; NIP-98 `u` binding on claim;
that a relay marking every channel open cannot trick the panel into joining
without the user seeing "open, no approval".

## Built on September 30 (branch `join-and-agent-dms`)

Not merged, installed or run against a real relay; every reference above was
re-read at the pinned revision before use. Corrections to this map: the
relay's claim answer is snake_case `{status, community_id, host, role}`
(Desktop maps it to camelCase); `POST /api/invites/accept-policy` is an
exempt route that reads no NIP-98 header (`{code, policy_version,
age_confirmed}` → `{receipt}`), so the helper sends none, like Desktop; the
39000 visibility is carried by exactly one of `["public"]` (visibility open)
or `["private"]` (`side_effects.rs:1208-1214`; `closed` is on every channel).
A relay requiring membership refuses NIP-42 for a non-member
(`handlers/auth.rs:272-297`, "restricted: not a relay member"), while the
claim route is exempt, so invites are redeemed over HTTP while the helper is
`disconnected`.

- Helper (`helper/src/join.rs`, capability `community_join`): `claim_invite
  {input}` parses the three forms exactly as `inviteHelpers.ts:26-77` (plus a
  code charset `[A-Za-z0-9._-]{1,1024}`), refuses an invite naming another
  relay (`invite_relay_mismatch`), reads `GET /api/join-policy` and publishes
  `status.setup = {state:"policy", inviteCode, joinPolicy:{text, version,
  ageRequired, truncated}|null}`. `accept_invite {code, policyVersion|null}`
  must name that code and version; it accepts the policy (with
  `age_confirmed` = the relay's `age_attestation_required`, which the panel
  states next to **I accept**) and makes the payload-bound NIP-98 claim,
  then `joined` with the validated claim and a joined-room re-check (or a
  reconnect when disconnected).
- `open_rooms`: authenticated `/query` `{"kinds":[39000],"limit":200}`,
  every event checked against the pinned relay signer with the catalog's
  shape rules; `status.openRooms` lists at most 50 `public` stream rooms not
  in the joined catalog. `join_room`/`leave_room {roomId}` publish kind 9021
  (`buzz_sdk::build_join`) / 9022 (`build_leave`), tracked in
  `status.roomAction` and resolved only by the exact-ID `OK`. 9021 rather
  than a 9000 self-add: it is the purpose-built self-join, skips the generic
  membership gate explicitly (`ingest.rs:2621`), is refused before `OK` for a
  private channel (`ingest.rs:3000-3018`), carries no role tag that could
  touch an existing role, and needs no `p` tag. Sole owners are not knowable
  from the helper's views; the relay's refusal (`channel_authz.rs:67-88`, `side_effects.rs:808-815`,
  validated before `OK`) becomes `leave_rejected`, explained in the panel.
- Panel: invite field and **Redeem** in the setup view (also while
  disconnected) and the sidebar footer (connected without rooms, or through
  **+ Join rooms**), the terms with **I accept**, **Open rooms** with **Join**
  and the no-approval note, and **Leave** beside the room's ↻ with a second
  click to confirm.

Still unverified: a real relay's join-policy, acceptance and claim answers,
the NIP-98 `u` binding behind a proxy, rate limiting, and 9021/9022 on the
deployed relay.

## Inviting

The other side of a claim: the relay's owner or an admin mints an invite with
the NIP-98 `POST /api/invites` (`{max_uses, ttl_secs}` → `{code, expires_at,
max_uses, uses_remaining, url}`; no role, claims always grant `member`; no list
or revoke route at this revision). Settings → **Invite people** does this
through the helper's `mint_invite` (capability `invite_mint`) and offers the
two forms `claim_invite` and Desktop accept, `buzz://join?relay=<wss://host>&code=<code>`
and `https://<host>/invite/<code>`, plus a message telling newcomers to install
Buzz for Omarchy (or Buzz Desktop elsewhere), choose the relay, create an
identity and paste the invite. Details in `docs/CHECKPOINT.md`, "Invite people
from Settings".

## Rooms: paging, creation and settings

Capability `room_manage` (branch `rooms-manage`). Desktop references: `commands/channels.rs`
(`create_channel`, `update_channel`, `set_channel_topic`, `add_channel_members`,
`remove_channel_member`), `events.rs` (the builders), `channels/fetch.rs` (`query_relay_all`,
`advance_directory_cursor`). The helper uses the pinned `buzz_sdk` builders and signs with the session key.

**Paging the joined rooms.** The old read was one `{"kinds":[39002],"#p":[me],"limit":20}`: joined rooms and DMs
past 20 were dropped without a word. Now `catalog::discover_pages` reads pages of 50 (`PAGE`), newest first, and
continues with `until` + `before_id` set to the previous page's last `(created_at, id)` (Desktop's composite
cursor; a timestamp alone can skip rows). The pinned relay's `/query` is the same endpoint Desktop pages; this
was **not** run against a live relay here. Bounds: 4 pages (`MAX_PAGES`, 200 rooms), 50 ids per metadata read,
and the status frame bound moved from 1 MiB to 2 MiB (`protocol::RESPONSE_LIMIT`, the panel's frame check)
because 200 worst-case rows alone are about 270 KB. A page newer than its cursor, or over 50 events, rejects the catalog.

- *Load more.* `load_more_rooms` starts `catalog::discover_more` from the cursor the last read returned
  (`hasMore` is "the last page was full", so an exactly full last page offers one empty read). It runs beside the
  background check (the check is cancelled, and cannot start while it runs) and **merges**: a failure sets
  `catalog.more` = `failed` (`room_catalog_timeout` or `room_catalog_unavailable`) and keeps every room;
  success appends the page's rooms and raises the page count. `catalog.more` is `none`, `available`, `loading`,
  `failed` or `limit` (200 held).
- *Refresh without loss.* The 30 s check re-reads `pages` pages. A roster change moves a room's 39002 to the
  front of the order, so a listed room can fall past the last page without having been left. For every listed
  room missing from the pages the helper reads that room's own roster (`#d`, up to 50 per read; a refusal
  asks room by room and a refused room is simply not confirmed) and keeps it only if the relay-signed roster
  still names this identity. Two snapshots of one room across pages keep the newer. Leaving, or being removed, is
  therefore still noticed. `auth.rs` touches: the paging variables, the success arm's `more` field and cursor, the
  `discover_pages` call (timeout 15 s + 5 s per extra page), two job arms and the new commands.
- `refresh_rooms` asks for the check at once (used to wait for a just-created room).

**Create and manage.** One change at a time through the join/leave slot (`join::RoomActions`), answered only by
the relay's `OK` for the exact event id; no `OK` in 15 s, a disconnect or re-authentication is `unknown`.

| Request | Event | Relay rule (side_effects.rs) |
| --- | --- | --- |
| `create_room` (name, about?, visibility) | 9007: `h` new UUID, `name`, `visibility`, `channel_type` `stream`, `about` | any member |
| `update_room` (name and/or about) | 9002: `h` plus only the fields given | owner or admin |
| `set_room_topic` | 9002: `h`, `topic` | any member (the panel offers it to owners and admins) |
| `add_room_member` (key) | 9000: `h`, `p` | owner or admin |
| `remove_room_member` (key) | 9001: `h`, `p` | owner or admin; others only |

The helper checks only what it can know: a joined stream room as target, text bounds (name 128, description 512,
topic 256 bytes, description 1024 for create, edit and detail, no controls; the relay stores names without a leading `#`), a removal naming a member of the
**verified roster** (`fetch_room_detail`) other than this identity (that is leaving), an addition not already on a
fully shown roster. It does not decide permissions: a refusal arrives as `rejected` with `create_rejected`,
`edit_rejected`, `member_add_rejected` or `member_remove_rejected` and the relay's own `OK false` words in
`roomAction.detail` (sanitized, at most 200 bytes, plain text in the panel). A created room's id is the helper's
fresh UUID, reported in `roomAction.roomId`; the panel selects it once the catalog lists it, polling
`refresh_rooms` up to four times 2 s apart because the relay may take a moment (Desktop keeps a "pending owner"
overlay for the same lag). After a change the panel reads the roster again, now and 1.5 s later.

**Room detail.** `fetch_room_detail` reads the room's 39002 and 39000, each relay-signed, scoped to the room,
exactly one of each; the roster must name this identity. `roomDetail` carries the topic (`topic` tag of the 39000),
visibility (exactly one of `public`/`private`), the viewer's role, and up to 100 members (owners and admins first)
with their role words (`owner`, `admin`, `member`, `guest`, `bot`, else `unknown`) and a profile-name hint.
Owners and admins see edit controls from the verified role; everyone else sees the topic and members. The relay,
not this role, is the authority.

**Session scope.** The five mutations (`create_room`, `update_room`, `set_room_topic`, `add_room_member`,
`remove_room_member`) carry `instanceId` and `generation` like `send_message`; the helper refuses a stale or
mismatched scope with `room_scope_changed` in `ipc.rs` before anything is prepared or signed (another panel may
have switched communities first), and a request without them is malformed. The panel shows "Community changed,
nothing was sent." Reads (`fetch_room_detail`) and `join_room`/`leave_room` stay unscoped, as before.

**Editing never rewrites what was not edited.** The catalog row's description is a 256-byte display copy, so an
edit must not round-trip it. `update_room` carries only the fields the user changed (a name-only edit has no
`about` tag); the description's edit baseline is `roomDetail.about`, the relay's full text up to 1024 bytes, and a
longer one (`aboutTruncated`) is shown read-only because it could only be saved back cut.

**Losing a room.** When a catalog check, a recipients/thread/activity read or an access denial removes a room (or
clears the catalog), its `roomDetail` is cleared to `room_detail_access_denied` and any read still running is
cancelled; a read that finishes for an unlisted room is never shown and leaves no `loading` state.

**Activity and notifications with more than 20 rooms.** The activity poll reads one room per 5 s cycle in rotation
(a head read and a replies read; the selected room's head comes from its own refresh). Up to 20 joined rooms that
is unchanged. The tracker used to refuse any room past the twentieth, so those never got indicators or notices;
it now holds every room the catalog can (`activity::ROOMS` = `catalog::MAX_ROOMS`, 200; the panel's activity list
is bounded by the same number, `RoomActivity.MAX_SUMMARIES`, asserted in a test). Past 20 rooms a cycle reads
`ceil(n / 20)` rooms, at most 4 (`activity::BUDGET`, up to 8 reads per cycle), so relay load stays a few queries
per cycle. Rooms are reached in rotation, so activity and notifications for a room can lag by up to
`ceil(n / rooms-per-cycle)` cycles of 5 s (n = 200: 50 cycles, about 4 minutes; n = 21: 11 cycles, under a minute).
A refused room is revoked at once and ends that cycle's reads.

**Not done.** Role changes (Desktop's `change_channel_member_role`), archive/delete, TTL, purpose, visibility
changes of an existing room and forum rooms; showing the topic in the room header; rosters over 100 members.
