use super::*;
use crate::protocol::StatusSet;
use crate::recipients::status;
use nostr::{EventBuilder, Kind};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn tags(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}
fn raw(author: &Keys, kind: u16, content: &str, tags: &[&[&str]], at: u64) -> Event {
    EventBuilder::new(Kind::Custom(kind), content)
        .tags(tags.iter().map(|t| Tag::parse(t.iter().copied()).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(author)
        .unwrap()
}
fn general(author: &Keys, content: &str, extra: &[&[&str]], at: u64) -> Event {
    let mut all: Vec<&[&str]> = vec![&["d", "general"]];
    all.extend_from_slice(extra);
    raw(author, KIND, content, &all, at)
}
const NOW: u64 = 1_800_000_000;
fn authenticated() -> Status {
    let mut status = Status::new(&crate::config::Config::default());
    status.connection = "authenticated".into();
    status
}
fn set(text: &str, emoji: Option<&str>, hours: Option<u32>) -> StatusIntent {
    StatusIntent {
        set: Some(StatusSet {
            text: text.into(),
            emoji: emoji.map(str::to_owned),
            hours,
        }),
    }
}

#[test]
fn builder_matches_the_pinned_sdk_plus_desktop_expiration() {
    let user = key(2);
    let event = build_event("  In a meeting ", Some("🗣️"), Some(NOW + 3600), &user, None).unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30315);
    assert_eq!(event.pubkey, user.public_key());
    assert_eq!(event.content, "In a meeting");
    let expires = (NOW + 3600).to_string();
    assert_eq!(
        tags(&event),
        vec![
            vec!["d".to_string(), "general".into()],
            vec!["emoji".into(), "🗣️".into()],
            vec!["expiration".into(), expires],
        ]
    );
    // Identical tags and content to the upstream builder for the same input.
    let upstream = buzz_sdk::build_user_status("  In a meeting ", Some("🗣️"))
        .unwrap()
        .sign_with_keys(&user)
        .unwrap();
    assert_eq!(upstream.content, event.content);
    assert_eq!(tags(&upstream), tags(&event)[..2].to_vec());
    // Clearing: the empty replacement, d tag only.
    let clear = build_event("", None, None, &user, None).unwrap();
    assert_eq!(clear.content, "");
    assert_eq!(tags(&clear), vec![vec!["d".to_string(), "general".into()]]);
    // A blank emoji is omitted, as upstream does.
    let blank = build_event("x", Some("  "), None, &user, None).unwrap();
    assert_eq!(tags(&blank), vec![vec!["d".to_string(), "general".into()]]);
}

#[test]
fn emoji_rule_accepts_glyphs_and_shortcodes_only() {
    for ok in [
        "🙂",
        "🗣️",
        "👍🏽",
        "👩‍💻",
        "🏳️‍🌈",
        "🇳🇱",
        "❤️",
        ":tada:",
        ":party_parrot:",
        ":+1:",
        ":a-b:",
        &format!(":{}:", "a".repeat(32)),
        "🙂🙂🙂🙂🙂🙂🙂🙂",
    ] {
        assert!(valid_emoji(ok), "{ok}");
    }
    for bad in [
        "",
        "a",
        "1",
        "ok",
        "#",
        "🙂a",
        "🙂 🙂",
        " 🙂",
        "🙂\n",
        "🙂\u{202e}",
        "\u{200d}🙂",
        "\u{fe0f}",
        "\u{1f3fb}",
        "🙂\u{200b}",
        "\u{feff}🙂",
        "\u{e000}",
        "🙂🙂🙂🙂🙂🙂🙂🙂🙂",
        "::",
        ":a b:",
        ":a:b:",
        ":é:",
        &format!(":{}:", "a".repeat(33)),
        ":tada",
        "tada:",
    ] {
        assert!(!valid_emoji(bad), "{bad:?}");
    }
}

#[test]
fn set_requests_are_bounded() {
    assert_eq!(
        check("  Lunch  ", None, None).unwrap(),
        Wanted {
            text: "Lunch".into(),
            emoji: None,
            hours: DEFAULT_HOURS
        }
    );
    assert_eq!(
        check("", Some(" 🍕 "), Some(1)).unwrap().emoji.as_deref(),
        Some("🍕")
    );
    assert_eq!(
        check(&"é".repeat(100), None, Some(168)).unwrap().text.len(),
        200
    );
    assert!(check(&format!("  {}  ", "a".repeat(200)), None, None).is_ok());
    for (text, emoji, hours) in [
        ("a".repeat(201), None, None),
        (format!("{}é", "a".repeat(199)), None, None),
        (String::new(), None, None),
        ("   ".into(), None, None),
        ("a\u{7}b".into(), None, None),
        ("a\tb".into(), None, None),
        ("a\u{202e}b".into(), None, None),
        ("ok".into(), Some("x"), None),
        ("ok".into(), Some(""), None),
        ("ok".into(), Some(":bad shortcode:"), None),
        ("ok".into(), None, Some(0)),
        ("ok".into(), None, Some(169)),
    ] {
        assert_eq!(
            check(&text, emoji, hours),
            Err("status_invalid"),
            "{text:?} {emoji:?} {hours:?}"
        );
    }
}

#[test]
fn the_rate_limit_admits_one_publication_per_gap() {
    let mut gate = Gate::default();
    let start = Instant::now();
    assert!(gate.admit(start, GAP));
    assert!(!gate.admit(start + Duration::from_millis(4999), GAP));
    // A refusal does not extend the window.
    assert!(gate.admit(start + GAP, GAP));
    assert!(!gate.admit(start + GAP, GAP));
}

#[test]
fn publisher_signs_once_and_resolves_only_its_exact_ok() {
    let user = key(2);
    let mut status = authenticated();
    let mut gate = Gate::default();
    let mut publisher = Publisher::default();
    let now = Instant::now();
    // Without a fresh, trusted session nothing is signed and the gate is untouched.
    assert_eq!(
        publisher
            .prepare(
                &set("x", None, None),
                &user,
                &status,
                false,
                &mut gate,
                GAP,
                now,
                NOW
            )
            .err(),
        Some("relay_unavailable")
    );
    status.connection = "connecting".into();
    assert_eq!(
        publisher
            .prepare(
                &set("x", None, None),
                &user,
                &status,
                true,
                &mut gate,
                GAP,
                now,
                NOW
            )
            .err(),
        Some("relay_unavailable")
    );
    status.connection = "authenticated".into();
    assert_eq!(
        publisher
            .prepare(
                &set("", None, None),
                &user,
                &status,
                true,
                &mut gate,
                GAP,
                now,
                NOW
            )
            .err(),
        Some("status_invalid")
    );
    assert!(!publisher.is_pending());
    let (view, event) = publisher
        .prepare(
            &set("Focus", Some("🎧"), Some(4)),
            &user,
            &status,
            true,
            &mut gate,
            GAP,
            now,
            NOW,
        )
        .unwrap();
    assert_eq!(
        (
            view.state.as_str(),
            view.mine.as_ref(),
            view.category.as_ref()
        ),
        ("sending", None, None)
    );
    assert_eq!(event.pubkey, user.public_key());
    assert!(tags(&event).contains(&vec!["expiration".into(), (NOW + 4 * 3600).to_string()]));
    assert!(publisher.is_pending());
    assert!(publisher.deadline() > now + TIMEOUT - Duration::from_millis(1));
    // One at a time, and an unrelated OK changes nothing.
    assert_eq!(
        publisher
            .prepare(
                &set("x", None, None),
                &user,
                &status,
                true,
                &mut Gate::default(),
                GAP,
                now,
                NOW
            )
            .err(),
        Some("status_rate_limited")
    );
    assert_eq!(publisher.acknowledge(&"0".repeat(64), true, NOW), None);
    assert_eq!(
        publisher.acknowledge(&event.id.to_hex(), true, NOW),
        Some(Outcome::Accepted(Some(UserStatus {
            text: "Focus".into(),
            emoji: Some("🎧".into()),
            expires_at: Some(NOW + 4 * 3600)
        })))
    );
    assert!(!publisher.is_pending());
    // The gate holds the next publication back for the whole gap.
    assert_eq!(
        publisher
            .prepare(
                &StatusIntent { set: None },
                &user,
                &status,
                true,
                &mut gate,
                GAP,
                now + Duration::from_secs(1),
                NOW
            )
            .err(),
        Some("status_rate_limited")
    );
    let (_, clear) = publisher
        .prepare(
            &StatusIntent { set: None },
            &user,
            &status,
            true,
            &mut gate,
            GAP,
            now + GAP,
            NOW,
        )
        .unwrap();
    assert_eq!((clear.content.as_str(), tags(&clear).len()), ("", 1));
    assert_eq!(
        publisher.acknowledge(&clear.id.to_hex(), true, NOW),
        Some(Outcome::Accepted(None))
    );
    let (_, rejected) = publisher
        .prepare(
            &set("x", None, None),
            &user,
            &status,
            true,
            &mut gate,
            GAP,
            now + GAP * 2,
            NOW,
        )
        .unwrap();
    assert_eq!(
        publisher.acknowledge(&rejected.id.to_hex(), false, NOW),
        Some(Outcome::Rejected)
    );
    publisher
        .prepare(
            &set("x", None, None),
            &user,
            &status,
            true,
            &mut gate,
            GAP,
            now + GAP * 3,
            NOW,
        )
        .unwrap();
    assert!(publisher.unknown());
    assert!(!publisher.unknown());
    // A failure keeps the shown status.
    status.user_status.mine = Some(UserStatus {
        text: "kept".into(),
        emoji: None,
        expires_at: None,
    });
    let view = failed(&status, "status_rejected");
    assert_eq!(view.state, "failed");
    assert_eq!(view.mine.unwrap().text, "kept");
    assert_eq!(view.category.as_deref(), Some("status_rejected"));
}

#[test]
fn verification_rejects_forged_foreign_or_misplaced_events() {
    let user = key(2);
    let other = key(3);
    let roster = vec![user.public_key().to_hex()];
    let good = general(&user, "Busy", &[], NOW - 10);
    assert_eq!(
        status(&roster, std::slice::from_ref(&good), NOW)
            .unwrap()
            .len(),
        1
    );
    let mut forged = good.clone();
    forged.content = "Forged".into();
    let wrong_kind = raw(&user, 30316, "Busy", &[&["d", "general"]], NOW - 10);
    for bad in [
        forged,
        wrong_kind,
        general(&other, "Outside the roster", &[], NOW - 10),
        general(&user, "From the future", &[], NOW + 61),
        raw(&user, KIND, "No d", &[], NOW - 10),
        raw(&user, KIND, "Music", &[&["d", "music"]], NOW - 10),
        raw(
            &user,
            KIND,
            "Two d",
            &[&["d", "general"], &["d", "general"]],
            NOW - 10,
        ),
        raw(&user, KIND, "Long d", &[&["d", "general", "x"]], NOW - 10),
    ] {
        assert_eq!(
            status(&roster, &[good.clone(), bad.clone()], NOW),
            Err("status_invalid"),
            "{}",
            bad.content
        );
    }
    assert!(status(&roster, &vec![good; 201], NOW).is_err());
    // A minute of clock tolerance, as for profiles.
    assert!(status(&roster, &[general(&user, "ok", &[], NOW + 60)], NOW).is_ok());
}

#[test]
fn newest_by_time_then_id_wins_and_expiry_is_the_readers() {
    let user = key(2);
    let own = user.public_key().to_hex();
    let roster = vec![own.clone()];
    let older = general(&user, "Older", &[], NOW - 100);
    let newer = general(&user, "Newer", &[], NOW - 50);
    for events in [
        [older.clone(), newer.clone()],
        [newer.clone(), older.clone()],
    ] {
        assert_eq!(status(&roster, &events, NOW).unwrap()[&own].text, "Newer");
    }
    // Same second: the lower event id (NIP-01, Desktop), whatever order the relay used.
    let a = general(&user, "A", &[], NOW - 10);
    let b = general(&user, "B", &[], NOW - 10);
    let expected = if a.id < b.id { "A" } else { "B" };
    for events in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
        assert_eq!(status(&roster, &events, NOW).unwrap()[&own].text, expected);
    }
    // A newest clear (empty) means no status; it never falls back to an older one.
    let clear = general(&user, "", &[], NOW - 5);
    assert!(status(&roster, &[newer.clone(), clear], NOW)
        .unwrap()
        .is_empty());
    // Expired at or before now: none, even with an older unexpired event.
    let soon = (NOW + 1).to_string();
    let past = NOW.to_string();
    let live = general(&user, "Live", &[&["expiration", soon.as_str()]], NOW - 1);
    assert_eq!(
        status(&roster, std::slice::from_ref(&live), NOW).unwrap()[&own].expires_at,
        Some(NOW + 1)
    );
    let expired = general(&user, "Expired", &[&["expiration", past.as_str()]], NOW - 1);
    assert!(status(&roster, &[newer.clone(), expired], NOW)
        .unwrap()
        .is_empty());
    assert!(status(&roster, std::slice::from_ref(&live), NOW + 1)
        .unwrap()
        .is_empty());
    // Malformed or repeated expiration or emoji tags: no status.
    for extra in [
        vec![vec!["expiration", "soon"]],
        vec![vec!["expiration", "-1"]],
        vec![vec!["expiration", ""]],
        vec![vec!["expiration", "999999999999999999999"]],
        vec![
            vec!["expiration", soon.as_str()],
            vec!["expiration", soon.as_str()],
        ],
        vec![vec!["emoji", "x"]],
        vec![vec!["emoji", "🙂\u{202e}"]],
        vec![vec!["emoji", "🙂"], vec!["emoji", "🙂"]],
    ] {
        let extra: Vec<&[&str]> = extra.iter().map(Vec::as_slice).collect();
        let event = general(&user, "Text", &extra, NOW - 1);
        assert!(
            status(&roster, &[event], NOW).unwrap().is_empty(),
            "{extra:?}"
        );
    }
    // Emoji only is a status.
    let emoji = general(&user, "", &[&["emoji", ":tada:"]], NOW - 1);
    let found = status(&roster, &[emoji], NOW).unwrap();
    assert_eq!(
        (found[&own].text.as_str(), found[&own].emoji.as_deref()),
        ("", Some(":tada:"))
    );
}

#[test]
fn status_text_is_sanitized_like_a_name() {
    let user = key(2);
    let own = user.public_key().to_hex();
    let roster = vec![own.clone()];
    let event = general(&user, "  a\u{202e}b\nc\u{7}d 👩‍💻 ", &[], NOW - 1);
    assert_eq!(
        status(&roster, &[event], NOW).unwrap()[&own].text,
        "a b c d 👩‍💻"
    );
    // Oversized text from another client is cut on a character boundary.
    let long = general(&user, &"é".repeat(150), &[], NOW - 1);
    let text = status(&roster, &[long], NOW).unwrap()[&own].text.clone();
    assert_eq!(text, "é".repeat(100));
    // Only controls: nothing to show.
    let blank = general(&user, "\u{202e}\n", &[], NOW - 1);
    assert!(status(&roster, &[blank], NOW).unwrap().is_empty());
}

#[test]
fn categories_are_a_fixed_list() {
    assert_eq!(
        CATEGORIES,
        [
            "status_invalid",
            "status_rate_limited",
            "status_rejected",
            "relay_unavailable"
        ]
    );
}
