//! Clock offset against the relay, from the HTTP `Date` response header.
//!
//! Informational only: the measurement never adjusts signing times or the
//! system clock. It explains a rejected NIP-42 authentication whose event was
//! signed with a local clock far from the relay's (a suspended VM's clock
//! stops). Round-trip time is ignored: `Date` has one-second resolution.
use reqwest::header::{HeaderMap, DATE};
use std::time::{SystemTime, UNIX_EPOCH};

/// Published offsets are bounded to ±10 years (of 365.25 days).
pub const BOUND: i64 = 315_576_000;
/// A rejected authentication with at least this offset reads `clock_skew`.
pub const THRESHOLD: i64 = 120;

/// Seconds since the Unix epoch of a strict RFC 7231 IMF-fixdate
/// (`Sun, 06 Nov 1994 08:49:37 GMT`). The obsolete RFC 850 and asctime forms,
/// other zones, extra whitespace and a wrong weekday are refused.
pub fn parse_http_date(value: &str) -> Option<i64> {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let b = value.as_bytes();
    if b.len() != 29
        || &b[3..5] != b", "
        || b[7] != b' '
        || b[11] != b' '
        || b[16] != b' '
        || b[19] != b':'
        || b[22] != b':'
        || &b[25..] != b" GMT"
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = &b[range];
        digits
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| digits.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let day = number(5..7)?;
    let month = MONTHS.iter().position(|m| m.as_bytes() == &b[8..11])? as i64 + 1;
    let year = number(12..16)?;
    let (hour, minute, second) = (number(17..19)?, number(20..22)?, number(23..25)?);
    // 60 would be a leap second; `Date` never needs one.
    if year < 1970 || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let length = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day < 1 || day > length {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if DAYS[days.rem_euclid(7) as usize].as_bytes() != &b[0..3] {
        return None;
    }
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `relay − local` in seconds, clamped to ±`BOUND`.
pub fn skew(relay: i64, local: i64) -> i64 {
    relay.saturating_sub(local).clamp(-BOUND, BOUND)
}

pub fn now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

/// The offset shown by a response's single valid `Date` header, measured
/// against the local clock now. None when missing, repeated or malformed.
pub fn from_headers(headers: &HeaderMap) -> Option<i64> {
    let mut values = headers.get_all(DATE).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let relay = parse_http_date(value.to_str().ok()?)?;
    Some(skew(relay, now()))
}

/// One `HEAD` of the relay's NIP-11 location, only for its `Date` header (any
/// status carries one). Used once per rejected authentication; the ordinary
/// NIP-11 `GET` of catalog discovery measures without an extra request.
pub async fn probe(relay: &str) -> Option<i64> {
    let url = crate::catalog::info_url(relay).ok()?;
    let client = crate::catalog::info_client().ok()?;
    let response = client.head(url).send().await.ok()?;
    from_headers(response.headers())
}

/// Whether a rejected authentication is explained by this offset.
pub fn explains_rejection(skew: Option<i64>) -> bool {
    skew.is_some_and(|s| s.unsigned_abs() >= THRESHOLD as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn imf_fixdate_parses() {
        assert_eq!(
            parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_http_date("Wed, 30 Sep 2026 18:05:00 GMT"),
            Some(1_790_791_500)
        );
        assert_eq!(
            parse_http_date("Tue, 29 Feb 2028 23:59:59 GMT"),
            Some(1_835_481_599)
        );
    }

    #[test]
    fn other_forms_and_malformed_dates_are_refused() {
        for bad in [
            "",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 Nov 1994 08:49:37 +0000",
            "Sun,  6 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:37 GMT ",
            " Sun, 06 Nov 1994 08:49:37 GMT",
            "sun, 06 Nov 1994 08:49:37 GMT",
            "Mon, 06 Nov 1994 08:49:37 GMT",
            "Sun, 06 nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 24:00:00 GMT",
            "Sun, 06 Nov 1994 08:60:00 GMT",
            "Sun, 06 Nov 1994 08:49:60 GMT",
            "Fri, 29 Feb 2030 00:00:00 GMT",
            "Sun, 00 Nov 1994 08:49:37 GMT",
            "Sun, 31 Nov 1994 08:49:37 GMT",
            "Sun, +6 Nov 1994 08:49:37 GMT",
            "Wed, 31 Dec 1969 23:59:59 GMT",
            "Sun, 06 Nov 1994 08:49:37 GMTX",
        ] {
            assert_eq!(parse_http_date(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn skew_is_signed_and_bounded() {
        // The relay is 73 minutes ahead of a stopped local clock.
        assert_eq!(skew(10_000 + 4_380, 10_000), 4_380);
        assert_eq!(skew(10_000, 10_000 + 90), -90);
        assert_eq!(skew(i64::MAX, 0), BOUND);
        assert_eq!(skew(0, i64::MAX), -BOUND);
        assert_eq!(skew(3_000_000_000, 0), BOUND);
        assert!(!explains_rejection(None));
        assert!(!explains_rejection(Some(119)));
        assert!(!explains_rejection(Some(-119)));
        assert!(explains_rejection(Some(120)));
        assert!(explains_rejection(Some(-4_380)));
    }

    #[test]
    fn headers_need_exactly_one_valid_date() {
        let mut headers = HeaderMap::new();
        assert_eq!(from_headers(&headers), None);
        headers.insert(DATE, HeaderValue::from_static("yesterday"));
        assert_eq!(from_headers(&headers), None);
        headers.insert(
            DATE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        );
        // A local clock after 1980 sees the epoch as the full bound behind.
        assert_eq!(from_headers(&headers), Some(-BOUND));
        headers.append(
            DATE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        );
        assert_eq!(from_headers(&headers), None);
    }
}
