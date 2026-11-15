//! The one `App` state of the Elm/TEA architecture. It is changed only by `update::update`.
//!
//! `App` holds no data of its own at construction: rooms, folders and messages arrive as
//! `Action`s from a data source (the D-13 seam).

pub mod command;
pub mod sidebar;
pub mod update;

use crate::data::{Folder, Message, Room, RoomId};
use crate::input::keymap::{Keymap, default_keymap};
use crate::input::matcher::Matcher;
use ratatui::layout::Size;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

impl Mode {
    /// The mode badge text shown in the status bar.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Messages,
    Input,
}

/// A one-line message for the status bar. No `Debug`: an error can echo the typed command line.
pub enum StatusMsg {
    Info(String),
    Error(String),
}

/// Upper bound on the command line length, in characters.
pub const MAX_CMDLINE: usize = 256;

/// The whole application state. No `Debug`: it holds typed text and message bodies.
pub struct App {
    pub mode: Mode,
    pub focus: Focus,
    pub prev_focus: Focus,
    pub size: Size,
    /// Seconds since the Unix epoch, as last reported by `Action::Clock`.
    pub clock_secs: i64,
    pub rooms: Vec<Room>,
    pub folders: Vec<Folder>,
    pub messages: HashMap<RoomId, Vec<Message>>,
    pub current_room: Option<RoomId>,
    pub sidebar: sidebar::SidebarState,
    pub cmdline: String,
    pub status_msg: Option<StatusMsg>,
    pub keymap: Keymap,
    /// Pending count and key sequence between keys (D-05, D-06).
    pub matcher: Matcher,
    pub should_quit: bool,
}

impl App {
    /// An empty app in Normal mode with the sidebar focused and the built-in keymap.
    pub fn new(size: Size) -> Self {
        App {
            mode: Mode::Normal,
            focus: Focus::Sidebar,
            prev_focus: Focus::Sidebar,
            size,
            clock_secs: 0,
            rooms: Vec::new(),
            folders: Vec::new(),
            messages: HashMap::new(),
            current_room: None,
            sidebar: sidebar::SidebarState::default(),
            cmdline: String::new(),
            status_msg: None,
            keymap: default_keymap(),
            matcher: Matcher::default(),
            should_quit: false,
        }
    }

    /// `App::new` followed by every Action of the fake data source, through the reducer.
    pub fn with_fake_data(size: Size) -> Self {
        let mut app = App::new(size);
        for action in crate::data::fake::seed() {
            update::update(&mut app, action);
        }
        app
    }

    /// Rows the input bar occupies (plan 01-06 derives this from the input buffer).
    pub fn input_rows(&self) -> u16 {
        1
    }

    /// Open a room: it becomes the current room and the messages pane takes focus (D-02).
    pub fn open_room(&mut self, id: RoomId) {
        self.current_room = Some(id);
        self.focus = Focus::Messages;
    }

    /// True while a count or the start of a key sequence is waiting for more keys.
    pub fn keys_pending(&self) -> bool {
        self.matcher.is_pending()
    }

    /// The pending count and keys in Vim notation, empty when nothing is pending.
    pub fn pending_display(&self) -> String {
        self.matcher.display()
    }

    /// Display name of the open room, if one is open.
    pub fn current_room_name(&self) -> Option<&str> {
        let id = self.current_room.as_ref()?;
        self.rooms
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.name.as_str())
    }
}
