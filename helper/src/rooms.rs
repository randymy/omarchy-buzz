//! Creating and managing rooms from the panel, as Desktop does it
//! (`desktop/src-tauri/src/commands/channels.rs`, `events.rs`):
//!
//! - create: kind 9007 (`h` new room UUID, `name`, `visibility`, `channel_type`
//!   `stream`, optional `about`);
//! - details: kind 9002 with `name` and `about` (relay: owner or admin);
//! - topic: kind 9002 with `topic` alone (relay: any member; the panel offers
//!   it to owners and admins only);
//! - members: kind 9000 add and 9001 remove, `h` + `p` (relay: owner or admin).
//!
//! Events come from the pinned `buzz_sdk` builders, signed with the session
//! key and answered only by the relay's `OK` (`join::RoomActions`). The helper
//! does not decide who may do what: the relay's answer is shown as it is.
//! `Detail` is the read side: the relay-signed roster with roles and the topic.
use crate::catalog::{clean, one_tag, validated};
use crate::protocol::Room;
use crate::query::{query, QueryRequest};
use nostr::{Event, EventBuilder, Keys, PublicKey, Timestamp};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Bounds the panel and the helper both enforce (bytes of UTF-8).
pub const NAME_BYTES: usize = 128;
pub const ABOUT_BYTES: usize = 1024;
pub const TOPIC_BYTES: usize = 256;
/// Roster rows in one `Detail`; the rest is counted as `truncated`.
pub const MEMBERS: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Create {
        name: String,
        about: String,
        private: bool,
    },
    /// Only the fields given are published: a field left out is left as it is
    /// (never rewritten from a shortened display copy). At least one.
    Details {
        room: Uuid,
        name: Option<String>,
        about: Option<String>,
    },
    Topic {
        room: Uuid,
        topic: String,
    },
    AddMember {
        room: Uuid,
        key: PublicKey,
    },
    RemoveMember {
        room: Uuid,
        key: PublicKey,
    },
}

/// Text the relay stores as typed: no controls (newline included), bounded.
pub fn plain(text: &str, limit: usize) -> Option<String> {
    let text = text.trim();
    (text.len() <= limit && !text.chars().any(char::is_control)).then(|| text.to_owned())
}

impl Change {
    pub fn word(&self) -> &'static str {
        match self {
            Change::Create { .. } => "create",
            Change::Details { .. } => "details",
            Change::Topic { .. } => "topic",
            Change::AddMember { .. } => "add_member",
            Change::RemoveMember { .. } => "remove_member",
        }
    }
    pub fn rejected(&self) -> &'static str {
        match self {
            Change::Create { .. } => "create_rejected",
            Change::Details { .. } | Change::Topic { .. } => "edit_rejected",
            Change::AddMember { .. } => "member_add_rejected",
            Change::RemoveMember { .. } => "member_remove_rejected",
        }
    }
    /// The room the event is about; a creation gets a fresh one.
    pub fn room(&self, fresh: Uuid) -> Uuid {
        match self {
            Change::Create { .. } => fresh,
            Change::Details { room, .. }
            | Change::Topic { room, .. }
            | Change::AddMember { room, .. }
            | Change::RemoveMember { room, .. } => *room,
        }
    }
    /// The unsigned event. Name and text bounds are checked here too, so the
    /// helper never signs what the panel's own limits would have stopped.
    pub fn build(&self, room: Uuid) -> Result<EventBuilder, &'static str> {
        let built = match self {
            Change::Create {
                name,
                about,
                private,
            } => {
                let name = plain(name, NAME_BYTES).ok_or("room_invalid")?;
                let about = plain(about, ABOUT_BYTES).ok_or("room_invalid")?;
                buzz_sdk::build_create_channel(
                    room,
                    &name,
                    Some(if *private {
                        buzz_sdk::Visibility::Private
                    } else {
                        buzz_sdk::Visibility::Open
                    }),
                    Some(buzz_sdk::ChannelKind::Stream),
                    (!about.is_empty()).then_some(about.as_str()),
                    None,
                )
            }
            Change::Details { name, about, .. } => {
                if name.is_none() && about.is_none() {
                    return Err("room_invalid");
                }
                let name = match name {
                    Some(n) => Some(plain(n, NAME_BYTES).ok_or("room_invalid")?),
                    None => None,
                };
                let about = match about {
                    Some(a) => Some(plain(a, ABOUT_BYTES).ok_or("room_invalid")?),
                    None => None,
                };
                buzz_sdk::build_update_channel(room, name.as_deref(), about.as_deref(), None, None)
            }
            Change::Topic { topic, .. } => {
                let topic = plain(topic, TOPIC_BYTES).ok_or("room_invalid")?;
                buzz_sdk::build_set_topic(room, &topic)
            }
            Change::AddMember { key, .. } => buzz_sdk::build_add_member(room, &key.to_hex(), None),
            Change::RemoveMember { key, .. } => buzz_sdk::build_remove_member(room, &key.to_hex()),
        };
        built.map_err(|_| "room_invalid")
    }
}

/// The panel's catalog row for a validated one.
pub fn project(r: crate::catalog::Room) -> Room {
    Room {
        id: r.id,
        name: r.name,
        description: r.description,
        kind: r.kind.into(),
        participants: r.participants,
        hidden: r.hidden,
    }
}

/// The `more` state after a read: a full last page means more may exist,
/// unless `pages` already reached `catalog::MAX_PAGES`.
/// A catalog cut to `catalog::MAX_ROOMS` (`trimmed`) is at its limit too, even
/// when no further page exists.
pub fn more(has_more: bool, pages: usize, trimmed: bool) -> &'static str {
    match (has_more, pages >= crate::catalog::MAX_PAGES) {
        _ if trimmed => "limit",
        (false, _) => "none",
        (true, true) => "limit",
        (true, false) => "available",
    }
}

/// Catalog rows after a page: a room read again replaces its older row, new
/// rooms follow. Nothing already listed is dropped unless the total would pass
/// `catalog::MAX_ROOMS`: then the newest additions at the end are cut (never
/// the room `keep`) and the second value says so (the list is at its limit).
pub fn merge(held: &[Room], page: Vec<Room>, keep: Option<&str>) -> (Vec<Room>, bool) {
    let fresh: BTreeSet<String> = page.iter().map(|r| r.id.clone()).collect();
    let mut rooms: Vec<Room> = held
        .iter()
        .filter(|r| !fresh.contains(&r.id))
        .cloned()
        .collect();
    rooms.extend(page);
    let mut excess = rooms.len().saturating_sub(crate::catalog::MAX_ROOMS);
    let trimmed = excess > 0;
    let mut at = rooms.len();
    while excess > 0 && at > 0 {
        at -= 1;
        if Some(rooms[at].id.as_str()) != keep {
            rooms.remove(at);
            excess -= 1;
        }
    }
    (rooms, trimmed)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub name: String,
    /// `owner`, `admin`, `member`, `guest`, `bot`, or `unknown` for any other
    /// word in the relay's roster.
    pub role: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub room: String,
    pub topic: String,
    /// `open` or `private`, from the relay-signed metadata.
    pub visibility: String,
    /// The room's description as the relay lists it, up to `ABOUT_BYTES` (the
    /// catalog row's is cut shorter for display). Controls become spaces.
    pub about: String,
    /// The description is longer than `ABOUT_BYTES`: `about` is a cut copy and
    /// is never to be edited and saved back.
    pub about_truncated: bool,
    /// This identity's role in the roster.
    pub role: String,
    pub members: Vec<Member>,
    /// Members beyond `MEMBERS` are not listed.
    pub truncated: bool,
}

fn role_word(value: Option<&String>) -> String {
    match value.map(String::as_str) {
        None | Some("") => "member".into(),
        Some(r @ ("owner" | "admin" | "member" | "guest" | "bot")) => r.into(),
        Some(_) => "unknown".into(),
    }
}
fn rank(role: &str) -> u8 {
    match role {
        "owner" => 0,
        "admin" => 1,
        _ => 2,
    }
}

/// A verified read of one room's relay-signed roster (39002) and metadata
/// (39000): both signed by the pinned relay, scoped to `room`, and the roster
/// must name `member`. Owners and admins are listed first, then by key.
pub fn detail(
    room: Uuid,
    member: PublicKey,
    signer: PublicKey,
    rosters: &[Event],
    metadata: &[Event],
    now: u64,
) -> Result<Detail, &'static str> {
    let ([roster], [meta]) = (rosters, metadata) else {
        return Err("room_detail_invalid");
    };
    let invalid = |_| "room_detail_invalid";
    if validated(roster, signer, 39002, now).map_err(invalid)? != room
        || validated(meta, signer, 39000, now).map_err(invalid)? != room
    {
        return Err("room_detail_invalid");
    }
    let mut seen = BTreeSet::new();
    let mut listed = Vec::new();
    for tag in roster
        .tags
        .iter()
        .filter(|t| t.as_slice().first().is_some_and(|v| v == "p"))
    {
        let slice = tag.as_slice();
        let key = slice.get(1).ok_or("room_detail_invalid")?;
        let public = PublicKey::from_hex(key).map_err(|_| "room_detail_invalid")?;
        if public.to_hex() != *key || !seen.insert(key.clone()) {
            return Err("room_detail_invalid");
        }
        listed.push((key.clone(), role_word(slice.get(3))));
    }
    let Some((_, role)) = listed.iter().find(|(k, _)| *k == member.to_hex()) else {
        return Err("room_detail_access_denied");
    };
    let role = role.clone();
    listed.sort_by(|a, b| rank(&a.1).cmp(&rank(&b.1)).then(a.0.cmp(&b.0)));
    let truncated = listed.len() > MEMBERS;
    listed.truncate(MEMBERS);
    let flag = |n: &str| {
        meta.tags
            .iter()
            .any(|t| t.as_slice().first().is_some_and(|v| v == n))
    };
    // Exactly one of `public` and `private`, as `catalog::open_rooms` requires.
    let visibility = match (flag("public"), flag("private")) {
        (true, false) => "open",
        (false, true) => "private",
        _ => return Err("room_detail_invalid"),
    };
    let topic = clean(
        one_tag(meta, "topic")
            .map_err(|_| "room_detail_invalid")?
            .unwrap_or(""),
        TOPIC_BYTES,
    );
    let raw_about = one_tag(meta, "about")
        .map_err(|_| "room_detail_invalid")?
        .unwrap_or("");
    Ok(Detail {
        room: room.to_string(),
        topic,
        about: clean(raw_about, ABOUT_BYTES),
        about_truncated: raw_about.len() > ABOUT_BYTES,
        visibility: visibility.into(),
        role,
        members: listed
            .into_iter()
            .map(|(key, role)| Member {
                key,
                name: String::new(),
                role,
            })
            .collect(),
        truncated,
    })
}

/// Names from the profile reads `catalog::dm_profile_names` makes, as hints.
pub fn apply_names(detail: &mut Detail, names: &BTreeMap<String, String>) {
    for member in &mut detail.members {
        if let Some(name) = names.get(&member.key) {
            member.name = name.clone();
        }
    }
}

pub async fn fetch_detail(
    relay: &str,
    keys: &Keys,
    signer: PublicKey,
    room: Uuid,
) -> Result<Detail, &'static str> {
    let category = |e: &'static str| match e {
        "query_access_denied" => "room_detail_access_denied",
        "query_invalid_response" | "query_invalid_scope" | "query_invalid_signature" => {
            "room_detail_invalid"
        }
        _ => "room_detail_unavailable",
    };
    let rosters = query(relay, keys, &QueryRequest::RoomMembers { room })
        .await
        .map_err(category)?;
    let metadata = query(
        relay,
        keys,
        &QueryRequest::RoomMetadata { rooms: vec![room] },
    )
    .await
    .map_err(category)?;
    let mut view = detail(
        room,
        keys.public_key(),
        signer,
        &rosters,
        &metadata,
        Timestamp::now().as_secs(),
    )?;
    let wanted = view.members.iter().map(|m| m.key.clone()).collect();
    let names = crate::catalog::dm_profile_names(relay, keys, wanted).await;
    apply_names(&mut view, &names);
    Ok(view)
}

#[cfg(test)]
#[path = "rooms_tests.rs"]
mod tests;
