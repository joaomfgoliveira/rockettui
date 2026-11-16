//! The one `App` state of the Elm/TEA architecture. It is changed only by `update::update`.
//!
//! `App` holds no data of its own at construction: rooms, folders and messages arrive as
//! `Action`s from a data source (the D-13 seam).

pub mod command;
pub mod input_buf;
pub mod messages;
pub mod sidebar;
pub mod update;

use crate::data::{Folder, Message, Room, RoomId};
use crate::input::keymap::Keymap;
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

/// Author of every locally composed message until Phase 4 sends them to the server (T-01-23).
pub const LOCAL_AUTHOR: &str = "me";

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
    /// Per-room message cursor and scroll position; a room with no entry shows its newest message.
    pub msg_views: HashMap<RoomId, messages::MsgView>,
    /// The runtime message layout (D-10). Set from `message_layout` in config.toml (D-12); kept as
    /// its own field so the reducer and the renderer read the layout in one place.
    pub message_layout: crate::ui::wrap::MessageLayout,
    /// The values of config.toml (D-12). `history_size` is stored for Phase 3 and read by nothing yet.
    pub settings: crate::config::Settings,
    pub sidebar: sidebar::SidebarState,
    pub cmdline: String,
    /// The multiline draft of the input bar (D-14).
    pub input: input_buf::InputBuffer,
    /// Counter behind the ids of locally appended messages.
    pub local_seq: u64,
    pub status_msg: Option<StatusMsg>,
    pub keymap: Keymap,
    /// Pending count and key sequence between keys (D-05, D-06).
    pub matcher: Matcher,
    pub should_quit: bool,
}

impl App {
    /// An empty app in Normal mode with the sidebar focused and the built-in defaults.
    pub fn new(size: Size) -> Self {
        App::with_config(size, crate::config::Config::default())
    }

    /// An empty app in Normal mode with the sidebar focused, configured from the config files.
    pub fn with_config(size: Size, config: crate::config::Config) -> Self {
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
            msg_views: HashMap::new(),
            message_layout: config.settings.message_layout,
            settings: config.settings,
            sidebar: sidebar::SidebarState::default(),
            cmdline: String::new(),
            input: input_buf::InputBuffer::new(),
            local_seq: 0,
            status_msg: None,
            keymap: config.keymap,
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

    /// The five screen regions for the current size, sidebar width and input height. The renderer
    /// and every reducer computation take their geometry from here, so they cannot disagree.
    pub fn areas(&self) -> crate::ui::layout::Areas {
        let area = ratatui::layout::Rect::new(0, 0, self.size.width, self.size.height);
        crate::ui::layout::areas(area, self.settings.sidebar_width, self.input_rows())
    }

    /// Rows the input bar occupies: the draft's visual rows, from 1 up to `MAX_INPUT_ROWS`.
    pub fn input_rows(&self) -> u16 {
        let area = ratatui::layout::Rect::new(0, 0, self.size.width, self.size.height);
        // The input pane's width does not depend on its own height.
        let width = crate::ui::layout::areas(area, self.settings.sidebar_width, 1)
            .input
            .width
            .saturating_sub(2);
        let rows = self.input.visual_row_count(width);
        u16::try_from(rows)
            .unwrap_or(u16::MAX)
            .clamp(1, input_buf::MAX_INPUT_ROWS)
    }

    /// Inner (width, height) of the input pane, borders removed; shared by reducer and renderer.
    pub fn input_viewport(&self) -> (u16, u16) {
        let pane = self.areas().input;
        (pane.width.saturating_sub(2), pane.height.saturating_sub(2))
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
