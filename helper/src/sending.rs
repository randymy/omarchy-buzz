//! Human kind-9 sending, plus edits, deletes and reactions on shown messages,
//! all through one delivery slot. No retransmission and no plaintext persistence.
use crate::{
    ledger::{Ledger, Outcome, Record},
    protocol::{Action, Delivery, PendingAttachment, SendIntent, Status},
};
use nostr::{Event, EventId, Keys, PublicKey};
use tokio::time::{Duration, Instant};

pub struct Sender {
    ledger: Option<Ledger>,
    pending: Option<(String, String, Instant)>,
    // Only the current request intent is retained, to reject conflicting reuse.
    last: Option<SendIntent>,
    last_outcome: Option<Outcome>,
    /// The draft scope and attachment hashes of the event in flight, cleared
    /// from the pending list once the relay accepts it.
    media: Option<(String, Vec<String>)>,
}
impl Sender {
    pub fn new(ledger: Option<Ledger>) -> Self {
        Self {
            ledger,
            pending: None,
            last: None,
            last_outcome: None,
            media: None,
        }
    }
    /// The accepted event's draft scope and attachment hashes, once.
    pub fn take_sent_media(&mut self) -> Option<(String, Vec<String>)> {
        self.media.take()
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
    pub fn pending_error(&self, intent: &SendIntent) -> Option<&'static str> {
        if !self.is_pending() {
            return None;
        }
        let active = self.last.as_ref()?;
        if active.request_id != intent.request_id {
            return Some("send_busy");
        }
        if active.action != intent.action
            || active.room != intent.room
            || active.root_id != intent.root_id
            || active.text != intent.text
            || active.mentions != intent.mentions
            || active.generation != intent.generation
        {
            return Some("send_request_reused");
        }
        None
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
        let post = intent.action == Action::Post;
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
        if let Some(root) = intent.root_id.as_deref().filter(|_| post) {
            if status.thread.state != "snapshot"
                || status.thread.room_id.as_deref() != Some(intent.room.as_str())
                || status.thread.root_id.as_deref() != Some(root)
                || status.history.state != "snapshot"
                || status.history.room_id.as_deref() != Some(intent.room.as_str())
                || !status
                    .history
                    .rows
                    .iter()
                    .any(|row| row.id == root && !row.unavailable)
            {
                return (fail("send_access_denied"), None);
            }
        }
        if post
            && !intent.mentions.is_empty()
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
                && (last.action != intent.action
                    || last.room != intent.room
                    || last.root_id != intent.root_id
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
                || record.root_id != intent.root_id
            {
                return (fail("send_request_reused"), None);
            }
            if !self.last.as_ref().is_some_and(|last| {
                last.request_id == intent.request_id
                    && last.action == intent.action
                    && last.room == intent.room
                    && last.root_id == intent.root_id
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
        // Uploaded attachments waiting in this draft travel with the message.
        let scope = crate::media::scope(&intent.room, intent.root_id.as_deref());
        let media: Vec<PendingAttachment> = if post {
            status
                .pending_attachments
                .iter()
                .filter(|p| p.scope == scope)
                .take(crate::media::PENDING)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        if post && intent.text.trim().is_empty() && media.is_empty() {
            return (fail("send_invalid"), None);
        }
        let built = if post {
            let recipients = recipients(&intent, keys, status);
            build_event(&intent, keys, &recipients, &media, None)
        } else {
            plan(&intent, keys, status)
                .and_then(|plan| build_action_event(&intent, &plan, keys, None))
        };
        let event = match built {
            Ok(event) => event,
            Err(category) => return (fail(category), None),
        };
        let id = event.id.to_hex();
        if ledger
            .reserve(Record {
                request_id: intent.request_id.clone(),
                origin: relay.into(),
                identity: keys.public_key().to_hex(),
                room: intent.room.clone(),
                root_id: intent.root_id.clone(),
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
        self.media =
            (!media.is_empty()).then(|| (scope, media.into_iter().map(|m| m.hash).collect()));
        self.last = Some(intent);
        self.last_outcome = Some(Outcome::Pending);
        (status, Some(event))
    }
    pub fn acknowledge(&mut self, id: &str, accepted: bool) -> Option<Delivery> {
        if !self.pending.as_ref().is_some_and(|p| p.1 == id) {
            return None;
        }
        if !accepted {
            self.media = None;
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
        // Delivery unknown: the attachments stay in the draft.
        self.media = None;
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
/// The keys a message addresses: the explicit mentions, then, in a direct
/// message, every other participant. Desktop tags a DM's participants even
/// without an `@mention` (`messageMentionPubkeys.ts`); agent harnesses
/// subscribed to mentions (`buzz-acp --subscribe mentions`, a `#p` filter) and
/// notification subscriptions rely on those tags. At most 20 + 8 keys.
fn recipients(intent: &SendIntent, keys: &Keys, status: &Status) -> Vec<String> {
    let own = keys.public_key().to_hex();
    let mut all = intent.mentions.clone();
    if let Some(room) = status
        .catalog
        .rooms
        .iter()
        .find(|r| r.id == intent.room && r.kind == "dm")
    {
        for key in &room.participants {
            if *key != own && !all.contains(key) {
                all.push(key.clone());
            }
        }
    }
    all
}
/// One `imeta` tag per attachment in Desktop's key order
/// (`imetaMediaMarkdown.ts` `buildImetaTags`): url, m, x, size, dim, filename.
pub(crate) fn imeta_tags(media: &[PendingAttachment]) -> Vec<Vec<String>> {
    media
        .iter()
        .map(|m| {
            let mut tag = vec![
                "imeta".to_owned(),
                format!("url {}", m.url),
                format!("m {}", m.mime),
                format!("x {}", m.hash),
                format!("size {}", m.size),
            ];
            if let Some(dim) = &m.dim {
                tag.push(format!("dim {dim}"));
            }
            tag.push(format!("filename {}", m.name));
            tag
        })
        .collect()
}
/// The text followed by Desktop's markdown line for each attachment
/// (`buildOutgoingMessage`), so other clients show them.
pub(crate) fn content_with(text: &str, media: &[PendingAttachment]) -> String {
    let mut content = text.to_owned();
    for m in media {
        content.push('\n');
        content.push_str(&crate::attachments::markdown_line(&m.mime, &m.name, &m.url));
    }
    content
}
// Local correlation extension; not a Buzz command, authority or protocol nonce.
fn build_event(
    intent: &SendIntent,
    keys: &Keys,
    recipients: &[String],
    media: &[PendingAttachment],
    created_at: Option<nostr::Timestamp>,
) -> Result<Event, &'static str> {
    let room = uuid::Uuid::parse_str(&intent.room).map_err(|_| "send_invalid")?;
    let mentions: Vec<&str> = recipients.iter().map(String::as_str).collect();
    let thread = intent
        .root_id
        .as_deref()
        .map(|root| {
            let id = EventId::from_hex(root).map_err(|_| "send_invalid")?;
            Ok::<_, &'static str>(buzz_sdk::ThreadRef {
                root_event_id: id,
                parent_event_id: id,
            })
        })
        .transpose()?;
    let correlation = nostr::Tag::parse(["omarchy-buzz-request", intent.request_id.as_str()])
        .map_err(|_| "send_invalid")?;
    let mut builder = buzz_sdk::build_message(
        room,
        &content_with(&intent.text, media),
        thread.as_ref(),
        &mentions,
        false,
        &imeta_tags(media),
        &[],
    )
    .map_err(|_| "send_invalid")?
    .tag(correlation);
    if let Some(timestamp) = created_at {
        builder = builder.custom_created_at(timestamp);
    }
    builder.sign_with_keys(keys).map_err(|_| "send_invalid")
}
/// What an action publishes, once its target is checked against the shown rows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    Edit(EventId),
    Delete(EventId),
    React(EventId),
    /// The reaction event to retract.
    Unreact(EventId),
}
/// Desktop edits and deletes through the relay's author rule (`canManageMessage`:
/// the author, or an agent's owner); only the author is known here, so only own
/// rows are accepted. Reactions need a row of the shown room page.
pub(crate) fn plan(
    intent: &SendIntent,
    keys: &Keys,
    status: &Status,
) -> Result<Plan, &'static str> {
    let own = keys.public_key().to_hex();
    let room = Some(intent.room.as_str());
    let history = (status.history.state == "snapshot" && status.history.room_id.as_deref() == room)
        .then_some(&status.history.rows);
    let (Action::Edit(id) | Action::Delete(id) | Action::React(id) | Action::Unreact(id)) =
        &intent.action
    else {
        return Err("send_invalid");
    };
    let held = history.and_then(|rows| rows.iter().find(|r| &r.id == id));
    let thread = (status.thread.state == "snapshot" && status.thread.room_id.as_deref() == room)
        .then(|| status.thread.rows.iter().find(|r| &r.row.id == id))
        .flatten()
        .map(|r| &r.row);
    let target = EventId::from_hex(id).map_err(|_| "send_invalid")?;
    match &intent.action {
        Action::Edit(_) | Action::Delete(_) => {
            let row = held.or(thread).ok_or("send_access_denied")?;
            if row.author != own {
                return Err("send_not_author");
            }
            if matches!(intent.action, Action::Delete(_)) {
                return Ok(Plan::Delete(target));
            }
            // Desktop re-sends imeta tags with an edit; a shortened or
            // attachment-bearing row cannot be rebuilt from its projection.
            if row.unavailable
                || row.truncated
                || row.attachments_unavailable
                || !row.attachments.is_empty()
            {
                return Err("send_unsupported");
            }
            Ok(Plan::Edit(target))
        }
        Action::React(_) | Action::Unreact(_) => {
            let row = held.ok_or("send_access_denied")?;
            if row.unavailable {
                return Err("send_access_denied");
            }
            let chip = row
                .reactions
                .as_ref()
                .and_then(|r| r.chips.iter().find(|c| c.emoji == intent.text));
            if matches!(intent.action, Action::React(_)) {
                return if chip.is_some_and(|c| c.mine) {
                    Err("send_invalid")
                } else {
                    Ok(Plan::React(target))
                };
            }
            let reaction = chip
                .and_then(|c| c.mine_id.as_deref())
                .ok_or("send_unsupported")?;
            Ok(Plan::Unreact(
                EventId::from_hex(reaction).map_err(|_| "send_invalid")?,
            ))
        }
        Action::Post => Err("send_invalid"),
    }
}
/// Desktop's builders (`edit_message` kind 40003, `delete_message` kind 5,
/// `add_reaction` kind 7, `remove_reaction` kind 5 on the reaction event),
/// with the local correlation tag.
pub(crate) fn build_action_event(
    intent: &SendIntent,
    plan: &Plan,
    keys: &Keys,
    created_at: Option<nostr::Timestamp>,
) -> Result<Event, &'static str> {
    let room = uuid::Uuid::parse_str(&intent.room).map_err(|_| "send_invalid")?;
    let builder = match plan {
        Plan::Edit(id) => buzz_sdk::build_edit(room, *id, intent.text.trim()),
        Plan::Delete(id) => buzz_sdk::build_delete_compat(room, *id),
        Plan::React(id) => buzz_sdk::build_reaction(*id, &intent.text),
        Plan::Unreact(id) => buzz_sdk::build_remove_reaction(*id),
    }
    .map_err(|_| "send_invalid")?;
    let correlation = nostr::Tag::parse(["omarchy-buzz-request", intent.request_id.as_str()])
        .map_err(|_| "send_invalid")?;
    let mut builder = builder.tag(correlation);
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
/// A target is a canonical event id; an edit has text and a reaction a native
/// emoji (never the agent 👀/💬); only a post may thread or mention.
fn action_valid(intent: &SendIntent) -> bool {
    let (Action::Edit(id) | Action::Delete(id) | Action::React(id) | Action::Unreact(id)) =
        &intent.action
    else {
        return true;
    };
    EventId::from_hex(id).is_ok_and(|e| e.to_hex() == *id)
        && intent.root_id.is_none()
        && intent.mentions.is_empty()
        && match &intent.action {
            Action::Edit(_) => !intent.text.trim().is_empty(),
            Action::Delete(_) => intent.text.is_empty(),
            _ => crate::history::chip_emoji(&intent.text),
        }
}
fn valid(intent: &SendIntent) -> bool {
    uuid::Uuid::parse_str(&intent.request_id).is_ok_and(|id| id.to_string() == intent.request_id)
        && uuid::Uuid::parse_str(&intent.room).is_ok_and(|id| id.to_string() == intent.room)
        && intent
            .root_id
            .as_deref()
            .is_none_or(|root| EventId::from_hex(root).is_ok_and(|id| id.to_hex() == root))
        && !intent.text.contains('\0')
        && intent.text.len() <= 4096
        && action_valid(intent)
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
