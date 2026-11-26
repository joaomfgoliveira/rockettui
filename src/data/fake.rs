//! The fake data source (D-13): produces the same `Action`s a real producer will in Phase 3.
//!
//! The dataset is deterministic: 8 rooms covering every `RoomKind`, two folders, long and empty
//! rooms, one encrypted room, and one very long message. Every body is plain text.

use crate::action::Action;
use crate::data::{
    Folder, FolderId, Message, MessageKind, Room, RoomId, RoomKind, SendState, Unread,
};

/// 2026-11-14T00:00:00Z. All fake timestamps are offsets from this.
pub const FAKE_EPOCH: i64 = 1_794_614_400;

/// Seconds after midnight of the first fake message.
const FIRST_MESSAGE_OFFSET: i64 = 28_800;
/// Seconds between consecutive fake messages.
const MESSAGE_STEP: i64 = 120;
/// Index of the very long message in `general`.
const LONG_MESSAGE_INDEX: usize = 30;

const AUTHORS: [&str; 4] = ["alice", "bob", "carol", "dave"];

const BODIES: [&str; 12] = [
    "morning, anyone seen the build status?",
    "build is green on main",
    "can someone review my PR when they get a chance?",
    "standup moved to 10:30 today",
    "lunch? \u{1F355}",
    "これはラップの確認に使う日本語のテストメッセージです。",
    "I'll take a look after the meeting",
    "nice catch, thanks!",
    "the deploy finished without errors",
    "heads up: maintenance window tonight at 22:00",
    "\u{1F44D}",
    "let's sync on this tomorrow",
];

/// One room of the dataset: (name, kind, unread, encrypted, last-activity offset, message count).
type RoomRow = (&'static str, RoomKind, bool, bool, i64, usize);

const ROOMS: [RoomRow; 8] = [
    ("general", RoomKind::Channel, true, false, 36_000, 60),
    ("random", RoomKind::Channel, false, false, 30_000, 12),
    ("quiet", RoomKind::Channel, false, false, 1_000, 0),
    ("dev-team", RoomKind::PrivateGroup, true, false, 35_000, 55),
    ("secrets", RoomKind::PrivateGroup, false, true, 20_000, 4),
    (
        "design-review",
        RoomKind::Discussion,
        false,
        false,
        25_000,
        5,
    ),
    ("platform", RoomKind::Team, false, false, 28_000, 8),
    ("alice", RoomKind::DirectMessage, true, false, 34_000, 6),
];

fn rooms() -> Vec<Room> {
    ROOMS
        .iter()
        .map(|&(name, kind, unread, encrypted, offset, _)| Room {
            id: RoomId(name.to_string()),
            name: name.to_string(),
            kind,
            encrypted,
            private: kind == RoomKind::PrivateGroup,
            unread: if unread {
                Unread::flag()
            } else {
                Unread::default()
            },
            last_seen: None,
            last_activity: FAKE_EPOCH + offset,
            threads: Default::default(),
        })
        .collect()
}

fn folders() -> Vec<Folder> {
    let ids = |names: &[&str]| names.iter().map(|n| RoomId((*n).to_string())).collect();
    vec![
        Folder {
            id: FolderId("work".to_string()),
            name: "Work".to_string(),
            room_ids: ids(&["dev-team", "design-review", "platform"]),
        },
        Folder {
            id: FolderId("people".to_string()),
            name: "People".to_string(),
            room_ids: ids(&["alice"]),
        },
    ]
}

/// 600+ characters of prose that also holds one unbroken 80-character URL-like token, so wrapping
/// and long-word splitting both get exercised.
fn long_body() -> String {
    let sentence = "This is a deliberately long message that keeps going so the message pane \
                    has to wrap it across several rows, with a hanging indent under the author. ";
    let token = format!("https://example.invalid/{}{}", "abcdefgh-".repeat(6), "ij");
    format!("{sentence}{sentence}see {token} for the details. {sentence}{sentence}")
}

fn messages(room: &str, count: usize) -> Vec<Message> {
    (0..count)
        .map(|n| {
            let body = if room == "general" && n == LONG_MESSAGE_INDEX {
                long_body()
            } else {
                BODIES[(n + room.len()) % BODIES.len()].to_string()
            };
            Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                reactions: Vec::new(),
                id: format!("{room}-{n:03}"),
                room_id: RoomId(room.to_string()),
                author: AUTHORS[n % AUTHORS.len()].to_string(),
                ts: FAKE_EPOCH + FIRST_MESSAGE_OFFSET + (n as i64) * MESSAGE_STEP,
                body,
                kind: MessageKind::Normal,
                send: SendState::Sent,
            }
        })
        .collect()
}

/// The Actions a data source sends at startup: rooms, folders, then one `MessagesLoaded` per room.
pub fn seed() -> Vec<Action> {
    let mut actions = vec![
        Action::RoomsLoaded(rooms()),
        Action::FoldersLoaded(folders()),
    ];
    for &(name, _, _, _, _, count) in &ROOMS {
        actions.push(Action::MessagesLoaded(
            RoomId(name.to_string()),
            messages(name, count),
        ));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(rooms.iter().filter(|r| r.unread.is_unread()).count(), 3);

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

        let all: Vec<&Message> = messages.iter().flat_map(|(_, m)| m.iter()).collect();
        assert!(
            all.iter().any(|m| m.body.chars().count() >= 500),
            "one very long message"
        );
        assert!(
            all.iter()
                .any(|m| m.body.split_whitespace().any(|w| w.chars().count() == 80)),
            "one unbroken 80-character token"
        );

        let general = messages
            .iter()
            .find(|(id, _)| id.0 == "general")
            .map(|(_, m)| m)
            .unwrap();
        assert!(general[LONG_MESSAGE_INDEX].body.chars().count() >= 600);

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
