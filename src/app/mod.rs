//! The one `App` state of the Elm/TEA architecture. It is changed only by `update::update`.
//!
//! `App` holds no data of its own at construction: rooms, folders and messages arrive as
//! `Action`s from a data source (the D-13 seam).

pub mod command;
pub mod context;
pub mod edit;
pub mod emoji;
pub mod help;
pub mod history;
pub mod input_buf;
pub mod jump;
pub mod messages;
pub mod react;
pub mod reactions;
pub mod read;
pub mod realtime;
pub mod results;
pub mod search;
pub mod send;
pub mod sidebar;
pub mod switcher;
pub mod thread;
pub mod update;

use crate::data::{Folder, Message, Room, RoomId, SendState};
use crate::input::keymap::Keymap;
use crate::input::matcher::Matcher;
use ratatui::layout::Size;
use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

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
    /// The thread pane, open only while `App.thread` is set.
    Thread,
}

/// Where the room load stands, shown in the status bar (D-05, D-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnState {
    /// Nothing requested: tests and an app that never connects.
    Idle,
    Connecting,
    Ready,
    Failed(crate::api::ApiError),
}

/// The realtime connection, shown in the status line. `Off` until the first connection is made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RtState {
    /// No realtime connection yet (or none wanted): nothing is shown.
    #[default]
    Off,
    Live,
    /// The connection dropped and the task is retrying.
    Reconnecting,
    /// The server rejected the login: no retry until `:reconnect`.
    Unavailable,
}

/// A one-line message for the status bar. No `Debug`: an error can echo the typed command line.
pub enum StatusMsg {
    Info(String),
    Error(String),
}

/// Upper bound on the command line length, in characters.
pub const MAX_CMDLINE: usize = 256;

/// The author of a pending row while the own username is unknown (D-17): `App.own_username` is
/// `None` until the username arrives from `GET /api/v1/me` (plan 04-06), and a message composed in
/// that time shows this name. Once the server answers, the server's copy carries the real author.
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
    /// Per-room history state (D-13): a room with no entry has never been fetched.
    pub history: HashMap<RoomId, history::HistoryState>,
    pub current_room: Option<RoomId>,
    /// Per-room message cursor and scroll position; a room with no entry shows its newest message.
    pub msg_views: HashMap<RoomId, messages::MsgView>,
    /// The runtime message layout (D-10). Set from `message_layout` in config.toml (D-12); kept as
    /// its own field so the reducer and the renderer read the layout in one place.
    pub message_layout: crate::ui::wrap::MessageLayout,
    /// The values of config.toml (D-12). `history_size` is the page size of a history request.
    pub settings: crate::config::Settings,
    /// The zone message times are shown in. UTC unless the binary switches it to the system local
    /// zone, so tests and snapshots never depend on the machine; the reducer reads no clock.
    pub zone: crate::ui::timefmt::Zone,
    pub sidebar: sidebar::SidebarState,
    pub cmdline: String,
    /// The multiline draft of the input bar (D-14).
    pub input: input_buf::InputBuffer,
    /// The unsent drafts of the rooms that are not open, one per room (D-10). In memory only and
    /// never written to disk; a draft leaves here only through `open_room` (back into `input`) and
    /// is sent only by Enter in its own room. An empty draft is not stored. `InputBuffer`'s `Debug`
    /// redacts the text.
    pub drafts: HashMap<RoomId, input_buf::InputBuffer>,
    /// The client id of a send whose row was confirmed `Sent` by an incoming server copy (a
    /// realtime echo or a history page, D-21) while its POST had not answered yet. The entry keeps
    /// that room's single in-flight slot occupied until the request's own `MessageSent` or
    /// `SendFailed` arrives (D-03). Consulted by `send::pump`, set only by `messages::confirm_held`
    /// and released only by `send::on_sent` and `send::on_failed`.
    pub awaiting_answer: HashMap<RoomId, String>,
    /// Counter behind the ids of locally appended messages.
    pub local_seq: u64,
    /// The prefix of every client message id: ids are `id_prefix` + `local_seq` (built by
    /// `send::submit`, plan 04-02). `with_config` sets `local-`, so test ids are `local-1`,
    /// `local-2`; `run()` replaces it once with a random session prefix, so a restarted client
    /// never reuses an id the server already stored (D-07).
    pub id_prefix: String,
    /// The user's own username (D-17), set from `GET /api/v1/me` by plan 04-06 and kept in memory
    /// only. While it is `None` a pending row falls back to `LOCAL_AUTHOR`.
    pub own_username: Option<String>,
    /// The wait before the open room is marked read (`app::read`).
    pub read: read::ReadState,
    pub status_msg: Option<StatusMsg>,
    /// The message being edited through the input bar, if any (`app::edit`). While it is set the
    /// input holds the edit text and the room's draft is stashed inside it.
    pub editing: Option<edit::Editing>,
    /// A question waiting for one key (the delete prompt). Checked at the top of `Action::Key`,
    /// drawn in the status bar, and never cleared by the status line clearing.
    pub confirm: Option<edit::Confirm>,
    /// The help overlay, when open: its scroll offset in lines. Not a mode (the badge keeps
    /// showing the real one); the reducer checks it before the keymap, like `confirm`.
    pub help: Option<usize>,
    /// The context popup (`app::context`), when open. Like `help` it is a state, not a mode; its
    /// content is recomputed from the app on every draw.
    pub context: bool,
    /// The reactions popup (`app::reactions`), when open: the message it was opened on. Like
    /// `context` it is a state, not a mode; its content is recomputed from the app on every draw.
    pub reactions_popup: Option<reactions::ReactionsPopup>,
    /// The server's custom emojis (`app::emoji`): requested when the rooms have loaded and kept for
    /// the session. Read by the reaction picker, message text and the reaction row.
    pub custom_emojis: emoji::CustomEmojis,
    /// Whether the terminal can show images (decided once at startup).
    pub graphics: emoji::Graphics,
    /// Milliseconds since the program started, set by the run loop before each draw: it picks the
    /// frame of an animated emoji. Nothing else reads it.
    pub anim_ms: u64,
    /// The custom emoji images of the session: fetched, failed and in flight (`app::emoji`).
    pub images: emoji::Images,
    /// The reaction picker (`app::react`), when open. Like `context` it is a state, not a mode.
    pub react_picker: Option<react::ReactPicker>,
    /// The room switcher (`app::switcher`), when open. Like `help` it is a state, not a mode. No
    /// `Debug` of its own beyond a redacting one: it holds the typed query.
    pub switcher: Option<switcher::Switcher>,
    /// The open thread of the open room (`app::thread`), if any. No `Debug`: it holds messages.
    pub thread: Option<thread::ThreadPane>,
    /// The rooms opened this session, most recently opened first (`open_room`). In memory only:
    /// never written to disk. The switcher orders an empty search by it.
    pub recent_rooms: Vec<RoomId>,
    /// The active find in the open room (`app::search`), if any. No `Debug`: it holds the query.
    pub search: Option<search::Search>,
    /// The text of the open search prompt (`/`), when open. Like `help` it is a state, not a mode.
    pub search_prompt: Option<String>,
    /// The server search results list (`app::results`), when open. A state, not a mode.
    pub results: Option<results::ResultsState>,
    /// The last context request's id (`app::jump`).
    pub context_seq: u64,
    /// Rooms showing a window of older history after a jump from a search result (`app::jump`).
    pub detached: std::collections::HashMap<RoomId, jump::Detached>,
    /// Counter behind the ids of server searches; an answer carrying an older id is dropped.
    pub search_seq: u64,
    /// The first page of a server search in flight: its id and query text.
    pub pending_search: Option<(u64, String)>,
    pub keymap: Keymap,
    /// The styles, border types and markers of theme.toml (D-08, D-09, D-11).
    pub theme: crate::config::Theme,
    /// Pending count and key sequence between keys (D-05, D-06).
    pub matcher: Matcher,
    pub should_quit: bool,
    /// Set when `:q` warned about undelivered messages (D-20): the next `:q` then quits.
    pub quit_warned: bool,
    /// Fetch intents the reducer records; only `run()` drains them and spawns the requests.
    pub effects: Vec<crate::effect::Effect>,
    /// The state of the room load, for the status bar.
    pub conn: ConnState,
    /// The realtime connection, for the status bar.
    pub rt: RtState,
    /// Set by `:reconnect`; only `run()` reads and clears it, and then restarts the realtime task.
    pub restart_realtime: bool,
    /// Loaded rooms that may have missed messages during an outage: reopening one refetches its
    /// newest page and merges it by id (see `app::realtime`).
    pub stale: std::collections::HashSet<RoomId>,
    /// Runtime warnings from the config (D-04), drawn on the status row on every frame. Unlike
    /// `status_msg` they are never cleared by a key press (Pattern 6). Empty for `App::new`.
    pub warnings: Vec<String>,
}

impl App {
    /// An empty app in Normal mode with the sidebar focused and the built-in defaults.
    pub fn new(size: Size) -> Self {
        App::with_config(size, crate::config::Config::default())
    }

    /// An empty app in Normal mode with the sidebar focused, configured from the config files.
    ///
    /// `config.connection` is ignored on purpose: the token goes into `api::Client` and never
    /// enters `App`.
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
            history: HashMap::new(),
            current_room: None,
            read: read::ReadState::default(),
            msg_views: HashMap::new(),
            message_layout: config.settings.message_layout,
            settings: config.settings,
            zone: crate::ui::timefmt::Zone::Utc,
            sidebar: sidebar::SidebarState::default(),
            cmdline: String::new(),
            input: input_buf::InputBuffer::new(),
            drafts: HashMap::new(),
            awaiting_answer: HashMap::new(),
            local_seq: 0,
            id_prefix: "local-".to_string(),
            own_username: None,
            status_msg: None,
            editing: None,
            confirm: None,
            help: None,
            context: false,
            reactions_popup: None,
            custom_emojis: emoji::CustomEmojis::default(),
            graphics: emoji::Graphics::default(),
            images: emoji::Images::default(),
            anim_ms: 0,
            react_picker: None,
            switcher: None,
            thread: None,
            recent_rooms: Vec::new(),
            search: None,
            search_prompt: None,
            results: None,
            context_seq: 0,
            detached: std::collections::HashMap::new(),
            search_seq: 0,
            pending_search: None,
            keymap: config.keymap,
            theme: config.theme,
            matcher: Matcher::default(),
            should_quit: false,
            quit_warned: false,
            effects: Vec::new(),
            conn: ConnState::Idle,
            rt: RtState::Off,
            restart_realtime: false,
            stale: std::collections::HashSet::new(),
            warnings: config.warnings,
        }
    }

    /// `App::new` followed by every Action of the fake data source, through the reducer.
    pub fn with_fake_data(size: Size) -> Self {
        let mut app = App::new(size);
        // The fake source has no server, so no custom emojis to ask for.
        app.custom_emojis = emoji::CustomEmojis::Loaded(Vec::new());
        for action in crate::data::fake::seed() {
            update::update(&mut app, action);
        }
        app
    }

    /// The five screen regions for the current size, sidebar width and input height. The renderer
    /// and every reducer computation take their geometry from here, so they cannot disagree.
    pub fn areas(&self) -> crate::ui::layout::Areas {
        let area = ratatui::layout::Rect::new(0, 0, self.size.width, self.size.height);
        crate::ui::layout::areas_with_thread(
            area,
            self.settings.sidebar_width,
            self.input_rows(),
            self.thread.is_some(),
        )
    }

    /// What the drawing of custom emojis reads: the list, the images and the terminal's support.
    pub fn emoji_view(&self) -> emoji::EmojiView<'_> {
        emoji::EmojiView {
            custom: &self.custom_emojis,
            images: &self.images,
            graphics: self.graphics,
            now_ms: self.anim_ms,
        }
    }

    /// The row context for `msgs`: layout, author and time columns, time format, zone and theme. It is
    /// the only place a `RowCtx` is built for the app, so the renderer and the reducer's row model
    /// always measure the same rows.
    pub fn row_ctx(&self, msgs: &[Message]) -> crate::ui::wrap::RowCtx<'_> {
        crate::ui::wrap::RowCtx {
            layout: self.message_layout,
            author_col: crate::ui::wrap::author_column(msgs),
            time_col: crate::ui::wrap::time_column(
                msgs,
                &self.settings.timestamp_format,
                self.zone,
            )
            .max(self.unsent_marker_column(msgs)),
            time_format: &self.settings.timestamp_format,
            zone: self.zone,
            theme: &self.theme,
            search: self.search.as_ref().map(|s| s.query.as_str()),
            aliases: &self.settings.emoji_aliases,
            emoji: self.emoji_view(),
            own_user: self.own_username.as_deref(),
            unread_before: None,
        }
    }

    /// `row_ctx` for the messages pane of `room`: the same, plus the message the `unread` divider
    /// sits above. Every list that is the open room's own (the pane and its row model) builds its
    /// context here, so the renderer and the row model agree on the divider's row.
    pub fn pane_row_ctx(&self, room: &RoomId, msgs: &[Message]) -> crate::ui::wrap::RowCtx<'_> {
        let mut ctx = self.row_ctx(msgs);
        ctx.unread_before = read::divider_before(self, room);
        ctx
    }

    /// Display width of the widest marker an unsent message of `msgs` shows where its time would
    /// be. `row_ctx` raises the room's time column to it, so a marker wider than the formatted time
    /// never moves a body (D-01).
    fn unsent_marker_column(&self, msgs: &[Message]) -> usize {
        let markers = &self.theme.markers;
        msgs.iter()
            .filter(|m| m.send.is_unsent())
            .map(|m| match m.send {
                SendState::Queued { .. } | SendState::Sending => markers.sending.width(),
                SendState::Failed => markers.failed.width(),
                SendState::Unconfirmed => markers.unconfirmed.width(),
                SendState::Sent => 0,
            })
            .max()
            .unwrap_or(0)
    }

    /// Rows the input bar occupies: the draft's visual rows, from 1 up to `MAX_INPUT_ROWS`.
    pub fn input_rows(&self) -> u16 {
        let area = ratatui::layout::Rect::new(0, 0, self.size.width, self.size.height);
        // The input pane's width does not depend on its own height.
        let width = crate::ui::layout::areas_with_thread(
            area,
            self.settings.sidebar_width,
            1,
            self.thread.is_some(),
        )
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

    /// Open a room: it becomes the current room and the messages pane takes focus (D-02). This is
    /// the single place a room becomes current, and so the single place drafts move (D-10).
    ///
    /// Opening the room that is already current only focuses the messages pane. Opening another
    /// room stores the live draft under the old room (unless it is empty) and shows the new room's
    /// own draft, empty when it has none. With no room open yet, a non-empty live draft stays in
    /// the input bar so text typed before the first room is not lost. The buffer is re-settled to
    /// the input viewport, so the input bar height follows the restored draft. A switch never sends
    /// anything and never touches disk.
    pub fn open_room(&mut self, id: RoomId) {
        if self.current_room.as_ref() != Some(&id) {
            // An edit in progress ends first, so the draft that moves with the room is the one the
            // user had before the edit, never the edit text.
            edit::cancel(self);
            self.confirm = None;
            search::clear(self);
            results::close_all(self);
            thread::discard(self);
            if let Some(old) = self.current_room.clone() {
                jump::reset(self, &old);
            }
            match self.current_room.clone() {
                Some(old) => {
                    let live = std::mem::take(&mut self.input);
                    if !live.is_empty() {
                        self.drafts.insert(old, live);
                    }
                    self.input = self.drafts.remove(&id).unwrap_or_default();
                }
                None => {
                    if self.input.is_empty() {
                        self.input = self.drafts.remove(&id).unwrap_or_default();
                    }
                }
            }
            self.recent_rooms.retain(|recent| *recent != id);
            self.recent_rooms.insert(0, id.clone());
            self.current_room = Some(id);
            read::on_open(self);
            emoji::ensure_loaded(self);
            let (width, height) = self.input_viewport();
            self.input.settle(width, height);
        }
        self.focus = Focus::Messages;
    }

    /// How long a count or key sequence may wait for its next key: config.toml `key_timeout_ms`
    /// (D-12; Phase 1 D-06 fixed it at one second).
    pub fn key_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.settings.key_timeout_ms)
    }

    /// How long a room stays open before it is marked read: config.toml `mark_read_delay_ms`.
    pub fn mark_read_delay(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.settings.mark_read_delay_ms)
    }

    /// True while a count or the start of a key sequence is waiting for more keys.
    pub fn keys_pending(&self) -> bool {
        self.matcher.is_pending()
    }

    /// The pending count and keys in Vim notation, empty when nothing is pending.
    pub fn pending_display(&self) -> String {
        self.matcher.display()
    }

    /// The number of messages, over every room, whose send state is not `Sent`: Queued, Sending,
    /// Failed or Unconfirmed (D-20). Such messages live in memory only.
    pub fn undelivered_count(&self) -> usize {
        self.messages
            .values()
            .flatten()
            .filter(|m| m.send.is_unsent())
            .count()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{app_with_config, config_from_toml};
    use crate::ui::timefmt::Zone;
    use std::time::Duration;

    #[test]
    fn key_timeout_from_settings() {
        assert_eq!(
            App::new(Size::new(80, 24)).key_timeout(),
            Duration::from_millis(1000)
        );
        let app = app_with_config(config_from_toml("key_timeout_ms = 250\n"), 80, 24);
        assert_eq!(app.key_timeout(), Duration::from_millis(250));
    }

    /// The status row (second from the bottom) of an 80x24 render.
    fn status_row(app: &App) -> String {
        let screen = crate::test_util::render_to_string(app, 80, 24);
        screen.lines().nth(22).expect("status row").to_string()
    }

    #[test]
    fn conn_state() {
        use crate::action::Action;
        use crate::api::ApiError;
        use crate::app::update::update;
        let mut app = App::new(Size::new(80, 24));
        assert_eq!(app.conn, ConnState::Idle);
        assert!(app.effects.is_empty());
        let idle = status_row(&app);
        assert!(!idle.contains("Connecting"), "{idle}");
        assert!(!idle.contains("unauthorized"), "{idle}");

        app.conn = ConnState::Connecting;
        let connecting = status_row(&app);
        assert!(connecting.contains("Connecting…"), "{connecting}");

        update(&mut app, Action::LoadFailed(ApiError::Unauthorized));
        assert_eq!(app.conn, ConnState::Failed(ApiError::Unauthorized));
        let failed = status_row(&app);
        assert!(failed.contains("unauthorized"), "{failed}");
        assert!(!failed.contains("Connecting"), "{failed}");

        update(&mut app, Action::RoomsLoaded(Vec::new()));
        assert_eq!(app.conn, ConnState::Ready);
        let ready = status_row(&app);
        assert!(!ready.contains("unauthorized"), "{ready}");
        assert!(!ready.contains("Connecting"), "{ready}");
    }

    #[test]
    fn send_fields_default() {
        let app = App::new(Size::new(80, 24));
        assert_eq!(app.id_prefix, "local-");
        assert_eq!(app.own_username, None);
        // Test ids are built from the prefix and the counter: local-1, local-2.
        assert_eq!(format!("{}{}", app.id_prefix, 1), "local-1");
        assert_eq!(format!("{}{}", app.id_prefix, 2), "local-2");
    }

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    /// Open `name` the way a user does: focus the sidebar, put its cursor on the room, press Enter.
    fn open_via_sidebar(app: &mut App, name: &str) {
        app.focus = Focus::Sidebar;
        let index = sidebar::rows(app)
            .iter()
            .position(|r| matches!(r, sidebar::SidebarRow::Room { id, .. } if id.0 == name))
            .expect("room row is listed");
        app.sidebar.cursor = index;
        crate::test_util::press(app, "<CR>");
        assert_eq!(app.current_room, Some(rid(name)));
    }

    /// The `SendMessage` effects recorded so far, as (room id, message id).
    fn send_effects(app: &App) -> Vec<(String, String)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                crate::effect::Effect::SendMessage { room, id, .. } => {
                    Some((room.0.clone(), id.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn drafts_switch_and_restore() {
        use crate::test_util::{press, render_to_string};
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        press(&mut app, "ifor general<Esc>");
        open_via_sidebar(&mut app, "random");
        assert!(app.input.is_empty(), "random has no draft yet");
        press(&mut app, "ifor random<Esc>");

        open_via_sidebar(&mut app, "general");
        assert_eq!(app.input.text(), "for general");
        assert!(render_to_string(&app, 80, 24).contains("for general"));
        open_via_sidebar(&mut app, "random");
        assert_eq!(app.input.text(), "for random");
        assert!(render_to_string(&app, 80, 24).contains("for random"));
    }

    #[test]
    fn drafts_same_room_keeps() {
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        crate::test_util::press(&mut app, "ikeep me<Esc>");
        open_via_sidebar(&mut app, "general");
        assert_eq!(app.input.text(), "keep me");
        assert!(
            app.drafts.is_empty(),
            "the open room's draft is the live buffer"
        );
    }

    #[test]
    fn drafts_empty_not_stored() {
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        open_via_sidebar(&mut app, "random");
        open_via_sidebar(&mut app, "general");
        assert!(app.drafts.is_empty());
    }

    #[test]
    fn drafts_before_any_room() {
        let mut app = crate::test_util::app_80x24();
        crate::test_util::press(&mut app, "ihello<Esc>");
        assert_eq!(app.current_room, None);
        open_via_sidebar(&mut app, "general");
        assert_eq!(app.input.text(), "hello");
        assert!(app.drafts.is_empty());
    }

    #[test]
    fn drafts_send_goes_to_own_room() {
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        crate::test_util::press(&mut app, "iin general<Esc>");
        open_via_sidebar(&mut app, "random");
        assert!(app.input.is_empty(), "random shows its own (empty) draft");
        open_via_sidebar(&mut app, "general");
        let random_before = app.messages[&rid("random")].len();
        let general_before = app.messages[&rid("general")].len();
        crate::test_util::press(&mut app, "i<CR>");
        assert_eq!(app.messages[&rid("random")].len(), random_before);
        let held = &app.messages[&rid("general")];
        assert_eq!(held.len(), general_before + 1);
        assert_eq!(held[general_before].body, "in general");
        assert_eq!(held[general_before].send, SendState::Sending);
        assert_eq!(
            send_effects(&app),
            vec![("general".to_string(), "local-1".to_string())]
        );
        assert!(app.input.is_empty());
    }

    #[test]
    fn drafts_resettle_height() {
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        crate::test_util::press(&mut app, "ia<A-CR>b<A-CR>c<Esc>");
        assert_eq!(app.input_rows(), 3);
        open_via_sidebar(&mut app, "random");
        assert_eq!(app.input_rows(), 1);
        open_via_sidebar(&mut app, "general");
        assert_eq!(app.input_rows(), 3);
        let (w, h) = app.input_viewport();
        assert_eq!(h, 3);
        assert!(w > 0);
        open_via_sidebar(&mut app, "random");
        assert_eq!(app.input_rows(), 1);
    }

    #[test]
    fn drafts_stay_in_memory_and_redacted() {
        // T-04-15: the draft map is plain App state, and its Debug never shows the text.
        let mut app = crate::test_util::app_80x24();
        open_via_sidebar(&mut app, "general");
        crate::test_util::press(&mut app, "isecret draft<Esc>");
        open_via_sidebar(&mut app, "random");
        assert_eq!(app.drafts.len(), 1);
        let shown = format!("{:?}", app.drafts[&rid("general")]);
        assert!(!shown.contains("secret"), "{shown}");
    }

    #[test]
    fn undelivered_count_states() {
        let mut app = crate::test_util::app_80x24();
        assert_eq!(app.undelivered_count(), 0, "server messages never count");
        let states = [
            ("general", SendState::Queued { reconcile: false }),
            ("random", SendState::Sending),
            ("general", SendState::Failed),
            ("general", SendState::Unconfirmed),
            ("general", SendState::Sent),
            ("random", SendState::Sent),
        ];
        for (n, (room, send)) in states.into_iter().enumerate() {
            app.messages
                .get_mut(&rid(room))
                .expect("room")
                .push(Message {
                    edited: false,
                    plain_text: true,
                    tmid: None,
                    tshow: false,
                    tcount: 0,
                    tlm: None,
                    reactions: Vec::new(),
                    id: format!("u-{n}"),
                    room_id: rid(room),
                    author: "me".to_string(),
                    ts: 0,
                    body: "text".to_string(),
                    kind: crate::data::MessageKind::Normal,
                    send,
                });
        }
        assert_eq!(app.undelivered_count(), 4);
    }

    #[test]
    fn with_config_defaults_to_utc() {
        assert_eq!(App::new(Size::new(80, 24)).zone, Zone::Utc);
        let app = app_with_config(config_from_toml("sidebar_width = 30\n"), 80, 24);
        assert_eq!(app.zone, Zone::Utc);
    }
}
