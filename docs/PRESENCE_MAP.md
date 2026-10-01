# Presence & "Update your status" — implementation map

Research pass on September 30, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de` (this checkout's `HEAD` resolves to
that commit; verified with `git rev-parse HEAD`). File:line references are to
that checkout unless prefixed `helper/`, which is this project's local
`~/Projects/omarchy-buzz/helper/src` (no upstream pin — current working
tree). This is a map for implementation, not a statement about the installed
plugin; confirm each reference before relying on it. The Desktop account
popover's "Update your status" entry exists upstream and is in scope to
match (ProfilePopover.tsx:239, quoted below).

## 1. Event kinds and tag shapes

Two independent, unrelated mechanisms share the word "status" upstream — do
not conflate them:

**(a) User status text/emoji — NIP-38, kind 30315 (`KIND_USER_STATUS`).**
Parameterized-replaceable (30000–39999 range), keyed by `(pubkey, kind,
d_tag)`; stored globally (`channel_id = NULL`), user-owned, not
channel-scoped (`crates/buzz-core/src/kind.rs:67-70`). Buzz pins the
coordinate to a single `d:general` tag — this is a Buzz convention on top of
NIP-38 (which also defines a `music` d-tag Buzz does not use):
`build_user_status` in `crates/buzz-sdk/src/builders.rs:1911-1924` always
emits `["d","general"]`; the Desktop subscription filter hardcodes `"#d":
["general"]` (`desktop/src/shared/api/relayClientSession.ts:408`). Content is
the status text (plain string, not JSON); an optional `["emoji", <string>]`
tag carries the emoji (native glyph or custom `:shortcode:`,
`crates/buzz-sdk/src/builders.rs:1920-1922`); an optional `["expiration",
<unix-secs>]` tag (NIP-40 shape) carries expiry
(`desktop/src/shared/api/relayClientSession.ts:388-394`). Clearing status is
modeled as a replacement event with empty content and no `emoji` tag — NIP-33
semantics mean the latest event *is* the current state, so "clear" is just
"publish empty" (`builders.rs:1913-1916`, confirmed by test
`user_status_clear_shape_is_empty_content_and_d_tag_only`,
`crates/buzz-sdk/src/builders.rs` test module ~4655-4662). Ingest treats
30315 as global-only (`is_global_only_kind`,
`crates/buzz-relay/src/handlers/ingest.rs:693`, listed again at `:3443`) and
requires `Scope::UsersWrite` (`ingest.rs:503`) — the same scope as kind:0
profile and kind:3 contacts, i.e. ordinary user-owned-state write
permission, no special presence/status scope. Relay content-size cap is the
generic `MAX_EVENT_CONTENT_BYTES = 256 * 1024` (`ingest.rs:2333-2339`); the
SDK builder additionally self-enforces 64 KiB (`check_content(text, 64 *
1024)`, `builders.rs:1918`, test `user_status_rejects_oversize_text`). There
is **no dedicated NIP-38 doc under `docs/nips/`** — grep of that directory
found no `NIP-38.md` or presence doc; the kind-registry doc comment in
`kind.rs:67-70` is the only in-repo spec. **The relay does not enforce or
purge the `expiration` tag** — no NIP-40 handling exists for kind 30315
anywhere in `buzz-relay` (grepped `"expiration"` across
`crates/buzz-relay/src` and `crates/buzz-core/src`; the only live NIP-40-style
checks are for unrelated kinds: reminders `ingest.rs:2053-2077`, moderation
timeouts `moderation_commands.rs:568`, push leases `push_lease.rs:113-116`,
and signed media URLs `api/media.rs:1308-1875`). Expiry for 30315 is a
**client-side convention only**: Desktop reads the tag, compares to wall
clock, and locally treats an expired status as cleared
(`desktop/src/features/user-status/hooks.ts:67-70, 150-154` — `statusIsExpired`
/ `expireUserStatusQueries`). An attacker-controlled or stale relay could
replay an "expired" status forever; nothing server-side stops it.

**(b) Online/away/offline presence — Buzz-custom, kind 20001
(`KIND_PRESENCE_UPDATE`).** Ephemeral range (20000–29999): never stored, WS-only
(`crates/buzz-core/src/kind.rs:465`; `ARCHITECTURE.md:140` table entry
"Ephemeral presence heartbeat"). Not a NIP at all — no NIP-OA/NIP-38
"general" overlap; it is Buzz's own kind. Content is a bare string, one of
`"online" | "away" | "offline"` (validated in
`crates/buzz-sdk/src/builders.rs:1898-1910`, `build_presence_update`); the
same value is duplicated into a `["status", <value>]` tag "for structured
access" (`builders.rs:1908`). No `d` tag, no expiry tag — ephemeral kinds are
never NIP-33-replaceable, so there's nothing to replace; current state lives
only in the relay's live Redis cache (see §4), not in any event history. A
second kind exists for relay-synthesized snapshots used over HTTP/REST
polling: `KIND_PRESENCE_SNAPSHOT = 40902` (`kind.rs:505`), part of the
40000+ "Buzz custom kinds" range, not ephemeral — it's the kind the relay
*answers with* when a REST client queries presence (see §4); it is not a
kind clients publish.

## 2. How Desktop publishes a status

Entry point: the account popover built in
`desktop/src/features/profile/ui/ProfilePopover.tsx`. The "Update your
status" row (line 239) is a plain button that opens
`SetStatusDialog` (`ui/SetStatusDialog.tsx`) via
`setStatusDialogOpen(true)` (`ProfilePopover.tsx:208-213`); when a status is
already set, the same button instead shows the current `StatusEmoji` +
`userStatusText` and still opens the dialog on click
(`ProfilePopover.tsx:214-237`). `SetStatusDialog` offers five canned presets
("In a meeting", "Commuting", "Out sick", "Vacationing", "Working remotely",
each with a fixed emoji, `SetStatusDialog.tsx:28-34`), a custom-emoji picker
(`EmojiPicker`), free-text, and a duration chooser ("1 hour" / "8 hours" /
"Today" / "This week" / "Custom" — `SetStatusDialog.tsx:36-42`) that computes
an absolute `expiresAt` from the duration. On save it calls
`onSave({text, emoji, expiresAt})` (`UserStatusInput`,
`desktop/src/features/user-status/types.ts`), wired through
`ProfilePopover` → `AppShell.tsx:907` → `useSetUserStatusMutation`
(`desktop/src/features/user-status/hooks.ts:419-461`). That mutation calls
`relayClient.publishUserStatus(status)`
(`desktop/src/shared/api/relayClientSession.ts:388-401`), which builds tags
directly (`["d","general"]`, optional `["emoji", …]`, optional
`["expiration", …]`) rather than going through the `buzz-sdk`
`build_user_status` builder — Desktop and the Rust SDK/CLI independently
construct the same tag shape; they are consistent today but not
code-shared. Signing itself is **not done in TypeScript**: `signRelayEvent`
(`desktop/src/shared/api/tauri.ts:543-561`) calls `invokeTauri<string>
("sign_event", input)`, i.e. the Tauri/Rust backend holds the identity key
and signs; the TS layer only assembles `{kind, content, tags}` and parses the
signed JSON back. Clear is the same mutation with blank text/no emoji
(`ProfilePopover.tsx` "Clear status" button at `SetStatusDialog.tsx:361`,
wired to `onClearUserStatus` → `AppShell.tsx:908-909`). There is **no
server-side text-length or emoji-shape validation on the Desktop input
path** — `SetStatusDialog.tsx` has no `maxLength` on the text field (grepped
`maxLength|MAX_` in that file: none found); the only caps are the relay's
generic 256 KB content ceiling and the SDK's 64 KB self-check, neither of
which Desktop's direct-tag-building publish path goes through (Desktop talks
straight to the relay via `signRelayEvent` + `publishEvent`, bypassing
`buzz-sdk`'s `check_content`). A pathologically long status that still fits
under 256 KB would reach the relay and be stored.

Presence (online/away/offline) is **not** manually "published" by the user
via a Desktop status action in the same sense — see §4 for how it's set
automatically, and `ProfilePopover.tsx`'s separate "online/away/offline"
dropdown (`ALL_STATUSES`, `ProfilePopover.tsx:58`) which calls
`handlePresenceSelect` → `onSetStatus` → `useSetPresenceMutation`
(`desktop/src/features/presence/hooks.ts:198+`), which itself calls
`buzz-sdk`'s `build_presence_update` indirectly through the same
`signRelayEvent`/WS publish path (ephemeral kinds must go over the WS
connection — HTTP rejects them, see §5).

## 3. How Desktop reads/displays others' status and presence

**Status (30315).** `useUserStatusQuery` (`hooks.ts` ~270-310) backs a
`useQuery` keyed `["user-status", ...sortedPubkeys]`; the query function
`fetchUserStatusLookup` chunks authors into groups of
`USER_STATUS_AUTHOR_CHUNK_SIZE = 1_000` and issues
`{kinds:[KIND_USER_STATUS], authors, "#d":["general"], limit:authors.length}`
fetches (`hooks.ts:198-250`). A shared
`UserStatusLookupProvider`/`useUserStatusLookupContext`
(`desktop/src/features/user-status/UserStatusLookupContext.tsx`) batches
every mounted `UserNameIndicators` instance's pubkey into one query via a
ref-counted `register()` map, so the member list, a channel header, and
message rows don't each fire independent fetches. Live updates arrive via
`relayClient.subscribeToUserStatusUpdates`
(`relayClientSession.ts:404-409`, `{kinds:[KIND_USER_STATUS], "#d":
["general"], limit:0}` — live-only, no backfill) and are merged into the
TanStack cache by `applyUserStatusEventToQueries`
(`hooks.ts:155-192`), which uses `(updatedAt, eventId)` ordering
(`statusVersionIsAtLeast`, `hooks.ts:29-37`) to resist out-of-order delivery —
**note:** this is `created_at`/id comparison, not NIP-33's strict newest-wins
rule enforced server-side; Desktop re-derives the same ordering client-side
as a defense against a stale live event beating a fresher polled one. A
60-second-tick/120-second-backstop refetch
(`USER_STATUS_REFETCH_INTERVAL_MS = 120_000`, `hooks.ts:194`) and periodic
expiry sweep (`expireUserStatusQueries`) keep the cache honest even without
a live event. Display: `UserNameIndicators`
(`desktop/src/features/user-status/ui/UserNameIndicators.tsx`) renders the
status emoji (falling back to `DEFAULT_USER_STATUS_EMOJI = "💬"`,
`StatusEmoji.tsx:31`) with a tooltip showing the status text; it is mounted
in three places — the channel header
(`desktop/src/features/channels/ui/ChannelScreenHeader.tsx:214`), message
rows (`desktop/src/features/messages/ui/MessageAuthorWithIndicators.tsx:31`),
and the sidebar member list (`desktop/src/features/sidebar/ui/
SidebarSection.tsx:322`). `StatusEmoji`
(`ui/StatusEmoji.tsx`) resolves `:shortcode:` emoji against the community's
custom-emoji set and rewrites the image URL through the local media proxy;
anything else (native glyph or unresolved shortcode) renders as plain text —
there is no signature/author check inside `StatusEmoji` itself, since by the
time it renders the event has already round-tripped through the TanStack
cache populated only from relay-returned events (no explicit client-side
`event.verify()` call was found anywhere in the user-status feature — see
§6 caveat).

**Presence (20001).** `usePresenceQuery`
(`desktop/src/features/presence/hooks.ts:91-112`) polls Tauri command
`get_presence` (backstop every 60s, `PRESENCE_REFETCH_INTERVAL_MS`); live
updates come through `usePresenceSubscription`
(`hooks.ts:117-180`) which opens one shared live WS subscription
(`openPresenceSubscription`) and dispatches `parseLivePresenceEvent`
(`presence.ts:7-15`) results into every matching cached query via
`mergePresenceUpdate`. Crucially, `parseLivePresenceEvent` **does not trust a
`p` tag** on live events — "the subject is always the event author... A p
tag is NOT trusted here — a client could forge one to spoof another user"
(`presence.ts:3-6`); only the relay-signed REST/synthesized snapshot path
trusts a `p`-tag subject (see §4). Display helpers
`getPresenceLabel`/`getPresenceDotClassName`/`getPresenceChipClassName`
(`presence.ts:72-100`) map status to label/color; shown via `PresenceDot`
(`features/presence/ui/PresenceBadge.tsx`, referenced from
`ProfilePopover.tsx:10`) next to the self avatar and (per
`AppShell.tsx:853,874`) elsewhere in the shell.

## 4. Presence heartbeat

Not a periodic background re-publish of "I'm still here" on a fixed timer in
the sense of a cron — it's activity-derived and connection-derived:

- **Automatic status derivation:** `resolveAutomaticPresenceStatus`
  (`presence.ts:62-70`) returns `"away"` once OS-reported idle time (preferred,
  via `getOsIdleSeconds`) or in-app last-activity exceeds
  `PRESENCE_IDLE_TIMEOUT_MS = 10 * 60_000` (10 minutes, `presence.ts:58`);
  otherwise `"online"`. This is Slack/Discord-style idle semantics, explicitly
  not "window focused" (`presence.ts:56-57` comment).
- **Heartbeat cadence:** `PRESENCE_HEARTBEAT_INTERVAL_MS = 60_000` (60s) and
  `PRESENCE_TTL_SECONDS = 3 * 60 = 180` (`presence.ts:51-52`) — "keep the
  local optimistic cache and relay expiry at three heartbeat windows... The
  relay owns the authoritative TTL." The actual re-publish-on-tick driver
  (`PRESENCE_STATUS_TICK_INTERVAL_MS = 30_000` in `hooks.ts:24`) and the
  mutation that fires `build_presence_update`/`signRelayEvent` on an interval
  live in `useSetPresenceMutation`/the session wiring in
  `AppShell.tsx:874-905` (`selfPresenceStatus`,
  `presenceSession.setStatus`); a manual override (`PresencePreference`
  "auto"/"away"/"offline" persisted to `localStorage`,
  `presence.ts` → `PRESENCE_PREFERENCE_STORAGE_KEY`, `hooks.ts:38-40`) can
  pin the status instead of auto-deriving it.
- **What the relay reports / how "online" is actually derived server-side:**
  presence state is **not stored as Nostr events** at rest — it lives in
  Redis, keyed per `(tenant, pubkey)`. On publish, `handle_ephemeral_event`
  (`crates/buzz-relay/src/handlers/event.rs:812-891`) special-cases kind
  20001: it accepts bare strings or legacy `{"status":"..."}" JSON (truncated
  to 128 bytes, `event.rs:838-847`), then calls `state.pubsub.set_presence`
  (or `clear_presence` for `"offline"`, `event.rs:862-883`) against Redis
  *before* falling through to the normal ephemeral fan-out
  (`event.rs:889-891` comment: "Presence is a channel-less ephemeral event").
  A Redis write failure is surfaced as `IngestError::Internal` — the ACK is
  refused rather than silently fanning out an event the storage layer never
  recorded (`event.rs:864-878`, explicit "we must also fan out nothing"
  rationale). On read, `synthesize_presence`
  (`crates/buzz-relay/src/api/bridge.rs:2539-2631`) intercepts any REST
  filter that targets only kind 20001 or 40902 with an `authors` list,
  bypasses the DB entirely, calls `pubsub.get_presence_bulk` against Redis,
  and **signs synthetic kind-20001 events with the relay's own key**, each
  carrying a `["p", <subject-hex>]` tag identifying who the event is about
  (`bridge.rs:2600-2618`) — this is the one place a `p`-tag subject actually
  is trusted, because the signer is the relay, not an arbitrary peer
  (confirmed again in `desktop/src-tauri/src/commands/profile.rs:356-383`,
  `get_presence`, which reads the `p` tag for relay-signed events and falls
  back to `event.pubkey` only for self-signed live WS events — matching the
  trust split documented in `presence.ts:3-6`). An empty Redis map is a
  authoritative "all offline" result; a Redis error must propagate as an
  error, never a fake-empty success (`bridge.rs:2545-2548` doc comment).
  There is no NIP-42/NIP-OA "auth session implies online" shortcut found —
  presence is a value the client explicitly asserts and the relay stores
  with (implicitly, via the 180s TTL referenced by the Desktop constant)
  an expiry; the TTL itself is set/owned on the Redis/pubsub side, not
  visible in the handler code reviewed here (not read in depth:
  `crates/buzz-pubsub` internals for the actual Redis `EXPIRE` value).

## 5. Pinned revision vs. upstream main

This checkout's `HEAD` **is** `781d39510cf23cfe224e8f521ae06a23377e06de`
(`git rev-parse HEAD`). I could not diff against
`ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43` without a network fetch, which this
task disallows; this checkout's last-known `origin/main` remote-tracking ref
(from a fetch made before this task, not by me) pointed at `14a752f`/`5fdb2e5`,
neither of which is `ebe99a46e8802b9ff20fdf6a1028ce93bdefaa43` — I cannot
confirm that hash exists or what it contains, and I'm flagging this plainly
rather than guessing at upstream deltas. What I can say about the pinned
revision's own internal consistency:

- Full user-status (30315) feature is present and wired end-to-end: builder,
  CLI (`cmd_set_status`, `crates/buzz-cli/src/commands/users.rs:516-528`),
  relay ingest/scope rules, Desktop publish/read/display, and an e2e test
  suite (`crates/buzz-test-client/tests/e2e_user_status.rs`) plus Playwright
  coverage (`desktop/tests/e2e/profile-custom-emoji-status.spec.ts`).
- Presence (20001) is also fully wired, but the pinned revision carries a
  **documented known bug** in the CLI's HTTP path: `desktop/src-tauri/src/
  managed_agents/nest_skill.md:124` states "`users set-presence` is broken —
  sends ephemeral kind:20001 via HTTP POST; relay rejects ephemeral kinds
  over HTTP. Will fail until WebSocket support is added." The CLI function
  itself (`crates/buzz-cli/src/commands/users.rs:505-507` doc comment)
  claims it already "connects to the relay over WS... bypassing the HTTP
  bridge" — these two statements are in tension in this checkout; I did not
  trace `client.publish_ephemeral_event`'s transport far enough to resolve
  which is current. Treat `set-presence` as unverified until exercised
  against a real relay.
- No in-repo evidence of a richer status model (custom statuses beyond
  text+emoji+expiry, a status "d-tag" other than `general`, or NIP-OA-style
  session-bound presence) at this pin — if upstream main has moved past this,
  it is not visible from this checkout.

## 6. Proposal for the plugin: minimal viable "Update your status"

**Scope.** Match the single entry Desktop exposes: set/clear a `d:general`
kind-30315 status (text + emoji, no custom duration UI in v1 — a fixed
"clears after 24h" default is simpler and avoids building the whole
preset/duration picker), surface the user's own status in place of the
"Update your status" row (same affordance as `ProfilePopover.tsx:214-245`),
and show others' status emoji beside names wherever QML already renders a
name, if the data is already in the roster/catalog projection (cheap) —
**not** a new always-on subscription just for status.

**Division of labor (per AGENTS.md: helper signs, QML presents).**

- **helper (Rust) does everything that touches keys or the wire:**
  - A new outbound builder analogous to `agents_service/enroll.rs:93`'s
    `EventBuilder::new(Kind::Custom(KIND_PERSONA), content).tags([d])`
    pattern — `EventBuilder::new(Kind::Custom(30315), text).tags([d_general,
    emoji_tag?])`, mirroring upstream `build_user_status`
    (`crates/buzz-sdk/src/builders.rs:1911-1924`) exactly: trim text, 64 KiB
    cap, `d:general` always, `emoji` tag only when non-blank after trim. Reuse
    the same bound rather than inventing a new one.
  - A new `QueryRequest` variant in `helper/src/query.rs`, sibling to
    `Self::Profiles { authors }` (`helper/src/query.rs:65-67`) and
    `Self::AgentProfiles` (`:68-70`): `Self::UserStatuses { authors }` building
    `{"kinds":[30315],"authors":[...],"#d":["general"],"limit":authors.len()}`
    — same 20-author cap pattern those two already use (`authors.len() <=
    20`), since status lookups piggyback on the same roster the catalog
    already resolved.
  - A new projection function in `helper/src/recipients.rs`, sibling to
    `name()` (`recipients.rs:81-110`) and `profiles()`
    (`recipients.rs:120-154`): `status()` that (a) requires
    `event.verify()` to succeed, (b) requires `event.kind.as_u16() == 30315`,
    (c) requires the `d` tag to equal `"general"` exactly (one `d` tag, no
    more — mirror the `roster()` d-tag check at `recipients.rs:38-48`), (d)
    requires the event's pubkey to be one of the already-verified roster
    keys (same `recipients.entries.iter().any(|r| r.key == ...)` guard
    `profiles()` already applies, `recipients.rs:127-132`), (e) picks
    newest-by-`created_at` with the same tie-conflict handling
    `profiles()` uses (`recipients.rs:136-150`) rather than trusting
    whichever event the relay happened to return first, (f) treats an
    expired `expiration` tag as "no status" client-side (upstream enforces
    nothing server-side — §1 — so the helper must do what Desktop's
    `statusIsExpired` does, `hooks.ts:67-70`), and (g) sanitizes the text the
    same way `name()` strips bidi/control characters
    (`recipients.rs:92-99`) before it ever reaches QML, since status text is
    just as much an untrusted self-asserted label as a display name.
  - `protocol.rs` additions: two new `Request.kind` values, e.g.
    `"set_status"` and `"clear_status"`, added to the match arm at
    `protocol.rs:81-99`; reuse the existing `text` field
    (`protocol.rs:17`) for status text (same field DM/send already uses —
    acceptable overload since the two kinds never co-occur in one request)
    and add one new optional field, e.g. `emoji: Option<String>`, bounded
    the same way `max_uses`/`expires_in_hours` are bounded today
    (`protocol.rs:33-36`) — reject outside a small range (e.g. 1–8 Unicode
    scalar values, enough for a glyph + variation selector/ZWJ sequence, not
    enough for a disguised payload). `clear_status` needs no extra field at
    all (sign the same builder with empty text/no emoji, matching upstream's
    "clear via empty replacement" semantics, §1).
  - Projected `Room`/`Recipient`-style output struct carries a `status:
    Option<{text, emoji}>` alongside the existing `Recipient.name`
    (`recipients.rs:9-12`), populated by the new `status()` function above —
    this is the "cheap, already-fetched" path: statuses for a room's roster
    ride along with the existing `fetch()` call
    (`recipients.rs:156-170`) that already issues one `RoomMembers` +
    profile-batch round trip, by adding one more batched `UserStatuses`
    query at the same call site, not a new per-name subscription.
  - Own-status display in the account control reuses the same projection
    (query authors = `[self]`) rather than a separate code path, so there is
    exactly one verification routine for status events in the whole helper.

- **QML presents only:** a status row in the account/profile control showing
  emoji + text (or "Update your status" placeholder when none), a small
  dialog with a text field + emoji picker + Set/Clear buttons that forwards
  `{text, emoji}` as a `set_status`/`clear_status` IPC request and renders
  whatever the helper's next snapshot reports back — no signing, no key
  material, no relay URL, no raw event construction in QML. Status emoji
  beside names in the roster/member list reads the same `status` field
  already attached to each projected recipient; no second render path needs
  to know about Nostr kinds at all.

**Bounds (mirroring upstream, since nothing here should be looser than
Desktop):**
- Text: trim, cap at the SDK's 64 KiB bound at minimum (`builders.rs:1918`);
  consider a much tighter practical cap in the helper itself (e.g. a few
  hundred bytes) since QML has no reason to render a 64 KB status line —
  upstream's Desktop UI has no `maxLength` (§2 finding) and that is arguably
  a gap, not a pattern to copy.
- Emoji: validate it's a native glyph or `:shortcode:` matching
  `StatusEmoji.tsx`'s `^:([^:\s]+):$` shape conceptually (the helper doesn't
  need to resolve custom-emoji images — that's a QML/community-asset
  concern — but should reject anything that is neither a short native glyph
  sequence nor a bounded `:word:` token, so a crafted tag can't smuggle
  control characters or an oversized string into a field QML will render).
- Rate: no kind-specific relay rate limit was found for 30315 (§1) beyond
  whatever generic per-connection/per-scope limiter `UsersWrite` is subject
  to; the helper should debounce/rate-limit `set_status` client-side anyway
  (e.g. refuse a resubmission within a few seconds) since nothing upstream
  stops a malicious or buggy QML surface from hammering it.

**What must be verified before display (non-negotiable, per the project's
own helper conventions already applied to names/rosters):** `event.verify()`
on every 30315 event before use; author must be a key the helper already
trusts for that room/roster (same closed-world check `profiles()` applies,
`recipients.rs:131`) — a status event from a pubkey outside the verified
roster must be dropped, not displayed; `d` tag must be exactly `general`
(reject multiple `d` tags or a different value, mirroring the strict roster
`d`-tag check at `recipients.rs:41-48`); newest-by-`(created_at, id)` wins,
with the same conflict-awareness `profiles()` uses rather than "last one the
relay happened to return"; expiration (if present) is evaluated and expired
statuses are treated as cleared, since the relay will not do this for the
helper (§1). None of this is optional — it's the same bar DM_MAP.md already
holds `recipients.rs`/`catalog.rs` to for names and rosters, and status text
is exactly as untrusted as a display name.

## Not read in depth

- `crates/buzz-pubsub` internals for the actual Redis key/TTL implementation
  backing presence (`set_presence`/`clear_presence`/`get_presence_bulk`) —
  confirmed their call sites and contracts (§4) but not the storage code
  itself.
- `desktop/src/features/presence/lib/presenceSubscriptionReconciler.ts` and
  `shared/api/presenceRelaySubscription.ts` — confirmed they exist and are
  wired into `usePresenceSubscription` (§3) but did not read their
  reconciliation logic line-by-line.
- Whether `client.publish_ephemeral_event` (buzz-cli) is actually WS or HTTP
  under the hood — flagged as an unresolved contradiction in §5 rather than
  guessed at.
