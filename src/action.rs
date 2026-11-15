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
    EnterCommand,
    Cancel,
    InsertChar(char),
    DeleteBack,
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
}

/// Redacting `Debug`: never prints typed characters, pasted text or message text. The match is
/// exhaustive on purpose, so every variant a later plan adds must choose how it prints.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::MoveDown => write!(f, "MoveDown"),
            Action::MoveUp => write!(f, "MoveUp"),
            Action::EnterCommand => write!(f, "EnterCommand"),
            Action::Cancel => write!(f, "Cancel"),
            Action::InsertChar(_) => write!(f, "InsertChar(..)"),
            Action::DeleteBack => write!(f, "DeleteBack"),
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
