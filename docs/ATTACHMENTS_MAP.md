# File attachments — implementation map

Research pass on September 30, 2026 against pinned Buzz
`781d39510cf23cfe224e8f521ae06a23377e06de`. File:line references are to that
checkout; confirm before relying on them. Nothing here was checked live.

## Wire format

A message (kind 9, edits 40002/40003) carries one NIP-92-style `imeta` tag per
attachment (`crates/buzz-sdk/src/builders.rs:207-213,238-260`) with keys
`url, m, x, size, dim, blurhash, alt, thumb, fallback, duration, bitrate,
image, filename` (`crates/buzz-relay/src/handlers/imeta.rs:11-18`); `url, m, x,
size` are required (`imeta.rs:161-163`); `x` is the 64-hex SHA-256
(`imeta.rs:82-88`); `filename` rejects path separators and control characters
(`imeta.rs:138-146`). The content also carries a redundant markdown line —
`![image](url)`/`![video](url)` for previewable media or `[filename](url)` for
other files (`desktop/src/features/messages/lib/imetaMediaMarkdown.ts:283-310`).
That line is what the panel shows today: `helper/src/history.rs` projects only
`event.content` and ignores tags; `reduce` checks only the `h` tag on rows, so
`imeta` tags already pass through.

## Upload

`PUT /upload` (alias `PUT /media/upload`), Blossom BUD-02
(`crates/buzz-relay/src/api/media.rs:1-13,237-260`): raw body, header
`X-SHA-256`, `Authorization: Nostr <base64 kind 24242>` with `t=upload`,
`x=<sha256>`, `expiration`, optional `server` (`crates/buzz-media/src/auth.rs:31-90`;
Desktop signs it in `desktop/src-tauri/src/commands/media.rs:355-381`). NIP-43
relay membership is enforced (`media.rs:297-309`). Limits in
`crates/buzz-media/src/config.rs:75-84` (images/GIF caps, video 500 MB, other
files 100 MB). Types: images allowlist jpeg/png/gif/webp
(`crates/buzz-media/src/validation.rs:16`), video by ISO-BMFF sniff, others
through a deny-list (SVG, JS, executables; `validation.rs:60-99`); unsniffable
bytes become `application/octet-stream` (`validation.rs:213-225`); image bomb
cap 25 Mpx (`validation.rs:281-283`). Response `BlobDescriptor {url, sha256,
size, type, uploaded, dim?, blurhash?, thumb?, duration?}`
(`crates/buzz-media/src/types.rs:1-29`), URL rewritten to the tenant host
(`media.rs:333-346`). The relay makes a 320×320 JPEG thumbnail and a blurhash
for images (`crates/buzz-media/src/thumbnail.rs:11-42`). No retention/expiry
was found. NIP-96 is not used.

## Download

`GET`/`HEAD /media/{sha256}.{ext}` needs a Blossom `t=get` token and relay
membership (`media.rs`, `extract_blossom_read_proof`,
`enforce_blossom_read_membership`); the URL alone grants nothing. Desktop mints
a 10-minute server-scoped token per relay
(`desktop/src-tauri/src/commands/media_download.rs:37-71`). Range requests up
to 16 MiB. Images/video are served inline, everything else as `attachment`,
with `CSP: default-src 'none'` and `nosniff` (`validation.rs:222-228`).
Desktop renders images/video inline and shows other files as a `FileCard`
whose click downloads through the app with a same-origin `/media/` check
(`desktop/src/shared/ui/markdown/FileCard.tsx:22-56`,
`media_download.rs:30-58,95-120`) and re-validates bytes before saving.

## Plan

1. Helper: parse `imeta` into a bounded `attachments` list per row (≤ 4:
   `{name, mime, size, url, hash, dim?}`; require `url,m,x,size`; `url` must be
   the configured relay host with a `/media/<64-hex>.<ext>` path; strict shapes,
   but a malformed `imeta` on one row marks that row's attachments unavailable
   rather than rejecting the page — attachments are metadata like summaries).
   Strip the redundant markdown line from the displayed text when it matches an
   attachment. New capability `attachments`.
2. Helper: `download_attachment {eventId, hash}` mints a `t=get` token, GETs
   `/media/…`, verifies the SHA-256 against `x`, enforces the size, writes to
   `~/Downloads/<sanitized name>` (never executable, unique suffix on
   collision), reports the path; images ≤ 8 MB additionally cached under the
   state directory for inline thumbnails (the panel shows them only after
   verification, no SVG ever). `open` uses `xdg-open` on the saved file only on
   an explicit click.
3. Helper: `upload_attachment {path}` from the composer (absolute path chosen
   in QML, validated by the helper: regular file, ≤ the relay limits, deny-list
   mirrored), computes SHA-256, signs `t=upload`, `PUT /upload`, stores the
   descriptor as a pending attachment for the draft; `send_message` then
   carries `media_tags` (`helper/src/sending.rs:298-306` currently passes
   empty slices) and the matching markdown line for other clients.
4. Panel: attachment cards under a message (name, size, mime; `Download` and
   `Open`; inline thumbnail for verified images), a paperclip control next to
   `@` with a path field, upload progress/state, and removal before send.

## Risks to confirm live

Token lifetime and `server` scoping; retention; behaviour behind the proxy for
`url` host matching; that the helper's identity has NIP-43 membership for
media routes (it does for messages).

## Built on branch `attachments` — September 30

The plan above is implemented (helper `attachments.rs`, `media.rs`,
`sending.rs`; panel `BuzzMessage.qml`, `BuzzComposer.qml`, `Service.qml`); the
checkpoint's entry of the same date has the contract and evidence. Differences
from the plan and corrections to the map:

- The `t=get` signing is `desktop/src-tauri/src/commands/media.rs:311-339`
  (`sign_blossom_get_auth_header`, 600 s at `:300`), not
  `media_download.rs:37-71` (that is `validate_download_url`, the same-origin
  `/media/` check). Desktop's read token has no `x`; the helper adds one.
- Default limits are set in `crates/buzz-relay/src/config.rs:857-872` (images
  50 MiB, GIF 10 MiB, video 500 MiB, files 100 MiB). The helper caps video like
  a file (100 MiB) and refuses HTML, which the relay accepts as an inert download.
- `upload_attachment` also carries `roomId` and optional `rootId`: the room and
  thread composers are visible together, so the draft scope cannot be implied.
- Four attachments on every held row do not fit the 1 MiB frame; at most 48 are
  projected across channel rows and 48 across thread replies (newest first).
- Added `status.upload` and the category `attachment_storage_unavailable`.
- Images are uploaded as they are; Desktop strips metadata by re-encoding and
  the relay refuses metadata-bearing images (`validation.rs` `validate_image_metadata_free`).
