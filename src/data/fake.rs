//! The fake data source (D-13): produces the same `Action`s a real producer will in Phase 3.

use crate::action::Action;
use crate::data::{Message, Room, RoomId, RoomKind};

fn room(name: &str, kind: RoomKind, unread: bool, last_activity: i64) -> Room {
    Room {
        id: RoomId(name.to_string()),
        name: name.to_string(),
        kind,
        encrypted: false,
        unread,
        last_activity,
    }
}

fn messages_for(room: &str, bodies: &[&str]) -> Vec<Message> {
    let authors = ["alice", "bob", "carol"];
    bodies
        .iter()
        .enumerate()
        .map(|(n, body)| Message {
            id: format!("{room}-{n:03}"),
            room_id: RoomId(room.to_string()),
            author: authors[n % authors.len()].to_string(),
            ts: 1_794_614_400 + 28_800 + (n as i64) * 120,
            body: (*body).to_string(),
        })
        .collect()
}

/// The Actions a data source sends at startup: rooms, folders, then one `MessagesLoaded` per room.
pub fn seed() -> Vec<Action> {
    let rooms = vec![
        room("general", RoomKind::Channel, true, 1_794_614_400 + 36_000),
        room("random", RoomKind::Channel, false, 1_794_614_400 + 30_000),
        room(
            "alice",
            RoomKind::DirectMessage,
            false,
            1_794_614_400 + 34_000,
        ),
    ];
    let mut actions = vec![
        Action::RoomsLoaded(rooms),
        Action::FoldersLoaded(Vec::new()),
    ];
    actions.push(Action::MessagesLoaded(
        RoomId("general".to_string()),
        messages_for("general", &["morning", "standup in five", "ship it"]),
    ));
    actions.push(Action::MessagesLoaded(
        RoomId("random".to_string()),
        messages_for("random", &["coffee anyone?", "yes please"]),
    ));
    actions.push(Action::MessagesLoaded(
        RoomId("alice".to_string()),
        messages_for("alice", &["hey", "got a minute?"]),
    ));
    actions
}
