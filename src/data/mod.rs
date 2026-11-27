//! Domain types shared by the model, the reducer and (later) the view, plus the data-source seam.
//!
//! Everything in here reaches `App` only as an `Action` produced by a data source
//! (`fake::seed()` in Phase 1, REST tasks in Phase 3).

pub mod fake;

use std::collections::HashSet;

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

/// The server's unread state of a room: the alert flag, the unread message count and the mention
/// total (user mentions plus group mentions). Never computed from the messages the app holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unread {
    pub flagged: bool,
    pub count: u32,
    pub mentions: u32,
}

/// What one live subscription update says about the unread state. A key the update lacks is `None`
/// and leaves the held value alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnreadUpdate {
    pub alert: Option<bool>,
    pub unread: Option<u32>,
    pub user_mentions: Option<u32>,
    pub group_mentions: Option<u32>,
    pub threads: ThreadUnreadUpdate,
}

/// The thread fields of one live subscription update. A field the update lacks is `None` and keeps
/// the held set; a field it carries, even empty, replaces the held set whole (so ids of deleted
/// threads drop out).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadUnreadUpdate {
    pub unread: Option<HashSet<String>>,
    pub user: Option<HashSet<String>>,
    pub group: Option<HashSet<String>>,
}

impl ThreadUnreadUpdate {
    pub fn is_empty(&self) -> bool {
        self.unread.is_none() && self.user.is_none() && self.group.is_none()
    }
}

impl UnreadUpdate {
    /// True when the update carries none of the unread keys (it is about something else).
    pub fn is_empty(&self) -> bool {
        self.alert.is_none()
            && self.unread.is_none()
            && self.user_mentions.is_none()
            && self.group_mentions.is_none()
            && self.threads.is_empty()
    }
}

impl Unread {
    /// An unread room with no count: what a test or the fake data needs.
    pub fn flag() -> Self {
        Unread {
            flagged: true,
            ..Unread::default()
        }
    }

    /// Whether the room is unread: the server flagged it or it holds a positive count.
    pub fn is_unread(&self) -> bool {
        self.flagged || self.count > 0
    }

    /// Apply a live update; only the keys it carries change. `flagged` follows `alert` (absent
    /// means not alerting, as long as the update speaks of unread at all). A zero count with no
    /// mention keys also clears the mentions, since a read room has none.
    pub fn merge(&mut self, update: UnreadUpdate) {
        if update.is_empty() {
            return;
        }
        if update.alert.is_some() || update.unread.is_some() {
            self.flagged = update.alert.unwrap_or(false);
        }
        if let Some(count) = update.unread {
            self.count = count;
        }
        if update.user_mentions.is_some() || update.group_mentions.is_some() {
            self.mentions = update
                .user_mentions
                .unwrap_or(0)
                .saturating_add(update.group_mentions.unwrap_or(0));
        } else if update.unread == Some(0) {
            self.mentions = 0;
        }
    }
}

/// The server's thread unread state of a room: the ids of the threads with replies the user has not
/// read (`tunread`) and the subsets where the user (`tunreadUser`) or a group (`tunreadGroup`,
/// `@all` and `@here`) was mentioned. Sets of ids, never counts: the app does not see every reply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadUnread {
    pub unread: HashSet<String>,
    pub user: HashSet<String>,
    pub group: HashSet<String>,
}

impl ThreadUnread {
    /// The number of unread threads: what the sidebar badge shows.
    pub fn count(&self) -> usize {
        self.unread.len()
    }

    /// Whether the thread is unread.
    pub fn is_unread(&self, thread: &str) -> bool {
        self.unread.contains(thread)
    }

    /// Apply a live update: a field it carries replaces the held set, a field it lacks keeps it.
    pub fn merge(&mut self, update: &ThreadUnreadUpdate) {
        if let Some(unread) = &update.unread {
            self.unread.clone_from(unread);
        }
        if let Some(user) = &update.user {
            self.user.clone_from(user);
        }
        if let Some(group) = &update.group {
            self.group.clone_from(group);
        }
    }

    /// Whether the user was mentioned (by name or by group) in any unread thread.
    pub fn has_mention(&self) -> bool {
        !self.user.is_empty() || !self.group.is_empty()
    }
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
    pub unread: Unread,
    /// Unix seconds when the user last read the room (`ls` of the subscription); `None` when the
    /// server has none (a room never read). Only the unread divider uses it.
    pub last_seen: Option<i64>,
    /// Unix seconds of the last activity in the room.
    pub last_activity: i64,
    /// Which threads of the room have replies the user has not read, as the server says.
    pub threads: ThreadUnread,
}

impl Room {
    /// Whether the room counts as unread for the sidebar: the server flagged it, it holds unread
    /// messages, or it has unread threads. The room's own count and flag stay what the server says
    /// (`unread`); this is only the one place the sidebar asks "is this room unread".
    pub fn is_unread(&self) -> bool {
        self.unread.is_unread() || self.threads.count() > 0
    }
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
    /// A message that was deleted while the app was running: a tombstone that keeps the id, author
    /// and time, with an empty body. Session-only: history never returns deleted messages.
    Deleted,
}

/// Where a message stands on its way to the server (MSG-02). Messages that came from the server
/// are `Sent`; the other four belong to a message this client composed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendState {
    /// A server message: delivered and stored, or loaded from history.
    #[default]
    Sent,
    /// Waiting its turn in its room. `reconcile` is true when the last attempt's outcome was
    /// unknown, so the retry first asks `chat.getMessage` whether the server already holds it.
    Queued { reconcile: bool },
    /// The one request in flight for its room.
    Sending,
    /// Definitely not delivered: the server refused it or the request never left (D-06).
    Failed,
    /// The outcome is unknown: the request may or may not have reached the server (D-07, D-18).
    Unconfirmed,
}

impl SendState {
    /// True while the message is waiting for or inside its request (`Queued` or `Sending`).
    pub fn is_pending(self) -> bool {
        matches!(self, SendState::Queued { .. } | SendState::Sending)
    }

    /// True for every state but `Sent`.
    pub fn is_unsent(self) -> bool {
        self != SendState::Sent
    }
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
    /// `Sent` for every server message; the other states mark a message this client sent.
    pub send: SendState,
    /// True when the server reports the message as edited (`editedAt`), live or in history.
    pub edited: bool,
    /// True when `body` is exactly the text the server stores for the message: false for a message
    /// with a file or attachment (its body then has ` [attachment: ..]` text appended), for the
    /// encrypted placeholder and for a system message. Only a plain-text message can be edited or
    /// deleted by this client, so edit text is never written back from display text.
    pub plain_text: bool,
    /// The reactions in the order the server lists them; empty when there are none.
    pub reactions: Vec<Reaction>,
    /// The id of the thread parent when this message is a reply (`tmid`); `None` for any other.
    pub tmid: Option<String>,
    /// True for a reply that was also sent to the channel (`tshow`): it shows in the main flow.
    pub tshow: bool,
    /// On a thread parent, how many replies it has (`tcount`); 0 for any other message.
    pub tcount: u32,
    /// On a thread parent, unix seconds of the last reply (`tlm`).
    pub tlm: Option<i64>,
    /// On a thread parent, the ids of the users who follow the thread (`replies`); `None` when the
    /// server did not say. Private: only compared with the own user id, never drawn or logged.
    pub followers: Option<Vec<String>>,
    /// The users and channels the server says the text references (`mentions`, `channels`);
    /// `None` when the message carried neither list (older history, other servers). Private:
    /// drawn as highlights and used to open a channel, never logged.
    pub refs: Option<Refs>,
}

/// The mentions and channel references of one message, as the server lists them.
#[derive(Clone, PartialEq, Default)]
pub struct Refs {
    /// Usernames without the `@`. Untrusted text: sanitize before drawing.
    pub mentions: Vec<String>,
    /// Room names without the `#`. Untrusted text: sanitize before drawing.
    pub channels: Vec<String>,
}

/// Redacting `Debug`: counts only. No username or room name is printed.
impl std::fmt::Debug for Refs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Refs")
            .field("mentions", &self.mentions.len())
            .field("channels", &self.channels.len())
            .finish()
    }
}

impl Message {
    /// True when the thread of this parent is followed by the user with id `own_id`. A parent
    /// whose follower list is unknown counts as not followed.
    pub fn followed_by(&self, own_id: &str) -> bool {
        self.followers
            .as_ref()
            .is_some_and(|f| f.iter().any(|id| id == own_id))
    }

    /// True when the main message flow draws and counts this message: everything but a thread
    /// reply that was not also sent to the channel.
    pub fn in_main_flow(&self) -> bool {
        self.tmid.is_none() || self.tshow
    }
}

/// One reaction of a message: the shortcode and who gave it.
#[derive(Clone, PartialEq)]
pub struct Reaction {
    /// The shortcode without the colons (`+1`, `thumbsup_tone2`, `partyparrot`). Untrusted text:
    /// sanitize before drawing.
    pub name: String,
    /// The usernames that gave it, in the order the server lists them. Never empty. Private: only
    /// drawn on screen, never logged.
    pub users: Vec<String>,
}

/// A custom emoji of the server: its name (no colons) and the other names it answers to. Untrusted
/// text: sanitize before drawing. Never printed: the `Debug` shows only how many aliases it has.
#[derive(Clone, Default, PartialEq)]
pub struct CustomEmoji {
    pub name: String,
    pub aliases: Vec<String>,
    /// The file extension of the image as the server lists it (`png`), empty when it lists none.
    pub ext: String,
    /// The server's `etag` of the image, empty when it lists none: a changed image has a new one.
    pub etag: String,
}

impl std::fmt::Debug for CustomEmoji {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomEmoji")
            .field("aliases", &self.aliases.len())
            .finish()
    }
}

/// Redacting `Debug`: the count only. Neither the shortcode nor the usernames are printed.
impl std::fmt::Debug for Reaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reaction")
            .field("users", &self.users.len())
            .finish()
    }
}

/// Redacting `Debug` (T-01-03): only the id, the room id, the body length, the send state and the
/// number of reactions.
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("room_id", &self.room_id)
            .field("body_len", &self.body.len())
            .field("send", &self.send)
            .field("edited", &self.edited)
            .field("plain_text", &self.plain_text)
            .field("reactions", &self.reactions.len())
            .field("refs", &self.refs)
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
    fn debug_shows_only_the_number_of_reactions() {
        let message = Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: vec![Reaction {
                name: "SENTINEL-REACTION-5c1e".to_string(),
                users: vec!["SENTINEL-USER-9a4d".to_string(), "other".to_string()],
            }],
            id: "general-000".to_string(),
            room_id: RoomId("general".to_string()),
            author: "someone".to_string(),
            ts: 0,
            body: "text".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        for shown in [format!("{message:?}"), format!("{:#?}", message)] {
            assert!(!shown.contains("SENTINEL-REACTION-5c1e"), "{shown}");
            assert!(!shown.contains("SENTINEL-USER-9a4d"), "{shown}");
            assert!(shown.contains("reactions: 1"), "{shown}");
        }
        let one = format!("{:?}", message.reactions[0]);
        assert!(!one.contains("SENTINEL"), "{one}");
    }

    #[test]
    fn debug_shows_only_the_number_of_mentions_and_channels() {
        let mut message = crate::test_util::app_80x24()
            .messages
            .values()
            .next()
            .unwrap()[0]
            .clone();
        message.refs = Some(Refs {
            mentions: vec!["SENTINEL-MENTION-6d2f".to_string()],
            channels: vec!["SENTINEL-CHANNEL-81ac".to_string(), "b".to_string()],
        });
        for shown in [format!("{message:?}"), format!("{message:#?}")] {
            assert!(!shown.contains("SENTINEL"), "{shown}");
            assert!(shown.contains("mentions: 1"), "{shown}");
            assert!(shown.contains("channels: 2"), "{shown}");
        }
    }

    #[test]
    fn debug_redacts() {
        let message = Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: "general-000".to_string(),
            room_id: RoomId("general".to_string()),
            author: "SENTINEL-AUTHOR-2b9d".to_string(),
            ts: 0,
            body: "SENTINEL-BODY-7f3a".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        let shown = format!("{message:?}");
        assert!(!shown.contains("SENTINEL-BODY-7f3a"));
        assert!(!shown.contains("SENTINEL-AUTHOR-2b9d"));
        assert!(shown.contains("general-000"));
        assert!(shown.contains("general"));
        assert!(shown.contains("send: Sent"), "{shown}");
        let failed = Message {
            send: SendState::Failed,
            ..message
        };
        let shown = format!("{failed:?}");
        assert!(shown.contains("send: Failed"), "{shown}");
        assert!(!shown.contains("SENTINEL"), "{shown}");
    }

    #[test]
    fn send_state_predicates() {
        let all = [
            (SendState::Sent, false, false),
            (SendState::Queued { reconcile: false }, true, true),
            (SendState::Queued { reconcile: true }, true, true),
            (SendState::Sending, true, true),
            (SendState::Failed, false, true),
            (SendState::Unconfirmed, false, true),
        ];
        for (state, pending, unsent) in all {
            assert_eq!(state.is_pending(), pending, "{state:?}");
            assert_eq!(state.is_unsent(), unsent, "{state:?}");
        }
        assert_eq!(SendState::default(), SendState::Sent);
    }
}
