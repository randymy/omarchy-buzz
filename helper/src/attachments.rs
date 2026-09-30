//! File attachments on messages: bounded projections of NIP-92 `imeta` tags.
//!
//! Wire format at the pinned Buzz revision (`781d3951`): one `imeta` tag per
//! attachment, each entry a `"key value"` string
//! (`crates/buzz-sdk/src/builders.rs` `imeta_tags`), keys from the relay's
//! allowlist with `url, m, x, size` required
//! (`crates/buzz-relay/src/handlers/imeta.rs` `validate_imeta_tags`). The
//! content additionally carries one redundant markdown line per attachment
//! (`desktop/src/features/messages/lib/imetaMediaMarkdown.ts`
//! `formatImetaMediaLine`), which the panel does not show twice.
//!
//! An attachment is metadata like a thread summary: a malformed `imeta` makes
//! that row's attachments unavailable, never the page.
use serde::Serialize;

/// Attachments shown per row; later `imeta` tags are ignored.
pub const MAX: usize = 4;
/// Attachments projected per status frame: across the held channel rows and
/// across the open thread's replies, newest rows first. Later rows show theirs
/// as unavailable. Four on each of 300 rows would not fit the 1 MiB frame
/// (`protocol::RESPONSE_LIMIT`).
pub const FRAME_HISTORY: usize = 48;
pub const FRAME_THREAD: usize = 48;
/// Largest declared size accepted (the relay's largest limit is 500 MB video).
pub const MAX_SIZE: u64 = 1 << 30;
pub const NAME_CHARS: usize = 128;
/// The relay's `filename` limit (`imeta.rs`): the name is sent back as one.
pub const NAME_BYTES: usize = 255;
pub const MIME_BYTES: usize = 64;
/// Longest media URL accepted: a 253-byte host, a port and `/media/<hash>.<ext>`.
pub const URL_BYTES: usize = 360;
const DIM_MAX: u32 = 16384;
/// The relay's `imeta` key allowlist (`imeta.rs` `ALLOWED_IMETA_KEYS`).
const KEYS: [&str; 13] = [
    "url", "m", "x", "size", "dim", "blurhash", "alt", "thumb", "fallback", "duration", "bitrate",
    "image", "filename",
];
/// Images the panel may preview; the relay's image allowlist
/// (`crates/buzz-media/src/validation.rs` `ALLOWED_MIME_TYPES`). Never SVG.
pub const IMAGE_MIME: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Attachment {
    /// Display and save name: `filename`, else the URL's last segment; sanitized.
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// `<relay http origin>/media/<hash>.<ext>`.
    pub url: String,
    /// Lowercase hex SHA-256 of the bytes (`x`), equal to the URL's hash.
    pub hash: String,
    /// `WxH`, or `null`.
    pub dim: Option<String>,
    /// `image`, `video` or `file`.
    pub kind: &'static str,
}

/// `http(s)://host[:port]` for the configured relay: where its media lives.
pub fn origin(relay: &str) -> Result<String, &'static str> {
    let url = crate::join::http_url(relay, "/")?;
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
/// 1–8 lowercase alphanumerics (`media.rs` `is_safe_ext`).
pub fn is_ext(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 8
        && value
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'))
}
pub fn is_mime(value: &str) -> bool {
    let Some((kind, sub)) = value.split_once('/') else {
        return false;
    };
    value.len() <= MIME_BYTES
        && !kind.is_empty()
        && !sub.is_empty()
        && !sub.contains('/')
        && value
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'+' | b'/' | b'-'))
}
fn canonical_number(value: &str) -> Option<u64> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    value.parse().ok()
}
pub fn is_dim(value: &str) -> bool {
    let Some((w, h)) = value.split_once('x') else {
        return false;
    };
    [w, h]
        .iter()
        .all(|part| canonical_number(part).is_some_and(|n| (1..=u64::from(DIM_MAX)).contains(&n)))
}
/// The URL's hash and extension when it is exactly `<origin>/media/<hash>.<ext>`.
pub fn media_path<'a>(url: &'a str, origin: &str) -> Option<(&'a str, &'a str)> {
    let rest = url.strip_prefix(origin)?.strip_prefix("/media/")?;
    let (hash, ext) = rest.split_once('.')?;
    (is_hash(hash) && is_ext(ext)).then_some((hash, ext))
}
pub fn kind(mime: &str) -> &'static str {
    if IMAGE_MIME.contains(&mime) {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else {
        "file"
    }
}

/// A name safe to show and to save under: no path separators, controls or
/// leading dots, at most `NAME_CHARS` characters (the extension is kept when
/// shortening). Never empty.
pub fn sanitize_name(value: &str) -> String {
    let mut name: String = value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = name.trim().trim_start_matches('.').trim().to_owned();
    if name.chars().count() > NAME_CHARS || name.len() > NAME_BYTES {
        let ext: String = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && ext.len() <= 16 => {
                format!(".{ext}")
            }
            _ => String::new(),
        };
        let (mut chars, mut bytes) = (ext.chars().count(), ext.len());
        let mut stem = String::new();
        for c in name.chars() {
            if chars + 1 > NAME_CHARS || bytes + c.len_utf8() > NAME_BYTES {
                break;
            }
            chars += 1;
            bytes += c.len_utf8();
            stem.push(c);
        }
        name = stem.trim_end().to_owned() + &ext;
    }
    if name.is_empty() {
        "attachment".into()
    } else {
        name
    }
}

/// Parse one `imeta` tag strictly against the configured relay origin.
fn one(parts: &[String], origin: &str) -> Result<Attachment, ()> {
    let mut seen = std::collections::BTreeSet::new();
    let (mut url, mut mime, mut hash, mut size, mut dim, mut filename) =
        (None, None, None, None, None, None);
    for part in &parts[1..] {
        let (key, value) = part.split_once(' ').ok_or(())?;
        if !KEYS.contains(&key) || (key != "fallback" && !seen.insert(key)) {
            return Err(());
        }
        match key {
            "url" => url = Some(value),
            "m" => mime = Some(value),
            "x" => hash = Some(value),
            "size" => size = Some(value),
            "dim" => dim = Some(value),
            "filename" => filename = Some(value),
            _ => {}
        }
    }
    let (url, mime, hash, size) = (
        url.ok_or(())?,
        mime.ok_or(())?,
        hash.ok_or(())?,
        size.ok_or(())?,
    );
    if !is_mime(mime) || !is_hash(hash) {
        return Err(());
    }
    let size = canonical_number(size)
        .filter(|n| (1..=MAX_SIZE).contains(n))
        .ok_or(())?;
    if url.len() > URL_BYTES {
        return Err(());
    }
    let (path_hash, _) = media_path(url, origin).ok_or(())?;
    if path_hash != hash {
        return Err(());
    }
    if dim.is_some_and(|d| !is_dim(d)) {
        return Err(());
    }
    // The relay's own filename rule (`imeta.rs`): 1–255 bytes, no separators or controls.
    if let Some(f) = filename {
        if f.is_empty()
            || f.len() > 255
            || f.contains(['/', '\\'])
            || f.chars().any(char::is_control)
        {
            return Err(());
        }
    }
    let fallback = url.rsplit('/').next().unwrap_or("attachment");
    Ok(Attachment {
        name: sanitize_name(filename.unwrap_or(fallback)),
        mime: mime.to_owned(),
        size,
        url: url.to_owned(),
        hash: hash.to_owned(),
        dim: dim.map(str::to_owned),
        kind: kind(mime),
    })
}

/// The first `MAX` `imeta` tags of an event, or `Err` when any of them is
/// malformed (the row then shows its attachments as unavailable).
pub fn parse(tags: &nostr::Tags, origin: &str) -> Result<Vec<Attachment>, ()> {
    let mut out = Vec::new();
    for tag in tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "imeta"))
        .take(MAX)
    {
        let parts = tag.as_slice();
        if parts.len() < 5 || parts.len() > 1 + 2 * KEYS.len() {
            return Err(());
        }
        let attachment = one(parts, origin)?;
        if out.iter().any(|a: &Attachment| a.hash == attachment.hash) {
            return Err(());
        }
        out.push(attachment);
    }
    Ok(out)
}

/// Keep at most `limit` attachments over `rows` (oldest first), preferring
/// the newest rows; a row that does not fit shows its attachments unavailable.
pub fn bound<'a>(
    rows: impl DoubleEndedIterator<Item = &'a mut crate::protocol::HistoryRow>,
    limit: usize,
) {
    let mut used = 0;
    for row in rows.rev() {
        if row.attachments.is_empty() {
            continue;
        }
        if used + row.attachments.len() > limit {
            row.attachments.clear();
            row.attachments_unavailable = true;
        } else {
            used += row.attachments.len();
        }
    }
}
/// Desktop's markdown label escaping (`formatImetaMediaLine`).
pub fn escape_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.chars() {
        if matches!(c, '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
/// The markdown line Desktop appends for an attachment. Images and video
/// render inline; every other file is a `[filename](url)` link.
pub fn markdown_line(mime: &str, name: &str, url: &str) -> String {
    if mime.starts_with("video/") {
        format!("![video]({url})")
    } else if mime.starts_with("image/") {
        format!("![image]({url})")
    } else {
        format!("[{}]({url})", escape_label(name))
    }
}
/// A generic file line `[label](url)` with Desktop's escaped label grammar.
fn file_line(line: &str, url: &str) -> bool {
    let Some(label) = line
        .strip_prefix('[')
        .and_then(|l| l.strip_suffix(&format!("]({url})")))
    else {
        return false;
    };
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.next().is_none() {
                    return false;
                }
            }
            '[' | ']' => return false,
            _ => {}
        }
    }
    true
}
fn redundant(line: &str, attachment: &Attachment) -> bool {
    let url = attachment.url.as_str();
    let line = line.trim_end();
    line == format!("![image]({url})") && attachment.mime.starts_with("image/")
        || line == format!("![video]({url})") && attachment.mime.starts_with("video/")
        || file_line(line, url)
}
/// Remove trailing content lines that exactly repeat one of `attachments`
/// (each at most once), so the card is not shown twice.
pub fn strip_markdown(content: &str, attachments: &[Attachment]) -> String {
    let mut body = content.trim_end_matches(['\n', '\r', ' ', '\t']);
    let mut used = vec![false; attachments.len()];
    loop {
        let (head, last) = match body.rsplit_once('\n') {
            Some((head, last)) => (head, last),
            None => ("", body),
        };
        let Some(index) = attachments
            .iter()
            .enumerate()
            .position(|(i, a)| !used[i] && redundant(last, a))
        else {
            break;
        };
        used[index] = true;
        body = head.trim_end_matches(['\n', '\r', ' ', '\t']);
        if body.is_empty() {
            break;
        }
    }
    body.to_owned()
}

/// Attachments of the event supplying a row's content and the text to show.
/// `(attachments, unavailable, text)`.
pub fn project(event: &nostr::Event, origin: &str) -> (Vec<Attachment>, bool, String) {
    match parse(&event.tags, origin) {
        Ok(list) => {
            let text = if list.is_empty() {
                event.content.clone()
            } else {
                strip_markdown(&event.content, &list)
            };
            (list, false, text)
        }
        Err(()) => (Vec::new(), true, event.content.clone()),
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
