//! Domain types shared by the model, the reducer and (later) the view, plus the data-source seam.
//!
//! Everything in here reaches `App` only as an `Action` produced by a data source
//! (`fake::seed()` in Phase 1, REST tasks in Phase 3).

pub mod fake;

/// Stable room identifier (the server room id in later phases).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoomId(pub String);

/// Stable folder identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(pub String);

/// The five Rocket.Chat room kinds the sidebar has to represent (D-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Channel,
    PrivateGroup,
    Discussion,
    Team,
    DirectMessage,
}

/// A room as the sidebar sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    pub kind: RoomKind,
    pub encrypted: bool,
    /// True when the server room type is `p`; it selects `groups.history`. Private groups are
    /// always `p`, discussions and teams can be either.
    pub private: bool,
    pub unread: bool,
    /// Unix seconds of the last activity in the room.
    pub last_activity: i64,
}

/// A user-defined sidebar folder; membership is by room id (D-12).
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub room_ids: Vec<RoomId>,
}

/// What a message is, for drawing: an ordinary message or a server system message (D-15).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Normal,
    /// A server system message (joined, left, topic changed, ...), drawn dim as a one-liner.
    System,
}

/// One chat message. Its `Debug` is written by hand and prints no body or author text.
#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub room_id: RoomId,
    pub author: String,
    /// Unix seconds.
    pub ts: i64,
    pub body: String,
    /// `System` marks a server system message, drawn dim per D-15.
    pub kind: MessageKind,
}

/// Redacting `Debug` (T-01-03): only the id, the room id and the body length.
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("room_id", &self.room_id)
            .field("body_len", &self.body.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::app_80x24;

    #[test]
    fn with_fake_data_loads_everything() {
        let app = app_80x24();
        assert_eq!(app.rooms.len(), 8);
        assert_eq!(app.folders.len(), 2);
        for room in &app.rooms {
            assert!(app.messages.contains_key(&room.id), "{}", room.name);
        }
    }

    #[test]
    fn debug_redacts() {
        let message = Message {
            id: "general-000".to_string(),
            room_id: RoomId("general".to_string()),
            author: "SENTINEL-AUTHOR-2b9d".to_string(),
            ts: 0,
            body: "SENTINEL-BODY-7f3a".to_string(),
            kind: MessageKind::Normal,
        };
        let shown = format!("{message:?}");
        assert!(!shown.contains("SENTINEL-BODY-7f3a"));
        assert!(!shown.contains("SENTINEL-AUTHOR-2b9d"));
        assert!(shown.contains("general-000"));
        assert!(shown.contains("general"));
    }
}
