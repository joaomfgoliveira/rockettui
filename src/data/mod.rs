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

/// One chat message. It has no derived `Debug`: bodies are never printed.
#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub room_id: RoomId,
    pub author: String,
    /// Unix seconds.
    pub ts: i64,
    pub body: String,
}
