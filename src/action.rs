//! The single `Action` enum of the Elm/TEA architecture.
//!
//! Unit variants are keymap-bindable. `Key`, `Resize`, `Clock`, `Paste` and the `*Loaded`
//! variants are event Actions that no keymap row may bind.
//!
//! `Action` carries typed, pasted and message text, so its `Debug` is written by hand and never
//! derived (T-01-03).

use crate::data::{Folder, Message, Room, RoomId};
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
    /// Cycle focus forward: Sidebar, Messages, Input, Sidebar (vim-extras `<Tab>`, D-14).
    FocusNext,
    /// Cycle focus backward (vim-extras `<S-Tab>`, D-14).
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
            Action::HistoryFailed { room, older, error } => {
                write!(f, "HistoryFailed({room:?}, older={older}, {error:?})")
            }
            Action::MessageSent { room, id, .. } => write!(f, "MessageSent({room:?}, {id})"),
            Action::SendFailed { room, id, error } => {
                write!(f, "SendFailed({room:?}, {id}, {error:?})")
            }
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

        let failed = format!(
            "{:?}",
            Action::LoadFailed(crate::api::ApiError::Server(502))
        );
        assert_eq!(failed, "LoadFailed(Server(502))");

        let history = Action::HistoryLoaded {
            room: RoomId("r-general".to_string()),
            messages: vec![Message {
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
