use super::*;
use nostr::{Event, JsonUtil};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDRsynthetic-png-bytes";

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("buzz-media-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn sha(bytes: &[u8]) -> String {
    sha256::Hash::hash(bytes).to_string()
}
fn attachment(relay: &str, body: &[u8], mime: &str, ext: &str, name: &str) -> Attachment {
    let hash = sha(body);
    Attachment {
        name: name.into(),
        mime: mime.into(),
        size: body.len() as u64,
        url: format!("{}/media/{hash}.{ext}", attachments::origin(relay).unwrap()),
        hash,
        dim: None,
        kind: attachments::kind(mime),
    }
}
fn response(status: u16, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} X\r\nconnection: close\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    let mut out = out.into_bytes();
    out.extend_from_slice(body);
    out
}
fn ok(body: &[u8]) -> Vec<u8> {
    response(200, &[("content-length", body.len().to_string())], body)
}
/// A scripted loopback relay origin: one raw response per connection; returns
/// each request's head and body (read by `content-length`).
async fn origin_fixture(
    script: Vec<Vec<u8>>,
) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut seen = Vec::new();
        for raw in script {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut byte = [0; 1];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                head.push(byte[0]);
            }
            let head = String::from_utf8(head).unwrap();
            let length = crate::join::tests::header(&head, "content-length")
                .map_or(0, |v| v.parse::<usize>().unwrap());
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            let _ = stream.write_all(&raw).await;
            let _ = stream.shutdown().await;
            seen.push((head, body));
        }
        seen
    });
    (relay, task)
}
/// The Blossom token on a request: decoded, signature checked, tags returned.
fn token(head: &str, keys: &Keys) -> (Event, Vec<Vec<String>>) {
    let value = crate::join::tests::header(head, "authorization").unwrap();
    let encoded = value.strip_prefix("Nostr ").unwrap();
    let event = Event::from_json(URL_SAFE_NO_PAD.decode(encoded).unwrap()).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey, keys.public_key());
    assert_eq!(event.kind.as_u16(), 24242);
    let tags = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();
    (event, tags)
}
fn tags_are(tags: &[Vec<String>], verb: &str, hash: &str, server: &str, expiry: u64, at: u64) {
    assert_eq!(tags.len(), 4);
    assert_eq!(tags[0], ["t", verb]);
    assert_eq!(tags[1], ["x", hash]);
    assert_eq!(tags[2][0], "expiration");
    assert_eq!(tags[2][1].parse::<u64>().unwrap(), at + expiry);
    assert_eq!(tags[3], ["server", server]);
}

#[test]
fn tokens_are_blossom_kind_24242_bound_to_the_blob_and_relay() {
    let keys = Keys::generate();
    assert_eq!(server("wss://relay.example/").unwrap(), "relay.example");
    assert_eq!(
        server("wss://relay.example:8443/").unwrap(),
        "relay.example:8443"
    );
    assert_eq!(server("ws://127.0.0.1:7000/").unwrap(), "127.0.0.1:7000");
    let value = authorization(
        &keys,
        "get",
        &"a".repeat(64),
        "relay.example",
        600,
        1_800_000_000,
    )
    .unwrap();
    let head = format!("authorization: {value}\r\n");
    let (event, tags) = token(&head, &keys);
    assert_eq!(event.content, "Get buzz-media");
    assert_eq!(event.created_at.as_secs(), 1_800_000_000);
    tags_are(
        &tags,
        "get",
        &"a".repeat(64),
        "relay.example",
        600,
        1_800_000_000,
    );
    // base64url without padding, as Desktop encodes it.
    assert!(!value.contains(['+', '/', '=']));
}

#[tokio::test]
async fn a_verified_download_is_saved_privately_with_a_unique_name() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let body = b"%PDF-1.7 synthetic".to_vec();
    let (relay, fixture) = origin_fixture(vec![ok(&body), ok(&body)]).await;
    let a = attachment(&relay, &body, "application/pdf", "pdf", "report.pdf");
    let (staging, downloads) = (temp_dir("staging"), temp_dir("downloads"));
    let progress = AtomicU64::new(0);
    let before = now();
    let temp = fetch(&keys, &relay, &a, &staging, DOWNLOAD_CAP, &progress)
        .await
        .unwrap();
    assert_eq!(progress.load(Ordering::Relaxed), body.len() as u64);
    assert_eq!(std::fs::read(&temp).unwrap(), body);
    let first = save(&temp, &downloads, &a.name).unwrap();
    assert_eq!(first, downloads.join("report.pdf"));
    assert!(!temp.exists());
    let temp = fetch(&keys, &relay, &a, &staging, DOWNLOAD_CAP, &progress)
        .await
        .unwrap();
    let second = save(&temp, &downloads, &a.name).unwrap();
    assert_eq!(second, downloads.join("report (2).pdf"));
    for path in [&first, &second] {
        let m = std::fs::metadata(path).unwrap();
        assert_eq!(m.mode() & 0o777, 0o600, "never executable, private");
        assert_eq!(m.nlink(), 1);
        assert_eq!(std::fs::read(path).unwrap(), body);
    }
    assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
    let seen = fixture.await.unwrap();
    let port = relay.trim_start_matches("ws://").trim_end_matches('/');
    for (head, request_body) in &seen {
        assert!(
            head.starts_with(&format!("GET /media/{}.pdf HTTP/1.1\r\n", a.hash)),
            "{head}"
        );
        assert!(request_body.is_empty());
        let (event, tags) = token(head, &keys);
        assert_eq!(event.content, "Get buzz-media");
        let at = event.created_at.as_secs();
        assert!(at >= before && at <= now());
        tags_are(&tags, "get", &a.hash, port, GET_EXPIRY, at);
    }
    // `open_download` accepts exactly the files saved here.
    assert_eq!(
        openable(first.to_str().unwrap(), &downloads).unwrap(),
        first
    );
    assert_eq!(
        open_argv(&first),
        vec![OPENER.to_string(), first.to_string_lossy().into_owned()]
    );
    std::fs::write(downloads.join("planted.pdf"), b"x").unwrap();
    for refused in [
        downloads.join("planted.pdf"),
        downloads.join("missing.pdf"),
        staging.join("report.pdf"),
        PathBuf::from("/etc/passwd"),
    ] {
        assert_eq!(
            openable(refused.to_str().unwrap(), &downloads),
            Err("attachment_unknown")
        );
    }
    std::fs::set_permissions(&second, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        openable(second.to_str().unwrap(), &downloads),
        Err("attachment_unknown")
    );
    std::fs::remove_dir_all(staging).unwrap();
    std::fs::remove_dir_all(downloads).unwrap();
}

#[tokio::test]
async fn refused_and_altered_downloads_leave_nothing_behind() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let body = PNG.to_vec();
    let mut altered = body.clone();
    *altered.last_mut().unwrap() ^= 1;
    let mut longer = body.clone();
    longer.extend_from_slice(&[0; 4096]);
    let script = vec![
        ok(&altered),
        response(403, &[("content-length", "0".into())], b""),
        response(401, &[("content-length", "0".into())], b""),
        // No length: the body runs past the declared size and is cut off.
        response(200, &[], &longer),
        // A declared length other than the attachment's.
        ok(&longer),
        response(
            302,
            &[
                ("location", "https://elsewhere.example/x".into()),
                ("content-length", "0".into()),
            ],
            b"",
        ),
        response(404, &[("content-length", "0".into())], b""),
        // Too short.
        response(200, &[], &body[..body.len() - 1]),
    ];
    let (relay, fixture) = origin_fixture(script).await;
    let a = attachment(&relay, &body, "image/png", "png", "a.png");
    let staging = temp_dir("refused");
    let progress = AtomicU64::new(0);
    for expected in [
        "attachment_mismatch",
        "attachment_forbidden",
        "attachment_forbidden",
        "attachment_mismatch",
        "attachment_mismatch",
        "relay_unavailable",
        "relay_unavailable",
        "attachment_mismatch",
    ] {
        assert_eq!(
            fetch(&keys, &relay, &a, &staging, DOWNLOAD_CAP, &progress).await,
            Err(expected)
        );
        assert_eq!(
            std::fs::read_dir(&staging).unwrap().count(),
            0,
            "{expected}"
        );
    }
    // Limits refuse before any request; so does a URL off the relay.
    assert_eq!(
        fetch(&keys, &relay, &a, &staging, a.size - 1, &progress).await,
        Err("attachment_too_large")
    );
    let mut elsewhere = a.clone();
    elsewhere.url = format!("https://elsewhere.example/media/{}.png", a.hash);
    assert_eq!(
        fetch(&keys, &relay, &elsewhere, &staging, DOWNLOAD_CAP, &progress).await,
        Err("attachment_unknown")
    );
    assert_eq!(fixture.await.unwrap().len(), 8);
    std::fs::remove_dir_all(staging).unwrap();
}

#[tokio::test]
async fn an_oversized_length_is_refused_before_the_body() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let body = PNG.to_vec();
    let (relay, fixture) = origin_fixture(vec![response(
        200,
        &[("content-length", (THUMB_SOURCE_BYTES + 1).to_string())],
        &body,
    )])
    .await;
    let mut a = attachment(&relay, &body, "image/png", "png", "a.png");
    a.size = THUMB_SOURCE_BYTES;
    let staging = temp_dir("oversized");
    assert_eq!(
        fetch(
            &keys,
            &relay,
            &a,
            &staging,
            THUMB_SOURCE_BYTES,
            &AtomicU64::new(0)
        )
        .await,
        Err("attachment_too_large")
    );
    fixture.await.unwrap();
    std::fs::remove_dir_all(staging).unwrap();
}

#[test]
fn save_names_never_escape_the_downloads_directory() {
    assert_eq!(numbered("a.tar.gz", 2), "a.tar (2).gz");
    assert_eq!(numbered("README", 3), "README (3)");
    for bad in [
        "", ".", "..", "../x", "a/b", "a\\b", ".hidden", "a\0b", "a\nb",
    ] {
        assert!(!safe_name(bad), "{bad:?}");
    }
    let (staging, downloads) = (temp_dir("names"), temp_dir("names-out"));
    let temp = staging.join("t.part");
    std::fs::write(&temp, b"x").unwrap();
    assert_eq!(save(&temp, &downloads, "../x"), Err("attachment_unknown"));
    assert!(!temp.exists(), "a refused staging file is removed");
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    std::fs::remove_dir_all(staging).unwrap();
    std::fs::remove_dir_all(downloads).unwrap();
}

#[test]
fn previews_are_verified_images_in_a_bounded_cache() {
    let (staging, thumbs) = (temp_dir("thumb-staging"), temp_dir("thumbs"));
    let a = attachment("wss://relay.example/", PNG, "image/png", "png", "a.png");
    let temp = staging.join("a.part");
    std::fs::write(&temp, PNG).unwrap();
    let path = keep_thumbnail(&temp, &thumbs, &a).unwrap();
    assert_eq!(path, thumbs.join(format!("{}.png", a.hash)));
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(cached(&thumbs, &a), Some(path.clone()));
    // Bytes that are not the declared image type are not kept.
    let mut gif = a.clone();
    gif.mime = "image/gif".into();
    std::fs::write(&temp, PNG).unwrap();
    assert_eq!(
        keep_thumbnail(&temp, &thumbs, &gif),
        Err("attachment_mismatch")
    );
    assert!(!temp.exists());
    // A changed cache file is not trusted.
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nchanged-bytes-of-same-lengthxx").unwrap();
    assert_eq!(cached(&thumbs, &a), None);
    // At most THUMBNAILS files; the oldest go first, the one just kept stays.
    let old = std::time::SystemTime::now() - Duration::from_secs(3600);
    for n in 0..(THUMBNAILS + 5) {
        let file = thumbs.join(format!("{n:064x}.jpg"));
        std::fs::write(&file, b"x").unwrap();
        let f = std::fs::File::options().write(true).open(&file).unwrap();
        f.set_modified(old + Duration::from_secs(n as u64)).unwrap();
    }
    std::fs::write(thumbs.join("notes.txt"), b"not ours").unwrap();
    let keep = thumbs.join(format!("{:064x}.jpg", 0));
    prune(&thumbs, &keep);
    let left: Vec<String> = std::fs::read_dir(&thumbs)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left.len(), THUMBNAILS + 1, "{left:?}");
    assert!(left.contains(&"notes.txt".to_string()));
    assert!(left.contains(&format!("{:064x}.jpg", 0)));
    assert!(!left.contains(&format!("{:064x}.jpg", 1)));
    assert!(left.contains(&format!("{:064x}.jpg", THUMBNAILS + 4)));
    std::fs::remove_dir_all(staging).unwrap();
    std::fs::remove_dir_all(thumbs).unwrap();
}

#[test]
fn upload_candidates_are_checked_before_anything_is_read_or_sent() {
    let dir = temp_dir("upload-check");
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    };
    let png = write("shot.png", PNG);
    let c = check_upload(&png).unwrap();
    assert_eq!(
        (c.name.as_str(), c.mime, c.size),
        ("shot.png", "image/png", PNG.len() as u64)
    );
    let doc = write("notes.md", b"# notes");
    assert_eq!(check_upload(&doc).unwrap().mime, "text/markdown");
    let bin = write("data", b"\x00\x01opaque");
    assert_eq!(check_upload(&bin).unwrap().mime, "application/octet-stream");
    std::os::unix::fs::symlink(&png, dir.join("link.png")).unwrap();
    let cases: Vec<(String, &str)> = vec![
        ("shot.png".into(), "attachment_invalid"),
        (
            format!("{}/../shot.png", dir.display()),
            "attachment_invalid",
        ),
        (dir.to_string_lossy().into_owned(), "attachment_invalid"),
        (
            dir.join("link.png").to_string_lossy().into_owned(),
            "attachment_invalid",
        ),
        (
            dir.join("missing.pdf").to_string_lossy().into_owned(),
            "attachment_invalid",
        ),
        (write("empty.txt", b""), "attachment_invalid"),
        (write("x.svg", b"<svg/>"), "attachment_type_refused"),
        (write("x.SVG", b"<svg/>"), "attachment_type_refused"),
        (write("x.js", b"alert(1)"), "attachment_type_refused"),
        (write("x.html", b"<p>"), "attachment_type_refused"),
        (write("x.exe", b"MZ"), "attachment_type_refused"),
        (write("x.mp3", b"ID3"), "attachment_type_refused"),
        (
            write("program", b"\x7fELF\x02\x01"),
            "attachment_type_refused",
        ),
        (
            write("page.txt", b"  <!DOCTYPE html><html>"),
            "attachment_type_refused",
        ),
        (
            write("image.txt", b"<svg xmlns='x'/>"),
            "attachment_type_refused",
        ),
        (
            write("clip.bin", b"\x00\x00\x00\x18ftypisom\x00\x00"),
            "attachment_type_refused",
        ),
        (
            write("fake.png", b"\xff\xd8\xff\xe0jpeg"),
            "attachment_invalid",
        ),
        (write("png.txt", PNG), "attachment_invalid"),
        (write("clip.mp4", b"not a video"), "attachment_invalid"),
    ];
    for (path, expected) in cases {
        assert_eq!(check_upload(&path).map(|_| ()), Err(expected), "{path}");
    }
    let big = std::fs::File::create(dir.join("big.pdf")).unwrap();
    big.set_len(FILE_LIMIT + 1).unwrap();
    assert_eq!(
        check_upload(dir.join("big.pdf").to_str().unwrap()).map(|_| ()),
        Err("attachment_too_large")
    );
    let image = std::fs::File::create(dir.join("big.png")).unwrap();
    image.set_len(IMAGE_LIMIT + 1).unwrap();
    assert_eq!(
        check_upload(dir.join("big.png").to_str().unwrap()).map(|_| ()),
        Err("attachment_too_large")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn blob_descriptors_are_strict() {
    let origin = "https://relay.example";
    let hash = sha(PNG);
    let good = serde_json::json!({
        "url": format!("{origin}/media/{hash}.png"), "sha256": hash, "size": PNG.len(),
        "type": "image/png", "uploaded": 1_800_000_000, "dim": "1x1",
        "blurhash": "LEHV6nWB2yk8", "thumb": format!("{origin}/media/{hash}.thumb.jpg"),
    });
    let parse = |v: &serde_json::Value| {
        parse_descriptor(
            &serde_json::to_vec(v).unwrap(),
            origin,
            &hash,
            PNG.len() as u64,
            Some("image/png"),
        )
    };
    assert_eq!(
        parse(&good).unwrap(),
        Stored {
            url: format!("{origin}/media/{hash}.png"),
            mime: "image/png".into(),
            dim: Some("1x1".into())
        }
    );
    let mut minimal = good.clone();
    for key in ["dim", "blurhash", "thumb"] {
        minimal.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(parse(&minimal).unwrap().dim, None);
    for (key, value) in [
        ("sha256", serde_json::json!("0".repeat(64))),
        ("size", serde_json::json!(1)),
        ("size", serde_json::json!("12")),
        ("type", serde_json::json!("image/jpeg")),
        ("type", serde_json::json!("image/svg+xml")),
        ("type", serde_json::json!("Image/PNG")),
        (
            "url",
            serde_json::json!(format!("https://other.example/media/{hash}.png")),
        ),
        (
            "url",
            serde_json::json!(format!("{origin}/media/{}.png", "0".repeat(64))),
        ),
        (
            "url",
            serde_json::json!(format!("{origin}/media/{hash}.thumb.jpg")),
        ),
        ("uploaded", serde_json::json!("now")),
        ("dim", serde_json::json!("big")),
        ("duration", serde_json::json!("long")),
        ("extra", serde_json::json!(1)),
    ] {
        let mut bad = good.clone();
        bad[key] = value.clone();
        assert_eq!(parse(&bad), Err("relay_unavailable"), "{key}={value}");
    }
    for key in ["url", "sha256", "size", "type", "uploaded"] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert_eq!(parse(&missing), Err("relay_unavailable"), "{key}");
    }
    assert_eq!(
        parse_descriptor(b"[]", origin, &hash, 1, None),
        Err("relay_unavailable")
    );
}

#[tokio::test]
async fn uploads_send_the_exact_blossom_request_and_answers_are_classified() {
    let _guard = crate::NETWORK_TEST_LOCK.lock().await;
    let keys = Keys::generate();
    let dir = temp_dir("upload");
    let path = dir.join("shot.png");
    std::fs::write(&path, PNG).unwrap();
    let hash = sha(PNG);
    let listener_script = |relay: &str| {
        let origin = attachments::origin(relay).unwrap();
        let descriptor = serde_json::json!({
            "url": format!("{origin}/media/{hash}.png"), "sha256": hash, "size": PNG.len(),
            "type": "image/png", "uploaded": 1_800_000_000, "dim": "1x1",
        })
        .to_string();
        descriptor
    };
    // The fixture needs the address before the script: bind it first.
    let (relay, fixture) = {
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = probe.local_addr().unwrap();
        drop(probe);
        let relay = format!("ws://{addr}/");
        let descriptor = listener_script(&relay);
        let listener = TcpListener::bind(addr).await.unwrap();
        let script: Vec<Vec<u8>> = vec![
            ok(descriptor.as_bytes()),
            response(403, &[("content-length", "0".into())], b""),
            response(413, &[("content-length", "0".into())], b""),
            response(415, &[("content-length", "0".into())], b""),
            response(422, &[("content-length", "0".into())], b""),
            response(500, &[("content-length", "0".into())], b""),
            ok(br#"{"url":"x"}"#),
        ];
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            for raw in script {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    let mut byte = [0; 1];
                    assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
                    head.push(byte[0]);
                }
                let head = String::from_utf8(head).unwrap();
                let length = crate::join::tests::header(&head, "content-length")
                    .map_or(0, |v| v.parse::<usize>().unwrap());
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                stream.write_all(&raw).await.unwrap();
                seen.push((head, body));
            }
            seen
        });
        (relay, task)
    };
    let before = now();
    let first = upload(&keys, &relay, check_upload(path.to_str().unwrap()).unwrap())
        .await
        .unwrap();
    let origin = attachments::origin(&relay).unwrap();
    assert_eq!(
        first,
        crate::protocol::PendingAttachment {
            scope: String::new(),
            name: "shot.png".into(),
            mime: "image/png".into(),
            size: PNG.len() as u64,
            url: format!("{origin}/media/{hash}.png"),
            hash: hash.clone(),
            dim: Some("1x1".into()),
        }
    );
    for expected in [
        "attachment_forbidden",
        "attachment_too_large",
        "attachment_type_refused",
        "attachment_type_refused",
        "relay_unavailable",
        "relay_unavailable",
    ] {
        assert_eq!(
            upload(&keys, &relay, check_upload(path.to_str().unwrap()).unwrap()).await,
            Err(expected)
        );
    }
    let seen = fixture.await.unwrap();
    let port = relay.trim_start_matches("ws://").trim_end_matches('/');
    for (head, body) in &seen {
        assert!(head.starts_with("PUT /upload HTTP/1.1\r\n"), "{head}");
        let header = |name| crate::join::tests::header(head, name);
        assert_eq!(header("x-sha-256"), Some(hash.as_str()));
        assert_eq!(header("content-type"), Some("image/png"));
        assert_eq!(
            header("content-length"),
            Some(PNG.len().to_string().as_str())
        );
        assert_eq!(header("transfer-encoding"), None);
        assert_eq!(body.as_slice(), PNG);
        assert_eq!(sha(body), hash);
        let (event, tags) = token(head, &keys);
        assert_eq!(event.content, "Upload buzz-media");
        let at = event.created_at.as_secs();
        assert!(at >= before && at <= now());
        tags_are(&tags, "upload", &hash, port, 300, at);
    }
    // A file replaced after the check is refused before anything is sent.
    let candidate = check_upload(path.to_str().unwrap()).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"more")
        .unwrap();
    assert_eq!(
        upload(&keys, &relay, candidate).await,
        Err("attachment_invalid")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_category_is_fixed() {
    assert!(CATEGORIES
        .iter()
        .all(|c| c.len() < 40 && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
}
