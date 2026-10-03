# Communities: join, switch, create — implementation map

Research pass on October 1, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de` (checked with `git -C
~/Projects/buzz log -1` — matches). File:line references with no prefix are
to that checkout. References prefixed `helper/` or `plugin/` are this
project's local working tree (no upstream pin). `origin/main` in the Buzz
checkout is fetched locally but was **not** diffed against the pin (no
network access in this task) — treat anything here as the pin's behavior
only, not necessarily current upstream. This is a map for implementation,
not a statement about the installed plugin; confirm each reference before
relying on it. Desktop's join-dialog copy the task quotes ("Join an existing
community" / "Use the community URL or invite link you received." /
"Community URL or invite link" / "https://community.example.com or paste an
invite link" / "Join community") is verified verbatim below
(`AddCommunityDialog.tsx:85,92`, `InviteRedeemForm.tsx:401-403,417-421,320`).

## 1. Community model in Desktop

A **community = a relay host plus its NIP-29 groups** — not a separate
tenant abstraction with its own identity. The `Community` type
(`desktop/src/features/communities/types.ts:1-27`) is: `id`, `name`,
`relayUrl`, optional `token`, optional `pubkey` (display-only — the comment
at `types.ts:6-10` is explicit: "Display-only — auth always uses the
persisted `identity.key` file resolved at startup, never this field"),
`addedAt`, optional `reposDir` (agent checkout root), and a `nsec?: never`
field kept only so old localStorage entries deserialize; it is stripped on
every read (`communityStorage.ts` `loadCommunities():85-96`) and never
written by new code.

**Storage: `localStorage`, not a Tauri store or per-community keyring
entries.** The full community list is one JSON array under
`"buzz-communities"`, the active id under `"buzz-active-community-id"`
(`communityStorage.ts:6-7`); Buzz used to call these "workspaces" and
migrates `"buzz-workspaces"`/`"buzz-active-workspace-id"` forward once
(`migrateLegacyCommunityStorage`, `:35-62`). There is no per-community
keyring entry and no Tauri `Store` plugin involved in holding the list.

**Identity: one Nostr key for the whole app, never per-community.** The only
identity material is the single on-disk `identity.key` file resolved by the
Tauri backend at startup; `useCommunityInit.ts:330-337` states this
explicitly: "we deliberately do NOT pass an nsec here... The persisted
`identity.key` file... is the single source of truth for the active key...
Older builds stored the nsec in localStorage and re-applied it on every
reload, which silently overwrote any imported key." A community's `pubkey`
field is a stale display cache from when it was added, nothing more.
Switching communities is therefore purely a **relay boundary**, not an
identity boundary, except when the user separately imports a different key
while already in a community (`CommunityIdentityReplacementSentinel`,
`App.tsx:276-307`, tracked by a `signerEpoch` that forces the same full
remount as a community switch).

**Active selection and switching.** `useCommunities()`
(`useCommunities.tsx:168-189`) resolves `activeCommunity` as `communities.find(id
=== activeId) ?? communities[0] ?? null`. The actual switch sequence lives in
`useCommunityNavigationTransitions.switchCommunity`
(`desktop/src/app/useCommunityNavigationTransitions.ts:46-71`): save the
current route as that community's destination, navigate Home first (an
explicit "teardown barrier" so the outgoing channel unmounts before the relay
changes — comment at `:43-44`), mark a pending restore, then call
`communities.switchCommunity(id)`. `App.tsx` builds a composite remount key,
`communityKey = \`${activeCommunity?.id ?? "none"}-${reinitKey}-${currentPubkey
?? "anonymous"}-${signerEpoch}\`` (`App.tsx:416`), and keys `<AppReady
key={communityKey}>` on it so React unmounts/remounts the entire
community-scoped subtree (documented in the Buzz repo's `CLAUDE.md` §
"Community Switching": "Switching communities does **not** reload the page —
it uses React key-based remounting... Module-level singletons must be
explicitly reset").

**What's community-scoped.** `useCommunityInit.ts`'s `resetCommunityState()`
(`:63-103`) is the canonical inventory: it disconnects the relay client,
clears drafts, agent-observer/turn state, avatar caches, media/link-preview
caches, the detached-toast scope, the search-hit cache, the markdown node
cache, and more — "the canonical inventory of community-scoped singletons...
If you add a new module-level cache... add its reset there" (`CLAUDE.md`).
After reset, `applyCommunity(relayUrl, undefined, token, reposDir, …)`
(`useCommunityInit.ts:347-353`) pushes the new relay/token/reposDir to the
Tauri backend, which performs the real reconnect. Identity is explicitly
**not** in that reset list (by design, per the comment above). One
cross-community exception: `communityRelaySetKey`
(`desktop/src/features/communities/communityRelaySet.ts:1-26`) derives the
avatar-trust origin set from **every** configured community's `relayUrl`
(not just the active one), so avatar images may be fetched cross-community
from any community the user has ever added — the one place state is scoped
wider than "active community only."

## 2. Join flow

**Entry point and exact copy.** `AddCommunityDialog.tsx` is a 3-mode dialog
(`"choose" | "create" | "join"`, `:25,36`). The "choose" screen shows two
option rows (`:135-173`): "Create a new community" / "Claim a Buzz address
for your team." and **"Join an existing community" / "Use a community URL or
invite link."** Selecting join sets the dialog title to `"Join an existing
community"` and description to `"Use the community URL or invite link you
received."` (`:81-93` — this is the exact copy the task quotes). The join
mode body is `<InviteRedeemForm variant="add-community" .../>`
(`:176-190`).

**The form** (`desktop/src/features/onboarding/ui/InviteRedeemForm.tsx`, a
shared component also used in first-run onboarding) labels the field
`"Community URL or invite link"` and placeholders
`"https://community.example.com or paste an invite link"`
(`:401-403,417-421` — again the task's exact quote), with a submit button
reading `"Join community"`, or `"Accept and join"` once a join policy
appears (`:316-321`).

**Parsing** (`desktop/src/shared/api/inviteHelpers.ts` `parseInviteInput`,
`:26-77`) accepts three shapes: `https(s)://<relay>/invite/<code>` →
`{relayWsUrl, code}`; `buzz://join?relay=<ws(s) url>&code=<code>` → same; a
bare code (no `://`, no `/`) → `{code}` with no relay. Any credentials or
fragment anywhere invalidates the input. In the `add-community` variant
specifically, when the typed text does **not** parse as an invite at all (or
parses as a bare code), the form falls back to treating the whole input as a
**plain relay URL** via `normalizeRelayUrl`
(`InviteRedeemForm.tsx:94-101,274-283`, from
`features/communities/relayProbe.ts`, not read in depth) and calls
`onConnect(relayUrl)` instead of `onRedeem(...)` — so "Join an existing
community" also accepts a bare community URL with **no invite code at all**,
matching the field's "Community URL **or** invite link" label.

**Network calls on submit** (`handleSubmit`, `:150-261`): `getJoinPolicy`
(`GET /api/join-policy`, matching the relay contract the helper's
`join.rs::fetch_policy` already mirrors) — if a policy exists, the form
shows it and requires age/agreement confirmation before resubmitting;
`acceptJoinPolicy` (`POST /api/invites/accept-policy`) → a receipt; then, for
a parsed invite, `onRedeem(relayUrl, code, receipt)` →
`communityOnboarding.start({source: "add-community", relayUrl, inviteCode,
communityName, policyReceipt})` (`AddCommunityDialog.tsx:53-79`), which hands
off to the onboarding transaction machine
(`features/onboarding/communityOnboarding.tsx`, **not read in depth here**)
that presumably performs the actual `POST /api/invites/claim` and, on
success, adds and switches to the community. For a bare relay URL (no
invite), `onConnect(relayUrl)` → `startConnection({relayUrl})` with no
invite code — i.e. the client can connect to a relay it already has open
access to with no invite step at all.

**On success:** the dialog closes (`handleClose`,
`AddCommunityDialog.tsx:47-51`); the rest (adding to
`useCommunities().communities`, which dedups by `relayUrl`
(`useCommunities.tsx` `addCommunity`, `:191-200+`), switching to it, and
loading rooms through the ordinary `useCommunityInit` → `applyCommunity` →
catalog path from §1) happens inside `communityOnboarding`'s transaction,
which this pass did not trace line-by-line.

**Error states and wording:** `"Finish connecting the community already in
progress, then try again."` (`AddCommunityDialog.tsx:71-74`, a second
onboarding attempt while one is in flight); policy errors `"Confirm that you
are at least 18 years old."` / `"Agree to the Terms of Service and Privacy
Policy."` (`InviteRedeemForm.tsx:177,184`); the spotlight (first-run) variant
shows `"Please enter a valid invite link or community URL"`
(`:436-441`) but the `add-community` variant has **no** equivalent inline
validation message — it relies solely on the disabled submit button; any
other relay/network failure surfaces as the raw thrown error's message
(`inviteErrorMessage`, `:189-194,244-248`) — not a fixed friendly string.

## 3. Create flow

**"Create a new community"** → `<HostedCommunityCreateFlow
onComplete={handleClose}/>` (`AddCommunityDialog.tsx:191-192`); the dialog's
sr-only description for this mode is literally `"Opens Builderlab in your
browser."` (`:89-90,126`).

**Hosted via Builderlab, not literally "buzz.xyz."** The actual API host at
this pin is `app.builderlab.xyz`:
`desktop/src-tauri/src/builderlab.rs:15` declares
`BUILDERLAB_API_BASE_URL = "https://app.builderlab.xyz/api/goose"` and `:22`
`BUILDERLAB_ORIGIN = "https://app.builderlab.xyz"`. `buzz.xyz` is the
public-facing brand/support-doc name (and is what this project's own
`DESIGN.md:316-317` and the plugin's hosted-setup button already point at —
see §6); the Tauri commands talk to Builderlab's API directly. Worth
correcting in this project's own docs where they say `*.communities.buzz.xyz`
is the API — that's the resulting relay hostname pattern, not the control
API's host.

**Auth is a loopback OAuth-style flow, no embedded webview.**
`start_builderlab_login` (`builderlab.rs:250-365`): binds an ephemeral
`127.0.0.1` TCP port, serves one Axum route `/callback/{nonce}`
(`:271-273`), opens the system browser (via `tauri_plugin_opener`) to
`{BASE}/v1/auth/login?type=cli&product=buzz&returnTo=http://127.0.0.1:{port}/callback/{nonce}`
(`login_url`, `:217-225`), waits up to `LOGIN_TIMEOUT = 10 min` (`:16`) for
the callback's `?code=`, then `POST`s `{code}` to `/v1/auth/login/exchange`
(`:320-327`) for a `session_credential`, and verifies it against `GET
/v1/auth/me` (`:342-345`, also checking `expires_at` matches). The session
credential lives **only in memory** — `Mutex<Option<StoredSession>>`
(`:142-155`) — never persisted to keyring or disk; restarting the app
requires signing in again. Every authenticated call sends it in a custom
header, `X-BB-Session-Credential` (`:17`), plus an explicit `Origin:
https://app.builderlab.xyz` (`:438-441`, needed because Builderlab enforces
an Origin check that only a real browser sends automatically — comment at
`:18-21`).

**Identity binding requires the local Nostr key to sign a non-NIP
challenge.** `bindBuilderlabIdentity` (`hostedCommunityApi.ts:193-195`;
`builderlab.rs:481-524`): `POST /v1/buzz/nostr-identities/challenge` →
`{challenge_id, nonce, verification_code, origin, expires_at}`; the LOCAL
identity key signs `build_nostr_identity_binding_event` (referenced at
`:504-511`; its construction lives in `commands.rs`, not read); that signed
event is `POST`ed to `/v1/buzz/nostr-identities/verify`. This is a
Builderlab-specific challenge shape, not a NIP.

**Create request.** `createHostedCommunity(name)`
(`hostedCommunityApi.ts:208-215`) → `POST /v1/buzz/communities {name}`
(`builderlab.rs:572-586`). Name rule client-side:
`VALID_HOSTED_COMMUNITY_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/`, 1–63 chars
(`hostedCommunityApi.ts:7`), live-checked via `POST
/v1/buzz/communities/availability` with a 500 ms debounce
(`HostedCommunityCreateFlow.tsx:215-241`). Client-enforced limit:
`HOSTED_COMMUNITY_LIMIT = 5` (`hostedCommunityApi.ts:6`) — DESIGN.md flags
that source and public docs disagree on the real server-side number, so
don't embed a quota promise. Response shape:
`{community: {id?, name?, slug?, normalized_host?, owner_pubkey?,
archived_at?}}` (`:32-39,54-58`); the relay URL is derived client-side as
`wss://<normalized_host>` (`hostedCommunityRelayUrl`, `:90-93`). Only field
asked of the user: the name (becomes
`<name>.communities.buzz.xyz`, `HOSTED_COMMUNITY_SUFFIX`, `:5`). On success,
create hands off to the **same** `communityOnboarding.start({source:
"add-community", relayUrl, communityName})` transaction the join flow uses
(`HostedCommunityCreateFlow.tsx:280-284`) — create-then-connect reuses §2's
success path exactly.

**Can a native third-party client call this at all?** Mechanically, yes —
it's plain HTTPS, no Tauri-exclusive transport. But: it needs a human
browser-based OAuth login against an undocumented Builderlab endpoint; it
needs to reproduce a bespoke (non-NIP) identity-binding challenge whose
payload shape lives in Tauri `commands.rs` (not inspected here); and this
project has **already decided this is out of scope**, in its own design
record: `DESIGN.md:313-317` — *"These desktop commands are not a supported
local API for an external Omarchy plugin. The official support guidance
directs users to buzz.xyz and explains invitation-based access... Initially
the hosted option opens the fixed official onboarding site only on a user
click, then connects to the resulting community address after upstream
account/invitation setup and explicit secure identity enrollment."* That
decision is already implemented for first-run setup (§5/§6) — the create
flow for a *second* community should do nothing more than extend the same
pattern, not attempt the Builderlab API.

## 4. Sidebar/switcher UI

Two surfaces. **`CommunityRail.tsx`** (far-left icon rail,
`desktop/src/features/sidebar/ui/CommunityRail.tsx`) renders **only when
`communities.length > 1`** — "Hidden entirely with a single community — a
rail of one adds no value" (doc comment at `:305`; guard at `:337`).
**`CommunitySwitcher.tsx`** (`desktop/src/features/communities/ui/CommunitySwitcher.tsx`)
is the account-menu/sidebar-header dropdown, used in three `variant`s:
`"sidebar"` (the normal sidebar header dropdown), `"profile"` (compact, under
the avatar), `"profile-menu"` (the full account-menu popover with
join/leave/settings actions, the Desktop-equivalent of what the task calls
"account menu / sidebar top").

**Icons vs. names — different sources.** Icons come from each community's
relay's NIP-11 document `icon` field, fetched for **every** configured
community (not just active), 5-minute `staleTime`, with a localStorage cache
fallback for unreachable relays (`useCommunityIcons.ts:16-54`,
`fetchCommunityIcon` → Tauri `fetch_workspace_icon` →
`communityProfile.ts:26-33`). That `icon` isn't a bare NIP-11 field upstream
either — it's a Buzz-custom extension: an admin/owner publishes kind:9033
(`KIND_SET_COMMUNITY_PROFILE`, `communityProfile.ts:19`, doc comment
`:1-13`) and the relay republishes it into its own NIP-11 document so
**inactive** communities' icons render without an authenticated fetch. With
no icon, the rail/switcher fall back to initials-from-name or a 🐝 emoji
(`CommunityRail.tsx:149-159`, `CommunitySwitcher.tsx` `CommunityEmojiIcon`
`:72-99`). By contrast `community.name` is a **purely local** label — either
user-typed or derived client-side from the relay hostname at add-time
(`deriveCommunityName`, `communityStorage.ts:188-210`) — never read live from
NIP-11 `name`.

**Unread.** `communityRailIndicators`
(`CommunityRail.tsx:67-86`) derives either a numeric mention-count badge
(capped `"99+"`) or a plain unread dot, strictly gated on `unread.state ===
"ready"` — "the `state` guard ensures we NEVER render any indicator for a
relay we could not observe (`unknown`/`loading`/`error`)" (`:60-65`).

**Ordering.** User-draggable via `@dnd-kit`
(`DndContext`/`SortableContext`, `CommunityRail.tsx:1-17`), persisted through
`applyCommunitiesOrder` (`useCommunities.tsx:93-116`) — a pure
reorder-by-id-list helper; ids not mentioned in the new order keep their
relative position, appended at the end.

**Removal ("Leave community").** Only in the `"profile-menu"` variant's
account-menu popover — a destructive item `"Leave community"`
(`CommunitySwitcher.tsx:324-333`). `CommunityRail`'s own right-click menu has
**no** leave option (only Mark all as read / Copy community URL / Invite to
community / Community settings, `:264-283`) — leaving is account-menu-only.
`handleLeaveCommunity` (`:163-191`) → `onRemoveCommunity(id)` →
`useCommunityNavigationTransitions.removeCommunity`
(`useCommunityNavigationTransitions.ts:76-130`): it **first** calls
`leaveCommunity(relayUrl, activeRelayUrl)`
(`desktop/src/features/communities/leaveCommunity.ts:55-88`), which signs and
publishes a **kind 28936** NIP-43 leave request (`KIND_NIP43_LEAVE_REQUEST`,
`:7`, tags `[["-"]]`) — but only if the relay actually requires membership
(`relayRequiresMembership`, `:60-62`; otherwise it trivially returns
`{status: "left"}`). The local list entry is removed only **after** the
relay accepts the leave (or reports "already not a member," surfaced as a
toast: *"Community removed — You were no longer a member, so Buzz removed
the community from this device."*, `CommunitySwitcher.tsx:176-179`).

**Identity on leave.** Unaffected. `leaveCommunity` only revokes relay
membership; it never touches `identity.key` or any keyring secret — per §1,
identity was never community-scoped, so there's nothing to clean up. If the
removed community was active and no fallback community remains, the app
falls into a "find a new community" discovery state
(`markCommunityDiscoveryAfterLeave`,
`useCommunityNavigationTransitions.ts:95-104`) rather than losing the
identity.

## 5. What this plugin has today

`helper/src/config.rs` holds **exactly one** relay/identity pair —
`pub struct Config { pub relay: Option<String>, pub identity: Option<String>
}` (`:9-14`) — serialized to a single `~/.config/omarchy-buzz/config.toml`
(`dir()`/`load`/`save`, `:44-103`). There is no list of communities anywhere
in the helper today. `canonical_relay` (`:16-43`) is the single source of
truth for "what counts as a relay URL" — `wss://` required (plain `ws://`
only for `localhost`/`127.0.0.1`/`[::1]`), no userinfo/query/fragment, path
forced to `/`. `with_relay(config, relay)` (`:104-112`) is the *first-time
setup* semantics: changing the relay **clears** `identity` (an identity is
scoped to its relay by that function's contract) — this is what `omarchy-buzz
setup relay` and the panel's `set_relay` IPC request use.

**But there's already a second, narrower relay-change path that does NOT
clear identity:** `setup::switch_community`
(`helper/src/setup.rs:66-109`). It keeps the already-loaded `nostr::Keys`,
canonicalizes the new relay, refuses if nothing changed about the source
account (`setup_busy` if the config moved under it, `:75-79`), refuses if
the new relay's own NIP-11 signer key *is* this identity
(`identity_unavailable`, `:86-88` — can't be both the community's relay
signer and a member), then writes `Config { relay: Some(new), identity:
original.identity }` and **re-stores the same secret under the new
`relay|identity` keyring account** (`config::account`, `config.rs:113-119`
— `"{relay}|{identity}"`) *before* persisting the config change (comment at
`setup.rs:101-103`: "Store before saving: a failed keyring write never
abandons the old relay"). Crucially, it never calls `secrets.forget()` on
the **old** account — the old relay's keyring entry is left behind,
unreferenced by `config.toml` but still present in Secret Service. This is
already the right primitive for "same identity, new relay," it's just not
exposed as a list — the helper (and the keyring) can already hold more than
one relay's worth of the same identity's secret; `config.toml` just only
ever points at one of them at a time.

This is wired end-to-end today: `ipc.rs:119,189,210` route the
`"switch_community"` request kind to `protocol::Command::SwitchCommunity`;
`protocol.rs:81-99` lists it among accepted request kinds, and `:160,215`
gate it (along with `send_message`/`open_dm`) as a "publishing" request that
must carry a correlation UUID, a `generation`, and an `instanceId`
(`:156-176`). `auth.rs::offline_command` (`:317-342`) handles
`Command::SwitchCommunity(url, generation, reply)`: it checks the caller's
`generation` still matches current state and that no send/DM/upload is
mid-flight (`:329-335`), then calls `setup::switch_community`. The actual
reconnect happens through `apply_loaded_config` (`auth.rs:152-184`): any
change to `relay`/`identity` bumps `status.generation` and resets **every**
status sub-view (`delivery`, `dm_open`, `catalog`, `history`, `thread`,
`activity`, `recipients`, `setup`, `open_rooms`, `room_action`, `download`,
`thumbnails`, `pending_attachments`, `upload`, `user_status`, `presence` —
`:158-183`) — this is the helper's equivalent of Desktop's
`resetCommunityState()` inventory in §1, and it's already comprehensive.

On the plugin side, `plugin/Service.qml` already exposes this as **"Join a
new community"** (`plugin/PanelContent.qml:1424-1470`,
`buzzCommunitySwitchSection`): one `Ui.TextField` (`communityUrlField`,
placeholder `"wss://your-community.example"`) and a **"Join community"**
button, calling `service.switchCommunity(url)`
(`plugin/Service.qml:503-512`), gated by `communitySwitchAvailable`/
`canSwitchCommunity` (`:445,455,457`) and a note: *"Switch the active relay
using your existing Buzz identity. You may need an invitation before you
can join the new community. Room drafts here will be cleared when you
switch; agent services stay configured for their current relay."*
(`PanelContent.qml:1433`). **This is a raw relay-URL switch, not an invite
flow** — it does not parse `https://…/invite/<code>` or `buzz://join?…`, has
no join-policy step, and does not call `join.rs`'s claim machinery at all.

Separately, `helper/src/join.rs` (`community_join` capability, documented in
`docs/JOIN_MAP.md`) implements the real invite-claim contract (`parse_invite`
`:94-168`, matching Desktop's `inviteHelpers.ts` grammar exactly down to the
`buzz://join?relay=&code=` and `/invite/<code>` shapes; `fetch_policy`
`/api/join-policy`; `accept_policy` `/api/invites/accept-policy`; `claim`
`/api/invites/claim`, NIP-98 signed). But `check_relay`
(`join.rs:172-178`) enforces: *"The invite's relay, if named, must be the
configured relay... The panel never switches relays"* — an invite naming a
**different** relay than `config.toml`'s current one is refused
(`invite_relay_mismatch`), full stop. So today, claiming an invite can only
grow membership on the relay you're *already* configured for; it cannot
bring you into a brand-new community the way Desktop's
`AddCommunityDialog` → `InviteRedeemForm` does.

`catalog.rs`'s NIP-11 reader, `relay_info`/`relay_signer`
(`:94-119`, `info_signer` `:43-58`), extracts **only** the `self` field (the
relay's signing pubkey, used for TOFU pinning) from the NIP-11 document.
Grepping the file for `"name"`/`"icon"` turns up nothing from the NIP-11
path — **the helper reads no `name` or `icon` from NIP-11 today.** (The two
`"name"` hits in the file are an unrelated `one_tag(event, "name")` check on
room-metadata events, not NIP-11.) A community switcher with icons/names per
§4 needs this added from scratch.

`auth.rs`'s `generation` counter (bumped in `apply_loaded_config`,
`:162`) is already the exact fencing mechanism described in `CLAUDE.md`'s
review-proven rule 2 ("Fence async results by generation") and is reused
throughout `auth.rs` (dozens of call sites comparing a captured `generation`
against `status.generation` before accepting a stale async result,
e.g. `:935-965,1381-1416,1573-1597,1682-1825`) — this is the one piece of
infrastructure that already generalizes cleanly to "N communities, one
active, fence everything on the active one's generation."

`docs/JOIN_MAP.md` (this project, same pin) already describes the identity
model correctly ("Identity: a keypair is generated on first run... No kind 0
profile is published by onboarding" — `JOIN_MAP.md:9-12`) and already notes
the hosted-community gap: *"Hosted communities live at
`*.communities.buzz.xyz`... and are created or listed through a browser
OAuth login driven by Tauri commands; there is no public HTTP API a native
client can call for that. DESIGN.md already decided to open buzz.xyz in the
browser instead."* (`JOIN_MAP.md:13-17`) — this pass's §3 above fills in the
mechanism (Builderlab, not literally `buzz.xyz`) behind that same
conclusion. `docs/CHECKPOINT.md`'s "Onboarding step one" (`:1996-2032`) and
"Onboarding step two and agent direct messages" (`:2097-2135`) sections
describe `set_relay`/`create_identity`/`claim_invite`/`accept_invite`/
`open_rooms`/`join_room`/`leave_room` exactly as read in `setup.rs`/`join.rs`
above, all as **synthetic-only, not installed, not run against a real
relay** evidence.

`plugin/Service.qml:16,422-424,1345-1347` already has a `setupProvider`
property (`"hosted" | "custom"`, presentation-only — "choosing a provider
never writes config or sends IPC") driving copy text, and
`plugin/PanelContent.qml:1788,1791` already has a working **"Open Buzz
hosted setup"** button that does `Qt.openUrlExternally("https://buzz.xyz")`
— this is
already the exact "honest browser handoff" pattern DESIGN.md mandates,
already implemented, just scoped to *first-run* setup only.

**Summary of what must change for N communities:** (1) `config.rs`'s
`Config` needs a list, not a single `relay`/`identity` pair — a
`communities: Vec<{relay, identity, name?, addedAt}>` plus an `active`
pointer, analogous to Desktop's `buzz-communities`/`buzz-active-community-id`
pair but in `config.toml`, and `with_relay`'s "changing relay clears
identity" semantics need to become "adding a community never touches
others." (2) Keyring entries already key correctly
(`relay|identity`, `config.rs:113-119`) and already survive a relay change
(`setup.rs` never forgets the old one) — this needs no new shape, just an
explicit `forget()` wired to an actual "leave" action (today nothing ever
calls `secrets.forget`, so switching leaves orphaned entries silently;
that's fine as a cache but should be deliberate, not accidental, once there's
a real "leave community" action). (3) No per-community `view.json` exists in
this helper at all (unlike `draft`/`avatars.json`-style per-key stores
referenced in `docs/CHECKPOINT.md`'s ANSI-art section) — any UI state that
should survive a switch (e.g. "last room viewed per community," mirroring
Desktop's `communityNavigationStorage.ts`) would be new, community-scoped
QML-local storage, not a helper concern. (4) Catalog/activity state is
already generation-scoped per §5 above and resets wholesale on any
relay/identity change (`apply_loaded_config`) — extending this to "N
communities, only the active one's generation is live" needs no new
mechanism, just confirmation that a community switch always goes through
`apply_loaded_config` (it does, via `config::save_to` + the config
watcher — not traced to its exact watcher call site in this pass).

## 6. Proposal

**Scope.** Match Desktop's two entry points from a single "Add a community"
surface, in the task's stated order: **Join an existing community** first,
**Create a new community** second. Do not attempt to replicate Desktop's
full switcher chrome (drag-reorder, per-community token/reposDir editing,
icon settings cards) — build the minimum that lets someone add a second
community, see which is active, and switch, matching Desktop's *behavior*
contract (same identity, generation-fenced switch, invite claim reused) more
than its exact visual layout.

**Join.** Reuse `helper/src/join.rs` almost entirely — its
`parse_invite`/`check_invite`/`redeem` already implement the identical
grammar and HTTP contract Desktop's `inviteHelpers.ts`/`invites.ts` use
(§2, §5). The one required change is `check_relay`
(`join.rs:172-178`): today it hard-refuses an invite naming a relay other
than the single configured one. For a multi-community world this becomes
"if the input is a bare code, target the currently-selected relay (as
today); if it names a relay (URL or `buzz://join?relay=…`) that is **not**
already in the community list, that's a *new-community* claim — validate
the relay with `catalog::relay_signer` (TOFU-pin it, exactly like
`setup::create_identity` already does at `setup.rs:130-144`) before ever
presenting a join-policy or attempting a claim, then run the existing
`check_invite`/`redeem` sequence against that new relay with the **same**
already-loaded identity keys (never generate a new key — see §1/§5: Desktop
never does, and nothing here should diverge from that)." On success, append
`{relay, identity: same_hex, name: derived_or_user_entered}` to the
community list, store the secret under `relay|identity` in the keyring
(already idempotent — `setup::switch_community`'s pattern, `setup.rs:89-106`),
and switch active to it — which is exactly `setup::switch_community`'s
existing "retain identity, write new relay, bump generation" sequence
(§5), just preceded by an invite claim instead of a bare URL. For a bare
community-URL join with no invite code (Desktop's `onConnect` fallback, §2),
skip the claim step and go straight to `switch_community`'s path — this is
*already implemented* as today's "Join a new community" raw-URL switch
(`Service.qml:503-512`); it only needs to become "add to a list" instead of
"replace the one config."

**Switching restarts the connection generation**, already true today
(`apply_loaded_config`, `auth.rs:152-184`) — no new mechanism needed, only
confirming every new community-list write path goes through the same config
save → generation bump → full status reset sequence that `switch_community`
already triggers.

**Identity: the same key across all communities, unless Desktop does
otherwise — it doesn't (§1).** Never generate a per-community key; never
prompt for one when joining. The only identity-related decision a join flow
makes is the TOFU pin check already present (`relay_signer` refusing if the
new relay's signer key equals the local identity, `setup.rs:86-88` — can't
join a relay you'd be signing as).

**Bounds / what must be verified**, mirroring the project's existing bar for
untrusted relay-sourced data (same posture `PRESENCE_MAP.md` §6 and
`JOIN_MAP.md` already hold the helper to): NIP-11 `name`/`icon` (new —
§5 found neither is read today) must be treated as **untrusted display
labels**, never raw-rendered without the same control-character/bidi
stripping `recipients.rs`'s `name()` already applies to profile names
(`recipients.rs:92-99`, cited in `PRESENCE_MAP.md` §6); the icon, if a
`data:` URL, needs a size cap before it ever reaches QML (the relay can
serve "inline community icons larger than 32 KiB" per `catalog.rs:13-14`'s
own comment about `INFO_BYTES`) — do not forward an unbounded blob into the
QML image model. The relay URL stored per community must always be
`config::canonical_relay`'s output (`config.rs:16-43`) — never the raw user
input — so the community list can never silently hold two entries that
differ only by trailing slash or scheme case. No credentials, session
tokens, or raw private keys belong in QML's data model at any point (per
`AGENTS.md`'s standing rule) — the community list QML holds is `{relay,
name, icon_url_or_null, active: bool}` only; the keyring account string
(`relay|identity`) is a helper-internal concept QML never sees.

**Create.** Per §3's DESIGN.md citation, this project has already decided
the Builderlab API is out of scope for a native client. The honest,
already-implemented pattern to extend is `plugin/PanelContent.qml`'s
existing **"Open Buzz hosted setup"** button
(`Qt.openUrlExternally("https://buzz.xyz")`) and `Service.qml`'s
`setupProvider` copy (`:422-424`) — reuse this verbatim for "Create a new
community" in the Add-a-community surface: a button that opens
`https://buzz.xyz` in the system browser, with copy matching Desktop's own
framing ("Claim a Buzz address for your team" — `AddCommunityDialog.tsx:149`)
plus a note that after creating it there, the user comes back and uses
**Join an existing community** with the resulting `wss://<name>.communities.buzz.xyz`
URL — i.e. "Create" in this plugin is not a separate code path at all, it's
"Join," preceded by a browser handoff. Do not build a Builderlab OAuth
loopback server, do not implement the Nostr-identity-binding challenge, and
do not embed the `HOSTED_COMMUNITY_LIMIT = 5` or any other Builderlab quota
number from `hostedCommunityApi.ts` — DESIGN.md already flags that number as
unreliable.

**Helper modules to touch:** `helper/src/config.rs` (community list +
active pointer, replacing the single `relay`/`identity` pair; keep
`canonical_relay`/`account` as-is); `helper/src/setup.rs`
(`switch_community` becomes "switch to an existing list entry" instead of
"replace the one relay"; a new `add_community`/`leave_community` pair
alongside it, `leave_community` wired to an explicit `secrets.forget()` —
today nothing calls it); `helper/src/join.rs` (`check_relay` relaxed as
described above; no change to `parse_invite`/`fetch_policy`/`accept_policy`/
`claim`, which already match Desktop's contract exactly); `helper/src/
catalog.rs` (new `name`/`icon` extraction from the NIP-11 document, bounded
and sanitized as above — net-new, nothing to reuse); `helper/src/protocol.rs`
(new request kinds, e.g. `add_community`/`leave_community`, following the
existing `switch_community` gating pattern at `:105,160,215` — correlation
UUID + `generation` + `instanceId`); `helper/src/auth.rs` (no new mechanism —
confirm every new path still goes through `apply_loaded_config`'s
generation bump and full status reset, `:152-184`).

**QML files to touch:** `plugin/Service.qml` (replace the single
`communitySwitchSupported`/`switchCommunity(url)` properties with a
community-list model and `addCommunity`/`switchCommunity(id)`/
`leaveCommunity(id)`, keeping the existing `generation`/`instanceId` framing
already used for `switch_community`, `:503-512`); `plugin/PanelContent.qml`
(replace `buzzCommunitySwitchSection`'s single text field with an
"Add a community" entry point offering the two options in the task's
order — Join first, Create second — reusing `InviteRedeemForm`-equivalent
parsing client-side-free, since the helper already does all parsing/
validation; and a per-community row list with active indicator, matching
the *behavior* of `CommunitySwitcher.tsx`'s account-menu popover more than
its exact styling); a new small QML component for the community list/rail
only if more than one community makes an always-visible switcher worthwhile
— Desktop's own rule ("Hidden entirely with a single community," §4) applies
here too, so this can be deferred until `add_community` actually exists and
is exercised.

## Not read in depth

- `desktop/src/features/onboarding/communityOnboarding.tsx` — confirmed its
  `start({source, relayUrl, inviteCode, communityName, policyReceipt})`
  entry point and that both join and create hand off to it, but did not
  trace its transaction state machine to the exact point it calls
  `useCommunities().addCommunity`/`switchCommunity`.
- `desktop/src/features/communities/relayProbe.ts` (`normalizeRelayUrl`
  used by the add-community bare-URL path) and `communityIconCache.ts` —
  confirmed their call sites, not their internals.
- `desktop-tauri/src/commands.rs`'s `build_nostr_identity_binding_event` —
  confirmed it exists and is called from `builderlab.rs:504-511`, did not
  read the exact signed-event shape.
- The helper's config-file watcher that turns a `config::save_to` into an
  `apply_loaded_config` call — confirmed the two ends (`setup.rs` writes,
  `auth.rs:152-184` applies) but not the exact plumbing between them.
- Desktop's `useMyRelayMembershipLookupQuery` (role gating for "Invite to
  community" in `CommunityRail.tsx`/`CommunitySwitcher.tsx`) — out of scope
  for this map (invite *sending*, not joining, is already covered by this
  project's `docs/JOIN_MAP.md` "Inviting" section).

## Addendum: rooms inside a community

Room paging, creation and settings (`room_manage`) are specified in
[JOIN_MAP.md](JOIN_MAP.md#rooms-paging-creation-and-settings). For a community that holds more than 20
joined rooms and direct messages, the first page of 50 is listed and **Load more rooms** reads further pages (up to
200); the background check keeps every loaded page. **+ New room** and **Room settings** act on the active
community only, with that community's relay deciding who may create, rename or manage members; switching
community resets them like every other room view.
