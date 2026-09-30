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
