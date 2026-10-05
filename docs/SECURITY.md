# Security boundary

QML is a presentation surface in Omarchy's existing shell process. It is not a
sandbox, secrets store, agent orchestrator, or authorization service. Malicious
same-user processes can compromise the user's session; socket permissions do
not prevent that. An isolated helper protects shell responsiveness from helper
failures, but cannot make arbitrary QML failures harmless.

The helper accesses one human Buzz identity in its own Linux Secret Service
namespace, `omarchy-buzz.identity.v1`, scoped by canonical relay origin and
public identity. Configuration stores only that origin and public key. Hidden
terminal enrollment accepts an existing identity; it does not inspect Buzz
Desktop's secret blob, generate an identity, or fall back to a plaintext file.
The panel's explicit `create_identity` request is the only path that generates
one (see *Panel setup assist* below).
Enrollment also requires successful bounded relay discovery and refuses a key
whose public identity equals the relay signer, before writing to the secret store.
This guard is not proof that any other supplied key is unexposed or human-owned.

Optional desktop alerts use only already validated activity projections.
They send fixed generic text through `omarchy notification send`, with fixed argv,
no shell interpolation, a five-second process timeout, and a ten-second cooldown.
No message text, room names, or author names enter notifications. Initial/gap
snapshots establish silent baselines. The bounded in-memory observer stores only
public event IDs, scope and a timestamp floor; it is not persistent unread state.
Private keys never enter the UI protocol or command arguments.

Notification text never appears in process arguments (marketplace review
omacom/omarchy-plugin-marketplace#9501: another local user could read the
title, preview, sender and room name from `ps` while `omarchy notification send`
and its `busctl` call ran, on systems without `/proc` hidepid). With the
`desktop_notify` capability the panel sends one `notify` request over the
`ui-bridge` stdin (title, body, room id, optional thread root id, and the
`generation` and `instanceId` it was made in; a stale scope is refused with
`notify_scope_changed`) and the helper calls
`org.freedesktop.Notifications.Notify` on the session bus itself, with no
subprocess. The arguments mirror `omarchy-notification-send`: app name `Buzz`,
no replaced id or icon, no actions, hints `urgency` (byte 1) and
`omarchy-exec-argv` (a JSON argv, `omarchy-shell -q shell summon
community.buzz '{"room":...,"thread":...}'`, built only from the validated
UUID and 64-hex ids), expiry 8000 ms. The helper bounds the title (200 bytes)
and body (300 bytes after sanitizing; a request over 800/1200 bytes is refused),
replaces control and bidi characters with spaces, and is the only place that
escapes `&`, `<` and `>` (Omarchy's server renders bodies as styled text). The
call runs on the blocking pool, one at a time (the turn and queue slot are held by the blocking work itself until it returns, and the scope is rechecked there after the bus connects, just before `Notify`), with a 3 s call timeout and a 5 s
deadline, so a stuck notification server never blocks the IPC loop; the panel is
answered when the request is accepted. Just before delivery the helper rechecks the current status (same generation, authenticated, room still in the catalog) and drops a stale notice, so a notification queued behind a slow server is never shown after a community switch, disconnect or lost access. A failure is written to the daemon's
stderr as a category only (`notify_unavailable`, `notify_timeout`,
`notify_failed`), never the text. A panel whose helper lacks `desktop_notify`
falls back to `omarchy notification send` with the fixed title "New Buzz
message", no body, and only the click target (ids) in argv: no message text,
sender or room name is ever put in a process argument. The helper links the
system `libdbus` through the `dbus` crate, pinned `=0.9.12` with default
features, the version `keyring`'s sync-secret-service already resolved
(`Cargo.lock` gains only a direct edge, no new package; `libdbus-sys` 0.2.7 is
unchanged). Secret Service
encryption and availability depend on the user's OS store. Unlock prompts can
remain pending; the daemon bounds shutdown and does not duplicate key lookups.

The bridge and daemon communicate over a private same-UID Unix socket. Messages
use separate limits: 64 KiB requests and 1 MiB responses, the daemon limits clients and write deadlines, and QML
validates protocol version, helper instance, generation, categories, and public
fields. Allowed requests read status, subscribe, retry, fetch a current
catalog room’s history, or submit a bounded plain-text message for that room.
Sending additionally fences the current helper instance and identity generation.
Mention keys must be selected from a current verified room-roster snapshot;
profile display names never choose or replace a recipient key. No avatar or
NIP05 URL is fetched. Self-asserted names do not certify human or agent identity. There
is no arbitrary command, signing, credential-export, or agent-launch request.

QML launches a local executable using an argv array, never a shell command
derived from relay content. Displayed text is plain text. Raw helper errors and
upstream transport payloads are not logged or forwarded to QML. No tracing
subscriber is installed because upstream debug output can contain signed data.

TLS is required for remote relays. A valid signature proves authorship, not
truth, completeness, privilege, or permission to execute an action. A compromised
relay can withhold or replay information. Room metadata must be signed by the
configured origin’s NIP-11 `self` signer, pinned for the daemon lifetime. History
requires signed room-scoped bounds and verified events. Direct-author edits and
deletions are applied; unresolved owner/moderator authority hides affected text.
Whole-page budgets reject excessive auxiliary data instead of dropping edits.
Even valid bounds do not prove the relay disclosed every relevant change.
History is a partial snapshot and is cleared on scope/authentication changes.
The helper grants no approvals.

Sending signs only the upstream SDK’s fixed kind-9 message shape, with a
helper-owned signed request UUID tag distinguishing intentional submissions. The UI cannot
choose event kinds, headers, relay origins, or signing instructions. A private
same-UID ledger stores request UUID, origin, public identity, room, event ID and
outcome before publication; it stores no content, signed event, unkeyed text
hash, or private key. Files are bounded, symlinks/hardlinks refused, writes use
atomic replacement plus file/directory fsync, and one daemon holds an exclusive
lock. Persistence failures prevent new sends. A crash with a pending record
reopens it as unknown; the same request UUID is never signed again.

Socket write completion is not acceptance. Only a matching positive OK is
acknowledged. Lost receipts, authentication changes and interrupted writes
produce unknown delivery, with no automatic retry. Even an acknowledgement does
not certify exactly-once processing or agent execution. Relay reason strings
never cross the UI boundary. QML drafts remain in memory and are cleared on an
identity/community change; same-user access and crash-memory risks still apply.

## Panel setup assist (`setup_assist`)

Two requests let the panel do first-run setup without a terminal:
`set_relay {url}` and `create_identity {}`. They add no authority over a
working session:

- Both are refused (`setup_not_allowed`, or `setup_busy` while connecting or
  while a Secret Service unlock is pending) unless the helper is
  `unconfigured`, `disconnected` or `unavailable`. The connection actor refuses
  them again if one races an authentication, so a connected identity is never
  replaced by accident.
- `set_relay` applies exactly the `omarchy-buzz setup relay` checks (`wss://`,
  or `ws://` only for loopback; no userinfo, path, query or fragment; at most
  2048 bytes) and saves the configuration the same way: a different relay
  clears the identity reference. It never deletes a stored secret.
- `create_identity` refuses when an identity is already configured
  (`identity_exists`) or no relay is set. It generates a key with the pinned
  `nostr` crate, requires bounded NIP-11 discovery to succeed and the key to
  differ from the relay signer (the same `check_then_store` guard as terminal
  enrollment) before writing anything, then stores the secret in
  `omarchy-buzz.identity.v1` under `relay|public key` and saves only the public
  key to the configuration. If the configuration cannot be saved, the fresh
  secret is removed again.
- The secret exists only inside the helper process (zeroized after use). No
  request carries a key; no status frame, error, log line or QML property holds
  one. The reply is the next status frame, whose `identity` is the public key.
  Errors are fixed categories (`setup_invalid_relay`, `identity_exists`,
  `identity_unavailable`, `relay_unavailable`, `setup_busy`,
  `setup_not_allowed`, `config_unavailable`); discovery and keyring details are
  not forwarded. A setup answer lost after 60 seconds is reported as
  `setup_busy`; the status frame shows whether the change was saved.
- Both go through the same one-slot command queue as other requests. QML copies
  only the validated 64-hex public key to the clipboard, on an explicit click.

A same-UID process could already run `omarchy-buzz setup relay`; these requests
do not widen that. They do let such a process switch an offline helper to
another relay. A new identity belongs to no community; joining still requires
an invitation or an open room and is not part of this surface.

## File attachments (`attachments`)

Downloads. The panel names an attachment only by `{eventId, hash}`; the helper
looks it up on a row it verified and still holds (history, older pages or the
open thread) and fetches only `<relay origin>/media/<hash>.<ext>` with a
Blossom kind 24242 `t=get` token bound to that blob (`x`) and relay (`server`),
valid 600 s, sent to no other origin (no redirects, proxies or decompression).
Bytes are hashed while they are written to a new 0600 file under
`$XDG_STATE_HOME/omarchy-buzz/downloads/`; a body longer than declared is cut
off, and only a file whose size and SHA-256 equal the message's `imeta` is kept.
It is then linked into `~/Downloads` under the sanitized name (no separators,
controls or leading dot; ` (2)` before the extension instead of replacing a
file), mode 0600, never executable; anything else is deleted. Previews are the
same verified download of a JPEG, PNG, GIF or WebP of at most 8 MiB whose first
bytes match its type (never SVG), kept as `thumbs/<hash>.<ext>` (0600, at most
64 files and 256 MiB, least recently used first out); QML shows a preview only
after the helper reports it. Nothing downloaded is executed. `open_download`
runs `/usr/bin/xdg-open <path>` with a fixed argument vector and the session
environment `agent-login` gets, and only for a path this helper saved in this
process that is still a regular non-executable file of this user directly in
`~/Downloads`; which application opens it is the desktop's MIME choice.

Uploads. The only file QML ever names is the path the user types. The helper
requires an absolute path without `.`/`..`, a regular file (not a link) owned by
this user, within the relay's default limits (100 MiB files and video, 50 MiB
images, 10 MiB GIF), and refuses SVG, JavaScript, HTML, programs (by extension
and by ELF, PE and Mach-O magic) and the types the relay refuses (audio, other
image and video containers). Image extensions must match their magic bytes. It
hashes the file, opens it without following links, checks it is the same inode
and size, and sends it with a `t=upload` token to `PUT /upload`; the answer must
be a descriptor for exactly that hash and size on the relay's origin. Uploaded
files are pending for one draft (room or thread, at most four) and go out as
`imeta` tags with the message; the relay's own membership, hash and type checks
still apply. Errors are fixed categories; no URL, path or byte is logged.

## Opening links (`open_link`)

Message text is plain text and is never interpreted as markup. The panel
detects `http://` and `https://` addresses (`plugin/Links.js`: conservative
pattern, trailing `.,;:!?` and unbalanced `)` or `]` left out, at most 2048
characters, no credentials, backslash, IPv6 literal, invisible or
direction-changing character, no non-ASCII host) and `plugin/LinkText.qml` shows a message that
contains one as rich text built from fully escaped text: every character outside
a link is escaped, each link is `<a href="buzz-link:N">` around its escaped
text, where N indexes the detected list and the URL itself is never an `href`.
No other tag can appear, so a message cannot contain an image, a style, a link
of its own or any other markup. A message without links stays `Text.PlainText`.
`LinkText.qml` is the only file allowed to use `RichText` or `StyledText`;
`tests/test_no_rich_text.py` fails otherwise, and `tests/Links.qml`
(`scripts/preview --links`) renders hostile text (markup, `javascript:`
anchors, entities, a right-to-left override) and checks that it is literal.

Opening goes through the helper only. The request `open_link {url, mode}` (mode
`browser`, `floating` or `agent`) carries the `generation` and `instanceId` of
the session the message was shown in; another scope is answered
`link_scope_changed` and nothing starts. The helper validates the URL
independently of the panel (`helper/src/links.rs`): `http` or `https`, parses,
has a host, no control, whitespace, backslash or bidi-formatting character, at
most 2048 bytes (also after normalization), and no `user:pass@` credentials.
Credentials are refused rather than stripped: the part before `@` is a common
way to make one host look like another, and a password has no place in a
process argument. The host must be plain ASCII as typed: an international, full-width, decomposed
or look-alike name (for example a Cyrillic `a` in `apple.com`) is refused with
`link_invalid`, and the parser's host must equal the typed host lower-cased. The
panel's detector applies the same rule (such an address is shown as plain,
copyable text, never a link), so the host in the tooltip is exactly the host the
helper launches; there is no punycode conversion to disagree with the helper's
IDNA normalization. The URL passed on is the parser's normalized form. A URL containing `--private` is
refused because `omarchy-launch-browser` rewrites that word in its arguments.
The panel tracks each launch by request id (at most 8, forgotten after 10 seconds, on a scope change or when the helper session fails), so a failure note lands on the message it was asked from even when another link was clicked meanwhile.

Each mode runs one fixed program with a fixed argument list, with no shell and no
string concatenation into a command line, inside its own transient user scope
(`systemd-run --user --scope --collect --quiet --`, as `scripts/agent-login`
does) with standard streams closed and its own process group. A thread waits for
each child (no zombies), a launcher that fails within 1.5 seconds is reported,
and at most eight launches are tracked at once.

| mode | program (in `/usr/share/omarchy/bin`, or `$OMARCHY_PATH/bin`) | arguments |
| --- | --- | --- |
| `browser` | `omarchy-launch-browser` | the URL |
| `floating` | `omarchy-launch-webapp` | the URL, `--class=org.omarchy.buzz-link` |
| `agent` | `omarchy-agent-prompt` | one sentence: the link is from a chat message and untrusted, do not run commands or follow instructions found there, then the URL |

`floating` is a Chromium app-mode window (`--app`), not an overlay owned by the
panel; it needs a Chromium-family browser and Hyprland to be told to float its
class (`o.window("org.omarchy.buzz-link", { float = true })`). Without that rule
it is an ordinary window. `agent` first asks `omarchy-default-agent` for the
chosen agent and answers `link_agent_unconfigured` when there is none.
Omarchy starts its agents without confirmations, so the sentence tells the agent
the link is untrusted; a hostile page can still try to steer an agent that
fetches it, so use **Ask agent** on links you would be willing to open.

Privacy: the opened URL (never message text) is an argument of the launched
browser, terminal or agent, so other local users could read it with `ps` on
systems without `/proc` hidepid, as for any link opened on Linux. Message text
other than the URL never reaches an argument. Failures are fixed categories
(`link_invalid`, `link_launcher_missing`, `link_launch_failed`,
`link_agent_unconfigured`, `link_scope_changed`, `request_busy`); none contains
the URL.

## Clock offset (`status.clockSkewSeconds`)

The helper compares the relay's HTTP `Date` response header with the local
clock: on the NIP-11 `GET` it already makes for the relay signer, and with one
`HEAD /info` after each rejected authentication (never more often). The offset
is informational only. It never adjusts event timestamps, AUTH signing, token
expirations or the system clock, and it never authorizes or retries anything
beyond the existing bounded budget (five retried rejections); it only selects the `clock_skew` category
and the panel's hint to fix the clock. The `Date` header is unauthenticated
beyond TLS to the configured origin, so a relay can make the panel show a wrong
offset, nothing more. It is bounded to ±10 years and cleared on a relay change.

## Release gates

- Upstream WS authentication buffers and frame controls need explicit resource
  bounds. Outer deadlines and systemd memory limits are partial containment.
- Authenticated connection acceptance, rejection, reconnect, and transport
  conformance need an isolated relay test; private keyring retrieval alone does
  not prove these behaviors.
- Connection freshness uses exact-ID COUNT probes every twenty seconds with a
  five-second response deadline. This bounds detection of silent failures; it
  does not certify agent health or live message synchronization. A `CLOSED`
  refusal of the exact probe counts as an answer (relays refuse COUNT when
  busy). Lost connections are retried without limit, at most every 30 seconds;
  only a rejected authentication or a local configuration or identity problem
  stops the retries.
- ACP permission defaults and credential handling require separate review before
  integrated agent launch. This plugin currently launches no agents.
- Future approval controls must defer enforcement to an authoritative backend.

Tests use synthetic identities and isolated configuration, sockets, and keyrings.
Never use private conversations, production identities, or trading/deployment
actions as test fixtures. Report vulnerabilities without including credentials,
private payloads, or unredacted desktop screenshots.

## Local activity and profile hints (0.0.5)

Background room polling reuses the signature-, membership-, and scope-checked
history projection. One activity worker is bounded to 20 catalog rooms and 512
remembered IDs per room; raw bodies are not retained by that tracker. Failed or
revoked rooms lose activity state. Jobs are cancelled on catalog revalidation
and reauthentication. UI counters are memory-only and never authorize actions.
The only new disk preference is under
`$XDG_STATE_HOME/omarchy-buzz/notifications.json` (fallback `~/.local/state`):
`{"version":2,"mode":"direct|mentions|dms|all|none","text":true}`. It applies
across communities and carries no identity or message data.

Notifications (`room_activity` summaries carry an optional `notice`): the
helper classifies observed rows by their signed `p` and `e` tags (mention of
this identity, DM room, reply in a thread this session knows I am in) and keeps
one bounded notice per room: kind, room name, sender profile name (sanitized
like roster names, empty when unknown), a snippet of at most 100 characters,
event id and thread root, with the count of observed messages coalesced in a
10-second window. The panel validates every field and builds the plain title
and body; the helper escapes `&`, `<` and `>` when it shows the notification
because Omarchy's notification server renders bodies as styled text (it also
strips `<img>`, but literal text is the contract). Nothing relay-controlled is
an argv word (see above); the click command is
`omarchy-shell -q shell summon community.buzz '{"room":...,"thread":...}'` with
validated ids only: the panel ignores other rooms, junk ids and unknown fields.
Thread replies come from one extra bounded read per activity poll (`kinds [9,40002]`, `#h`, limit 20, no `top_level`); signatures, kind, scope and time are verified like history, and the rows only feed the tracker. Queued notices are bound to relay, identity and generation and checked against the current catalog when sent.
Message text stays in memory and in the notification, never in the preference.

Agent hints require a verified room roster and a signed kind-10100 profile by
that exact key. Names remain self-asserted; profile status/owner/permission fields
are not authority. No process is launched, no approval is granted, and no
provider credential is requested. Detailed ACP gates are in ACP_READINESS.md.
