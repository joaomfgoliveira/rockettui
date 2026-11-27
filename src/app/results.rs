//! The server search results list (`:search <text>`): a read-only list over the messages pane.
//!
//! Like the help overlay it is a state of the app (`App.results`), not a mode: the mode badge
//! keeps its value and the reducer checks it before the keymap. The list holds the messages the
//! server returned, newest first, and is never written anywhere. No `Debug`: it holds the query
//! and message text.

use crate::action::Action;
use crate::api::{ApiError, SearchHits};
use crate::app::jump;
use crate::app::messages::{self, MsgView};
use crate::app::{App, Mode, StatusMsg};
use crate::data::{Message, RoomId};
use crate::effect::{Effect, SendText};
use crate::input::key::Chord;
use crate::input::matcher::Outcome;

/// How many results one request asks for. 50 is the server default; its maximum is 100.
pub const PAGE: u16 = 50;

const NEED_TEXT: &str = "Search needs a text: :search <text>";
const NEED_ROOM: &str = "Open a room to search it";
const ENCRYPTED: &str = "Encrypted rooms cannot be searched";
const NO_RESULTS: &str = "No results";
const REFUSED: &str = "Search refused: no access to this room, or the query is not valid";

/// The open results list.
pub struct ResultsState {
    /// The search this list belongs to: answers carry it, and a stale one is dropped.
    pub id: u64,
    pub room: RoomId,
    pub query: String,
    /// Newest first, as the server sent them.
    pub messages: Vec<Message>,
    /// The raw server timestamp of each result, parallel to `messages` (`None`: not openable).
    pub raw_ts: Vec<Option<String>>,
    pub selected: usize,
    pub top_row: usize,
    /// A page came back shorter than `PAGE`: the server has no more.
    pub exhausted: bool,
    /// A next-page request is in flight.
    pub loading_more: bool,
    /// The open context view of one result (`app::jump`), drawn over the list.
    pub context: Option<jump::ContextView>,
    /// The context request in flight, by `App.context_seq`; an answer with another id is dropped.
    pub pending_context: Option<u64>,
}

fn error(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Error(text.to_string()));
}

/// `:search <text>`: validate, then ask the server for the first page. A refusal changes nothing.
pub fn start(app: &mut App, query: &str) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Err(NEED_TEXT.to_string());
    }
    let Some(room) = app.current_room.clone() else {
        return Err(NEED_ROOM.to_string());
    };
    if app.rooms.iter().any(|r| r.id == room && r.encrypted) {
        return Err(ENCRYPTED.to_string());
    }
    app.search_seq += 1;
    app.pending_search = Some((app.search_seq, query.to_string()));
    app.effects.push(Effect::SearchRoom {
        room,
        query: SendText::new(query.to_string()),
        search_id: app.search_seq,
        offset: 0,
        count: PAGE,
    });
    Ok(())
}

/// The answer to a search request. The first page (`offset` 0) opens or replaces the list; a later
/// page is appended. Anything that no longer matches the app is dropped.
pub fn on_page(
    app: &mut App,
    room: RoomId,
    search_id: u64,
    offset: u32,
    result: Result<SearchHits, ApiError>,
) {
    if offset == 0 {
        if app.current_room.as_ref() != Some(&room)
            || app.pending_search.as_ref().map(|(id, _)| *id) != Some(search_id)
        {
            return;
        }
        let Some((_, query)) = app.pending_search.take() else {
            return;
        };
        match result {
            Err(e) => fail(app, e),
            Ok(found) if found.messages.is_empty() => {
                app.status_msg = Some(StatusMsg::Info(NO_RESULTS.to_string()));
            }
            Ok(found) => {
                let exhausted = found.messages.len() < usize::from(PAGE);
                app.results = Some(ResultsState {
                    id: search_id,
                    room,
                    query,
                    messages: found.messages,
                    raw_ts: found.raw_ts,
                    selected: 0,
                    top_row: 0,
                    exhausted,
                    loading_more: false,
                    context: None,
                    pending_context: None,
                });
            }
        }
        return;
    }
    let Some(list) = app.results.as_mut() else {
        return;
    };
    if list.id != search_id || list.room != room || !list.loading_more {
        return;
    }
    list.loading_more = false;
    match result {
        Err(e) => fail(app, e),
        Ok(found) => {
            list.exhausted = found.messages.len() < usize::from(PAGE);
            list.messages.extend(found.messages);
            list.raw_ts.extend(found.raw_ts);
        }
    }
}

fn fail(app: &mut App, e: ApiError) {
    let text = if e == ApiError::Rejected {
        REFUSED.to_string()
    } else {
        e.to_string()
    };
    error(app, &text);
}

/// Close the list and forget a first page still in flight (opening another room).
pub fn close_all(app: &mut App) {
    app.results = None;
    app.pending_search = None;
}

/// The inner height and width of the list: the messages pane minus its border.
fn viewport(app: &App) -> (u16, u16) {
    messages::viewport(app)
}

/// The effective selection and scroll of the list at the current size.
pub fn view(app: &App) -> MsgView {
    let Some(list) = app.results.as_ref() else {
        return MsgView::default();
    };
    let (width, height) = viewport(app);
    let ctx = app.row_ctx(&list.messages);
    let starts = messages::row_starts_of(&list.messages, width, &ctx);
    let selected = list.selected.min(list.messages.len().saturating_sub(1));
    MsgView {
        cursor: selected,
        top_row: messages::scroll_into_view(&starts, selected, list.top_row, usize::from(height)),
    }
}

/// A key while the list is open (Normal mode only). `q`, Esc and Ctrl-c close it; every other key
/// goes through the Normal keymap, and only the motions and `:` get through.
pub fn key(app: &mut App, chord: Chord) {
    use crossterm::event::{KeyCode, KeyModifiers};
    let plain_q = chord.code == KeyCode::Char('q') && chord.mods.is_empty();
    let esc = chord.code == KeyCode::Esc && chord.mods.is_empty();
    let ctrl_c = chord.code == KeyCode::Char('c') && chord.mods == KeyModifiers::CONTROL;
    if plain_q || esc || ctrl_c {
        app.matcher.reset();
        // `q` in a context view goes back to the list; a second `q` closes the list.
        match app.results.as_mut() {
            Some(list) if list.context.is_some() => list.context = None,
            _ => app.results = None,
        }
        return;
    }
    let outcome = app
        .matcher
        .feed(app.keymap.trie(Mode::Normal), Mode::Normal, chord);
    dispatch(app, outcome);
}

/// Apply what the matcher produced while the list is open: move, open the command line, or nothing.
pub fn dispatch(app: &mut App, outcome: Outcome) {
    let Outcome::Fire { action, count } = outcome else {
        return;
    };
    if action == Action::EnterCommand {
        app.mode = Mode::Command;
        app.cmdline.clear();
        return;
    }
    if action == Action::ShowContext {
        app.context = true;
        return;
    }
    if app.results.as_ref().is_some_and(|l| l.context.is_some()) {
        jump::dispatch(app, &action, count);
        return;
    }
    if action == Action::Select {
        jump::open_selected(app);
        return;
    }
    let Some(list) = app.results.as_ref() else {
        return;
    };
    let (width, height) = viewport(app);
    let ctx = app.row_ctx(&list.messages);
    let starts = messages::row_starts_of(&list.messages, width, &ctx);
    let current = view(app);
    if let Some(next) = messages::step(&starts, current, usize::from(height), &action, count)
        && let Some(list) = app.results.as_mut()
    {
        list.selected = next.cursor;
        list.top_row = next.top_row;
    }
    maybe_load_more(app);
}

/// Ask for the next page when the selection is on the last loaded result and the server may hold
/// more. One request at a time.
fn maybe_load_more(app: &mut App) {
    let Some(list) = app.results.as_mut() else {
        return;
    };
    if list.exhausted || list.loading_more || list.selected + 1 < list.messages.len() {
        return;
    }
    list.loading_more = true;
    let effect = Effect::SearchRoom {
        room: list.room.clone(),
        query: SendText::new(list.query.clone()),
        search_id: list.id,
        offset: u32::try_from(list.messages.len()).unwrap_or(u32::MAX),
        count: PAGE,
    };
    app.effects.push(effect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{MessageKind, SendState};
    use crate::test_util::{app_80x24, press};

    fn found(n: usize, room: &RoomId) -> Vec<Message> {
        (0..n)
            .map(|i| Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                refs: None,
                attachments: Vec::new(),
                reactions: Vec::new(),
                id: format!("s{i}"),
                room_id: room.clone(),
                author: "alice".to_string(),
                ts: 1_000_000 - i as i64,
                body: format!("result {i}"),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            })
            .collect()
    }

    fn open_plain_room(app: &mut App) -> RoomId {
        let id = app
            .rooms
            .iter()
            .find(|r| !r.encrypted)
            .expect("a plain room")
            .id
            .clone();
        app.open_room(id.clone());
        id
    }

    fn run(app: &mut App, line: &str) {
        crate::app::command::execute(app, line);
    }

    fn status(app: &App) -> String {
        match &app.status_msg {
            Some(StatusMsg::Info(t) | StatusMsg::Error(t)) => t.clone(),
            None => String::new(),
        }
    }

    /// Run `:search` and answer its first page with `n` results.
    fn open_list(app: &mut App, room: &RoomId, n: usize) {
        run(app, "search deploy");
        let id = app.search_seq;
        crate::app::update::update(
            app,
            Action::SearchPage {
                room: room.clone(),
                search_id: id,
                offset: 0,
                result: Ok(found(n, room).into()),
            },
        );
    }

    #[test]
    fn search_sends_one_request_with_the_raw_text() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        app.effects.clear();
        run(&mut app, "search  from:ana   two  words ");
        assert_eq!(
            app.effects,
            [Effect::SearchRoom {
                room,
                query: SendText::new("from:ana   two  words".to_string()),
                search_id: 1,
                offset: 0,
                count: PAGE,
            }]
        );
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn refusals_send_nothing_and_keep_an_open_list() {
        let mut app = app_80x24();
        run(&mut app, "search hello");
        assert_eq!(status(&app), NEED_ROOM);
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        app.effects.clear();
        run(&mut app, "search");
        assert_eq!(status(&app), NEED_TEXT);
        let secret = app
            .rooms
            .iter()
            .find(|r| r.encrypted)
            .expect("an encrypted room")
            .id
            .clone();
        app.current_room = Some(secret);
        run(&mut app, "search hello");
        assert_eq!(status(&app), ENCRYPTED);
        assert!(app.effects.is_empty());
        assert!(app.results.is_some(), "a refused search keeps the list");
    }

    #[test]
    fn first_page_opens_the_list_in_server_order() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        let list = app.results.as_ref().expect("list");
        assert_eq!(list.messages[0].id, "s0");
        assert_eq!(list.selected, 0);
        assert!(list.exhausted, "a short page is the end");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn no_results_and_failures_use_the_status_line() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 0);
        assert_eq!(status(&app), NO_RESULTS);
        assert!(app.results.is_none());

        run(&mut app, "search x");
        let id = app.search_seq;
        crate::app::update::update(
            &mut app,
            Action::SearchPage {
                room: room.clone(),
                search_id: id,
                offset: 0,
                result: Err(ApiError::Timeout),
            },
        );
        assert_eq!(status(&app), ApiError::Timeout.to_string());
        run(&mut app, "search x");
        let id = app.search_seq;
        crate::app::update::update(
            &mut app,
            Action::SearchPage {
                room,
                search_id: id,
                offset: 0,
                result: Err(ApiError::Rejected),
            },
        );
        assert_eq!(status(&app), REFUSED);
        assert!(app.results.is_none());
    }

    #[test]
    fn late_answers_are_dropped() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        run(&mut app, "search a");
        let old = app.search_seq;
        run(&mut app, "search b");
        let page = |id| Action::SearchPage {
            room: room.clone(),
            search_id: id,
            offset: 0,
            result: Ok(found(2, &room).into()),
        };
        crate::app::update::update(&mut app, page(old));
        assert!(app.results.is_none(), "an older search is ignored");

        // Another room opened meanwhile.
        let other = app
            .rooms
            .iter()
            .find(|r| !r.encrypted && r.id != room)
            .expect("another room")
            .id
            .clone();
        app.open_room(other);
        let current = app.search_seq;
        crate::app::update::update(&mut app, page(current));
        assert!(
            app.results.is_none(),
            "an answer for a closed room is ignored"
        );
    }

    #[test]
    fn motions_move_and_q_closes() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 10);
        press(&mut app, "j");
        assert_eq!(app.results.as_ref().map(|l| l.selected), Some(1));
        press(&mut app, "G");
        assert_eq!(app.results.as_ref().map(|l| l.selected), Some(9));
        press(&mut app, "gg");
        assert_eq!(app.results.as_ref().map(|l| l.selected), Some(0));
        press(&mut app, "3j");
        assert_eq!(app.results.as_ref().map(|l| l.selected), Some(3));
        let focus = app.focus;
        press(&mut app, "q");
        assert!(app.results.is_none());
        assert_eq!(app.focus, focus);
        assert_eq!(app.current_room, Some(room));
    }

    #[test]
    fn esc_and_ctrl_c_close() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        press(&mut app, "<Esc>");
        assert!(app.results.is_none());
        open_list(&mut app, &room, 3);
        press(&mut app, "<C-c>");
        assert!(app.results.is_none());
    }

    #[test]
    fn message_actions_do_nothing_while_open() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        let before = app.messages.get(&room).map(Vec::len);
        press(&mut app, "ddixyz");
        assert!(app.results.is_some());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.confirm.is_none());
        assert_eq!(app.messages.get(&room).map(Vec::len), before);
    }

    #[test]
    fn colon_still_opens_the_command_line() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        press(&mut app, ":");
        assert_eq!(app.mode, Mode::Command);
        press(&mut app, "search again<CR>");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.pending_search.is_some());
    }

    #[test]
    fn full_page_loads_the_next_one_once() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, usize::from(PAGE));
        assert!(!app.results.as_ref().expect("list").exhausted);
        app.effects.clear();
        press(&mut app, "G");
        assert_eq!(app.effects.len(), 1);
        match &app.effects[0] {
            Effect::SearchRoom { offset, count, .. } => {
                assert_eq!((*offset, *count), (u32::from(PAGE), PAGE));
            }
            other => panic!("unexpected {other:?}"),
        }
        press(&mut app, "j");
        assert_eq!(
            app.effects.len(),
            1,
            "no second request while one is in flight"
        );

        let id = app.results.as_ref().map(|l| l.id).expect("list");
        crate::app::update::update(
            &mut app,
            Action::SearchPage {
                room: room.clone(),
                search_id: id,
                offset: u32::from(PAGE),
                result: Ok(found(4, &room).into()),
            },
        );
        let list = app.results.as_ref().expect("list");
        assert_eq!(list.messages.len(), usize::from(PAGE) + 4);
        assert!(list.exhausted, "a short page is the end");
        app.effects.clear();
        press(&mut app, "G");
        assert!(app.effects.is_empty(), "no request after the end");
    }

    #[test]
    fn failed_next_page_keeps_the_results() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, usize::from(PAGE));
        press(&mut app, "G");
        let id = app.results.as_ref().map(|l| l.id).expect("list");
        crate::app::update::update(
            &mut app,
            Action::SearchPage {
                room,
                search_id: id,
                offset: u32::from(PAGE),
                result: Err(ApiError::Unreachable),
            },
        );
        let list = app.results.as_ref().expect("list stays");
        assert_eq!(list.messages.len(), usize::from(PAGE));
        assert!(!list.loading_more);
        assert_eq!(status(&app), ApiError::Unreachable.to_string());
    }

    #[test]
    fn opening_another_room_closes_the_list() {
        let mut app = app_80x24();
        let room = open_plain_room(&mut app);
        open_list(&mut app, &room, 3);
        let other = app
            .rooms
            .iter()
            .find(|r| r.id != room)
            .expect("another room")
            .id
            .clone();
        app.open_room(other);
        assert!(app.results.is_none());
    }

    #[test]
    fn debug_never_prints_the_query_or_results() {
        let room = RoomId("r1".to_string());
        let effect = Effect::SearchRoom {
            room: room.clone(),
            query: SendText::new("SECRETQUERY".to_string()),
            search_id: 1,
            offset: 0,
            count: PAGE,
        };
        assert!(!format!("{effect:?}").contains("SECRETQUERY"));
        let mut msgs = found(1, &room);
        msgs[0].body = "SECRETBODY".to_string();
        let action = Action::SearchPage {
            room,
            search_id: 1,
            offset: 0,
            result: Ok(msgs.into()),
        };
        assert!(!format!("{action:?}").contains("SECRETBODY"));
    }
}
