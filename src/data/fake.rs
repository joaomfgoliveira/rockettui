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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Folder;

    type Parts = (Vec<Room>, Vec<Folder>, Vec<(RoomId, Vec<Message>)>);

    fn parts() -> Parts {
        let mut rooms = Vec::new();
        let mut folders = Vec::new();
        let mut messages = Vec::new();
        for action in seed() {
            match action {
                Action::RoomsLoaded(r) => rooms = r,
                Action::FoldersLoaded(f) => folders = f,
                Action::MessagesLoaded(id, m) => messages.push((id, m)),
                _ => panic!("seed holds only data Actions"),
            }
        }
        (rooms, folders, messages)
    }

    #[test]
    fn seed_matches_d13() {
        let (rooms, folders, messages) = parts();
        assert_eq!(rooms.len(), 8);
        assert_eq!(rooms[0].name, "general");

        for kind in [
            RoomKind::Channel,
            RoomKind::PrivateGroup,
            RoomKind::Discussion,
            RoomKind::Team,
            RoomKind::DirectMessage,
        ] {
            assert!(rooms.iter().any(|r| r.kind == kind), "{kind:?}");
        }

        let count_of = |id: &RoomId| {
            messages
                .iter()
                .find(|(room_id, _)| room_id == id)
                .map_or(0, |(_, m)| m.len())
        };

        let encrypted: Vec<&Room> = rooms.iter().filter(|r| r.encrypted).collect();
        assert_eq!(encrypted.len(), 1);
        assert_eq!(encrypted[0].name, "secrets");
        assert!(count_of(&encrypted[0].id) > 0);

        assert_eq!(rooms.iter().filter(|r| r.unread).count(), 3);

        let names: Vec<&str> = folders.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"Work"));
        assert!(names.contains(&"People"));
        for folder in &folders {
            for id in &folder.room_ids {
                assert!(rooms.iter().any(|r| &r.id == id), "{id:?}");
            }
        }

        assert!(
            rooms.iter().filter(|r| count_of(&r.id) >= 50).count() >= 2,
            "at least two rooms with 50+ messages"
        );

        let empty: Vec<&Room> = rooms.iter().filter(|r| count_of(&r.id) == 0).collect();
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].name, "quiet");
        assert!(!empty[0].encrypted);

        assert!(
            messages
                .iter()
                .flat_map(|(_, m)| m.iter())
                .any(|m| m.body.chars().count() >= 500),
            "one very long message"
        );

        assert_eq!(messages.len(), rooms.len());
        for room in &rooms {
            assert_eq!(
                messages.iter().filter(|(id, _)| id == &room.id).count(),
                1,
                "{}",
                room.name
            );
        }
    }
}
