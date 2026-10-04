use super::*;
use nostr::{EventBuilder, Kind};

fn key(n: u8) -> Keys {
    Keys::parse(&format!("{n:064x}")).unwrap()
}
fn hexes(keys: &[PublicKey]) -> Vec<String> {
    keys.iter().map(|k| k.to_hex()).collect()
}
fn profile(user: &Keys, body: &str, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(0), body)
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(user)
        .unwrap()
}
fn plain(n: u8) -> String {
    key(n).public_key().to_hex()
}

#[test]
fn wanted_skips_known_cached_and_missed_keys_and_is_bounded_newest_first() {
    let mut cache = Cache::default();
    let now = Instant::now();
    let rows: Vec<String> = (1..=120).map(plain).collect();
    let asked = cache.wanted(&rows, &BTreeSet::new(), now);
    assert_eq!(asked.len(), BATCH);
    // Newest rows are last, so they are asked first.
    assert_eq!(asked[0].to_hex(), plain(120));
    assert_eq!(asked[BATCH - 1].to_hex(), plain(120 - 49));
    // Roster keys are never asked again.
    let skip: BTreeSet<String> = [plain(120)].into();
    assert!(!hexes(&cache.wanted(&rows, &skip, now)).contains(&plain(120)));
    // Duplicates are asked once; malformed keys are not asked.
    let dup = vec![plain(5), "zz".into(), plain(5), plain(5).to_uppercase()];
    assert_eq!(
        hexes(&cache.wanted(&dup, &BTreeSet::new(), now)),
        vec![plain(5)]
    );
    // A cached name is not asked again until the TTL passes.
    let one = vec![plain(7)];
    let found = BTreeMap::from([(plain(7), "Seven".to_owned())]);
    cache.record(&[key(7).public_key()], found, now);
    assert!(cache
        .wanted(&one, &BTreeSet::new(), now + TTL - Duration::from_secs(1))
        .is_empty());
    assert_eq!(cache.wanted(&one, &BTreeSet::new(), now + TTL).len(), 1);
}

#[test]
fn a_missing_profile_and_a_failed_read_are_not_asked_again_within_their_ttl() {
    let mut cache = Cache::default();
    let now = Instant::now();
    let (nameless, failed) = (key(8).public_key(), key(9).public_key());
    cache.record(&[nameless], BTreeMap::new(), now);
    cache.fail(&[failed], now);
    let both = vec![plain(8), plain(9)];
    assert!(cache
        .wanted(&both, &BTreeSet::new(), now + Duration::from_secs(59))
        .is_empty());
    // A failure is retried soon, a profile the relay does not have much later.
    let later = cache.wanted(&both, &BTreeSet::new(), now + FAILED_TTL);
    assert_eq!(hexes(&later), vec![plain(9)]);
    assert_eq!(cache.wanted(&both, &BTreeSet::new(), now + TTL).len(), 2);
}

#[test]
fn served_names_are_bounded_oldest_dropped_and_expire() {
    let mut cache = Cache::default();
    let now = Instant::now();
    for n in 0..(SERVED as u32 + 10) {
        let who = Keys::parse(&format!("{:064x}", n + 1))
            .unwrap()
            .public_key();
        cache.record(
            &[who],
            BTreeMap::from([(who.to_hex(), format!("P{n}"))]),
            now,
        );
    }
    let served = cache.served(now);
    assert_eq!(served.len(), SERVED);
    assert_eq!(served[0].name, "P10");
    assert_eq!(served[SERVED - 1].name, format!("P{}", SERVED + 9));
    assert!(cache.served(now + TTL).is_empty());
}

#[test]
fn parse_keeps_the_newest_sanitized_name_of_each_asked_author() {
    let (a, b, c) = (key(2), key(3), key(4));
    let asked = vec![a.public_key(), b.public_key(), c.public_key()];
    let events = vec![
        profile(&a, r#"{"name":"Old"}"#, 100),
        profile(
            &a,
            "{\"display_name\":\"Ann \\u202e\\u0007Lee\",\"name\":\"ann\"}",
            200,
        ),
        profile(&b, r#"{"name":"   "}"#, 100),
        profile(&c, "not json", 100),
    ];
    let found = parse(&asked, &events, 1000).unwrap();
    // Bidi and control characters become spaces and never survive.
    assert_eq!(
        found,
        BTreeMap::from([(a.public_key().to_hex(), "Ann   Lee".to_owned())])
    );
    let long = format!(r#"{{"name":"{}"}}"#, "x".repeat(200));
    let found = parse(&asked, &[profile(&b, &long, 100)], 1000).unwrap();
    assert_eq!(found[&b.public_key().to_hex()].len(), 64);
}

#[test]
fn parse_rejects_the_whole_read_for_forgery_scope_time_or_size() {
    let (a, b) = (key(2), key(3));
    let asked = vec![a.public_key()];
    let good = profile(&a, r#"{"name":"Ann"}"#, 100);
    assert_eq!(parse(&asked, &[good.clone()], 1000).unwrap().len(), 1);
    // A signature that does not verify.
    let mut forged = serde_json::to_value(&good).unwrap();
    forged["content"] = serde_json::json!(r#"{"name":"Mallory"}"#);
    let forged: Event = serde_json::from_value(forged).unwrap();
    assert_eq!(
        parse(&asked, &[good.clone(), forged], 1000),
        Err("profiles_invalid")
    );
    // An author nobody asked for, another kind, a future time.
    assert!(parse(
        &asked,
        &[good.clone(), profile(&b, r#"{"name":"Bob"}"#, 100)],
        1000
    )
    .is_err());
    let other_kind = EventBuilder::new(Kind::Custom(1), r#"{"name":"Ann"}"#)
        .sign_with_keys(&a)
        .unwrap();
    assert!(parse(&asked, &[other_kind], 1000).is_err());
    assert!(parse(&asked, &[profile(&a, r#"{"name":"Ann"}"#, 5000)], 1000).is_err());
    assert!(parse(&vec![a.public_key(); BATCH + 1], &[], 1000).is_err());
}
