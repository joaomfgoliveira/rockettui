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
    /// Enter in Insert mode: send the draft (Phase 1 appends it locally, D-15).
    SubmitInput,
    ExecuteCommand,
    Quit,
    /// A raw key event, resolved through the keymap by the reducer.
    Key(KeyEvent),
    Resize(u16, u16),
    /// Seconds since the Unix epoch, produced by the run loop (the reducer never reads a clock).
    Clock(i64),
    Paste(String),
    RoomsLoaded(Vec<Room>),
    FoldersLoaded(Vec<Folder>),
    MessagesLoaded(RoomId, Vec<Message>),
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
            Action::KeyTimeout => write!(f, "KeyTimeout"),
            Action::Counted(n, inner) => write!(f, "Counted({n}, {inner:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        };
        let loaded = format!(
            "{:?}",
            Action::MessagesLoaded(RoomId("general".to_string()), vec![message])
        );
        assert!(!loaded.contains("SENTINEL-BODY-7f3a"));
        assert!(loaded.contains("general"));
        assert!(loaded.contains('1'));

        assert!(format!("{:?}", Action::Clock(42)).contains("42"));
    }
}
