//! Human kind-9 sending. No retransmission and no plaintext persistence.
use crate::{
    ledger::{Ledger, Outcome, Record},
    protocol::{Delivery, SendIntent, Status},
};
use nostr::{Event, Keys, PublicKey};
use tokio::time::{Duration, Instant};

pub struct Sender {
    ledger: Option<Ledger>,
    pending: Option<(String, String, Instant)>,
    // Only the current request intent is retained, to reject conflicting reuse.
    last: Option<SendIntent>,
    last_outcome: Option<Outcome>,
}
impl Sender {
    pub fn new(ledger: Option<Ledger>) -> Self {
        Self {
            ledger,
            pending: None,
            last: None,
            last_outcome: None,
        }
    }
    pub fn clear_scope(&mut self) {
        self.last_outcome = None;
        if let Some(mut intent) = self.last.take() {
            zeroize::Zeroize::zeroize(&mut intent.text);
        }
    }
    pub fn deadline(&self) -> Instant {
        self.pending
            .as_ref()
            .map(|p| p.2)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn prepare(
        &mut self,
        intent: SendIntent,
        relay: &str,
        keys: &Keys,
        status: &Status,
        fresh: bool,
        trusted: bool,
    ) -> (Delivery, Option<Event>) {
        let fail = |category| delivery(&intent, None, "failed", Some(category));
        if let Some((_, event_id, _)) = &self.pending {
            // Preserve the active receipt, including identical in-flight replay.
            // A second submission must never hide the first one's eventual OK.
            if let Some(active) = &self.last {
                return (
                    delivery(active, Some(event_id.clone()), "sending", None),
                    None,
                );
            }
            return (fail("send_unavailable"), None);
        }
        if !valid(&intent) {
            return (fail("send_invalid"), None);
        }
        if intent.generation != status.generation {
            return (fail("send_scope_changed"), None);
        }
        if !fresh || !trusted || status.connection != "authenticated" {
            return (fail("send_unavailable"), None);
        }
        if !status.catalog.rooms.iter().any(|r| r.id == intent.room) {
            return (fail("send_access_denied"), None);
        }
        if status.recipients.room_id.as_deref() == Some(intent.room.as_str())
            && status.recipients.category.as_deref() == Some("recipients_access_denied")
        {
            return (fail("send_access_denied"), None);
        }
        if !intent.mentions.is_empty()
            && (status.recipients.state != "snapshot"
                || status.recipients.room_id.as_deref() != Some(intent.room.as_str())
                || !intent.mentions.iter().all(|key| {
                    status
                        .recipients
                        .entries
                        .iter()
                        .any(|recipient| recipient.key == *key)
                }))
        {
            return (fail("send_access_denied"), None);
        }
        if let Some(last) = &self.last {
            if last.request_id == intent.request_id
                && (last.room != intent.room
                    || last.text != intent.text
                    || last.mentions != intent.mentions
                    || last.generation != intent.generation)
            {
                return (fail("send_request_reused"), None);
            }
        }
        let Some(ledger) = self.ledger.as_mut() else {
            return (fail("send_ledger_unavailable"), None);
        };
        if let Some(record) = ledger.lookup(&intent.request_id) {
            if record.origin != relay
                || record.identity != keys.public_key().to_hex()
                || record.room != intent.room
            {
                return (fail("send_request_reused"), None);
            }
            if !self.last.as_ref().is_some_and(|last| {
                last.request_id == intent.request_id
                    && last.room == intent.room
                    && last.text == intent.text
                    && last.mentions == intent.mentions
                    && last.generation == intent.generation
            }) {
                return (
                    delivery(
                        &intent,
                        Some(record.event_id),
                        "failed",
                        Some("send_request_reused"),
                    ),
                    None,
                );
            }
            let (state, category) =
                outcome_status(self.last_outcome.as_ref().unwrap_or(&record.outcome));
            let category = if self
                .last_outcome
                .is_some_and(|known| known != record.outcome)
            {
                Some("send_ledger_unavailable")
            } else {
                category
            };
            return (
                delivery(&intent, Some(record.event_id), state, category),
                None,
            );
        }
        let event = match build_event(&intent, keys, None) {
            Ok(event) => event,
            Err(_) => return (fail("send_invalid"), None),
        };
        let id = event.id.to_hex();
        if ledger
            .reserve(Record {
                request_id: intent.request_id.clone(),
                origin: relay.into(),
                identity: keys.public_key().to_hex(),
                room: intent.room.clone(),
                event_id: id.clone(),
                outcome: Outcome::Pending,
            })
            .is_err()
        {
            return (fail("send_ledger_unavailable"), None);
        }
        // Persisted before the first socket write. Any subsequent failure is ambiguous.
        self.pending = Some((
            intent.request_id.clone(),
            id.clone(),
            Instant::now() + Duration::from_secs(15),
        ));
        let status = delivery(&intent, Some(id), "sending", None);
        self.clear_scope();
        self.last = Some(intent);
        self.last_outcome = Some(Outcome::Pending);
        (status, Some(event))
    }
    pub fn acknowledge(&mut self, id: &str, accepted: bool) -> Option<Delivery> {
        if !self.pending.as_ref().is_some_and(|p| p.1 == id) {
            return None;
        }
        self.finish(if accepted {
            Outcome::Acknowledged
        } else {
            Outcome::Rejected
        })
    }
    pub fn revoke_room(&mut self, room: &str) -> Option<Delivery> {
        if self.last.as_ref().is_some_and(|intent| intent.room == room) {
            self.unknown()
        } else {
            None
        }
    }
    pub fn unknown(&mut self) -> Option<Delivery> {
        let result = self.finish(Outcome::Unknown);
        self.clear_scope();
        result
    }
    fn finish(&mut self, outcome: Outcome) -> Option<Delivery> {
        let (request, id, _) = self.pending.take()?;
        self.last_outcome = Some(outcome);
        let intent = self.last.as_ref()?;
        let persisted = self
            .ledger
            .as_mut()
            .is_some_and(|l| l.outcome(&request, outcome.clone()).is_ok());
        let (state, category) = outcome_status(&outcome);
        Some(delivery(
            intent,
            Some(id),
            state,
            if persisted {
                category
            } else {
                Some("send_ledger_unavailable")
            },
        ))
    }
}
// Local correlation extension; not a Buzz command, authority or protocol nonce.
fn build_event(
    intent: &SendIntent,
    keys: &Keys,
    created_at: Option<nostr::Timestamp>,
) -> Result<Event, &'static str> {
    let room = uuid::Uuid::parse_str(&intent.room).map_err(|_| "send_invalid")?;
    let mentions: Vec<&str> = intent.mentions.iter().map(String::as_str).collect();
    let correlation = nostr::Tag::parse(["omarchy-buzz-request", intent.request_id.as_str()])
        .map_err(|_| "send_invalid")?;
    let mut builder = buzz_sdk::build_message(room, &intent.text, None, &mentions, false, &[], &[])
        .map_err(|_| "send_invalid")?
        .tag(correlation);
    if let Some(timestamp) = created_at {
        builder = builder.custom_created_at(timestamp);
    }
    builder.sign_with_keys(keys).map_err(|_| "send_invalid")
}
fn outcome_status(outcome: &Outcome) -> (&'static str, Option<&'static str>) {
    match outcome {
        Outcome::Acknowledged => ("acknowledged", None),
        Outcome::Rejected => ("rejected", Some("send_rejected")),
        Outcome::Unknown | Outcome::Pending => ("unknown", Some("delivery_unknown")),
    }
}
fn delivery(
    intent: &SendIntent,
    event_id: Option<String>,
    state: &str,
    category: Option<&str>,
) -> Delivery {
    Delivery {
        request_id: Some(intent.request_id.clone()),
        room_id: Some(intent.room.clone()),
        event_id,
        state: state.into(),
        category: category.map(str::to_owned),
    }
}
fn valid(intent: &SendIntent) -> bool {
    uuid::Uuid::parse_str(&intent.request_id).is_ok_and(|id| id.to_string() == intent.request_id)
        && uuid::Uuid::parse_str(&intent.room).is_ok_and(|id| id.to_string() == intent.room)
        && !intent.text.trim().is_empty()
        && !intent.text.contains('\0')
        && intent.text.len() <= 4096
        && intent.mentions.len() <= 20
        && intent
            .mentions
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == intent.mentions.len()
        && intent
            .mentions
            .iter()
            .all(|p| PublicKey::from_hex(p).is_ok_and(|key| key.to_hex() == *p))
}

#[cfg(test)]
#[path = "sending_tests.rs"]
mod tests;

impl Drop for Sender {
    fn drop(&mut self) {
        self.clear_scope();
    }
}
