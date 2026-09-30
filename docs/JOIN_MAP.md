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
