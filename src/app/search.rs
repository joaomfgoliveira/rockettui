//! Find in the open room (`/`, `n`, `N`): the pure matcher and the search state.
//!
//! The state is the query plus a `seeking` flag, never a list of matches: matches are found by
//! running the matcher over the room's messages when they are needed, so live messages, edits,
//! deletes and older pages can never leave a stale list behind. Neither `Search` nor the prompt
//! text has a `Debug` impl: they hold what the user typed.

use crate::api::map::ENCRYPTED_TEXT;
use crate::app::messages::{self, MsgView};
use crate::app::{App, Focus, MAX_CMDLINE, Mode, StatusMsg, history};
use crate::data::{Message, MessageKind, RoomId};
use crate::input::key::Chord;
use crate::ui::sanitize::{sanitize, sanitize_line};
use crossterm::event::{KeyCode, KeyModifiers};
use std::ops::Range;

/// An active search in the open room.
pub struct Search {
    pub query: String,
    /// True while the search waits for a history page it asked for.
    pub seeking: bool,
    /// The direction of that wait: newer pages (`N` in a detached room) instead of older ones.
    /// Only meaningful while `seeking` is true.
    pub seeking_newer: bool,
}

/// The needle for `query` and whether the comparison ignores case: smart case, so a query with an
/// uppercase letter is case-sensitive.
fn needle(query: &str) -> (String, bool) {
    if query.chars().any(char::is_uppercase) {
        (query.to_string(), false)
    } else {
        (query.to_lowercase(), true)
    }
}

/// True when the sanitized `text` holds `query` as plain text (smart case). An empty query matches
/// nothing.
pub fn matches(text: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let (needle, fold) = needle(query);
    let clean = sanitize(text);
    if fold {
        clean.to_lowercase().contains(&needle)
    } else {
        clean.contains(&needle)
    }
}

/// The byte ranges of every occurrence of `query` in the sanitized `text`, for highlighting.
/// Ranges index the sanitized text. When lowercasing moves a character boundary (a few Unicode
/// characters change length), the ranges cannot be trusted and none are returned: the text still
/// matches but is not highlighted, rather than being sliced in the middle of a character.
pub fn find(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let (needle, fold) = needle(query);
    let clean = sanitize(text);
    let ranges: Vec<Range<usize>> = if fold {
        let lower = clean.to_lowercase();
        if lower.len() != clean.len() {
            return Vec::new();
        }
        lower
            .match_indices(&needle)
            .map(|(i, m)| i..i + m.len())
            .collect()
    } else {
        clean
            .match_indices(&needle)
            .map(|(i, m)| i..i + m.len())
            .collect()
    };
    if ranges
        .iter()
        .all(|r| clean.is_char_boundary(r.start) && clean.is_char_boundary(r.end))
    {
        ranges
    } else {
        Vec::new()
    }
}

/// True when the text of `message` is drawn as searchable body text: not a deleted line and not
/// the encrypted placeholder.
pub fn highlightable(message: &Message) -> bool {
    message.kind != MessageKind::Deleted && message.body != ENCRYPTED_TEXT
}

/// True when `message` can match: a deleted line and the encrypted placeholder never do.
pub fn message_matches(message: &Message, query: &str) -> bool {
    highlightable(message) && matches(&message.body, query)
}

const NOT_FOUND: &str = "Not found";
const NO_SEARCH: &str = "No search";
const NO_OLDER: &str = "No older match";
const NO_NEWER: &str = "No newer match";

fn info(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Info(text.to_string()));
}

/// The index of the nearest matching message at or before `from`, scanning toward older ones.
fn scan_older(msgs: &[Message], from: usize, query: &str) -> Option<usize> {
    let last = from.min(msgs.len().checked_sub(1)?);
    (0..=last).rev().find(|&i| message_matches(&msgs[i], query))
}

/// The index of the nearest matching message at or after `from`, scanning toward newer ones.
fn scan_newer(msgs: &[Message], from: usize, query: &str) -> Option<usize> {
    (from..msgs.len()).find(|&i| message_matches(&msgs[i], query))
}

/// Put the cursor of `room` on message `index` and scroll it into view.
fn jump(app: &mut App, room: &RoomId, index: usize) {
    let top_row = messages::view(app, room).top_row;
    app.msg_views.insert(
        room.clone(),
        MsgView {
            cursor: index,
            top_row,
        },
    );
    let view = messages::view(app, room);
    app.msg_views.insert(room.clone(), view);
}

/// `/`: open the prompt. Only with the messages pane focused in Normal mode and a room open;
/// anywhere else it does nothing.
pub fn start(app: &mut App) {
    if app.mode != Mode::Normal || app.focus != Focus::Messages || app.current_room.is_none() {
        return;
    }
    app.matcher.reset();
    app.search_prompt = Some(String::new());
}

/// A key while the prompt is open: it never reaches the keymap.
pub fn prompt_key(app: &mut App, chord: Chord) {
    let Some(prompt) = app.search_prompt.as_mut() else {
        return;
    };
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    if chord.code == KeyCode::Esc || ctrl('c') {
        app.search_prompt = None;
    } else if chord.code == KeyCode::Enter {
        let query = app.search_prompt.take().unwrap_or_default();
        run(app, query);
    } else if chord.code == KeyCode::Backspace {
        if prompt.pop().is_none() {
            app.search_prompt = None;
        }
    } else if ctrl('u') {
        prompt.clear();
    } else if let KeyCode::Char(c) = chord.code
        && chord.mods.is_empty()
        && prompt.chars().count() < MAX_CMDLINE
    {
        prompt.push(c);
    }
}

/// A paste while the prompt is open: one line, capped, never a run.
pub fn prompt_paste(app: &mut App, text: &str) {
    if let Some(prompt) = app.search_prompt.as_mut() {
        let room = MAX_CMDLINE.saturating_sub(prompt.chars().count());
        prompt.extend(sanitize_line(text).chars().take(room));
    }
}

/// `Enter` on the prompt: make `query` the active search and go to the nearest match at or older
/// than the cursor. An empty query only closes the prompt and keeps the earlier search.
fn run(app: &mut App, query: String) {
    let Some(room) = app.current_room.clone() else {
        return;
    };
    if query.is_empty() {
        return;
    }
    app.search = Some(Search {
        query: query.clone(),
        seeking: false,
        seeking_newer: false,
    });
    let cursor = messages::view(app, &room).cursor;
    let found = app
        .messages
        .get(&room)
        .and_then(|msgs| scan_older(msgs, cursor, &query));
    match found {
        Some(i) => jump(app, &room, i),
        None => seek_older(app, &room, NOT_FOUND),
    }
}

/// Nothing older is held: ask for one older page and wait for it (`on_page`), or say the search
/// is over when the room has no more history or its first page is not here.
fn seek_older(app: &mut App, room: &RoomId, none: &str) {
    let exhausted_or_unloaded = !matches!(
        app.history.get(room),
        Some(history::HistoryState::Loaded {
            exhausted: false,
            ..
        })
    );
    if exhausted_or_unloaded {
        stop_seeking(app);
        info(app, none);
        return;
    }
    if let Some(search) = app.search.as_mut() {
        search.seeking = true;
        search.seeking_newer = false;
    }
    history::request_older(app, room);
}

/// Nothing newer is loaded: in a detached room ask for the next newer page and wait for it
/// (`on_newer_page`); in any other room the loaded messages already end at the newest.
fn seek_newer(app: &mut App, room: &RoomId) {
    if !crate::app::jump::is_detached(app, room) {
        info(app, NO_NEWER);
        return;
    }
    if let Some(search) = app.search.as_mut() {
        search.seeking = true;
        search.seeking_newer = true;
    }
    crate::app::jump::request_newer(app, room);
}

fn stop_seeking(app: &mut App) {
    if let Some(search) = app.search.as_mut() {
        search.seeking = false;
    }
}

/// `n`: the next match toward older messages. No wrap.
pub fn next(app: &mut App) {
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let Some(search) = app.search.as_ref() else {
        info(app, NO_SEARCH);
        return;
    };
    if search.seeking {
        return;
    }
    let query = search.query.clone();
    let cursor = messages::view(app, &room).cursor;
    let found = cursor.checked_sub(1).and_then(|from| {
        app.messages
            .get(&room)
            .and_then(|msgs| scan_older(msgs, from, &query))
    });
    match found {
        Some(i) => jump(app, &room, i),
        None => seek_older(app, &room, NO_OLDER),
    }
}

/// `N`: the previous match, toward newer messages. No wrap.
pub fn prev(app: &mut App) {
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let Some(search) = app.search.as_ref() else {
        info(app, NO_SEARCH);
        return;
    };
    if search.seeking {
        return;
    }
    let query = search.query.clone();
    let cursor = messages::view(app, &room).cursor;
    let found = app
        .messages
        .get(&room)
        .and_then(|msgs| scan_newer(msgs, cursor + 1, &query));
    match found {
        Some(i) => jump(app, &room, i),
        None => seek_newer(app, &room),
    }
}

/// An older page was merged into `room`. If a search is waiting for it, look again: jump on a
/// match, ask for the next page when there is none, and stop when history is exhausted. A page
/// for a room that is no longer open, or with no search waiting, changes nothing.
pub fn on_page(app: &mut App, room: &RoomId) {
    let Some(search) = app.search.as_ref() else {
        return;
    };
    if !search.seeking || search.seeking_newer || app.current_room.as_ref() != Some(room) {
        return;
    }
    let query = search.query.clone();
    let cursor = messages::view(app, room).cursor;
    let found = cursor.checked_sub(1).and_then(|from| {
        app.messages
            .get(room)
            .and_then(|msgs| scan_older(msgs, from, &query))
    });
    match found {
        Some(i) => {
            stop_seeking(app);
            jump(app, room, i);
        }
        None => seek_older(app, room, NOT_FOUND),
    }
}

/// A newer page was applied to `room`. If a search is waiting toward newer, look again after the
/// cursor: jump on a match, ask for the next page while the room is still detached, and stop with
/// "No newer match" once the newest message was reached. The cursor never moves without a match.
pub fn on_newer_page(app: &mut App, room: &RoomId) {
    let Some(search) = app.search.as_ref() else {
        return;
    };
    if !search.seeking || !search.seeking_newer || app.current_room.as_ref() != Some(room) {
        return;
    }
    let query = search.query.clone();
    let cursor = messages::view(app, room).cursor;
    let found = app
        .messages
        .get(room)
        .and_then(|msgs| scan_newer(msgs, cursor + 1, &query));
    match found {
        Some(i) => {
            stop_seeking(app);
            jump(app, room, i);
        }
        None if crate::app::jump::is_detached(app, room) => {
            crate::app::jump::request_newer(app, room);
        }
        None => {
            stop_seeking(app);
            info(app, NO_NEWER);
        }
    }
}

/// A page failed: a waiting search stops, and the history error stays on the status line.
pub fn on_failed(app: &mut App, room: &RoomId) {
    if app.current_room.as_ref() == Some(room) {
        stop_seeking(app);
    }
}

/// Forget the search and the prompt (`Esc` in Normal mode, opening another room).
pub fn clear(app: &mut App) {
    app.search = None;
    app.search_prompt = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_query_ignores_case() {
        assert!(matches("Hello there", "hello"));
        assert!(matches("HELLO", "hello"));
    }

    #[test]
    fn uppercase_query_is_case_sensitive() {
        assert!(!matches("hello there", "Hello"));
        assert!(matches("say Hello", "Hello"));
    }

    #[test]
    fn query_is_plain_text() {
        assert!(!matches("abc", "a.c"));
        assert!(matches("a.c", "a.c"));
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(!matches("anything", ""));
        assert!(find("anything", "").is_empty());
    }

    #[test]
    fn find_returns_every_occurrence() {
        assert_eq!(find("abcabc ABC", "abc"), vec![0..3, 3..6, 7..10]);
        assert_eq!(find("abc ABC", "ABC"), vec![4..7]);
    }

    #[test]
    fn unicode_never_slices_a_character() {
        let body = "İstanbul ÄÖÜ ß ǅ 日本語";
        for q in ["i", "İ", "ä", "ß", "ǆ", "本"] {
            for r in find(body, q) {
                assert!(body.is_char_boundary(r.start) && body.is_char_boundary(r.end));
            }
        }
        // A character whose lowercase is longer: still a match, no highlight, no panic.
        assert!(matches("İ", "i\u{307}"));
        assert!(find("İ", "i\u{307}").is_empty());
    }

    #[test]
    fn matches_what_is_drawn() {
        assert!(matches("a\u{1b}b", "ab"));
    }

    fn message(body: &str, kind: MessageKind) -> Message {
        Message {
            id: "m".into(),
            room_id: crate::data::RoomId("r".into()),
            author: "a".into(),
            ts: 0,
            body: body.into(),
            kind,
            send: crate::data::SendState::Sent,
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            reactions: Vec::new(),
        }
    }

    #[test]
    fn deleted_and_encrypted_never_match() {
        assert!(message_matches(
            &message("hello", MessageKind::Normal),
            "hello"
        ));
        assert!(!message_matches(
            &message("hello", MessageKind::Deleted),
            "hello"
        ));
        assert!(!message_matches(
            &message(ENCRYPTED_TEXT, MessageKind::Normal),
            "encrypted"
        ));
    }

    /// `Search` and `App` hold what the user typed, so neither may ever gain a `Debug` impl. An
    /// inherent const wins over the trait const only when the type is `Debug`.
    #[test]
    fn search_and_app_are_not_debug() {
        use std::marker::PhantomData;
        trait Fallback {
            const IS_DEBUG: bool = false;
        }
        impl<T> Fallback for T {}
        struct Probe<T>(PhantomData<T>);
        #[allow(dead_code)]
        impl<T: std::fmt::Debug> Probe<T> {
            const IS_DEBUG: bool = true;
        }
        const { assert!(Probe::<String>::IS_DEBUG, "the probe detects Debug") };
        const { assert!(!Probe::<Search>::IS_DEBUG) };
        const { assert!(!Probe::<crate::app::App>::IS_DEBUG) };
    }
}

#[cfg(test)]
mod reducer_tests {
    use super::*;
    use crate::action::Action;
    use crate::app::history::HistoryState;
    use crate::app::update::update;
    use crate::data::SendState;
    use crate::effect::Effect;
    use crate::test_util::{app_80x24, press};

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn msg(id: &str, body: &str, ts: i64) -> Message {
        Message {
            id: id.to_string(),
            room_id: general(),
            author: "alice".to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            reactions: Vec::new(),
        }
    }

    /// `general` open with these bodies (oldest first), cursor on the newest, history Loaded.
    fn app_with(bodies: &[&str], exhausted: bool) -> App {
        let mut app = app_80x24();
        app.open_room(general());
        let msgs = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| msg(&format!("m{i}"), b, 1000 + i as i64 * 60))
            .collect();
        app.messages.insert(general(), msgs);
        app.msg_views.remove(&general());
        app.history.insert(
            general(),
            HistoryState::Loaded {
                oldest_ts: Some("t0".to_string()),
                exhausted,
                loading_older: false,
            },
        );
        app.effects.clear();
        app
    }

    fn cursor(app: &App) -> usize {
        messages::view(app, &general()).cursor
    }

    fn status(app: &App) -> String {
        match &app.status_msg {
            Some(StatusMsg::Info(t)) | Some(StatusMsg::Error(t)) => t.clone(),
            None => String::new(),
        }
    }

    fn seeking(app: &App) -> bool {
        app.search.as_ref().is_some_and(|s| s.seeking)
    }

    fn older_requests(app: &App) -> usize {
        app.effects
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Effect::LoadHistory {
                        before: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    fn page(app: &mut App, bodies: &[&str], older_than: usize) {
        let messages = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| {
                msg(
                    &format!("p{older_than}-{i}"),
                    b,
                    500 - older_than as i64 * 100 + i as i64,
                )
            })
            .collect();
        update(
            app,
            Action::HistoryLoaded {
                room: general(),
                messages,
                oldest_ts: Some(format!("t{older_than}")),
                older: true,
            },
        );
    }

    #[test]
    fn prompt_opens_without_changing_mode_or_focus() {
        let mut app = app_with(&["a", "b"], true);
        press(&mut app, "/");
        assert_eq!(app.search_prompt.as_deref(), Some(""));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn typing_is_text_not_commands() {
        let mut app = app_with(&["a", "b", "c"], true);
        press(&mut app, "/jk");
        assert_eq!(app.search_prompt.as_deref(), Some("jk"));
        assert_eq!(cursor(&app), 2);
        press(&mut app, "<BS>");
        assert_eq!(app.search_prompt.as_deref(), Some("j"));
        press(&mut app, "<C-u>");
        assert_eq!(app.search_prompt.as_deref(), Some(""));
    }

    #[test]
    fn paste_joins_lines_and_is_capped() {
        let mut app = app_with(&["a"], true);
        press(&mut app, "/");
        update(&mut app, Action::Paste("one\ntwo".to_string()));
        assert_eq!(app.search_prompt.as_deref(), Some("one two"));
        update(&mut app, Action::Paste("x".repeat(MAX_CMDLINE * 2)));
        assert_eq!(
            app.search_prompt.as_ref().map(|p| p.chars().count()),
            Some(MAX_CMDLINE)
        );
        assert!(app.search.is_none(), "a paste never runs the search");
    }

    #[test]
    fn cancel_and_empty_backspace_close() {
        for keys in ["/ab<Esc>", "/ab<C-c>", "/<BS>"] {
            let mut app = app_with(&["ab"], true);
            press(&mut app, keys);
            assert!(app.search_prompt.is_none(), "{keys}");
            assert!(app.search.is_none(), "{keys}");
            assert_eq!(app.mode, Mode::Normal);
        }
    }

    #[test]
    fn not_available_elsewhere() {
        let mut app = app_with(&["a"], true);
        app.focus = Focus::Sidebar;
        press(&mut app, "/");
        assert!(app.search_prompt.is_none());
        app.focus = Focus::Input;
        press(&mut app, "/");
        assert!(app.search_prompt.is_none());
        let mut app = app_80x24();
        app.focus = Focus::Messages;
        press(&mut app, "/");
        assert!(app.search_prompt.is_none(), "no room open");
    }

    #[test]
    fn enter_moves_to_nearest_match_at_or_older() {
        let mut app = app_with(&["x", "foo", "x", "x", "foo", "x"], true);
        press(&mut app, "/foo<CR>");
        assert!(app.search_prompt.is_none());
        assert_eq!(cursor(&app), 4);
        // Cursor already on a match: it stays.
        press(&mut app, "/foo<CR>");
        assert_eq!(cursor(&app), 4);
    }

    #[test]
    fn no_match_keeps_cursor_and_says_so() {
        let mut app = app_with(&["x", "y"], true);
        press(&mut app, "/zzz<CR>");
        assert_eq!(cursor(&app), 1);
        assert_eq!(status(&app), NOT_FOUND);
        assert!(!seeking(&app));
    }

    #[test]
    fn new_search_replaces_old_and_empty_keeps_it() {
        let mut app = app_with(&["aa", "bb", "cc"], true);
        press(&mut app, "/aa<CR>");
        press(&mut app, "/bb<CR>");
        assert_eq!(app.search.as_ref().map(|s| s.query.as_str()), Some("bb"));
        press(&mut app, "/<CR>");
        assert!(app.search_prompt.is_none());
        assert_eq!(app.search.as_ref().map(|s| s.query.as_str()), Some("bb"));
    }

    #[test]
    fn n_goes_older_and_n_upper_goes_newer() {
        let mut app = app_with(&["foo", "x", "foo", "x", "foo", "x"], true);
        press(&mut app, "/foo<CR>");
        assert_eq!(cursor(&app), 4);
        press(&mut app, "n");
        assert_eq!(cursor(&app), 2);
        press(&mut app, "n");
        assert_eq!(cursor(&app), 0);
        press(&mut app, "N");
        assert_eq!(cursor(&app), 2);
        press(&mut app, "2N");
        assert_eq!(cursor(&app), 4, "the second N has no newer match");
        assert_eq!(status(&app), NO_NEWER);
        press(&mut app, "n");
        assert_eq!(status(&app), "", "a key press clears the old status");
        press(&mut app, "N");
        press(&mut app, "N");
        assert_eq!(cursor(&app), 4);
        assert_eq!(status(&app), NO_NEWER);
        press(&mut app, "2n");
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn n_at_the_oldest_match_of_an_exhausted_room_stops() {
        let mut app = app_with(&["foo", "x"], true);
        press(&mut app, "/foo<CR>");
        press(&mut app, "n");
        assert_eq!(cursor(&app), 0);
        assert_eq!(status(&app), NO_OLDER);
        assert!(app.effects.is_empty());
    }

    #[test]
    fn n_without_a_search_reports_it() {
        let mut app = app_with(&["foo"], true);
        press(&mut app, "n");
        assert_eq!(status(&app), NO_SEARCH);
        press(&mut app, "N");
        assert_eq!(status(&app), NO_SEARCH);
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn a_search_with_nothing_older_asks_for_one_page() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        assert!(seeking(&app));
        assert_eq!(older_requests(&app), 1);
        // Keys while waiting do not queue more requests.
        press(&mut app, "n");
        assert_eq!(older_requests(&app), 1);
    }

    #[test]
    fn match_in_the_first_older_page() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        page(&mut app, &["a", "foo", "b"], 1);
        assert!(!seeking(&app));
        assert_eq!(cursor(&app), 1, "the matching message of the page");
        assert_eq!(messages_len(&app), 5);
    }

    #[test]
    fn match_only_in_the_second_page() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        page(&mut app, &["a", "b"], 1);
        assert!(seeking(&app));
        assert_eq!(older_requests(&app), 2, "one more page was asked for");
        page(&mut app, &["foo"], 2);
        assert!(!seeking(&app));
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn end_of_history_stops_without_moving() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        let before = app.effects.len();
        page(&mut app, &[], 1);
        assert!(!seeking(&app));
        assert_eq!(app.effects.len(), before, "no further request");
        assert_eq!(status(&app), NOT_FOUND);
        assert_eq!(cursor(&app), 1);
    }

    #[test]
    fn failed_page_stops_the_search() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        update(
            &mut app,
            Action::HistoryFailed {
                room: general(),
                older: true,
                error: crate::api::ApiError::Unreachable,
            },
        );
        assert!(!seeking(&app));
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
        assert_eq!(older_requests(&app), 1);
    }

    #[test]
    fn page_in_flight_from_the_cursor_is_reused() {
        let mut app = app_with(&["x", "y"], false);
        history::request_older(&mut app, &general());
        assert_eq!(older_requests(&app), 1);
        press(&mut app, "/foo<CR>");
        assert!(seeking(&app));
        assert_eq!(older_requests(&app), 1, "no second request");
        page(&mut app, &["foo"], 1);
        assert!(!seeking(&app));
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn clearing_or_switching_room_abandons_the_wait() {
        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        press(&mut app, "<Esc>");
        assert!(app.search.is_none());
        let before = app.effects.len();
        page(&mut app, &["a"], 1);
        assert_eq!(app.effects.len(), before);

        let mut app = app_with(&["x", "y"], false);
        press(&mut app, "/foo<CR>");
        app.open_room(RoomId("random".to_string()));
        assert!(app.search.is_none());
        let before = app.effects.len();
        page(&mut app, &["a"], 1);
        assert_eq!(app.effects.len(), before);
    }

    #[test]
    fn mode_badge_never_changes() {
        let mut app = app_with(&["foo"], true);
        press(&mut app, "/fo");
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, "o<CR>n");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn esc_in_normal_mode_clears_the_search() {
        let mut app = app_with(&["foo", "x"], true);
        press(&mut app, "/foo<CR>");
        assert!(app.search.is_some());
        press(&mut app, "<Esc>");
        assert!(app.search.is_none());
        press(&mut app, "n");
        assert_eq!(status(&app), NO_SEARCH);
    }

    #[test]
    fn a_live_matching_message_keeps_the_search_and_the_cursor() {
        // Enough messages that the oldest one is far above the bottom of the window.
        let mut bodies = vec!["foo"];
        bodies.extend(std::iter::repeat_n("x", 49));
        let mut app = app_with(&bodies, true);
        press(&mut app, "/foo<CR>");
        assert_eq!(cursor(&app), 0);
        update(
            &mut app,
            Action::RtMessage(msg("live", "another foo", 5000)),
        );
        assert_eq!(messages_len(&app), 51);
        assert_eq!(cursor(&app), 0, "the cursor message did not change");
        assert!(app.search.is_some());
        // The new message is a match too: N finds it from the oldest one.
        press(&mut app, "N");
        assert_eq!(cursor(&app), 50);
    }

    fn messages_len(app: &App) -> usize {
        app.messages[&general()].len()
    }
}
