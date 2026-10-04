//! Display names for message authors the room roster does not list.
//! Rosters are bounded and can omit authors, so history rows may carry keys
//! with no known name. One bounded kind 0 read per history refresh names them.
//! Names are self-asserted presentation hints, never ownership or authority.
use crate::query::{query, QueryRequest};
use crate::recipients::{name, Person};
use nostr::{Event, Keys, PublicKey, Timestamp};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Keys read at once.
pub const BATCH: usize = 50;
/// Names served in a status, newest last.
pub const SERVED: usize = 200;
/// A name, or the lack of one, is kept this long before the key is read again.
pub const TTL: Duration = Duration::from_secs(600);
/// A failed read (busy relay, timeout) blocks only a short retry, so a relay
/// that was busy during a reconnect burst does not hide names for ten minutes.
pub const FAILED_TTL: Duration = Duration::from_secs(60);
/// Most remembered misses; the oldest are dropped.
const MISSES: usize = 500;

#[derive(Default)]
pub struct Cache {
    /// key -> (sanitized name, when read); `order` lists the keys oldest first.
    named: BTreeMap<String, (String, Instant)>,
    order: Vec<String>,
    /// key -> when it may be read again.
    missed: BTreeMap<String, Instant>,
}

impl Cache {
    /// Keys to read now: authors (newest first) that are not in `skip`
    /// (the roster read already looked them up), not cached and not recently
    /// missed or failed, at most `BATCH`.
    pub fn wanted(
        &mut self,
        authors: &[String],
        skip: &BTreeSet<String>,
        now: Instant,
    ) -> Vec<PublicKey> {
        self.missed.retain(|_, until| *until > now);
        self.expire(now);
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for key in authors.iter().rev() {
            if out.len() == BATCH {
                break;
            }
            if skip.contains(key)
                || self.named.contains_key(key)
                || self.missed.contains_key(key)
                || !seen.insert(key.as_str())
            {
                continue;
            }
            if let Ok(public) = PublicKey::from_hex(key) {
                if public.to_hex() == *key {
                    out.push(public);
                }
            }
        }
        out
    }
    fn expire(&mut self, now: Instant) {
        self.named
            .retain(|_, (_, read)| now.saturating_duration_since(*read) < TTL);
        let named = &self.named;
        self.order.retain(|key| named.contains_key(key));
    }
    /// A completed read: asked keys with a name are cached, the rest are
    /// remembered as having none.
    pub fn record(&mut self, asked: &[PublicKey], found: BTreeMap<String, String>, now: Instant) {
        for key in asked {
            let key = key.to_hex();
            match found.get(&key) {
                Some(found) => {
                    self.order.retain(|k| *k != key);
                    self.order.push(key.clone());
                    self.named.insert(key, (found.clone(), now));
                }
                None => self.miss(key, now + TTL),
            }
        }
        while self.order.len() > SERVED {
            let oldest = self.order.remove(0);
            self.named.remove(&oldest);
        }
    }
    /// A failed read: the asked keys are not read again before `FAILED_TTL`.
    pub fn fail(&mut self, asked: &[PublicKey], now: Instant) {
        for key in asked {
            self.miss(key.to_hex(), now + FAILED_TTL);
        }
    }
    fn miss(&mut self, key: String, until: Instant) {
        self.missed.insert(key, until);
        while self.missed.len() > MISSES {
            let oldest = self
                .missed
                .iter()
                .min_by_key(|(_, until)| **until)
                .map(|(key, _)| key.clone())
                .expect("non-empty");
            self.missed.remove(&oldest);
        }
    }
    /// The unexpired names, at most `SERVED`, oldest first.
    pub fn served(&mut self, now: Instant) -> Vec<Person> {
        self.expire(now);
        self.order
            .iter()
            .filter_map(|key| {
                self.named.get(key).map(|(name, _)| Person {
                    key: key.clone(),
                    name: name.clone(),
                })
            })
            .collect()
    }
}

/// Names from one kind 0 read for `asked`. Any event with a bad signature,
/// another kind, an author that was not asked for, or a future time rejects
/// the whole read. Per author the newest (the lower id on a tie) counts; its
/// name is sanitized like every served name and an empty one is no name.
pub fn parse(
    asked: &[PublicKey],
    events: &[Event],
    now: u64,
) -> Result<BTreeMap<String, String>, &'static str> {
    if asked.len() > BATCH
        || events.len() > BATCH * 4
        || events.iter().any(|e| {
            e.verify().is_err()
                || e.kind.as_u16() != 0
                || e.created_at.as_secs() > now.saturating_add(60)
                || !asked.contains(&e.pubkey)
        })
    {
        return Err("profiles_invalid");
    }
    let mut latest: BTreeMap<String, &Event> = BTreeMap::new();
    for event in events {
        let key = event.pubkey.to_hex();
        if latest.get(&key).is_none_or(|old| {
            event.created_at > old.created_at
                || (event.created_at == old.created_at && event.id < old.id)
        }) {
            latest.insert(key, event);
        }
    }
    Ok(latest
        .into_iter()
        .filter_map(|(key, event)| {
            let name = name(event);
            (!name.is_empty()).then_some((key, name))
        })
        .collect())
}

/// One signed read of the profiles of `asked` (at most `BATCH`).
pub async fn fetch(
    relay: &str,
    keys: &Keys,
    asked: &[PublicKey],
) -> Result<BTreeMap<String, String>, &'static str> {
    if asked.is_empty() || asked.len() > BATCH {
        return Err("profiles_invalid");
    }
    let events = query(
        relay,
        keys,
        &QueryRequest::AuthorProfiles {
            authors: asked.to_vec(),
        },
    )
    .await?;
    parse(asked, &events, Timestamp::now().as_secs())
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
