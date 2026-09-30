use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

const ORIGIN: &str = "https://relay.example";
fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn url(n: u8, ext: &str) -> String {
    format!("{ORIGIN}/media/{}.{ext}", hash(n))
}
fn imeta(parts: &[&str]) -> Vec<String> {
    std::iter::once("imeta")
        .chain(parts.iter().copied())
        .map(str::to_owned)
        .collect()
}
fn png(n: u8) -> Vec<String> {
    imeta(&[
        &format!("url {}", url(n, "png")),
        "m image/png",
        &format!("x {}", hash(n)),
        "size 1234",
        "dim 640x480",
    ])
}
fn pdf(n: u8, name: &str) -> Vec<String> {
    imeta(&[
        &format!("url {}", url(n, "pdf")),
        "m application/pdf",
        &format!("x {}", hash(n)),
        "size 99",
        &format!("filename {name}"),
    ])
}
fn event(content: &str, tags: Vec<Vec<String>>) -> nostr::Event {
    EventBuilder::new(Kind::Custom(9), content)
        .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
        .custom_created_at(Timestamp::from(100))
        .sign_with_keys(&Keys::generate())
        .unwrap()
}
fn parsed(tags: Vec<Vec<String>>) -> Result<Vec<Attachment>, ()> {
    parse(&event("", tags).tags, ORIGIN)
}

#[test]
fn origin_follows_the_configured_relay() {
    assert_eq!(
        origin("wss://relay.example/").unwrap(),
        "https://relay.example"
    );
    assert_eq!(
        origin("wss://relay.example:8443/").unwrap(),
        "https://relay.example:8443"
    );
    assert_eq!(
        origin("ws://127.0.0.1:7000/").unwrap(),
        "http://127.0.0.1:7000"
    );
}

#[test]
fn valid_imeta_is_projected_exactly() {
    let list = parsed(vec![png(1), pdf(2, "Q3 report.pdf")]).unwrap();
    assert_eq!(
        list,
        vec![
            Attachment {
                name: format!("{}.png", hash(1)),
                mime: "image/png".into(),
                size: 1234,
                url: url(1, "png"),
                hash: hash(1),
                dim: Some("640x480".into()),
                kind: "image",
            },
            Attachment {
                name: "Q3 report.pdf".into(),
                mime: "application/pdf".into(),
                size: 99,
                url: url(2, "pdf"),
                hash: hash(2),
                dim: None,
                kind: "file",
            },
        ]
    );
    let video = imeta(&[
        &format!("url {}", url(3, "mp4")),
        "m video/mp4",
        &format!("x {}", hash(3)),
        "size 5",
        "duration 1.5",
        "blurhash LEHV6nWB2yk8",
        "alt a clip",
    ]);
    assert_eq!(parsed(vec![video]).unwrap()[0].kind, "video");
    // No attachments at all is not unavailable.
    assert_eq!(parsed(vec![]).unwrap(), vec![]);
    assert_eq!(
        serde_json::to_value(&list[1]).unwrap()["dim"],
        serde_json::Value::Null
    );
    // SVG is never an image the panel may preview.
    assert_eq!(kind("image/svg+xml"), "file");
}

#[test]
fn a_missing_required_key_is_malformed() {
    for key in ["url", "m", "x", "size"] {
        let tag: Vec<String> = png(1)
            .into_iter()
            .filter(|p| !p.starts_with(&format!("{key} ")))
            .collect();
        assert_eq!(parsed(vec![tag]), Err(()), "{key}");
    }
}

#[test]
fn only_media_urls_on_the_configured_relay_are_accepted() {
    for bad in [
        format!("https://other.example/media/{}.png", hash(1)),
        format!("http://relay.example/media/{}.png", hash(1)),
        format!("https://relay.example:444/media/{}.png", hash(1)),
        format!("https://relay.example.evil/media/{}.png", hash(1)),
        format!("/media/{}.png", hash(1)),
        format!("https://relay.example/files/{}.png", hash(1)),
        format!("https://relay.example/media/{}.thumb.jpg", hash(1)),
        format!("https://relay.example/media/{}", hash(1)),
        format!("https://relay.example/media/{}.PNG", hash(1)),
        format!("https://relay.example/media/{}.png?x=1", hash(1)),
        format!("https://relay.example/media/{}.toolongext", hash(1)),
        format!(
            "https://relay.example/media/{}.png",
            hash(0xab).to_uppercase()
        ),
    ] {
        let mut tag = png(1);
        tag[1] = format!("url {bad}");
        assert_eq!(parsed(vec![tag]), Err(()), "{bad}");
    }
}

#[test]
fn the_hash_must_match_the_url() {
    let mut tag = png(1);
    tag[3] = format!("x {}", hash(2));
    assert_eq!(parsed(vec![tag]), Err(()));
    let mut tag = png(1);
    tag[3] = format!("x {}", "g".repeat(64));
    assert_eq!(parsed(vec![tag]), Err(()));
}

#[test]
fn sizes_dimensions_types_and_keys_are_strict() {
    let edit = |index: usize, value: &str| {
        let mut tag = png(1);
        tag[index] = value.to_owned();
        parsed(vec![tag])
    };
    assert!(edit(4, &format!("size {}", MAX_SIZE)).is_ok());
    for bad in [
        format!("size {}", MAX_SIZE + 1),
        "size 0".into(),
        "size +5".into(),
        "size 05".into(),
        "size -1".into(),
        "size 1.5".into(),
        "size ".into(),
    ] {
        assert_eq!(edit(4, &bad), Err(()), "{bad}");
    }
    for bad in [
        "dim 0x10",
        "dim 16385x1",
        "dim 10",
        "dim 10x",
        "dim 010x10",
        "dim 1x1x1",
    ] {
        assert_eq!(edit(5, bad), Err(()), "{bad}");
    }
    assert!(edit(5, "dim 16384x16384").is_ok());
    for bad in [
        "m image/PNG",
        "m image",
        "m /png",
        "m image/png/x",
        "m image/p ng",
    ] {
        assert_eq!(edit(2, bad), Err(()), "{bad}");
    }
    assert_eq!(edit(2, &format!("m {}/x", "a".repeat(63))), Err(()));
    for bad in ["unknown 1", "novalue", "size 5"] {
        let mut tag = png(1);
        tag.push(bad.to_owned());
        assert_eq!(parsed(vec![tag]), Err(()), "{bad}");
    }
    for bad in ["filename a/b.pdf", "filename a\\b.pdf", "filename a\u{7}b"] {
        assert_eq!(
            parsed(vec![pdf(2, &bad["filename ".len()..])]),
            Err(()),
            "{bad}"
        );
    }
    assert_eq!(parsed(vec![pdf(2, &"a".repeat(256))]), Err(()));
    // The same blob twice in one message is not two attachments.
    assert_eq!(parsed(vec![png(1), png(1)]), Err(()));
}

#[test]
fn a_fifth_attachment_is_ignored() {
    let mut tags: Vec<Vec<String>> = (1..=4).map(png).collect();
    tags.push(imeta(&["url nowhere", "m x/y", "x 1", "size 1"]));
    let list = parsed(tags).unwrap();
    assert_eq!(list.len(), MAX);
    assert_eq!(list[3].hash, hash(4));
}

#[test]
fn names_are_sanitized_for_display_and_saving() {
    assert_eq!(sanitize_name("../../etc/passwd"), "_.._etc_passwd");
    assert_eq!(sanitize_name(".bashrc"), "bashrc");
    assert_eq!(sanitize_name("a\u{0}b\nc.txt"), "a_b_c.txt");
    assert_eq!(sanitize_name("   "), "attachment");
    assert_eq!(sanitize_name("..."), "attachment");
    let long = format!("{}.pdf", "x".repeat(300));
    let short = sanitize_name(&long);
    assert_eq!(short.chars().count(), NAME_CHARS);
    assert!(short.ends_with("x.pdf"));
    let wide = format!("{}.pdf", "😀".repeat(100));
    let short = sanitize_name(&wide);
    assert!(
        short.len() <= NAME_BYTES && short.ends_with("😀.pdf"),
        "{short}"
    );
}

#[test]
fn the_redundant_markdown_line_is_removed_once() {
    let image = &parsed(vec![png(1)]).unwrap()[0];
    let file = &parsed(vec![pdf(2, "a]b.pdf")]).unwrap()[0];
    let line = |a: &Attachment| markdown_line(&a.mime, &a.name, &a.url);
    assert_eq!(line(image), format!("![image]({})", url(1, "png")));
    assert_eq!(line(file), format!("[a\\]b.pdf]({})", url(2, "pdf")));
    let both = [image.clone(), file.clone()];
    let body = format!("hello\n{}\n{}", line(image), line(file));
    assert_eq!(strip_markdown(&body, &both), "hello");
    // Desktop's attachment-only message: the body starts with the newline.
    assert_eq!(strip_markdown(&format!("\n{}", line(image)), &both), "");
    assert_eq!(
        strip_markdown(&format!("hi\n{}\n", line(image)), &both),
        "hi"
    );
    // Any escaped label for the same URL is Desktop's file line.
    let relabeled = format!("hi\n[Quarterly \\[draft\\]]({})", url(2, "pdf"));
    assert_eq!(strip_markdown(&relabeled, &both), "hi");
    // Not the last line, another URL, a different form, or one line per
    // attachment only: left alone.
    for kept in [
        format!("{}\nhello", line(image)),
        format!("hi\n![image]({})", url(9, "png")),
        format!("hi\n![video]({})", url(1, "png")),
        format!("hi\n||![image]({})||", url(1, "png")),
        format!("hi\nsee ![image]({})", url(1, "png")),
        format!("hi\n[a]b]({})", url(2, "pdf")),
    ] {
        assert_eq!(strip_markdown(&kept, &both), kept.trim_end(), "{kept}");
    }
    assert_eq!(
        strip_markdown(&format!("hi\n{}\n{}", line(image), line(image)), &both),
        format!("hi\n{}", line(image))
    );
    // A malformed imeta keeps the whole content.
    let broken = event(
        &format!("hi\n{}", line(image)),
        vec![imeta(&["url x", "m a/b", "x y", "size 1"])],
    );
    assert_eq!(
        project(&broken, ORIGIN),
        (vec![], true, format!("hi\n{}", line(image)))
    );
}

#[test]
fn history_rows_carry_attachments_and_malformed_ones_do_not_reject_the_page() {
    let relay = Keys::parse(&format!("{:064x}", 1)).unwrap();
    let user = Keys::parse(&format!("{:064x}", 2)).unwrap();
    let room = uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let h = room.to_string();
    let signed = |key: &Keys, kind: u16, content: &str, tags: Vec<Vec<String>>, at: u64| {
        EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(key)
            .unwrap()
    };
    let with = signed(
        &user,
        9,
        &format!("look\n![image]({})", url(1, "png")),
        vec![vec!["h".into(), h.clone()], png(1)],
        100,
    );
    let mut bad = png(2);
    bad[3] = format!("x {}", hash(3));
    let broken = signed(
        &user,
        9,
        "broken",
        vec![vec!["h".into(), h.clone()], bad],
        101,
    );
    let bounds = signed(
        &relay,
        39006,
        r#"{"has_more":false,"next_cursor":null}"#,
        vec![
            vec!["h".into(), h.clone()],
            vec!["d".into(), format!("{h}:head")],
        ],
        200,
    );
    let page = crate::history::reduce(
        room,
        relay.public_key(),
        &[with.clone(), broken, bounds],
        200,
    )
    .unwrap();
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0].text, "look");
    assert_eq!(page.rows[0].attachments, parsed(vec![png(1)]).unwrap());
    assert!(!page.rows[0].attachments_unavailable);
    assert_eq!(page.rows[1].text, "broken");
    assert!(page.rows[1].attachments.is_empty() && page.rows[1].attachments_unavailable);
    // An edit's tags replace the original's.
    let edit = signed(
        &user,
        40003,
        "edited",
        vec![
            vec!["h".into(), h.clone()],
            vec!["e".into(), with.id.to_hex()],
            pdf(4, "b.pdf"),
        ],
        150,
    );
    let bounds = signed(
        &relay,
        39006,
        r#"{"has_more":false,"next_cursor":null}"#,
        vec![
            vec!["h".into(), h.clone()],
            vec!["d".into(), format!("{h}:head")],
        ],
        200,
    );
    let page =
        crate::history::reduce(room, relay.public_key(), &[with, edit, bounds], 200).unwrap();
    assert_eq!(page.rows[0].attachments[0].hash, hash(4));
    assert_eq!(page.rows[0].text, "edited");
}

#[test]
fn frames_keep_the_newest_attachments() {
    let row = |n: usize| crate::protocol::HistoryRow {
        reactions: None,
        thread: None,
        id: format!("{n:064x}"),
        author: "a".repeat(64),
        time: n as u64,
        text: String::new(),
        edited: false,
        truncated: false,
        unavailable: false,
        attachments: parsed((1..=3).map(png).collect()).unwrap(),
        attachments_unavailable: false,
    };
    let mut rows: Vec<_> = (0..4).map(row).collect();
    bound(rows.iter_mut(), 7);
    let counts: Vec<(usize, bool)> = rows
        .iter()
        .map(|r| (r.attachments.len(), r.attachments_unavailable))
        .collect();
    assert_eq!(counts, [(0, true), (0, true), (3, false), (3, false)]);
}
