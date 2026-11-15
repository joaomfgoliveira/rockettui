//! The single `Action` enum of the Elm/TEA architecture.
//!
//! Unit variants are keymap-bindable. `Key`, `Resize`, `Clock`, `Paste` and the `*Loaded`
//! variants are event Actions that no keymap row may bind.
//!
//! `Action` deliberately has no `Debug` yet (it carries typed, pasted and message text).

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
