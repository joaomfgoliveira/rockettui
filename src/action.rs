//! The single `Action` enum of the Elm/TEA architecture.
//!
//! Unit variants are keymap-bindable. `Key`, `Resize`, `Clock`, `Paste` and the `*Loaded`
//! variants are event Actions that no keymap row may bind.
//!
//! `Action` carries typed, pasted and message text, so its `Debug` is written by hand and never
//! derived (T-01-03).

use crate::data::{Folder, Message, Room, RoomId, UnreadUpdate};
use crossterm::event::KeyEvent;

#[derive(Clone, PartialEq)]
pub enum Action {
    MoveDown,
    MoveUp,
    MoveLeft,
    MoveRight,
    GotoTop,
    GotoBottom,
    HalfPageDown,
    HalfPageUp,
    Select,
    ToggleFold,
    FocusLeft,
    FocusDown,
    FocusRight,
    /// Cycle focus forward through the sidebar and the messages pane, skipping the input bar
    /// (vim-extras `<Tab>`, D-14).
    FocusNext,
    /// Cycle focus backward, the reverse of `FocusNext` (vim-extras `<S-Tab>`, D-14).
    FocusPrev,
    EnterCommand,
    Cancel,
    /// Enter Insert mode on the input bar (`i`, `a`, `o`).
    EnterInsert,
    InsertChar(char),
    DeleteBack,
    InsertNewline,
    DeleteWordBack,
    DeleteToLineStart,
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorHome,
    CursorEnd,
    /// Enter in Insert mode: send the draft to the open room (D-15).
    SubmitInput,
    /// Normal mode, messages pane focused, cursor on a `Failed` or `Unconfirmed` message of the open
    /// room: put it back in its room's queue under the same client id (D-06, D-07). Anything else
    /// is a silent no-op. Not a motion, so a count repeats it.
    RetrySend,
    /// Normal mode, messages pane focused, cursor on a `Failed` or `Unconfirmed` message of the open
    /// room: remove it from the room (D-06). Anything else is a silent no-op. Not a motion, so a
    /// count repeats it, each repetition re-reading the cursor message.
    DiscardSend,
    /// Normal mode, messages pane focused, cursor on one of the user's own plain-text server
    /// messages (see `app::edit::editable`): load its text into the input and start editing it.
    /// Anything else is a silent no-op.
    EditMessage,
    /// Normal mode, messages pane focused: on a `Failed` or `Unconfirmed` local row discard it
    /// (like `DiscardSend`); on one of the user's own server messages open the confirmation
    /// prompt of a delete. Anything else is a silent no-op. Not a motion, so a count repeats it,
    /// but an open prompt is never opened twice.
    DeleteMessage,
    /// Normal mode, messages pane focused, cursor on a message the server holds (`+`): open the
    /// reaction picker, to add a reaction or take one back. Anything else is a silent no-op, and a
    /// detached room or an unknown own username is refused with a status line.
    ReactMessage,
    /// Normal mode (`Ctrl-k`): open the room switcher, a search over the joined rooms that opens
    /// the chosen one. It changes neither the mode, the focus nor the open room.
    SwitchRoom,
    /// Normal mode, messages pane focused, cursor on a message that has reactions (`gr`): open the
    /// popup that lists who gave each one. Anything else is a silent no-op.
    ShowReactions,
    /// Normal mode, messages pane focused (`gt`): open the thread of the message under the cursor
    /// in the thread pane. A message with no thread says so on the status line.
    OpenThread,
    /// Normal mode, messages or thread pane focused (`gR`): start a reply to the message under the
    /// cursor, opening its thread (empty when it has no replies yet).
    ReplyInThread,
    /// Insert mode (`<C-b>`), reply to a thread: toggle whether it is also shown in the channel.
    ToggleReplyToChannel,
    /// Normal mode, messages or thread pane focused (`gf`): follow the thread of the message under
    /// the cursor, or the open thread, or unfollow it when it is already followed.
    ToggleFollowThread,
    /// Close the thread pane (`Esc` with the pane focused; no default key otherwise).
    CloseThread,
    /// Open the help overlay (no default key; `?` then `?` or `:help`); while it is open, close it.
    ShowHelp,
    /// Open the context popup (`?`): the actions that apply to the selected object.
    ShowContext,
    /// Open the search prompt on the command row (`/`): find text in the open room.
    SearchStart,
    /// Jump to the next match toward older messages (`n`), loading older history if needed.
    SearchNext,
    /// Jump to the previous match, toward newer messages (`N`).
    SearchPrev,
    ExecuteCommand,
    Quit,
    /// A `:` command line bound in keymap.toml as `cmd:<command>`; produced only by the keymap.
    /// The text is user configuration, so `Debug` never prints it (T-02-12).
    RunCommand(String),
    /// A raw key event, resolved through the keymap by the reducer.
    Key(KeyEvent),
    Resize(u16, u16),
    /// Seconds since the Unix epoch, produced by the run loop (the reducer never reads a clock).
    Clock(i64),
    Paste(String),
    RoomsLoaded(Vec<Room>),
    FoldersLoaded(Vec<Folder>),
    MessagesLoaded(RoomId, Vec<Message>),
    /// The user's own username from `GET /api/v1/me` (D-17). Its `Debug` never prints it.
    MeLoaded(String),
    /// The user's own id from the same answer, to tell their follows from the others'. Its `Debug`
    /// never prints it.
    OwnIdLoaded(String),
    /// The wait before the open room is marked read is over (the loop's timer fired).
    MarkReadDue,
    /// The server marked `room` read.
    MarkReadDone {
        room: RoomId,
    },
    /// The room load failed; the error is a fixed kind with no payload text.
    LoadFailed(crate::api::ApiError),
    /// A page of history arrived, oldest first. `oldest_ts` is the raw server timestamp of its
    /// oldest message; `older` is true for a page requested by scrolling back.
    HistoryLoaded {
        room: RoomId,
        messages: Vec<Message>,
        oldest_ts: Option<String>,
        older: bool,
    },
    /// A page of server search results arrived, newest first. `offset` is where the page started
    /// (0 for the first page); `search_id` tells which `:search` asked for it.
    SearchPage {
        room: RoomId,
        search_id: u64,
        offset: u32,
        result: Result<crate::api::SearchHits, crate::api::ApiError>,
    },
    /// The history around a search result arrived. `context_id` tells which open request asked
    /// for it; a stale one is dropped.
    ContextLoaded {
        room: RoomId,
        context_id: u64,
        window: crate::api::ContextWindow,
    },
    /// A page of messages after the newest one of a detached room arrived (`newer_id` tells which
    /// request asked for it; a stale one is dropped).
    NewerLoaded {
        room: RoomId,
        newer_id: u64,
        page: crate::api::NewerPage,
    },
    /// A forward page failed; the error is a fixed kind with no payload text.
    NewerFailed {
        room: RoomId,
        newer_id: u64,
        error: crate::api::ApiError,
    },
    /// A context request failed; the error is a fixed kind with no payload text.
    ContextFailed {
        room: RoomId,
        context_id: u64,
        error: crate::api::ApiError,
    },
    /// A page of a thread's replies arrived, oldest first, for the thread `parent_id` of `room`.
    /// `offset` is where the page started; `parent` is the parent message when the request had to
    /// fetch it (it was not held by the main flow).
    ThreadLoaded {
        room: RoomId,
        parent_id: String,
        offset: u32,
        parent: Option<Message>,
        page: crate::api::ThreadPage,
    },
    /// A thread request failed; the error is a fixed kind with no payload text.
    ThreadLoadFailed {
        room: RoomId,
        parent_id: String,
        offset: u32,
        error: crate::api::ApiError,
    },
    /// A history request failed; the error is a fixed kind with no payload text.
    HistoryFailed {
        room: RoomId,
        older: bool,
        error: crate::api::ApiError,
    },
    /// The server accepted a sent message and answered with its stored copy. `id` is the client
    /// id the message was sent under; the reducer applies the answer by `room` and `id`, never by
    /// the current room.
    MessageSent {
        room: RoomId,
        id: String,
        message: Message,
    },
    /// A send failed; the error is a fixed kind with no payload text. Applied by `room` and `id`
    /// like `MessageSent`.
    SendFailed {
        room: RoomId,
        id: String,
        error: crate::api::ApiError,
    },
    /// The server accepted an edit of message `id` of `room` and answered with the stored copy.
    /// Applied by `room` and `id`, never by the current room.
    EditDone {
        room: RoomId,
        id: String,
        message: Message,
    },
    /// An edit failed; the error is a fixed kind with no payload text.
    EditFailed {
        room: RoomId,
        id: String,
        error: crate::api::ApiError,
    },
    /// The server accepted the delete of message `id` of `room`.
    DeleteDone {
        room: RoomId,
        id: String,
    },
    /// A delete failed; the error is a fixed kind with no payload text.
    DeleteFailed {
        room: RoomId,
        id: String,
        error: crate::api::ApiError,
    },
    /// A reaction could not be sent; the error is a fixed kind with no payload text. A reaction
    /// that was sent produces no action: the server streams the reacted message back.
    ReactFailed {
        error: crate::api::ApiError,
    },
    /// The server accepted a follow (`follow`) or unfollow of thread `parent_id` of `room`.
    FollowDone {
        room: RoomId,
        parent_id: String,
        follow: bool,
    },
    /// A follow or unfollow request failed; the error is a fixed kind with no payload text.
    FollowFailed {
        follow: bool,
        error: crate::api::ApiError,
    },
    /// The server's custom emojis arrived (`Effect::LoadCustomEmojis`).
    CustomEmojisLoaded(Vec<crate::data::CustomEmoji>),
    /// The custom emojis could not be fetched; the picker goes on without them.
    CustomEmojisFailed,
    /// The image of a custom emoji was fetched and decoded (`Effect::FetchEmojiImage`).
    EmojiImageReady {
        emoji: crate::effect::EmojiRef,
        image: crate::emoji_image::Rgba,
    },
    /// The image of a custom emoji could not be fetched or decoded. Final for the session.
    EmojiImageFailed(crate::effect::EmojiRef),
    /// The realtime connection is live (logged in and subscribed). Produced only by the realtime
    /// task. The reducer decides from `App.rt_seen_live` whether it is a reconnect.
    RtConnected,
    /// The realtime connection dropped and the task is about to retry.
    RtDisconnected,
    /// The server rejected the realtime login; the task stops retrying until `:reconnect`.
    RtRejected,
    /// A new message arrived on the realtime stream, already mapped (sanitizing and the
    /// encrypted-message rule included). Its `Debug` never prints the body.
    RtMessage(Message),
    /// The server reported a change to the user's subscription to `room`: what it now says about unread (alert, counts, mentions).
    RtSubscription {
        room: RoomId,
        update: UnreadUpdate,
    },
    /// A message of `room` was deleted (`stream-notify-room` `<rid>/deleteMessage`). The payload
    /// has only the id, so there is no text to print.
    RtDeleted {
        room: RoomId,
        id: String,
    },
    /// The pending key sequence timed out; produced by the run loop's deadline, never by a key.
    KeyTimeout,
    /// An action with a typed count prefix (`5j`), produced by the reducer from the matcher.
    Counted(u32, Box<Action>),
}

impl Action {
    /// True for the motions that the per-pane router handles and that accept a count as a jump.
    pub fn is_motion(&self) -> bool {
        matches!(
            self,
            Action::MoveDown
                | Action::MoveUp
                | Action::MoveLeft
                | Action::MoveRight
                | Action::GotoTop
                | Action::GotoBottom
                | Action::HalfPageDown
                | Action::HalfPageUp
                | Action::Select
                | Action::ToggleFold
        )
    }
}

/// Redacting `Debug`: never prints typed characters, pasted text or message text. The match is
/// exhaustive on purpose, so every variant a later plan adds must choose how it prints.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::MoveDown => write!(f, "MoveDown"),
            Action::MoveUp => write!(f, "MoveUp"),
            Action::MoveLeft => write!(f, "MoveLeft"),
            Action::MoveRight => write!(f, "MoveRight"),
            Action::GotoTop => write!(f, "GotoTop"),
            Action::GotoBottom => write!(f, "GotoBottom"),
            Action::HalfPageDown => write!(f, "HalfPageDown"),
            Action::HalfPageUp => write!(f, "HalfPageUp"),
            Action::Select => write!(f, "Select"),
            Action::ToggleFold => write!(f, "ToggleFold"),
            Action::FocusLeft => write!(f, "FocusLeft"),
            Action::FocusDown => write!(f, "FocusDown"),
            Action::FocusRight => write!(f, "FocusRight"),
            Action::FocusNext => write!(f, "FocusNext"),
            Action::FocusPrev => write!(f, "FocusPrev"),
            Action::EnterCommand => write!(f, "EnterCommand"),
            Action::Cancel => write!(f, "Cancel"),
            Action::EnterInsert => write!(f, "EnterInsert"),
            Action::InsertChar(_) => write!(f, "InsertChar(..)"),
            Action::DeleteBack => write!(f, "DeleteBack"),
            Action::InsertNewline => write!(f, "InsertNewline"),
            Action::DeleteWordBack => write!(f, "DeleteWordBack"),
            Action::DeleteToLineStart => write!(f, "DeleteToLineStart"),
            Action::CursorLeft => write!(f, "CursorLeft"),
            Action::CursorRight => write!(f, "CursorRight"),
            Action::CursorUp => write!(f, "CursorUp"),
            Action::CursorDown => write!(f, "CursorDown"),
            Action::CursorHome => write!(f, "CursorHome"),
            Action::CursorEnd => write!(f, "CursorEnd"),
            Action::SubmitInput => write!(f, "SubmitInput"),
            Action::RetrySend => write!(f, "RetrySend"),
            Action::DiscardSend => write!(f, "DiscardSend"),
            Action::EditMessage => write!(f, "EditMessage"),
            Action::DeleteMessage => write!(f, "DeleteMessage"),
            Action::ShowReactions => write!(f, "ShowReactions"),
            Action::ReactMessage => write!(f, "ReactMessage"),
            Action::SwitchRoom => write!(f, "SwitchRoom"),
            Action::OpenThread => write!(f, "OpenThread"),
            Action::ReplyInThread => write!(f, "ReplyInThread"),
            Action::ToggleReplyToChannel => write!(f, "ToggleReplyToChannel"),
            Action::CloseThread => write!(f, "CloseThread"),
            Action::ShowHelp => write!(f, "ShowHelp"),
            Action::ShowContext => write!(f, "ShowContext"),
            Action::SearchStart => write!(f, "SearchStart"),
            Action::SearchNext => write!(f, "SearchNext"),
            Action::SearchPrev => write!(f, "SearchPrev"),
            Action::ExecuteCommand => write!(f, "ExecuteCommand"),
            Action::Quit => write!(f, "Quit"),
            Action::RunCommand(_) => write!(f, "RunCommand(..)"),
            Action::Key(_) => write!(f, "Key(..)"),
            Action::Resize(w, h) => write!(f, "Resize({w}, {h})"),
            Action::Clock(secs) => write!(f, "Clock({secs})"),
            Action::Paste(text) => write!(f, "Paste(<{} bytes>)", text.len()),
            Action::RoomsLoaded(rooms) => write!(f, "RoomsLoaded(<{} rooms>)", rooms.len()),
            Action::FoldersLoaded(folders) => {
                write!(f, "FoldersLoaded(<{} folders>)", folders.len())
            }
            Action::MessagesLoaded(room_id, messages) => {
                write!(
                    f,
                    "MessagesLoaded({:?}, <{} messages>)",
                    room_id,
                    messages.len()
                )
            }
            Action::MeLoaded(_) => write!(f, "MeLoaded(..)"),
            Action::OwnIdLoaded(_) => write!(f, "OwnIdLoaded(..)"),
            Action::ToggleFollowThread => write!(f, "ToggleFollowThread"),
            Action::FollowDone { follow, .. } => write!(f, "FollowDone({follow})"),
            Action::FollowFailed { follow, error } => {
                write!(f, "FollowFailed({follow}, {error:?})")
            }
            Action::MarkReadDue => write!(f, "MarkReadDue"),
            Action::MarkReadDone { room } => write!(f, "MarkReadDone({room:?})"),
            Action::LoadFailed(err) => write!(f, "LoadFailed({err:?})"),
            Action::HistoryLoaded {
                room,
                messages,
                older,
                ..
            } => write!(
                f,
                "HistoryLoaded({:?}, <{} messages>, older={older})",
                room,
                messages.len()
            ),
            Action::SearchPage {
                room,
                search_id,
                offset,
                result,
            } => match result {
                Ok(found) => write!(
                    f,
                    "SearchPage({room:?}, #{search_id}, offset={offset}, <{} messages>)",
                    found.messages.len()
                ),
                Err(error) => write!(
                    f,
                    "SearchPage({room:?}, #{search_id}, offset={offset}, {error:?})"
                ),
            },
            Action::ContextLoaded {
                room,
                context_id,
                window,
            } => write!(f, "ContextLoaded({room:?}, #{context_id}, {window:?})"),
            Action::NewerLoaded {
                room,
                newer_id,
                page,
            } => write!(f, "NewerLoaded({room:?}, #{newer_id}, {page:?})"),
            Action::NewerFailed {
                room,
                newer_id,
                error,
            } => write!(f, "NewerFailed({room:?}, #{newer_id}, {error:?})"),
            Action::ContextFailed {
                room,
                context_id,
                error,
            } => write!(f, "ContextFailed({room:?}, #{context_id}, {error:?})"),
            Action::ThreadLoaded {
                room,
                parent_id,
                offset,
                page,
                ..
            } => write!(
                f,
                "ThreadLoaded({room:?}, {parent_id}, offset={offset}, {page:?})"
            ),
            Action::ThreadLoadFailed {
                room,
                parent_id,
                offset,
                error,
            } => write!(
                f,
                "ThreadLoadFailed({room:?}, {parent_id}, offset={offset}, {error:?})"
            ),
            Action::HistoryFailed { room, older, error } => {
                write!(f, "HistoryFailed({room:?}, older={older}, {error:?})")
            }
            Action::MessageSent { room, id, .. } => write!(f, "MessageSent({room:?}, {id})"),
            Action::SendFailed { room, id, error } => {
                write!(f, "SendFailed({room:?}, {id}, {error:?})")
            }
            Action::EditDone { room, id, .. } => write!(f, "EditDone({room:?}, {id})"),
            Action::EditFailed { room, id, error } => {
                write!(f, "EditFailed({room:?}, {id}, {error:?})")
            }
            Action::DeleteDone { room, id } => write!(f, "DeleteDone({room:?}, {id})"),
            Action::DeleteFailed { room, id, error } => {
                write!(f, "DeleteFailed({room:?}, {id}, {error:?})")
            }
            Action::ReactFailed { error } => write!(f, "ReactFailed({error:?})"),
            Action::CustomEmojisLoaded(list) => write!(f, "CustomEmojisLoaded(<{}>)", list.len()),
            Action::CustomEmojisFailed => write!(f, "CustomEmojisFailed"),
            Action::EmojiImageReady { emoji, image } => {
                write!(f, "EmojiImageReady({emoji:?}, {image:?})")
            }
            Action::EmojiImageFailed(emoji) => write!(f, "EmojiImageFailed({emoji:?})"),
            Action::RtConnected => write!(f, "RtConnected"),
            Action::RtDisconnected => write!(f, "RtDisconnected"),
            Action::RtRejected => write!(f, "RtRejected"),
            Action::RtMessage(message) => write!(f, "RtMessage({message:?})"),
            Action::RtSubscription { room, update } => {
                write!(f, "RtSubscription({room:?}, {update:?})")
            }
            Action::RtDeleted { room, id } => write!(f, "RtDeleted({room:?}, {id})"),
            Action::KeyTimeout => write!(f, "KeyTimeout"),
            Action::Counted(n, inner) => write!(f, "Counted({n}, {inner:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{MessageKind, SendState};
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn debug_redacts() {
        let paste = format!("{:?}", Action::Paste("SENTINEL-PASTE-91c2".to_string()));
        assert!(!paste.contains("SENTINEL-PASTE-91c2"));

        let insert = format!("{:?}", Action::InsertChar('Z'));
        assert!(!insert.contains('Z'));
        let key = format!(
            "{:?}",
            Action::Key(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::NONE))
        );
        assert!(!key.contains('Z'));

        let message = Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            reactions: Vec::new(),
            id: "general-000".to_string(),
            room_id: RoomId("general".to_string()),
            author: "alice".to_string(),
            ts: 0,
            body: "SENTINEL-BODY-7f3a".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        let loaded = format!(
            "{:?}",
            Action::MessagesLoaded(RoomId("general".to_string()), vec![message])
        );
        assert!(!loaded.contains("SENTINEL-BODY-7f3a"));
        assert!(loaded.contains("general"));
        assert!(loaded.contains('1'));

        assert!(format!("{:?}", Action::Clock(42)).contains("42"));

        let run = format!("{:?}", Action::RunCommand("SENTINEL-CMD-5e2a".to_string()));
        assert!(!run.contains("SENTINEL-CMD-5e2a"));
        assert_eq!(run, "RunCommand(..)");

        let me = format!("{:?}", Action::MeLoaded("SENTINEL-AUTHOR-2b9d".to_string()));
        assert!(!me.contains("SENTINEL-AUTHOR-2b9d"), "{me}");
        assert_eq!(me, "MeLoaded(..)");

        let failed = format!(
            "{:?}",
            Action::LoadFailed(crate::api::ApiError::Server(502))
        );
        assert_eq!(failed, "LoadFailed(Server(502))");

        let history = Action::HistoryLoaded {
            room: RoomId("r-general".to_string()),
            messages: vec![Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                reactions: Vec::new(),
                id: "m1".to_string(),
                room_id: RoomId("r-general".to_string()),
                author: "alice".to_string(),
                ts: 0,
                body: "SENTINEL-BODY-7f3a".to_string(),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            }],
            oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
            older: false,
        };
        let shown = format!("{history:?}");
        assert!(!shown.contains("SENTINEL-BODY-7f3a"), "{shown}");
        assert!(!shown.contains("2026-11-16"), "{shown}");
        assert_eq!(
            shown,
            "HistoryLoaded(RoomId(\"r-general\"), <1 messages>, older=false)"
        );
        let failed = format!(
            "{:?}",
            Action::HistoryFailed {
                room: RoomId("r-general".to_string()),
                older: true,
                error: crate::api::ApiError::Timeout,
            }
        );
        assert_eq!(
            failed,
            "HistoryFailed(RoomId(\"r-general\"), older=true, Timeout)"
        );

        let sent = Action::MessageSent {
            room: RoomId("r-general".to_string()),
            id: "rt-test-1".to_string(),
            message: Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                reactions: Vec::new(),
                id: "rt-test-1".to_string(),
                room_id: RoomId("r-general".to_string()),
                author: "smoke".to_string(),
                ts: 0,
                body: "SENTINEL-BODY-7f3a".to_string(),
                send: SendState::Sent,
                kind: MessageKind::Normal,
            },
        };
        let shown = format!("{sent:?}");
        assert!(!shown.contains("SENTINEL-BODY-7f3a"), "{shown}");
        assert_eq!(shown, "MessageSent(RoomId(\"r-general\"), rt-test-1)");
        let failed = Action::SendFailed {
            room: RoomId("r-general".to_string()),
            id: "rt-test-1".to_string(),
            error: crate::api::ApiError::Server(502),
        };
        assert_eq!(
            format!("{failed:?}"),
            "SendFailed(RoomId(\"r-general\"), rt-test-1, Server(502))"
        );
    }
}
