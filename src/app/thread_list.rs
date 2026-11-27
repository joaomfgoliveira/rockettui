//! The thread list (`list_threads`, `gT`): a pane in the thread column listing the threads of the
//! open room, to find one and open it in that same column.
//!
//! It is state on `App`, not a mode, and a pane like the thread pane (`Focus::ThreadList`): its
//! motions and `select` come through the keymap, `<Tab>`, `<S-Tab>`, `Esc` and `q` are fixed keys
//! it handles first, and `/` opens a search line inside it. While a thread opened from the list is
//! shown the state is kept, so closing the thread shows the list as it was. Unlike the switcher
//! its rows come from the server a page at a time, because a room
//! has an unbounded number of threads, and the filter (all, followed, unread) and the search text
//! are server parameters. Every request carries a `generation` that changes with the filter and
//! the text, so a late answer for an older query is dropped. Typing waits `SEARCH_DELAY` after
//! the last key before the request, with a timer the event loop owns, like the mark-read wait.
//! Pure: no I/O and no `Frame`. Neither the typed text nor a row's text is ever printed.

use crate::action::Action;
use crate::api::{ApiError, ThreadFilter, ThreadListPage, ThreadRow};
use crate::app::{App, Focus, MAX_CMDLINE, Mode, StatusMsg, thread};
use crate::data::{Message, RoomId};
use crate::effect::{Effect, SendText};
use crate::input::key::Chord;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};
use std::time::Duration;

/// How many threads one request asks for.
pub const PAGE: u16 = 50;

/// The wait after the last typed key before the search request.
pub const SEARCH_DELAY: Duration = Duration::from_millis(250);

/// The next page loads when the selection is this close to the end of the loaded rows.
const PAGE_AHEAD: usize = 5;

/// Where the first page of the current query stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListLoad {
    /// The request is in flight, or waiting for the typing delay.
    Loading,
    Loaded,
    /// The request failed; only the class of the failure is kept.
    Failed(ApiError),
}

/// The open thread list. No derived `Debug`: it holds the typed text and the rows' text.
pub struct ThreadList {
    /// The room whose threads are listed: the open room when the list was opened.
    pub room: RoomId,
    pub filter: ThreadFilter,
    pub query: String,
    pub rows: Vec<ThreadRow>,
    pub selected: usize,
    pub load: ListLoad,
    /// How many threads the server says match the filter and the text.
    pub total: u32,
    /// Identifies the query the rows belong to. Taken from `App::thread_list_seq`, so it is never
    /// reused, not even by a list opened later.
    pub generation: u64,
    /// The typing delay is running: the request for `generation` has not been sent.
    pub search_pending: bool,
    /// A further page is in flight.
    pub loading_more: bool,
    /// A further page failed; the loaded rows stay. Cleared by the next selection move.
    pub more_failed: Option<ApiError>,
    /// The id of the thread being opened while its parent is fetched (it is not in the main flow).
    pub choosing: Option<String>,
    /// The search line is open: typed keys are text. A state of the pane, not a mode.
    pub searching: bool,
    /// The pane that had focus when the list was opened: closing the list returns there.
    pub opened_from: Focus,
}

/// Redacting `Debug`: counts only, never the typed text or the rows.
impl std::fmt::Debug for ThreadList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadList")
            .field("room", &self.room)
            .field("filter", &self.filter)
            .field("query_len", &self.query.chars().count())
            .field("rows", &self.rows.len())
            .field("selected", &self.selected)
            .field("load", &self.load)
            .finish()
    }
}

impl ThreadList {
    /// More threads exist than are loaded.
    pub fn has_more(&self) -> bool {
        self.rows.len() < self.total as usize
    }
}

/// `list_threads`: show the thread list of the open room in the thread column, focused, and ask for
/// its first page. With no room open, or an encrypted one, nothing opens and the status line says
/// why. A list already open for the room is shown again as it is (a thread opened from it closes,
/// its unsent replies kept); any other thread is closed and a new list starts. The mode does not
/// change.
pub fn open(app: &mut App) {
    let Some(room) = app.current_room.clone() else {
        app.status_msg = Some(StatusMsg::Info("No room is open".to_string()));
        return;
    };
    if app.rooms.iter().any(|r| r.id == room && r.encrypted) {
        app.status_msg = Some(StatusMsg::Info(
            "Encrypted rooms have no thread list".to_string(),
        ));
        return;
    }
    app.matcher.reset();
    if app.thread_list.as_ref().is_some_and(|l| l.room == room) {
        if app.thread.as_ref().is_some_and(|p| p.from_list) {
            thread::close(app);
        }
        if app.thread.is_none() {
            app.focus = Focus::ThreadList;
            return;
        }
    }
    let opened_from = match app.focus {
        Focus::Thread | Focus::ThreadList | Focus::Input => Focus::Messages,
        other => other,
    };
    thread::discard(app);
    app.thread_list_seq += 1;
    app.thread_list = Some(ThreadList {
        room,
        filter: ThreadFilter::All,
        query: String::new(),
        rows: Vec::new(),
        selected: 0,
        load: ListLoad::Loading,
        total: 0,
        generation: app.thread_list_seq,
        search_pending: false,
        loading_more: false,
        more_failed: None,
        choosing: None,
        searching: false,
        opened_from,
    });
    app.focus = Focus::ThreadList;
    request_first(app);
}

/// Close the list and give the focus back to the pane it was opened from. The message pane, which
/// kept its cursor and scroll, shows again.
pub fn close(app: &mut App) {
    let Some(list) = app.thread_list.take() else {
        return;
    };
    app.matcher.reset();
    if app.focus == Focus::ThreadList {
        app.focus = list.opened_from;
    }
}

/// Drop the list without touching the focus rules of `close`: the room it belonged to is no
/// longer open.
pub fn discard(app: &mut App) {
    app.thread_list = None;
    if app.focus == Focus::ThreadList {
        app.focus = Focus::Messages;
    }
}

/// Ask for the first page of the current query.
fn request_first(app: &mut App) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    list.search_pending = false;
    list.load = ListLoad::Loading;
    app.effects.push(effect(list, 0));
}

fn effect(list: &ThreadList, offset: u32) -> Effect {
    Effect::ListThreads {
        room: list.room.clone(),
        filter: list.filter,
        query: SendText::new(list.query.clone()),
        generation: list.generation,
        offset,
        count: PAGE,
    }
}

/// A key with the list focused in Normal mode. Returns true when the list took it: every key while
/// the search line is open, and otherwise `Esc` and `q` (close), unless a key sequence is half typed. Any other key is left to the keymap, whose
/// motions and `select` reach the list through `apply_motion`.
pub fn key(app: &mut App, chord: Chord) -> bool {
    if app.thread_list.as_ref().is_some_and(|l| l.searching) {
        search_key(app, chord);
        return true;
    }
    if app.matcher.is_pending() {
        return false;
    }
    let plain = chord.mods.is_empty();
    if (chord.code == KeyCode::Esc && plain) || (chord.code == KeyCode::Char('q') && plain) {
        close(app);
    } else {
        return false;
    }
    true
}

/// `search_start` (`/`) with the list focused: open the search line.
pub fn start_search(app: &mut App) {
    if app.mode != Mode::Normal || app.focus != Focus::ThreadList {
        return;
    }
    app.matcher.reset();
    if let Some(list) = app.thread_list.as_mut() {
        list.searching = true;
    }
}

/// A key while the search line is open: text. `Enter` keeps it and returns to the list keys, `Esc`
/// and `<C-c>` clear it and return to the list keys, `Backspace` deletes a character and `<C-u>`
/// clears the text.
fn search_key(app: &mut App, chord: Chord) {
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    if chord.code == KeyCode::Enter {
        if let Some(list) = app.thread_list.as_mut() {
            list.searching = false;
            // Do not wait out the typing delay: the user is done typing.
            if list.search_pending {
                request_first(app);
            }
        }
    } else if ctrl('c') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        if let Some(list) = app.thread_list.as_mut() {
            list.searching = false;
            if !list.query.is_empty() {
                list.query.clear();
                restart(app);
                request_first(app);
            }
        }
    } else if ctrl('u') {
        edit(app, String::clear);
    } else if chord.code == KeyCode::Backspace {
        edit(app, |query| {
            query.pop();
        });
    } else if let KeyCode::Char(c) = chord.code
        && chord.mods.is_empty()
        && !c.is_control()
    {
        edit(app, |query| {
            if query.chars().count() < MAX_CMDLINE {
                query.push(c);
            }
        });
    }
}

/// A paste into the search line: one line, capped, control characters removed. Ignored while the
/// line is closed.
pub fn paste(app: &mut App, text: &str) {
    let Some(room_left) = app
        .thread_list
        .as_ref()
        .filter(|l| l.searching)
        .map(|l| MAX_CMDLINE.saturating_sub(l.query.chars().count()))
    else {
        return;
    };
    let text: String = sanitize_line(text).chars().take(room_left).collect();
    edit(app, |query| query.push_str(&text));
}

/// How many rows the pane shows: the column less its borders and the search line, four lines each with the divider.
fn page_rows(app: &App) -> usize {
    app.areas().column().map_or(1, |c| {
        ((usize::from(c.height.saturating_sub(3)) + 1) / 3).max(1)
    })
}

/// A motion with the list focused, as the keymap binds it: `j`/`k` a row, `gg`/`G` the ends (a
/// count picks the row), the half-page keys half the shown rows, and `select` opens the thread.
/// `h` and `l` (`move_left`, `move_right`) pick the previous and the next filter; the fold toggle
/// does nothing here.
pub fn apply_motion(app: &mut App, motion: &Action, count: Option<u32>) {
    match motion {
        Action::MoveLeft => return change_filter(app, ThreadFilter::prev),
        Action::MoveRight => return change_filter(app, ThreadFilter::next),
        _ => {}
    }
    let n = count.map_or(1, |c| (c as usize).max(1));
    let half = (page_rows(app) / 2).max(1).saturating_mul(n);
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    let last = list.rows.len().saturating_sub(1);
    let at = |row: usize| row.min(last);
    list.selected = match motion {
        Action::MoveDown => at(list.selected.saturating_add(n)),
        Action::MoveUp => list.selected.saturating_sub(n),
        Action::HalfPageDown => at(list.selected.saturating_add(half)),
        Action::HalfPageUp => list.selected.saturating_sub(half),
        Action::GotoTop => count.map_or(0, |_| at(n - 1)),
        Action::GotoBottom => count.map_or(last, |_| at(n - 1)),
        Action::Select => {
            choose(app);
            return;
        }
        _ => return,
    };
    list.more_failed = None;
    request_more(app);
}

/// Start a new query: a new generation, the rows of the old one dropped and the selection back on
/// the first row.
fn restart(app: &mut App) {
    app.thread_list_seq += 1;
    let seq = app.thread_list_seq;
    if let Some(list) = app.thread_list.as_mut() {
        list.generation = seq;
        list.rows.clear();
        list.selected = 0;
        list.total = 0;
        list.loading_more = false;
        list.more_failed = None;
        list.choosing = None;
        list.load = ListLoad::Loading;
    }
}

/// A different filter reloads the list from its start at once.
fn change_filter(app: &mut App, step: fn(ThreadFilter) -> ThreadFilter) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    list.filter = step(list.filter);
    restart(app);
    request_first(app);
}

/// Change the search text. The request waits for `SEARCH_DELAY` (the event loop's timer sends
/// `ThreadListSearchDue`), so a burst of keys is one request.
fn edit(app: &mut App, change: impl FnOnce(&mut String)) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    let before = list.query.clone();
    change(&mut list.query);
    if list.query == before {
        return;
    }
    restart(app);
    if let Some(list) = app.thread_list.as_mut() {
        list.search_pending = true;
    }
}

/// The typing delay ran out: send the request for the current text.
pub fn on_search_due(app: &mut App) {
    if app.thread_list.as_ref().is_some_and(|l| l.search_pending) {
        request_first(app);
    }
}

/// Ask for the next page when the selection is near the end of the loaded rows and the server has
/// more. Nothing is asked while a request is in flight or after a failed page (the next selection
/// move tries again).
fn request_more(app: &mut App) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    let near_end = list.selected + PAGE_AHEAD >= list.rows.len();
    if list.load != ListLoad::Loaded
        || list.search_pending
        || list.loading_more
        || list.more_failed.is_some()
        || !list.has_more()
        || !near_end
    {
        return;
    }
    list.loading_more = true;
    let offset = u32::try_from(list.rows.len()).unwrap_or(u32::MAX);
    app.effects.push(effect(list, offset));
}

/// A page of the list arrived for `generation`. An answer for another room or query is dropped.
pub fn on_page(
    app: &mut App,
    room: &RoomId,
    generation: u64,
    offset: u32,
    result: Result<ThreadListPage, ApiError>,
) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    if list.room != *room || list.generation != generation {
        return;
    }
    match result {
        Ok(page) if offset == 0 => {
            list.total = page.total;
            list.rows = page.rows;
            list.selected = 0;
            list.load = ListLoad::Loaded;
            list.loading_more = false;
        }
        Ok(page) if offset as usize == list.rows.len() => {
            list.loading_more = false;
            // A page with nothing in it ends the list, so a short total cannot loop.
            list.total = if page.rows.is_empty() {
                u32::try_from(list.rows.len()).unwrap_or(u32::MAX)
            } else {
                page.total
            };
            list.rows.extend(page.rows);
        }
        // A page for an offset the list no longer has.
        Ok(_) => {}
        Err(error) if offset == 0 => {
            list.load = ListLoad::Failed(error);
            list.loading_more = false;
        }
        Err(error) => {
            list.loading_more = false;
            list.more_failed = Some(error);
        }
    }
    request_more(app);
}

/// `select`: open the selected thread in the column, focused, as `open_thread` does. The list
/// state is kept, hidden, and closing the thread shows it again. A parent held by the main flow
/// opens at once. Any other parent is fetched first and the list stays shown until it arrives, so
/// a thread whose parent is gone can say so and leave the list where it was. The main flow does
/// not move.
fn choose(app: &mut App) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    let Some(row) = list.rows.get(list.selected) else {
        return;
    };
    if list.choosing.is_some() {
        return;
    }
    let (room, parent_id) = (list.room.clone(), row.id.clone());
    let held = app
        .messages
        .get(&room)
        .and_then(|held| held.iter().find(|m| m.id == parent_id))
        .cloned();
    if held.is_some() {
        thread::open_from_list(app, room, parent_id, None);
        return;
    }
    list.choosing = Some(parent_id.clone());
    app.effects
        .push(Effect::FetchThreadParent { room, parent_id });
}

/// The parent of the chosen thread arrived, or could not be fetched.
pub fn on_parent(app: &mut App, room: &RoomId, parent_id: &str, result: Result<Message, ApiError>) {
    let Some(list) = app.thread_list.as_mut() else {
        return;
    };
    if list.room != *room || list.choosing.as_deref() != Some(parent_id) {
        return;
    }
    list.choosing = None;
    match result {
        Ok(parent) => {
            thread::open_from_list(app, room.clone(), parent_id.to_string(), Some(parent));
        }
        Err(ApiError::NotFound | ApiError::Rejected | ApiError::BadResponse) => {
            app.status_msg = Some(StatusMsg::Error("The thread is gone".to_string()));
        }
        Err(error) => {
            app.status_msg = Some(StatusMsg::Error(format!(
                "Could not open the thread: {error}"
            )));
        }
    }
}

/// Keeps one timer in step with `ThreadList::search_pending` for the event loop, which has the
/// clock. It is armed when a new wait appears (a new generation) and cleared when there is none.
#[derive(Debug, Default)]
pub struct SearchTimer {
    armed: Option<u64>,
    deadline: Option<tokio::time::Instant>,
}

impl SearchTimer {
    /// Bring the timer in line with the list, a wait that started "now".
    pub fn sync(&mut self, list: Option<&ThreadList>, now: tokio::time::Instant) {
        let wanted = list.filter(|l| l.search_pending).map(|l| l.generation);
        if wanted != self.armed {
            self.armed = wanted;
            self.deadline = wanted.map(|_| now + SEARCH_DELAY);
        }
    }

    pub fn deadline(&self) -> Option<tokio::time::Instant> {
        self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::api::map;
    use crate::api::wire::{RoomsResp, SubscriptionsResp};
    use crate::app::update::update;
    use crate::app::{Focus, Mode};
    use crate::data::{MessageKind, SendState};
    use crate::test_util::press;
    use ratatui::layout::Size;

    const SUBS: &str = include_str!("../api/fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("../api/fixtures/rooms.json");

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    fn row(id: &str) -> ThreadRow {
        ThreadRow {
            id: id.to_string(),
            author: "ana".to_string(),
            first_line: format!("text of {id}"),
            replies: 2,
            last_reply: Some(1_000),
            followers: Vec::new(),
        }
    }

    fn rows_page(first: usize, n: usize, total: u32) -> ThreadListPage {
        ThreadListPage {
            rows: (first..first + n).map(|i| row(&format!("t{i}"))).collect(),
            total,
        }
    }

    fn held(id: &str) -> Message {
        Message {
            id: id.to_string(),
            room_id: rid("r-general"),
            author: "ana".to_string(),
            ts: 100,
            body: format!("body of {id}"),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            tmid: None,
            tshow: false,
            tcount: 2,
            tlm: Some(110),
            followers: None,
            refs: None,
        }
    }

    /// `r-general` open with `main` loaded, in Normal mode with the messages pane focused.
    fn app_with(main: Vec<Message>) -> App {
        let subs = serde_json::from_str::<SubscriptionsResp>(SUBS)
            .expect("subscriptions")
            .update;
        let records = serde_json::from_str::<RoomsResp>(ROOMS)
            .expect("rooms")
            .update;
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(map::rooms(&subs, &records)));
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: main,
                oldest_ts: None,
                older: false,
            },
        );
        app.effects.clear();
        app.focus = Focus::Messages;
        app.mode = Mode::Normal;
        app
    }

    /// The list opened and its first page of `n` rows (of `total`) answered.
    fn opened(app: &mut App, n: usize, total: u32) {
        update(app, Action::ListThreads);
        app.effects.clear();
        let generation = app.thread_list.as_ref().expect("open").generation;
        answer(app, generation, 0, Ok(rows_page(0, n, total)));
    }

    fn answer(
        app: &mut App,
        generation: u64,
        offset: u32,
        result: Result<ThreadListPage, ApiError>,
    ) {
        update(
            app,
            Action::ThreadListPage {
                room: rid("r-general"),
                generation,
                offset,
                result,
            },
        );
    }

    fn list(app: &App) -> &ThreadList {
        app.thread_list.as_ref().expect("the list is open")
    }

    fn requests(app: &App) -> Vec<(ThreadFilter, u64, u32)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::ListThreads {
                    filter,
                    generation,
                    offset,
                    ..
                } => Some((*filter, *generation, *offset)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn open_asks_for_the_first_page_focuses_the_pane_and_keeps_the_mode() {
        let mut app = app_with(vec![]);
        update(&mut app, Action::ListThreads);
        let open = list(&app);
        assert_eq!(
            (open.filter, open.load),
            (ThreadFilter::All, ListLoad::Loading)
        );
        assert!(matches!(
            app.effects.as_slice(),
            [Effect::ListThreads { room, filter: ThreadFilter::All, offset: 0, count: PAGE, .. }]
                if *room == rid("r-general")
        ));
        assert_eq!((app.mode, app.focus), (Mode::Normal, Focus::ThreadList));
        assert_eq!(list(&app).opened_from, Focus::Messages);
    }

    /// Open the search line and type `text` into it.
    fn search(app: &mut App, text: &str) {
        press(app, "/");
        for c in text.chars() {
            press(app, &c.to_string());
        }
    }

    #[test]
    fn no_room_and_an_encrypted_room_open_nothing_and_say_why() {
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::ListThreads);
        assert!(app.thread_list.is_none() && app.effects.is_empty());
        assert!(matches!(&app.status_msg, Some(StatusMsg::Info(t)) if t.contains("No room")));

        let mut app = app_with(vec![]);
        let id = app.current_room.clone().expect("open");
        app.rooms
            .iter_mut()
            .find(|r| r.id == id)
            .expect("room")
            .encrypted = true;
        update(&mut app, Action::ListThreads);
        assert!(app.thread_list.is_none() && app.effects.is_empty());
        assert!(matches!(&app.status_msg, Some(StatusMsg::Info(t)) if t.contains("Encrypted")));
    }

    #[test]
    fn the_first_page_fills_the_list_and_a_stale_answer_is_dropped() {
        let mut app = app_with(vec![]);
        update(&mut app, Action::ListThreads);
        let generation = list(&app).generation;
        answer(&mut app, generation + 99, 0, Ok(rows_page(0, 3, 3)));
        assert!(list(&app).rows.is_empty(), "an answer for another query");
        answer(&mut app, generation, 0, Ok(rows_page(0, 3, 3)));
        assert_eq!(
            (list(&app).rows.len(), list(&app).load),
            (3, ListLoad::Loaded)
        );

        // A list opened later never reuses a generation, so the old answer cannot land in it.
        update(&mut app, Action::Key(key_esc()));
        update(&mut app, Action::ListThreads);
        answer(&mut app, generation, 0, Ok(rows_page(0, 3, 3)));
        assert!(list(&app).rows.is_empty());
    }

    fn key_esc() -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    #[test]
    fn the_filter_cycles_and_reloads_from_the_start() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        let before = list(&app).generation;
        press(&mut app, "l");
        assert_eq!(list(&app).filter, ThreadFilter::Following);
        assert!(list(&app).rows.is_empty());
        assert_eq!(list(&app).load, ListLoad::Loading);
        let sent = requests(&app);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0, sent[0].2), (ThreadFilter::Following, 0));
        assert_ne!(sent[0].1, before);

        // The answer of the filter before is dropped.
        answer(&mut app, before, 0, Ok(rows_page(0, 3, 3)));
        assert!(list(&app).rows.is_empty());

        app.effects.clear();
        press(&mut app, "l");
        press(&mut app, "l");
        assert_eq!(list(&app).filter, ThreadFilter::All);
        press(&mut app, "h");
        assert_eq!(list(&app).filter, ThreadFilter::Unread);
    }

    #[test]
    fn typing_goes_to_the_search_and_a_burst_is_one_request() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        search(&mut app, "dej");
        assert_eq!(list(&app).query, "dej");
        assert!(app.effects.is_empty(), "nothing is sent while typing");
        assert!(list(&app).search_pending);
        assert_eq!(app.mode, Mode::Normal);

        let generation = list(&app).generation;
        update(&mut app, Action::ThreadListSearchDue);
        assert_eq!(requests(&app).len(), 1);
        assert_eq!(requests(&app)[0].1, generation);
        assert!(matches!(
            &app.effects[0],
            Effect::ListThreads { query, .. } if query.as_str() == "dej"
        ));
        assert!(!list(&app).search_pending);
        // The timer firing again with nothing pending sends nothing.
        app.effects.clear();
        update(&mut app, Action::ThreadListSearchDue);
        assert!(app.effects.is_empty());

        // Deleting the text lists the filter's threads again.
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        assert_eq!(list(&app).query, "");
        assert!(list(&app).searching, "the line stays open while editing");
        update(&mut app, Action::ThreadListSearchDue);
        assert!(matches!(
            app.effects.last(),
            Some(Effect::ListThreads { query, .. }) if query.as_str().is_empty()
        ));
    }

    #[test]
    fn an_answer_for_older_text_is_dropped() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        search(&mut app, "a");
        let old = list(&app).generation;
        update(&mut app, Action::ThreadListSearchDue);
        press(&mut app, "b");
        answer(&mut app, old, 0, Ok(rows_page(0, 1, 1)));
        assert!(list(&app).rows.is_empty(), "the stale answer is not shown");
        assert!(list(&app).search_pending);
    }

    #[test]
    fn selection_moves_through_the_keymap_and_stops_at_both_ends() {
        let mut app = app_with(vec![]);
        opened(&mut app, 5, 5);
        press(&mut app, "k");
        assert_eq!(list(&app).selected, 0);
        press(&mut app, "j");
        press(&mut app, "j");
        assert_eq!(list(&app).selected, 2);
        press(&mut app, "k");
        assert_eq!(list(&app).selected, 1);
        press(&mut app, "G");
        assert_eq!(list(&app).selected, 4);
        press(&mut app, "j");
        assert_eq!(list(&app).selected, 4);
        press(&mut app, "gg");
        assert_eq!(list(&app).selected, 0);
        press(&mut app, "3j");
        assert_eq!(list(&app).selected, 3);
        press(&mut app, "<C-d>");
        assert_eq!(list(&app).selected, 4);
        press(&mut app, "<C-u>");
        assert!(list(&app).selected < 4);
        assert_eq!(list(&app).query, "", "motions are not text");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn a_rebound_motion_applies_in_the_pane() {
        use crate::test_util::{app_with_config, config_from_keymap};
        let mut app = app_with_config(
            config_from_keymap("[normal]\n\"n\" = \"move_down\"\n"),
            80,
            24,
        );
        press(&mut app, "<CR>");
        update(&mut app, Action::ListThreads);
        let generation = list(&app).generation;
        let room = app.current_room.clone().expect("room");
        update(
            &mut app,
            Action::ThreadListPage {
                room,
                generation,
                offset: 0,
                result: Ok(rows_page(0, 3, 3)),
            },
        );
        press(&mut app, "n");
        assert_eq!(list(&app).selected, 1);
    }

    #[test]
    fn typing_in_the_search_line_is_text_and_the_badge_stays_normal() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        press(&mut app, "/");
        assert!(list(&app).searching);
        press(&mut app, "j");
        press(&mut app, "q");
        assert_eq!(list(&app).query, "jq");
        assert_eq!(list(&app).selected, 0, "nothing moved");
        assert!(app.thread_list.is_some(), "q is text here, not close");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn enter_keeps_the_text_and_esc_clears_it() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        search(&mut app, "dep");
        press(&mut app, "<CR>");
        assert!(!list(&app).searching);
        assert_eq!(list(&app).query, "dep");
        // Enter does not wait for the typing delay.
        assert!(matches!(
            app.effects.last(),
            Some(Effect::ListThreads { query, .. }) if query.as_str() == "dep"
        ));
        let generation = list(&app).generation;
        answer(&mut app, generation, 0, Ok(rows_page(0, 3, 3)));
        press(&mut app, "j");
        assert_eq!(list(&app).selected, 1, "the list keys are back");

        app.effects.clear();
        search(&mut app, "x");
        press(&mut app, "<Esc>");
        assert!(!list(&app).searching);
        assert_eq!(list(&app).query, "");
        assert!(matches!(
            app.effects.last(),
            Some(Effect::ListThreads { query, .. }) if query.as_str().is_empty()
        ));
        assert!(app.thread_list.is_some(), "Esc in the line does not close");
    }

    #[test]
    fn q_and_esc_close_the_list_unless_a_sequence_is_half_typed() {
        for key in ["q", "<Esc>"] {
            let mut app = app_with(vec![]);
            opened(&mut app, 3, 3);
            press(&mut app, key);
            assert!(app.thread_list.is_none(), "{key}");
            assert_eq!((app.mode, app.focus), (Mode::Normal, Focus::Messages));
        }
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        press(&mut app, "j");
        press(&mut app, "g");
        press(&mut app, "q");
        assert!(
            app.thread_list.is_some(),
            "`q` after a pending `g` is the keymap's"
        );
    }

    #[test]
    fn closing_returns_focus_to_where_the_list_was_opened_from() {
        let mut app = app_with(vec![]);
        app.focus = Focus::Sidebar;
        update(&mut app, Action::ListThreads);
        assert_eq!(app.focus, Focus::ThreadList);
        press(&mut app, "q");
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn focus_keys_leave_the_pane_without_closing_it_and_tab_comes_back() {
        let mut app = app_with(vec![]);
        opened(&mut app, 3, 3);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Messages);
        assert!(app.thread_list.is_some());
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::ThreadList);
        // `<Tab>` is bound only in the vim-extras preset; the action is what the cycle runs.
        press(&mut app, "<C-w>h");
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::ThreadList, "the cycle from the messages");
        // Inside the pane `l` is the next filter and the cycle goes on to the sidebar.
        press(&mut app, "l");
        assert_eq!(app.focus, Focus::ThreadList);
        assert_eq!(list(&app).filter, ThreadFilter::Following);
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::Sidebar, "the cycle goes on past the list");
    }

    #[test]
    fn the_next_page_loads_near_the_end_and_a_failed_page_keeps_the_rows() {
        let mut app = app_with(vec![]);
        opened(&mut app, 50, 120);
        // Far from the end: nothing is asked.
        press(&mut app, "j");
        assert!(requests(&app).is_empty());
        for _ in 0..44 {
            press(&mut app, "j");
        }
        let sent = requests(&app);
        assert_eq!(sent.len(), 1, "one request near the end");
        assert_eq!(sent[0].2, 50);
        assert!(list(&app).loading_more);
        // Still moving does not ask again while the page is in flight.
        press(&mut app, "j");
        assert_eq!(requests(&app).len(), 1);

        let generation = list(&app).generation;
        answer(&mut app, generation, 50, Ok(rows_page(50, 50, 120)));
        assert_eq!(list(&app).rows.len(), 100);
        assert_eq!(list(&app).selected, 46);

        app.effects.clear();
        for _ in 0..50 {
            press(&mut app, "j");
        }
        assert_eq!(
            requests(&app).iter().map(|r| r.2).collect::<Vec<_>>(),
            [100]
        );
        answer(&mut app, generation, 100, Err(ApiError::Server(503)));
        assert_eq!(list(&app).rows.len(), 100, "the loaded rows stay");
        assert_eq!(list(&app).more_failed, Some(ApiError::Server(503)));
        assert_eq!(list(&app).load, ListLoad::Loaded);
        // No automatic retry; the next move down tries again.
        app.effects.clear();
        answer(&mut app, generation, 100, Err(ApiError::Server(503)));
        assert!(requests(&app).is_empty());
        press(&mut app, "j");
        assert_eq!(requests(&app).len(), 1);
    }

    #[test]
    fn a_failed_first_page_is_kept_as_its_class() {
        let mut app = app_with(vec![]);
        update(&mut app, Action::ListThreads);
        let generation = list(&app).generation;
        answer(&mut app, generation, 0, Err(ApiError::Rejected));
        assert_eq!(list(&app).load, ListLoad::Failed(ApiError::Rejected));
    }

    #[test]
    fn a_loaded_parent_opens_the_pane_at_once_and_the_main_flow_stays() {
        let mut app = app_with(vec![held("t0"), held("t9")]);
        let cursor = crate::app::messages::view(&app, &rid("r-general")).cursor;
        opened(&mut app, 2, 2);
        press(&mut app, "<CR>");
        assert!(app.thread_list.is_some(), "kept, hidden, for the way back");
        assert!(!app.thread_list_shown());
        let pane = app.thread.as_ref().expect("the pane");
        assert!(pane.from_list);
        assert_eq!(pane.parent_id, "t0");
        assert_eq!(app.focus, Focus::Thread);
        assert!(matches!(
            app.effects.as_slice(),
            [Effect::LoadThread {
                fetch_parent: false,
                offset: 0,
                ..
            }]
        ));
        assert_eq!(
            crate::app::messages::view(&app, &rid("r-general")).cursor,
            cursor
        );
    }

    #[test]
    fn an_unloaded_parent_is_fetched_first_and_the_list_stays_until_it_arrives() {
        let mut app = app_with(vec![held("t9")]);
        opened(&mut app, 1, 1);
        press(&mut app, "<CR>");
        assert!(app.thread_list.is_some() && app.thread.is_none());
        assert!(matches!(
            app.effects.as_slice(),
            [Effect::FetchThreadParent { parent_id, .. }] if parent_id == "t0"
        ));
        // A second Enter while fetching does not ask again.
        app.effects.clear();
        press(&mut app, "<CR>");
        assert!(app.effects.is_empty());

        update(
            &mut app,
            Action::ThreadListParent {
                room: rid("r-general"),
                parent_id: "t0".to_string(),
                result: Ok(held("t0")),
            },
        );
        assert!(app.thread_list.is_some() && !app.thread_list_shown());
        let pane = app.thread.as_ref().expect("the pane");
        assert_eq!(pane.messages.first().map(|m| m.id.as_str()), Some("t0"));
        assert_eq!(app.focus, Focus::Thread);
        // The parent is in hand, so only the replies are asked for.
        assert!(matches!(
            app.effects.as_slice(),
            [Effect::LoadThread {
                fetch_parent: false,
                ..
            }]
        ));
    }

    #[test]
    fn closing_the_thread_returns_to_the_list_as_it_was() {
        let mut app = app_with(vec![held("t0"), held("t1"), held("t2")]);
        opened(&mut app, 3, 3);
        press(&mut app, "j");
        press(&mut app, "<CR>");
        assert_eq!(
            app.thread.as_ref().map(|p| p.parent_id.as_str()),
            Some("t1")
        );
        app.effects.clear();
        press(&mut app, "<Esc>");
        assert!(app.thread.is_none());
        assert!(app.thread_list_shown());
        assert_eq!(app.focus, Focus::ThreadList);
        assert_eq!((list(&app).selected, list(&app).rows.len()), (1, 3));
        assert!(
            app.effects.is_empty(),
            "no new request when the rows are held"
        );
        // The list's own Esc then closes it and the messages are back.
        press(&mut app, "<Esc>");
        assert!(app.thread_list.is_none());
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn a_thread_opened_another_way_takes_the_column_for_good() {
        let mut app = app_with(vec![held("t0")]);
        opened(&mut app, 1, 1);
        press(&mut app, "<C-w>h");
        press(&mut app, "gt");
        assert!(app.thread.is_some());
        assert!(app.thread_list.is_none(), "the list gave the column up");
        press(&mut app, "<Esc>");
        assert_eq!(app.focus, Focus::Messages);
        assert!(!app.thread_list_shown());
    }

    #[test]
    fn gt_capital_over_an_open_thread_shows_the_list_and_keeps_unsent_replies() {
        let mut app = app_with(vec![held("t0")]);
        opened(&mut app, 1, 1);
        press(&mut app, "<CR>");
        let unsent = Message {
            send: SendState::Failed,
            ..held("local-1")
        };
        app.thread.as_mut().expect("pane").messages.push(unsent);
        press(&mut app, "gT");
        assert!(app.thread.is_none());
        assert!(app.thread_list_shown());
        assert_eq!(app.focus, Focus::ThreadList);
        assert!(app.outbox.iter().any(|m| m.id == "local-1"), "kept");
        assert_eq!(list(&app).rows.len(), 1, "the list state is the one it was");

        // A thread opened some other way is closed and a new list starts.
        press(&mut app, "q");
        press(&mut app, "gt");
        assert!(app.thread.is_some());
        press(&mut app, "gT");
        assert!(app.thread.is_none() && app.thread_list_shown());
        assert_eq!(list(&app).load, ListLoad::Loading);
    }

    #[test]
    fn a_room_change_closes_the_list_and_a_thread_opened_from_it() {
        let mut app = app_with(vec![held("t0")]);
        opened(&mut app, 1, 1);
        app.open_room(rid("r-other"));
        assert!(app.thread_list.is_none() && app.thread.is_none());
        assert_eq!(app.focus, Focus::Messages);

        let mut app = app_with(vec![held("t0")]);
        opened(&mut app, 1, 1);
        press(&mut app, "<CR>");
        assert!(app.thread.is_some());
        app.open_room(rid("r-other"));
        assert!(app.thread_list.is_none() && app.thread.is_none());
    }

    #[test]
    fn a_gone_parent_says_so_and_keeps_the_list_open() {
        for error in [
            ApiError::NotFound,
            ApiError::BadResponse,
            ApiError::Rejected,
        ] {
            let mut app = app_with(vec![]);
            opened(&mut app, 1, 1);
            press(&mut app, "<CR>");
            update(
                &mut app,
                Action::ThreadListParent {
                    room: rid("r-general"),
                    parent_id: "t0".to_string(),
                    result: Err(error),
                },
            );
            assert!(
                app.thread_list.is_some() && app.thread.is_none(),
                "{error:?}"
            );
            assert!(
                matches!(&app.status_msg, Some(StatusMsg::Error(t)) if t.contains("gone")),
                "{error:?}"
            );
            // It can be chosen again, or another one picked.
            assert!(list(&app).choosing.is_none());
        }
    }

    #[test]
    fn a_late_parent_for_a_closed_list_is_dropped() {
        let mut app = app_with(vec![]);
        opened(&mut app, 1, 1);
        press(&mut app, "<CR>");
        press(&mut app, "<Esc>");
        update(
            &mut app,
            Action::ThreadListParent {
                room: rid("r-general"),
                parent_id: "t0".to_string(),
                result: Ok(held("t0")),
            },
        );
        assert!(app.thread.is_none());
    }

    #[test]
    fn nothing_is_chosen_from_an_empty_list() {
        let mut app = app_with(vec![]);
        opened(&mut app, 0, 0);
        press(&mut app, "<CR>");
        assert!(app.thread_list.is_some() && app.effects.is_empty());
    }

    #[test]
    fn debug_shows_counts_and_never_text() {
        let mut app = app_with(vec![]);
        opened(&mut app, 2, 2);
        search(&mut app, "S");
        let shown = format!("{:?}", list(&app));
        assert!(
            !shown.contains('S') || !shown.contains("text of"),
            "{shown}"
        );
        assert!(!shown.contains("text of"), "{shown}");
        assert!(shown.contains("query_len"), "{shown}");
        let effect = Effect::ListThreads {
            room: rid("r"),
            filter: ThreadFilter::All,
            query: SendText::new("SENTINEL-QUERY".to_string()),
            generation: 1,
            offset: 0,
            count: PAGE,
        };
        assert!(!format!("{effect:?}").contains("SENTINEL"));
        let action = Action::ThreadListPage {
            room: rid("r"),
            generation: 1,
            offset: 0,
            result: Ok(rows_page(0, 1, 1)),
        };
        assert!(!format!("{action:?}").contains("text of"));
    }

    #[test]
    fn the_search_timer_arms_one_delay_after_a_wait_appears() {
        let mut app = app_with(vec![]);
        opened(&mut app, 1, 1);
        let mut timer = SearchTimer::default();
        let t0 = tokio::time::Instant::now();
        timer.sync(app.thread_list.as_ref(), t0);
        assert!(timer.deadline().is_none());
        search(&mut app, "a");
        timer.sync(app.thread_list.as_ref(), t0);
        assert_eq!(timer.deadline(), Some(t0 + SEARCH_DELAY));
        // Another key restarts the wait from "now".
        press(&mut app, "b");
        let t1 = t0 + Duration::from_millis(100);
        timer.sync(app.thread_list.as_ref(), t1);
        assert_eq!(timer.deadline(), Some(t1 + SEARCH_DELAY));
        // The same wait seen again does not push the deadline.
        timer.sync(app.thread_list.as_ref(), t1 + Duration::from_millis(50));
        assert_eq!(timer.deadline(), Some(t1 + SEARCH_DELAY));
        update(&mut app, Action::ThreadListSearchDue);
        timer.sync(app.thread_list.as_ref(), t1);
        assert!(timer.deadline().is_none());
    }
}
