//! Attachment transfers (`attachments`): verified downloads, cached image
//! previews and uploads for the composer.
//!
//! Relay contract at the pinned Buzz revision (`781d3951`):
//! - Reads: `GET /media/<sha256>.<ext>` with `Authorization: Nostr <base64url
//!   kind 24242>` carrying `t=get`, `expiration` and a `server` and/or `x`
//!   scope (`crates/buzz-media/src/auth.rs` `verify_blossom_get_auth`;
//!   Desktop's server-scoped token is `desktop/src-tauri/src/commands/media.rs`
//!   `sign_blossom_get_auth_header`, 600 s). Here the token is also bound to the
//!   one blob with `x`.
//! - Uploads: `PUT /upload` (BUD-02), raw body, `X-SHA-256`, a `t=upload`
//!   token with `x`, `expiration` and `server` (`media.rs`
//!   `sign_blossom_upload_auth`, 300 s, 3600 s for video), answered with a
//!   `BlobDescriptor` (`crates/buzz-media/src/types.rs`) whose URL the relay
//!   rewrites to the tenant host (`crates/buzz-relay/src/api/media.rs`
//!   `rewrite_descriptor_urls_for_tenant`).
//!
//! Every downloaded byte is hashed while it is written to a private staging
//! file; only a file whose size and SHA-256 equal the attachment's is moved to
//! the Downloads directory (mode 0600, never executable) or the preview cache.
//! Nothing downloaded is ever executed; `xdg-open` is run only on a file this
//! helper saved, on the user's click.
use crate::attachments::{self, Attachment};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use nostr::{
    hashes::{sha256, Hash, HashEngine},
    EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp,
};
use reqwest::{header::AUTHORIZATION, redirect::Policy, StatusCode};
use std::{
    io::{Read, Seek},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

/// Verified previews listed in `status.thumbnails` and kept on disk.
pub const THUMBNAILS: usize = 64;
const THUMB_CACHE_BYTES: u64 = 256 * 1024 * 1024;
/// Longest preview path published (the state directory is the user's).
pub const THUMB_PATH_BYTES: usize = 256;
/// Largest image the panel previews inline.
pub const THUMB_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
/// Queued preview requests.
pub const THUMB_QUEUE: usize = 16;
/// Pending attachments per draft and in all.
pub const PENDING: usize = 4;
pub const PENDING_TOTAL: usize = 16;
/// Hard cap for any download.
pub const DOWNLOAD_CAP: u64 = 1 << 30;
/// The relay's default upload limits (`crates/buzz-relay/src/config.rs:857-872`):
/// generic files 100 MiB, images 50 MiB, GIF 10 MiB. Video is capped like a file.
pub const FILE_LIMIT: u64 = 100 * 1024 * 1024;
pub const IMAGE_LIMIT: u64 = 50 * 1024 * 1024;
pub const GIF_LIMIT: u64 = 10 * 1024 * 1024;
/// Desktop's `MEDIA_GET_AUTH_EXPIRY_SECS`.
pub const GET_EXPIRY: u64 = 600;
const DESCRIPTOR_BYTES: usize = 16 * 1024;
const RECENT: usize = 32;
/// The only program started for an attachment, with the file as its only argument.
pub const OPENER: &str = "/usr/bin/xdg-open";

/// Fixed categories for transfers.
#[cfg(test)]
pub const CATEGORIES: [&str; 9] = [
    "attachment_unknown",
    "attachment_forbidden",
    "attachment_mismatch",
    "attachment_too_large",
    "attachment_invalid",
    "attachment_type_refused",
    "attachment_storage_unavailable",
    "relay_unavailable",
    "setup_busy",
];
const STORAGE: &str = "attachment_storage_unavailable";

/// Where transfers land. Resolved from the environment by the daemon; tests
/// pass private temporary directories.
#[derive(Clone, Debug)]
pub struct Dirs {
    /// Partial downloads: `$XDG_STATE_HOME/omarchy-buzz/downloads`.
    pub staging: PathBuf,
    /// Verified previews: `$XDG_STATE_HOME/omarchy-buzz/thumbs`.
    pub thumbs: PathBuf,
    /// Saved attachments: `~/Downloads`.
    pub downloads: PathBuf,
}
impl Dirs {
    pub fn from_env() -> Result<Self, &'static str> {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or(STORAGE)?);
        let state = match std::env::var_os("XDG_STATE_HOME") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => home.join(".local/state"),
        };
        if !home.is_absolute() || !state.is_absolute() {
            return Err(STORAGE);
        }
        Ok(Self {
            staging: state.join("omarchy-buzz/downloads"),
            thumbs: state.join("omarchy-buzz/thumbs"),
            downloads: home.join("Downloads"),
        })
    }
}

/// A draft's scope: `<roomId>` for the room composer, `<roomId>:<rootId>` for a thread's.
pub fn scope(room: &str, root: Option<&str>) -> String {
    match root {
        Some(root) => format!("{room}:{root}"),
        None => room.to_owned(),
    }
}
/// The attachment `hash` on the held row `event_id`: the channel rows shown
/// (head and older pages) or the open thread's replies.
pub fn held(status: &crate::protocol::Status, event_id: &str, hash: &str) -> Option<Attachment> {
    status
        .history
        .rows
        .iter()
        .chain(status.thread.rows.iter().map(|r| &r.row))
        .filter(|row| row.id == event_id && !row.unavailable)
        .flat_map(|row| row.attachments.iter())
        .find(|a| a.hash == hash)
        .cloned()
}
/// `status.download` views.
pub fn downloading(event_id: &str, attachment: &Attachment) -> crate::protocol::Download {
    crate::protocol::Download {
        state: "downloading".into(),
        event_id: Some(event_id.to_owned()),
        hash: Some(attachment.hash.clone()),
        path: None,
        received: 0,
        size: Some(attachment.size),
        category: None,
    }
}
fn uid() -> u32 {
    rustix::process::geteuid().as_raw()
}
/// Create (if needed) a directory only this user can use; refuse a link or a
/// directory owned by someone else.
pub fn private_dir(path: &Path) -> Result<(), &'static str> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| STORAGE)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(STORAGE),
    }
    let m = std::fs::symlink_metadata(path).map_err(|_| STORAGE)?;
    if !m.is_dir() || m.uid() != uid() {
        return Err(STORAGE);
    }
    if m.mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| STORAGE)?;
    }
    Ok(())
}
/// The Downloads directory: created private when missing; it may be a link
/// (to another disk) but must resolve to a directory this user owns.
fn downloads_dir(path: &Path) -> Result<(), &'static str> {
    if std::fs::symlink_metadata(path).is_err() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| STORAGE)?;
    }
    let m = std::fs::metadata(path).map_err(|_| STORAGE)?;
    if !m.is_dir() || m.uid() != uid() {
        return Err(STORAGE);
    }
    Ok(())
}

/// `host[:port]` of the configured relay: Desktop's `extract_server_authority`.
pub fn server(relay: &str) -> Result<String, &'static str> {
    let url = crate::join::http_url(relay, "/")?;
    let host = url.host_str().ok_or("relay_unavailable")?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}
/// A Blossom kind 24242 token (`verb` is `get` or `upload`) bound to one blob
/// and this relay, as an `Authorization` value: `Nostr <base64url event>`.
pub fn authorization(
    keys: &Keys,
    verb: &str,
    hash: &str,
    server: &str,
    expiry: u64,
    now: u64,
) -> Result<String, &'static str> {
    let content = if verb == "get" {
        "Get buzz-media"
    } else {
        "Upload buzz-media"
    };
    let expiration = (now + expiry).to_string();
    let tags = [
        ["t", verb],
        ["x", hash],
        ["expiration", expiration.as_str()],
        ["server", server],
    ]
    .into_iter()
    .map(Tag::parse)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| "relay_unavailable")?;
    let event = EventBuilder::new(Kind::Custom(24242), content)
        .tags(tags)
        .custom_created_at(Timestamp::from(now))
        .sign_with_keys(keys)
        .map_err(|_| "relay_unavailable")?;
    Ok(format!(
        "Nostr {}",
        URL_SAFE_NO_PAD.encode(event.as_json().as_bytes())
    ))
}
fn now() -> u64 {
    Timestamp::now().as_secs()
}
/// No proxy, redirect, retry or decompression: the bytes hashed are the bytes
/// the relay sent, and the token never leaves the relay origin.
fn client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "relay_unavailable")
}

/// Downloads `attachment` into a new private staging file and returns its
/// path once the size and SHA-256 match. `progress` counts verified bytes
/// written so far. Any failure removes the staging file.
pub async fn fetch(
    keys: &Keys,
    relay: &str,
    attachment: &Attachment,
    staging: &Path,
    cap: u64,
    progress: &AtomicU64,
) -> Result<PathBuf, &'static str> {
    if attachment.size > cap.min(DOWNLOAD_CAP) {
        return Err("attachment_too_large");
    }
    let origin = attachments::origin(relay)?;
    if attachments::media_path(&attachment.url, &origin).map(|(hash, _)| hash)
        != Some(attachment.hash.as_str())
    {
        return Err("attachment_unknown");
    }
    let auth = authorization(
        keys,
        "get",
        &attachment.hash,
        &server(relay)?,
        GET_EXPIRY,
        now(),
    )?;
    private_dir(staging)?;
    let temp = staging.join(format!("{}.part", uuid::Uuid::new_v4()));
    let result = stream_to(&temp, attachment, cap, &auth, progress).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map(|()| temp)
}
async fn stream_to(
    temp: &Path,
    attachment: &Attachment,
    cap: u64,
    auth: &str,
    progress: &AtomicU64,
) -> Result<(), &'static str> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temp)
        .await
        .map_err(|_| STORAGE)?;
    let mut response = client()?
        .get(&attachment.url)
        .header(AUTHORIZATION, auth)
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err("attachment_forbidden"),
        _ => return Err("relay_unavailable"),
    }
    let limit = cap.min(DOWNLOAD_CAP);
    if let Some(length) = response.content_length() {
        if length > limit {
            return Err("attachment_too_large");
        }
        if length != attachment.size {
            return Err("attachment_mismatch");
        }
    }
    let mut engine = sha256::Hash::engine();
    let mut total = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(|_| "relay_unavailable")? {
        total = total.saturating_add(chunk.len() as u64);
        if total > limit {
            return Err("attachment_too_large");
        }
        if total > attachment.size {
            // More than declared: cut off, never kept.
            return Err("attachment_mismatch");
        }
        engine.input(&chunk);
        file.write_all(&chunk).await.map_err(|_| STORAGE)?;
        progress.store(total, Ordering::Relaxed);
    }
    if total != attachment.size || sha256::Hash::from_engine(engine).to_string() != attachment.hash
    {
        return Err("attachment_mismatch");
    }
    file.sync_all().await.map_err(|_| STORAGE)?;
    Ok(())
}

/// A save name: no separators, NUL, `.`/`..` or leading dot.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains(['/', '\\', '\0'])
        && !name.starts_with('.')
        && !name.chars().any(char::is_control)
}
/// `name (n).ext`, or `name (n)` without an extension.
pub fn numbered(name: &str, n: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
}
fn recent() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    static RECENT_PATHS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    RECENT_PATHS.lock().unwrap_or_else(|e| e.into_inner())
}
/// Move a verified staging file into `downloads` as `name`, never replacing
/// an existing file: ` (2)`, ` (3)`… before the extension on collision.
pub fn save(temp: &Path, downloads: &Path, name: &str) -> Result<PathBuf, &'static str> {
    let result = save_inner(temp, downloads, name);
    let _ = std::fs::remove_file(temp);
    let path = result?;
    let mut recent = recent();
    recent.retain(|p| p != &path);
    recent.push(path.clone());
    if recent.len() > RECENT {
        recent.remove(0);
    }
    Ok(path)
}
fn save_inner(temp: &Path, downloads: &Path, name: &str) -> Result<PathBuf, &'static str> {
    if !safe_name(name) {
        return Err("attachment_unknown");
    }
    downloads_dir(downloads)?;
    for n in 1..=100 {
        let candidate = if n == 1 {
            name.to_owned()
        } else {
            numbered(name, n)
        };
        if !safe_name(&candidate) {
            return Err("attachment_unknown");
        }
        let dest = downloads.join(&candidate);
        match std::fs::hard_link(temp, &dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                match copy_new(temp, &dest) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => {
                        let _ = std::fs::remove_file(&dest);
                        return Err(STORAGE);
                    }
                }
            }
            Err(_) => return Err(STORAGE),
        }
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| STORAGE)?;
        return Ok(dest);
    }
    Err(STORAGE)
}
fn copy_new(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut source = std::fs::File::open(from)?;
    let mut dest = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)?;
    std::io::copy(&mut source, &mut dest)?;
    dest.sync_all()
}
/// A path `open_download` may hand to `xdg-open`: one this helper saved
/// recently, directly in `downloads`, still a regular non-executable file of
/// this user.
pub fn openable(path: &str, downloads: &Path) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(path);
    if !recent().contains(&path) || path.parent() != Some(downloads) {
        return Err("attachment_unknown");
    }
    let m = std::fs::symlink_metadata(&path).map_err(|_| "attachment_unknown")?;
    if !m.is_file() || m.uid() != uid() || m.mode() & 0o111 != 0 {
        return Err("attachment_unknown");
    }
    Ok(path)
}
/// Exactly `xdg-open <path>`.
pub fn open_argv(path: &Path) -> Vec<String> {
    vec![OPENER.to_owned(), path.to_string_lossy().into_owned()]
}
/// Start the opener with the session environment only (the variables
/// `agent-login` gets) and reap it in the background.
pub fn open(path: &Path) -> Result<(), &'static str> {
    let argv = open_argv(path);
    let env = crate::agents_service::harness::login_environment(std::env::vars_os());
    let mut child = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "attachment_unknown")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The first bytes of a file identify it as one of the preview images.
pub fn sniff_image(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
fn image_ext(mime: &str) -> Option<&'static str> {
    match mime {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}
fn head_of(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut head = vec![0; 16];
    let mut file = std::fs::File::open(path)?;
    let mut read = 0;
    while read < head.len() {
        let n = file.read(&mut head[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    head.truncate(read);
    Ok(head)
}
fn hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    hash_reader(&mut file)
}
fn hash_reader(reader: &mut impl Read) -> std::io::Result<(String, u64)> {
    let mut engine = sha256::Hash::engine();
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        engine.input(&buffer[..n]);
        total += n as u64;
    }
    Ok((sha256::Hash::from_engine(engine).to_string(), total))
}
/// `thumbs/<hash>.<ext>` for a preview image.
pub fn thumb_path(thumbs: &Path, attachment: &Attachment) -> Option<PathBuf> {
    let ext = image_ext(&attachment.mime)?;
    Some(thumbs.join(format!("{}.{ext}", attachment.hash)))
}
/// A preview already on disk whose bytes still hash to the attachment (its
/// modification time is refreshed for the LRU).
pub fn cached(thumbs: &Path, attachment: &Attachment) -> Option<PathBuf> {
    let path = thumb_path(thumbs, attachment)?;
    let m = std::fs::symlink_metadata(&path).ok()?;
    if !m.is_file() || m.uid() != uid() || m.len() != attachment.size {
        return None;
    }
    let (hash, size) = hash_file(&path).ok()?;
    if hash != attachment.hash || size != attachment.size {
        return None;
    }
    if sniff_image(&head_of(&path).ok()?) != Some(attachment.mime.as_str()) {
        return None;
    }
    let file = std::fs::File::options().write(true).open(&path).ok()?;
    let _ = file.set_modified(std::time::SystemTime::now());
    Some(path)
}
/// Keep a verified staging file as the preview for `attachment` when its
/// bytes are the declared image type, then bound the cache.
pub fn keep_thumbnail(
    temp: &Path,
    thumbs: &Path,
    attachment: &Attachment,
) -> Result<PathBuf, &'static str> {
    let result = (|| {
        let path = thumb_path(thumbs, attachment).ok_or("attachment_too_large")?;
        let head = head_of(temp).map_err(|_| STORAGE)?;
        if sniff_image(&head) != Some(attachment.mime.as_str()) {
            return Err("attachment_mismatch");
        }
        private_dir(thumbs)?;
        std::fs::rename(temp, &path).map_err(|_| STORAGE)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| STORAGE)?;
        prune(thumbs, &path);
        Ok(path)
    })();
    let _ = std::fs::remove_file(temp);
    result
}
/// Least recently used first out: at most `THUMBNAILS` files and
/// `THUMB_CACHE_BYTES`, always keeping `keep`. Only `<hash>.<ext>` files.
pub fn prune(thumbs: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(thumbs) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (hash, ext) = name.split_once('.')?;
            if !attachments::is_hash(hash) || !matches!(ext, "jpg" | "png" | "gif" | "webp") {
                return None;
            }
            let m = std::fs::symlink_metadata(entry.path()).ok()?;
            if !m.is_file() {
                return None;
            }
            Some((m.modified().ok()?, m.len(), entry.path()))
        })
        .collect();
    files.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| (b.2 == keep).cmp(&(a.2 == keep)))
    });
    if let Some(index) = files.iter().position(|f| f.2 == keep) {
        let entry = files.remove(index);
        files.insert(0, entry);
    }
    let mut count = 0;
    let mut bytes = 0_u64;
    for (_, size, path) in files {
        count += 1;
        bytes = bytes.saturating_add(size);
        if path != keep && (count > THUMBNAILS || bytes > THUMB_CACHE_BYTES) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// A file the user chose, checked before anything is read or sent.
#[derive(Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    /// Advisory `Content-Type`; the relay's answer decides the type sent.
    pub mime: &'static str,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
}
/// The relay refuses these (`validation.rs` `BLOCKED_FILE_MIME_TYPES`, audio,
/// other image and video containers) or they are active content or programs.
const REFUSED_EXT: [&str; 40] = [
    "svg", "svgz", "js", "mjs", "cjs", "html", "htm", "xhtml", "xht", "exe", "dll", "msi", "com",
    "scr", "bat", "cmd", "apk", "dmg", "app", "so", "dylib", "elf", "appimage", "bmp", "tif",
    "tiff", "heic", "heif", "avif", "ico", "mp3", "wav", "flac", "ogg", "m4a", "aac", "opus",
    "mov", "webm", "mkv",
];
fn mime_for(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
}
/// Programs and active content recognised by their first bytes.
fn refused_magic(head: &[u8]) -> bool {
    const PROGRAMS: [&[u8]; 7] = [
        b"\x7fELF",
        b"MZ",
        &[0xFE, 0xED, 0xFA, 0xCE],
        &[0xFE, 0xED, 0xFA, 0xCF],
        &[0xCE, 0xFA, 0xED, 0xFE],
        &[0xCF, 0xFA, 0xED, 0xFE],
        &[0xCA, 0xFE, 0xBA, 0xBE],
    ];
    if PROGRAMS.iter().any(|m| head.starts_with(m)) {
        return true;
    }
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let text = text.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    text.starts_with("<svg") || text.starts_with("<!doctype") || text.starts_with("<html")
}
fn iso_bmff(head: &[u8]) -> bool {
    head.len() >= 12 && &head[4..8] == b"ftyp"
}
/// Check the chosen path: absolute, no `..`, a regular file owned by this
/// user (not a link), within the relay's limits and not refused by type.
pub fn check_upload(path: &str) -> Result<Candidate, &'static str> {
    if !crate::protocol::plain_absolute_path(path) {
        return Err("attachment_invalid");
    }
    let path = PathBuf::from(path);
    let m = std::fs::symlink_metadata(&path).map_err(|_| "attachment_invalid")?;
    if !m.is_file() || m.uid() != uid() {
        return Err("attachment_invalid");
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("attachment_invalid")?;
    let ext = file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if REFUSED_EXT.contains(&ext.as_str()) {
        return Err("attachment_type_refused");
    }
    let mime = mime_for(&ext);
    let size = m.len();
    if size == 0 {
        return Err("attachment_invalid");
    }
    let limit = match mime {
        "image/gif" => GIF_LIMIT,
        m if m.starts_with("image/") => IMAGE_LIMIT,
        _ => FILE_LIMIT,
    };
    if size > limit {
        return Err("attachment_too_large");
    }
    let head = head_of(&path).map_err(|_| "attachment_invalid")?;
    if refused_magic(&head) {
        return Err("attachment_type_refused");
    }
    let sniffed = sniff_image(&head);
    if mime.starts_with("image/") && sniffed != Some(mime)
        || !mime.starts_with("image/") && sniffed.is_some()
    {
        return Err("attachment_invalid");
    }
    if (mime == "video/mp4") != iso_bmff(&head) {
        return Err(if mime == "video/mp4" {
            "attachment_invalid"
        } else {
            "attachment_type_refused"
        });
    }
    Ok(Candidate {
        name: attachments::sanitize_name(file_name),
        path,
        mime,
        size,
        dev: m.dev(),
        ino: m.ino(),
    })
}
/// What the relay stored, validated against what was sent.
#[derive(Debug, PartialEq)]
pub struct Stored {
    pub url: String,
    pub mime: String,
    pub dim: Option<String>,
}
/// A `BlobDescriptor` for exactly this blob on this relay: known keys only,
/// the same hash and size, a media URL on the relay's origin with that hash,
/// a well-formed type (the declared image type for an image).
pub fn parse_descriptor(
    bytes: &[u8],
    origin: &str,
    hash: &str,
    size: u64,
    image: Option<&str>,
) -> Result<Stored, &'static str> {
    const INVALID: &str = "relay_unavailable";
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let object = value.as_object().ok_or(INVALID)?;
    const KNOWN: [&str; 9] = [
        "url", "sha256", "size", "type", "uploaded", "dim", "blurhash", "thumb", "duration",
    ];
    if object.keys().any(|k| !KNOWN.contains(&k.as_str())) {
        return Err(INVALID);
    }
    let text = |k: &str| object.get(k).and_then(serde_json::Value::as_str);
    if text("sha256") != Some(hash)
        || object.get("size").and_then(serde_json::Value::as_u64) != Some(size)
        || object
            .get("uploaded")
            .and_then(serde_json::Value::as_i64)
            .is_none()
    {
        return Err(INVALID);
    }
    let mime = text("type")
        .filter(|m| attachments::is_mime(m))
        .ok_or(INVALID)?;
    if image.is_some_and(|image| image != mime) || mime == "image/svg+xml" {
        return Err(INVALID);
    }
    let url = text("url")
        .filter(|u| u.len() <= attachments::URL_BYTES)
        .ok_or(INVALID)?;
    if attachments::media_path(url, origin).map(|(h, _)| h) != Some(hash) {
        return Err(INVALID);
    }
    let dim = match object.get("dim") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(d)) if attachments::is_dim(d) => Some(d.clone()),
        Some(_) => return Err(INVALID),
    };
    let optional = |k: &str, ok: fn(&serde_json::Value) -> bool| {
        object.get(k).is_none_or(|v| v.is_null() || ok(v))
    };
    if !optional("blurhash", |v| v.as_str().is_some_and(|s| s.len() <= 256))
        || !optional("thumb", serde_json::Value::is_string)
        || !optional("duration", serde_json::Value::is_number)
    {
        return Err(INVALID);
    }
    Ok(Stored {
        url: url.to_owned(),
        mime: mime.to_owned(),
        dim,
    })
}
/// Hash the chosen file, then `PUT /upload` it with a `t=upload` token.
/// Returns the pending attachment (without its draft scope). The file is
/// opened without following links and must still be the checked inode.
pub async fn upload(
    keys: &Keys,
    relay: &str,
    candidate: Candidate,
) -> Result<crate::protocol::PendingAttachment, &'static str> {
    let origin = attachments::origin(relay)?;
    let Candidate {
        path,
        name,
        mime,
        size,
        dev,
        ino,
    } = candidate;
    let (file, hash) = tokio::task::spawn_blocking(move || {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)
            .map_err(|_| "attachment_invalid")?;
        let m = file.metadata().map_err(|_| "attachment_invalid")?;
        if !m.is_file() || m.dev() != dev || m.ino() != ino || m.len() != size {
            return Err("attachment_invalid");
        }
        let (hash, total) = hash_reader(&mut file).map_err(|_| "attachment_invalid")?;
        if total != size {
            return Err("attachment_invalid");
        }
        file.rewind().map_err(|_| "attachment_invalid")?;
        Ok((file, hash))
    })
    .await
    .map_err(|_| "attachment_invalid")??;
    // Desktop's token lifetimes: five minutes, an hour for video.
    let expiry = if mime.starts_with("video/") {
        3600
    } else {
        300
    };
    let auth = authorization(keys, "upload", &hash, &server(relay)?, expiry, now())?;
    let url = format!("{origin}/upload");
    // The relay hashes what it receives; a file changed after hashing is refused.
    let body = reqwest::Body::from(tokio::fs::File::from_std(file));
    let response = client()?
        .put(url)
        .header(AUTHORIZATION, auth)
        .header("X-SHA-256", &hash)
        .header(reqwest::header::CONTENT_TYPE, mime)
        .header(reqwest::header::CONTENT_LENGTH, size)
        .body(body)
        .send()
        .await
        .map_err(|_| "relay_unavailable")?;
    match response.status() {
        StatusCode::OK | StatusCode::CREATED => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err("attachment_forbidden"),
        StatusCode::PAYLOAD_TOO_LARGE => return Err("attachment_too_large"),
        StatusCode::UNSUPPORTED_MEDIA_TYPE | StatusCode::UNPROCESSABLE_ENTITY => {
            return Err("attachment_type_refused")
        }
        _ => return Err("relay_unavailable"),
    }
    let bytes = crate::join::body(response, DESCRIPTOR_BYTES).await?;
    let image = mime.starts_with("image/").then_some(mime);
    let stored = parse_descriptor(&bytes, &origin, &hash, size, image)?;
    Ok(crate::protocol::PendingAttachment {
        scope: String::new(),
        name,
        mime: stored.mime,
        size,
        url: stored.url,
        hash,
        dim: stored.dim,
    })
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
