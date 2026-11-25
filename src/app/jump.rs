//! From a search result to its conversation: the context view and the jump into the room.
//!
//! `Enter` on a result in the results list asks for the history around it (`Effect::LoadContext`)
//! and, when it arrives, shows it as a read-only window over the list (`ContextView`, owned by
//! `ResultsState` so closing the list drops it and any request in flight). `Enter` again in the
//! window jumps into the real chat: the window's messages become the room's loaded messages and the
//! room is marked **detached** from the newest messages. While detached, live messages are not
//! appended (they would sit after a gap), sending, editing and deleting are refused, older pages
//! load as usual, and `G` on the last loaded message, or opening another room, returns to the
//! newest by dropping the loaded messages. No `Debug`: the view holds message text.

use crate::action::Action;
use crate::api::{ApiError, ContextWindow, NewerPage};
use crate::app::history::{self, HistoryState};
use crate::app::messages::{self, MsgView};
use crate::app::{App, Focus, StatusMsg};
use crate::data::{Message, RoomId};
use crate::effect::Effect;

/// Messages asked for before and after the result.
pub const BEFORE: u16 = 25;
pub const AFTER: u16 = 25;

const NOT_OPENABLE: &str = "This result cannot be opened";
const GONE: &str = "That message was not found on the server";
const UNSENT: &str = "Resolve the unsent message first (r to retry, dd to discard)";
const OLDER: &str =
    "Showing older messages: scroll down to read on, or G on the last message for the newest";
const REACHED: &str = "Reached the newest messages";
/// Messages one forward page asks for.
pub const NEWER_PAGE: u16 = 50;

/// A room showing a window of older history (see the module doc).
pub struct Detached {
    /// The raw timestamp of the newest loaded message: the `oldest` of the next forward page.
    pub newest_raw: Option<String>,
    /// The forward request in flight, by `App.context_seq`; an answer with another id is dropped.
    pub pending_newer: Option<u64>,
}

/// The read-only window around one search result, oldest first.
pub struct ContextView {
    pub messages: Vec<Message>,
    pub selected: usize,
    pub top_row: usize,
    /// The raw timestamp of the newest message, the `oldest` of the page after the window.
    pub newest_raw: Option<String>,
    /// Nothing newer than the window exists.
    pub reaches_newest: bool,
    /// The raw timestamp of the oldest message, the `latest` of the next older page after a jump.
    pub oldest_raw: Option<String>,
}

fn info(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Info(text.to_string()));
}

fn error(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Error(text.to_string()));
}

/// `Enter` on the selected result of the list: ask for its context. A result with no raw
/// timestamp cannot be opened. Opening another result replaces the pending request.
pub fn open_selected(app: &mut App) {
    let Some(list) = app.results.as_ref() else {
        return;
    };
    let selected = list.selected.min(list.messages.len().saturating_sub(1));
    let (Some(message), Some(Some(raw_ts))) =
        (list.messages.get(selected), list.raw_ts.get(selected))
    else {
        error(app, NOT_OPENABLE);
        return;
    };
    let Some(room) = app.rooms.iter().find(|r| r.id == list.room) else {
        return;
    };
    app.context_seq += 1;
    let effect = Effect::LoadContext {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        id: message.id.clone(),
        raw_ts: raw_ts.clone(),
        context_id: app.context_seq,
        before: BEFORE,
        after: AFTER,
    };
    let id = app.context_seq;
    if let Some(list) = app.results.as_mut() {
        list.pending_context = Some(id);
    }
    app.effects.push(effect);
}

/// The window arrived. It opens only if the list is still open for that room and this is the
/// request it is waiting for; anything else is dropped.
pub fn on_loaded(app: &mut App, room: &RoomId, context_id: u64, window: ContextWindow) {
    let Some(list) = app.results.as_mut() else {
        return;
    };
    if &list.room != room || list.pending_context != Some(context_id) {
        return;
    }
    list.pending_context = None;
    list.context = Some(ContextView {
        messages: window.messages,
        selected: window.selected,
        top_row: 0,
        newest_raw: window.newest_raw,
        reaches_newest: window.reaches_newest,
        oldest_raw: window.oldest_raw,
    });
}

/// The request failed. Same addressing rule as `on_loaded`; the list stays as it was.
pub fn on_failed(app: &mut App, room: &RoomId, context_id: u64, e: ApiError) {
    let Some(list) = app.results.as_mut() else {
        return;
    };
    if &list.room != room || list.pending_context != Some(context_id) {
        return;
    }
    list.pending_context = None;
    let text = if e == ApiError::NotFound {
        GONE.to_string()
    } else {
        e.to_string()
    };
    error(app, &text);
}

/// The effective selection and scroll of the open context view.
pub fn view(app: &App) -> MsgView {
    let Some(ctx) = app.results.as_ref().and_then(|l| l.context.as_ref()) else {
        return MsgView::default();
    };
    let (width, height) = messages::viewport(app);
    let rows = app.row_ctx(&ctx.messages);
    let starts = messages::row_starts_of(&ctx.messages, width, &rows);
    let selected = ctx.selected.min(ctx.messages.len().saturating_sub(1));
    MsgView {
        cursor: selected,
        top_row: messages::scroll_into_view(&starts, selected, ctx.top_row, usize::from(height)),
    }
}

/// An action while the context view is open: the motions move the selection (no request), select
/// jumps into the chat, everything else does nothing.
pub fn dispatch(app: &mut App, action: &Action, count: Option<u32>) {
    if *action == Action::Select {
        jump_to_chat(app);
        return;
    }
    let (width, height) = messages::viewport(app);
    let Some(ctx) = app.results.as_ref().and_then(|l| l.context.as_ref()) else {
        return;
    };
    let rows = app.row_ctx(&ctx.messages);
    let starts = messages::row_starts_of(&ctx.messages, width, &rows);
    let current = view(app);
    if let Some(next) = messages::step(&starts, current, usize::from(height), action, count)
        && let Some(ctx) = app.results.as_mut().and_then(|l| l.context.as_mut())
    {
        ctx.selected = next.cursor;
        ctx.top_row = next.top_row;
    }
}

/// Make the window the room's loaded messages and detach the room from the newest. Refused while
/// the room holds an unsent message, which the replacement would lose.
fn jump_to_chat(app: &mut App) {
    let Some(list) = app.results.as_ref() else {
        return;
    };
    let room = list.room.clone();
    if app
        .messages
        .get(&room)
        .is_some_and(|held| held.iter().any(|m| m.send.is_unsent()))
    {
        error(app, UNSENT);
        return;
    }
    let selected = view(app).cursor;
    let Some(ctx) = app.results.as_mut().and_then(|l| l.context.take()) else {
        return;
    };
    let (newest_raw, reaches_newest) = (ctx.newest_raw, ctx.reaches_newest);
    app.messages.insert(room.clone(), ctx.messages);
    app.history.insert(
        room.clone(),
        HistoryState::Loaded {
            oldest_ts: ctx.oldest_raw,
            exhausted: false,
            loading_older: false,
        },
    );
    if !reaches_newest {
        app.detached.insert(
            room.clone(),
            Detached {
                newest_raw,
                pending_newer: None,
            },
        );
    }
    crate::app::search::clear(app);
    crate::app::results::close_all(app);
    app.focus = Focus::Messages;
    let (_, height) = messages::viewport(app);
    let starts = messages::row_starts(app, &room);
    let top_row = messages::scroll_into_view(&starts, selected, 0, usize::from(height));
    app.msg_views.insert(
        room.clone(),
        MsgView {
            cursor: selected,
            top_row,
        },
    );
    if reaches_newest {
        // The window ends at the newest: the room is live again, and the merge by id closes the
        // gap of messages that arrived while the list was open.
        history::refetch_newest(app, &room);
    } else {
        info(app, OLDER);
    }
}

/// True while `room` shows a window of older history instead of its newest messages.
pub fn is_detached(app: &App, room: &RoomId) -> bool {
    app.detached.contains_key(room)
}

/// Refuse a send, edit or delete in a detached room: sets the status line and returns true.
pub fn refuse_if_detached(app: &mut App) -> bool {
    let detached = app
        .current_room
        .as_ref()
        .is_some_and(|room| is_detached(app, room));
    if detached {
        error(
            app,
            "Showing older messages: press G on the last message to return before writing",
        );
    }
    detached
}

/// End the detached state of `room`: drop its loaded messages, history state and view, so the next
/// open loads the newest page. Does nothing for a room that is not detached.
pub fn reset(app: &mut App, room: &RoomId) {
    if app.detached.remove(room).is_some() {
        app.messages.remove(room);
        app.history.remove(room);
        app.msg_views.remove(room);
    }
}

/// After a messages-pane motion: when the cursor is on the last loaded message of a detached room
/// and no forward page is in flight, ask for the next page of newer messages. A page that failed is
/// asked for again by moving onto the last message again.
pub fn maybe_load_newer(app: &mut App) {
    let Some(room_id) = app.current_room.clone() else {
        return;
    };
    let len = app.messages.get(&room_id).map_or(0, Vec::len);
    if len == 0 || messages::view(app, &room_id).cursor + 1 < len {
        return;
    }
    request_newer(app, &room_id);
}

/// Record one request for the page of newer messages after the newest loaded message of the
/// detached room `room_id`, unless the room is not detached or a page is already in flight. Shared
/// by the cursor trigger (`maybe_load_newer`) and the search toward newer (`app::search`).
pub fn request_newer(app: &mut App, room_id: &RoomId) {
    let Some(detached) = app.detached.get(room_id) else {
        return;
    };
    let Some(raw_ts) = detached
        .newest_raw
        .clone()
        .filter(|_| detached.pending_newer.is_none())
    else {
        return;
    };
    let Some(room) = app.rooms.iter().find(|r| &r.id == room_id) else {
        return;
    };
    app.context_seq += 1;
    let newer_id = app.context_seq;
    let effect = Effect::LoadNewer {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        raw_ts,
        n: NEWER_PAGE,
        newer_id,
    };
    if let Some(detached) = app.detached.get_mut(room_id) {
        detached.pending_newer = Some(newer_id);
    }
    app.effects.push(effect);
}

/// A forward page arrived. It is applied only if the room is still detached and this is the
/// request it waits for. Held ids are skipped, the rest are appended (an append shifts no index, so
/// the cursor and the scroll stay). A page that reaches the newest ends the detached state.
pub fn on_newer_loaded(app: &mut App, room: &RoomId, newer_id: u64, page: NewerPage) {
    let Some(detached) = app.detached.get_mut(room) else {
        return;
    };
    if detached.pending_newer != Some(newer_id) {
        return;
    }
    detached.pending_newer = None;
    if let Some(raw) = page.newest_raw {
        detached.newest_raw = Some(raw);
    }
    // An empty page cannot be followed by another: treat it as the end.
    let reached = page.reached_newest || page.messages.is_empty();
    let held = app.messages.entry(room.clone()).or_default();
    for message in page.messages {
        if !held.iter().any(|m| m.id == message.id) {
            held.push(message);
        }
    }
    if reached {
        app.detached.remove(room);
        info(app, REACHED);
        history::refetch_newest(app, room);
    }
    // A search waiting toward newer looks at the new messages (after the status above, so its
    // own "No newer match" is what stays on the line when the newest was reached without one).
    crate::app::search::on_newer_page(app, room);
}

/// A forward page failed: the room stays detached and shows the error.
pub fn on_newer_failed(app: &mut App, room: &RoomId, newer_id: u64, e: ApiError) {
    let Some(detached) = app.detached.get_mut(room) else {
        return;
    };
    if detached.pending_newer != Some(newer_id) {
        return;
    }
    detached.pending_newer = None;
    error(app, &e.to_string());
    crate::app::search::on_failed(app, room);
}

/// `G` in a detached room with the cursor already on the last loaded message: return to the newest
/// messages by loading the room again. Returns true when it did.
pub fn return_if_at_end(app: &mut App, motion: &Action, count: Option<u32>) -> bool {
    if *motion != Action::GotoBottom || count.is_some() {
        return false;
    }
    let Some(room) = app.current_room.clone() else {
        return false;
    };
    if !is_detached(app, &room) {
        return false;
    }
    let len = app.messages.get(&room).map_or(0, Vec::len);
    if messages::view(app, &room).cursor + 1 < len {
        return false;
    }
    reset(app, &room);
    history::on_open(app, &room);
    app.status_msg = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SearchHits;
    use crate::app::update::update;
    use crate::data::{MessageKind, SendState};
    use crate::test_util::{app_80x24, press};

    fn msg(room: &RoomId, n: usize, body: &str) -> Message {
        Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            reactions: Vec::new(),
            id: format!("c{n}"),
            room_id: room.clone(),
            author: "alice".to_string(),
            ts: 1_700_000_000 + n as i64 * 60,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    /// An app with a plain room open and a results list of `n` openable results.
    fn with_list(n: usize) -> (App, RoomId) {
        let mut app = app_80x24();
        let room = app
            .rooms
            .iter()
            .find(|r| !r.encrypted)
            .expect("room")
            .id
            .clone();
        app.open_room(room.clone());
        crate::app::command::execute(&mut app, "search deploy");
        let found: Vec<Message> = (0..n).map(|i| msg(&room, 1000 - i, "hit")).collect();
        let raw_ts = (0..n).map(|i| Some(format!("raw{i}"))).collect();
        let id = app.search_seq;
        update(
            &mut app,
            Action::SearchPage {
                room: room.clone(),
                search_id: id,
                offset: 0,
                result: Ok(SearchHits {
                    messages: found,
                    raw_ts,
                }),
            },
        );
        assert!(app.results.is_some());
        (app, room)
    }

    fn window(room: &RoomId, count: usize, selected: usize, reaches_newest: bool) -> ContextWindow {
        ContextWindow {
            messages: (0..count).map(|i| msg(room, i, "ctx")).collect(),
            selected,
            newest_raw: Some("newest-raw".to_string()),
            reaches_newest,
            oldest_raw: Some("oldest-raw".to_string()),
        }
    }

    fn loaded(app: &mut App, room: &RoomId, w: ContextWindow) {
        let id = app.context_seq;
        update(
            app,
            Action::ContextLoaded {
                room: room.clone(),
                context_id: id,
                window: w,
            },
        );
    }

    fn status(app: &App) -> String {
        match &app.status_msg {
            Some(StatusMsg::Info(t) | StatusMsg::Error(t)) => t.clone(),
            None => String::new(),
        }
    }

    fn open_view(app: &mut App, room: &RoomId, count: usize, selected: usize) {
        press(app, "<CR>");
        loaded(app, room, window(room, count, selected, false));
        assert!(app.results.as_ref().expect("list").context.is_some());
    }

    #[test]
    fn enter_requests_the_selected_result() {
        let (mut app, room) = with_list(3);
        press(&mut app, "j");
        app.effects.clear();
        press(&mut app, "<CR>");
        assert_eq!(
            app.effects,
            [Effect::LoadContext {
                room: room.clone(),
                kind: app.rooms.iter().find(|r| r.id == room).expect("room").kind,
                private: app
                    .rooms
                    .iter()
                    .find(|r| r.id == room)
                    .expect("room")
                    .private,
                id: "c999".to_string(),
                raw_ts: "raw1".to_string(),
                context_id: 1,
                before: BEFORE,
                after: AFTER,
            }]
        );
        assert_eq!(app.results.as_ref().expect("list").pending_context, Some(1));
        // Another result replaces the pending request.
        press(&mut app, "j");
        press(&mut app, "<CR>");
        assert_eq!(app.results.as_ref().expect("list").pending_context, Some(2));
        // The first answer is now stale.
        update(
            &mut app,
            Action::ContextLoaded {
                room: room.clone(),
                context_id: 1,
                window: window(&room, 3, 1, false),
            },
        );
        assert!(app.results.as_ref().expect("list").context.is_none());
    }

    #[test]
    fn a_result_without_a_raw_time_cannot_be_opened() {
        let (mut app, _) = with_list(1);
        app.results.as_mut().expect("list").raw_ts = vec![None];
        app.effects.clear();
        press(&mut app, "<CR>");
        assert!(app.effects.is_empty());
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    #[test]
    fn the_window_opens_with_the_result_selected() {
        let (mut app, room) = with_list(2);
        press(&mut app, "<CR>");
        loaded(&mut app, &room, window(&room, 6, 3, false));
        let ctx = app
            .results
            .as_ref()
            .expect("list")
            .context
            .as_ref()
            .expect("view");
        assert_eq!((ctx.messages.len(), ctx.selected), (6, 3));
        assert_eq!(view(&app).cursor, 3);
        assert_eq!(app.mode, crate::app::Mode::Normal);
    }

    #[test]
    fn late_and_foreign_answers_are_dropped() {
        let (mut app, room) = with_list(2);
        press(&mut app, "<CR>");
        let id = app.context_seq;
        // Another room.
        update(
            &mut app,
            Action::ContextLoaded {
                room: RoomId("other".to_string()),
                context_id: id,
                window: window(&room, 2, 0, false),
            },
        );
        assert!(app.results.as_ref().expect("list").context.is_none());
        // The list closed before the answer.
        press(&mut app, "q");
        assert!(app.results.is_none());
        update(
            &mut app,
            Action::ContextLoaded {
                room: room.clone(),
                context_id: id,
                window: window(&room, 2, 0, false),
            },
        );
        assert!(app.results.is_none());
        update(
            &mut app,
            Action::ContextFailed {
                room,
                context_id: id,
                error: ApiError::Timeout,
            },
        );
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn failures_keep_the_list_and_say_why() {
        let (mut app, room) = with_list(2);
        for (error, want) in [
            (ApiError::NotFound, "not found on the server"),
            (ApiError::Timeout, "timed out"),
        ] {
            press(&mut app, "<CR>");
            let id = app.context_seq;
            update(
                &mut app,
                Action::ContextFailed {
                    room: room.clone(),
                    context_id: id,
                    error,
                },
            );
            assert!(status(&app).contains(want), "{}", status(&app));
            let list = app.results.as_ref().expect("list stays");
            assert!(list.context.is_none() && list.pending_context.is_none());
        }
    }

    #[test]
    fn q_returns_to_the_list_then_closes_it() {
        let (mut app, room) = with_list(3);
        press(&mut app, "j");
        open_view(&mut app, &room, 5, 2);
        press(&mut app, "q");
        let list = app.results.as_ref().expect("list is back");
        assert!(list.context.is_none());
        assert_eq!(list.selected, 1, "same result selected");
        press(&mut app, "q");
        assert!(app.results.is_none());
    }

    #[test]
    fn motions_move_the_selection_without_a_request() {
        let (mut app, room) = with_list(1);
        open_view(&mut app, &room, 6, 2);
        app.effects.clear();
        press(&mut app, "gg");
        assert_eq!(view(&app).cursor, 0);
        press(&mut app, "G");
        assert_eq!(view(&app).cursor, 5);
        press(&mut app, "k");
        assert_eq!(view(&app).cursor, 4);
        assert!(app.effects.is_empty());
    }

    #[test]
    fn message_keys_do_nothing_in_the_view() {
        let (mut app, room) = with_list(1);
        open_view(&mut app, &room, 3, 1);
        let before = app.messages.get(&room).cloned();
        for keys in ["dd", "e", "i", "r"] {
            press(&mut app, keys);
        }
        assert_eq!(app.mode, crate::app::Mode::Normal);
        assert!(app.confirm.is_none() && app.editing.is_none());
        assert_eq!(app.messages.get(&room).cloned(), before);
    }

    #[test]
    fn live_messages_stay_out_of_the_view_but_reach_the_room() {
        let (mut app, room) = with_list(1);
        open_view(&mut app, &room, 4, 1);
        let before = app.messages[&room].len();
        update(&mut app, Action::RtMessage(msg(&room, 9002, "live")));
        let ctx = app
            .results
            .as_ref()
            .expect("list")
            .context
            .as_ref()
            .expect("view");
        assert_eq!(ctx.messages.len(), 4);
        press(&mut app, "q");
        press(&mut app, "q");
        assert_eq!(app.messages[&room].len(), before + 1);
    }

    #[test]
    fn enter_in_the_view_jumps_into_the_chat() {
        let (mut app, room) = with_list(2);
        open_view(&mut app, &room, 6, 3);
        press(&mut app, "<CR>");
        assert!(app.results.is_none(), "list and view are closed");
        assert!(app.detached.contains_key(&room));
        assert_eq!(app.focus, Focus::Messages);
        let held = &app.messages[&room];
        assert_eq!(held.len(), 6);
        assert_eq!(held[3].id, "c3");
        assert_eq!(messages::view(&app, &room).cursor, 3);
        assert_eq!(
            app.history.get(&room),
            Some(&HistoryState::Loaded {
                oldest_ts: Some("oldest-raw".to_string()),
                exhausted: false,
                loading_older: false,
            })
        );
        assert!(status(&app).contains("older messages"));
    }

    #[test]
    fn a_window_that_reaches_the_newest_does_not_detach_the_room() {
        let (mut app, room) = with_list(1);
        press(&mut app, "<CR>");
        loaded(&mut app, &room, window(&room, 4, 2, true));
        app.effects.clear();
        press(&mut app, "<CR>");
        assert!(!app.detached.contains_key(&room));
        assert_eq!(app.messages[&room].len(), 4);
        // The room is live again; one refetch closes the gap of what arrived while reading.
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { before: None, .. })),
            "{:?}",
            app.effects
        );
        update(&mut app, Action::RtMessage(msg(&room, 9100, "live")));
        assert!(app.messages[&room].iter().any(|m| m.id == "c9100"));
    }

    #[test]
    fn jump_is_refused_while_a_message_is_unsent() {
        let (mut app, room) = with_list(1);
        let mut unsent = msg(&room, 5000, "pending");
        unsent.send = SendState::Failed;
        app.messages.entry(room.clone()).or_default().push(unsent);
        open_view(&mut app, &room, 3, 1);
        press(&mut app, "<CR>");
        assert!(!app.detached.contains_key(&room));
        assert!(app.results.as_ref().expect("still open").context.is_some());
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    fn jumped(count: usize, selected: usize) -> (App, RoomId) {
        let (mut app, room) = with_list(1);
        open_view(&mut app, &room, count, selected);
        press(&mut app, "<CR>");
        assert!(app.detached.contains_key(&room));
        app.effects.clear();
        (app, room)
    }

    #[test]
    fn live_messages_are_not_added_while_detached() {
        let (mut app, room) = jumped(4, 1);
        update(&mut app, Action::RtMessage(msg(&room, 9000, "live")));
        assert_eq!(app.messages[&room].len(), 4);
    }

    #[test]
    fn writing_is_refused_while_detached() {
        let (mut app, room) = jumped(4, 3);
        press(&mut app, "ihello<CR>");
        assert!(app.effects.is_empty(), "{:?}", app.effects);
        assert_eq!(app.messages[&room].len(), 4);
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
        press(&mut app, "<Esc>");
        for keys in ["e", "dd"] {
            press(&mut app, keys);
        }
        assert!(app.editing.is_none() && app.confirm.is_none());
    }

    #[test]
    fn older_pages_still_load_and_merge() {
        let (mut app, room) = jumped(4, 3);
        press(&mut app, "gg");
        assert!(
            app.effects.iter().any(|e| matches!(
                e,
                Effect::LoadHistory { before: Some(b), .. } if b == "oldest-raw"
            )),
            "{:?}",
            app.effects
        );
        assert!(app.detached.contains_key(&room));
    }

    #[test]
    fn g_on_the_last_message_returns_to_the_newest() {
        let (mut app, room) = jumped(4, 1);
        press(&mut app, "G");
        assert!(app.detached.contains_key(&room), "first G only moves");
        assert_eq!(messages::view(&app, &room).cursor, 3);
        app.effects.clear();
        press(&mut app, "G");
        assert!(!app.detached.contains_key(&room));
        assert!(app.messages.get(&room).is_none_or(Vec::is_empty));
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { before: None, .. }))
        );
    }

    #[test]
    fn opening_another_room_ends_the_detached_state() {
        let (mut app, room) = jumped(4, 1);
        let other = app
            .rooms
            .iter()
            .find(|r| r.id != room && !r.encrypted)
            .expect("other room")
            .id
            .clone();
        app.open_room(other);
        assert!(!app.detached.contains_key(&room));
        assert!(!app.messages.contains_key(&room));
        assert!(!app.history.contains_key(&room));
    }

    #[test]
    fn a_room_that_is_not_detached_is_unchanged() {
        let mut app = app_80x24();
        let room = RoomId("general".to_string());
        app.open_room(room.clone());
        let before = app.messages[&room].len();
        update(&mut app, Action::RtMessage(msg(&room, 9001, "live")));
        assert_eq!(app.messages[&room].len(), before + 1);
        press(&mut app, "ihello<CR>");
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. }))
        );
    }

    fn page(room: &RoomId, from: usize, to: usize, reached: bool) -> NewerPage {
        NewerPage {
            messages: (from..=to).map(|i| msg(room, i, "newer")).collect(),
            newest_raw: Some(format!("raw-{to}")),
            reached_newest: reached,
        }
    }

    fn newer_effect(app: &App) -> Option<(String, u64)> {
        app.effects.iter().find_map(|e| match e {
            Effect::LoadNewer {
                raw_ts,
                newer_id,
                n,
                ..
            } => {
                assert_eq!(*n, NEWER_PAGE);
                Some((raw_ts.clone(), *newer_id))
            }
            _ => None,
        })
    }

    #[test]
    fn moving_onto_the_last_message_asks_for_the_next_newer_page() {
        let (mut app, room) = jumped(5, 1);
        press(&mut app, "j");
        assert!(newer_effect(&app).is_none(), "not on the last message yet");
        press(&mut app, "jj");
        let (raw, id) = newer_effect(&app).expect("a request on the last message");
        assert_eq!(raw, "newest-raw");
        assert_eq!(app.detached[&room].pending_newer, Some(id));
        // One request at a time: more motions on the last message ask nothing.
        app.effects.clear();
        press(&mut app, "j");
        press(&mut app, "k");
        press(&mut app, "j");
        assert!(newer_effect(&app).is_none());
    }

    #[test]
    fn a_newer_page_is_appended_without_moving_the_view() {
        let (mut app, room) = jumped(5, 1);
        press(&mut app, "G");
        let (_, id) = newer_effect(&app).expect("request");
        let before = messages::view(&app, &room);
        update(
            &mut app,
            Action::NewerLoaded {
                room: room.clone(),
                newer_id: id,
                page: page(&room, 100, 102, false),
            },
        );
        let ids: Vec<&str> = app.messages[&room].iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), 8);
        assert_eq!(&ids[5..], ["c100", "c101", "c102"]);
        assert_eq!(messages::view(&app, &room), before);
        let detached = &app.detached[&room];
        assert_eq!(detached.newest_raw.as_deref(), Some("raw-102"));
        assert_eq!(detached.pending_newer, None);
        // Moving onto the new last message asks for the following page from its raw time.
        app.effects.clear();
        press(&mut app, "G");
        let (raw, _) = newer_effect(&app).expect("next request");
        assert_eq!(raw, "raw-102");
    }

    #[test]
    fn a_page_that_reaches_the_newest_ends_the_detached_state() {
        let (mut app, room) = jumped(5, 1);
        press(&mut app, "G");
        let (_, id) = newer_effect(&app).expect("request");
        app.effects.clear();
        update(
            &mut app,
            Action::NewerLoaded {
                room: room.clone(),
                newer_id: id,
                page: page(&room, 100, 101, true),
            },
        );
        assert!(!app.detached.contains_key(&room));
        assert!(status(&app).contains("Reached the newest"));
        // Writing works and live messages are added again, and one refetch closes the gap.
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { before: None, .. }))
        );
        update(&mut app, Action::RtMessage(msg(&room, 9200, "live")));
        assert!(app.messages[&room].iter().any(|m| m.id == "c9200"));
        press(&mut app, "ihello<CR>");
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. }))
        );
    }

    #[test]
    fn stale_and_failed_newer_answers() {
        let (mut app, room) = jumped(5, 1);
        press(&mut app, "G");
        let (_, id) = newer_effect(&app).expect("request");
        // A stale id and another room change nothing.
        update(
            &mut app,
            Action::NewerLoaded {
                room: room.clone(),
                newer_id: id + 1,
                page: page(&room, 100, 100, false),
            },
        );
        update(
            &mut app,
            Action::NewerLoaded {
                room: RoomId("other".to_string()),
                newer_id: id,
                page: page(&room, 100, 100, false),
            },
        );
        assert_eq!(app.messages[&room].len(), 5);
        assert_eq!(app.detached[&room].pending_newer, Some(id));
        // A failure shows the error, keeps the room detached and allows another request.
        update(
            &mut app,
            Action::NewerFailed {
                room: room.clone(),
                newer_id: id,
                error: ApiError::Timeout,
            },
        );
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
        assert!(app.detached.contains_key(&room));
        app.effects.clear();
        press(&mut app, "k");
        press(&mut app, "j");
        assert!(
            newer_effect(&app).is_some(),
            "moving onto the last message asks again"
        );
    }

    #[test]
    fn g_and_reload_still_return_to_the_newest() {
        let (mut app, room) = jumped(3, 2);
        assert_eq!(messages::view(&app, &room).cursor, 2);
        // The cursor is on the last message: a first request may be pending, `G` still returns.
        app.effects.clear();
        press(&mut app, "G");
        assert!(!app.detached.contains_key(&room));
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { before: None, .. }))
        );
        // A late forward page for the dropped window changes nothing.
        update(
            &mut app,
            Action::NewerLoaded {
                room: room.clone(),
                newer_id: 1,
                page: page(&room, 100, 100, false),
            },
        );
        assert!(app.messages.get(&room).is_none_or(Vec::is_empty));
    }

    // ---- N toward newer in a detached room ----

    /// A page of messages `from..=to` whose bodies are `body`.
    fn page_of(room: &RoomId, from: usize, to: usize, body: &str, reached: bool) -> NewerPage {
        NewerPage {
            messages: (from..=to).map(|i| msg(room, i, body)).collect(),
            newest_raw: Some(format!("raw-{to}")),
            reached_newest: reached,
        }
    }

    fn deliver(app: &mut App, room: &RoomId, id: u64, p: NewerPage) {
        update(
            app,
            Action::NewerLoaded {
                room: room.clone(),
                newer_id: id,
                page: p,
            },
        );
    }

    /// A detached room (window bodies "ctx", cursor on message 1) with a search for "needle" and
    /// `N` already pressed: returns the id of the forward page it asked for.
    fn seeking_newer() -> (App, RoomId, u64) {
        let (mut app, room) = jumped(5, 1);
        app.search = Some(crate::app::search::Search {
            query: "needle".to_string(),
            seeking: false,
            seeking_newer: false,
        });
        app.effects.clear();
        press(&mut app, "N");
        let (_, id) = newer_effect(&app).expect("N asks for a newer page");
        assert!(app.search.as_ref().expect("search").seeking);
        (app, room, id)
    }

    fn cursor(app: &App, room: &RoomId) -> usize {
        messages::view(app, room).cursor
    }

    #[test]
    fn n_toward_newer_finds_a_match_in_the_first_page() {
        let (mut app, room, id) = seeking_newer();
        deliver(
            &mut app,
            &room,
            id,
            page_of(&room, 100, 103, "a needle here", false),
        );
        assert_eq!(
            cursor(&app, &room),
            5,
            "the first matching message of the page"
        );
        assert!(!app.search.as_ref().expect("search").seeking);
        app.effects.clear();
        press(&mut app, "k");
        assert!(
            newer_effect(&app).is_none(),
            "no more fetching after a match"
        );
    }

    #[test]
    fn n_toward_newer_keeps_fetching_until_a_page_matches() {
        let (mut app, room, id) = seeking_newer();
        app.effects.clear();
        deliver(
            &mut app,
            &room,
            id,
            page_of(&room, 100, 102, "nothing", false),
        );
        let (raw, id2) = newer_effect(&app).expect("the next page is asked for");
        assert_eq!(raw, "raw-102");
        assert_eq!(cursor(&app, &room), 1, "the cursor waits");
        deliver(
            &mut app,
            &room,
            id2,
            page_of(&room, 103, 105, "needle", false),
        );
        assert_eq!(cursor(&app, &room), 8, "the first match of the second page");
        assert_eq!(app.messages[&room].len(), 5 + 6);
    }

    #[test]
    fn reaching_the_newest_without_a_match_says_so_and_keeps_the_cursor() {
        let (mut app, room, id) = seeking_newer();
        app.effects.clear();
        deliver(
            &mut app,
            &room,
            id,
            page_of(&room, 100, 101, "nothing", true),
        );
        assert_eq!(cursor(&app, &room), 1);
        assert!(!app.search.as_ref().expect("search").seeking);
        assert!(newer_effect(&app).is_none(), "no further page");
        assert!(!app.detached.contains_key(&room), "the newest was reached");
        assert_eq!(status(&app), "No newer match");
    }

    #[test]
    fn a_failed_newer_page_stops_the_search() {
        let (mut app, room, id) = seeking_newer();
        update(
            &mut app,
            Action::NewerFailed {
                room: room.clone(),
                newer_id: id,
                error: ApiError::Timeout,
            },
        );
        assert!(!app.search.as_ref().expect("search").seeking);
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
        assert_eq!(cursor(&app, &room), 1);
    }

    #[test]
    fn leaving_the_room_or_clearing_the_search_stops_the_fetching() {
        // Clearing the search: the page is still appended, but nothing more is asked for.
        let (mut app, room, id) = seeking_newer();
        press(&mut app, "<Esc>");
        assert!(app.search.is_none());
        app.effects.clear();
        deliver(
            &mut app,
            &room,
            id,
            page_of(&room, 100, 101, "nothing", false),
        );
        assert!(newer_effect(&app).is_none());
        assert_eq!(app.messages[&room].len(), 7);
        // Opening another room: the detached state is dropped and the late page is ignored.
        let (mut app, room, id) = seeking_newer();
        let other = app
            .rooms
            .iter()
            .find(|r| r.id != room && !r.encrypted)
            .expect("other")
            .id
            .clone();
        app.open_room(other);
        assert!(app.search.is_none());
        app.effects.clear();
        deliver(
            &mut app,
            &room,
            id,
            page_of(&room, 100, 101, "needle", false),
        );
        assert!(newer_effect(&app).is_none());
        assert!(!app.messages.contains_key(&room));
    }

    #[test]
    fn keys_are_ignored_while_waiting_and_a_page_in_flight_is_reused() {
        let (mut app, room, id) = seeking_newer();
        app.effects.clear();
        press(&mut app, "N");
        press(&mut app, "n");
        assert!(newer_effect(&app).is_none(), "no second request");
        assert_eq!(cursor(&app, &room), 1);
        // A page the cursor asked for resolves a wait started afterwards.
        let (mut app, room) = jumped(5, 1);
        press(&mut app, "G");
        let (_, id2) = newer_effect(&app).expect("cursor-triggered page");
        app.search = Some(crate::app::search::Search {
            query: "needle".to_string(),
            seeking: false,
            seeking_newer: false,
        });
        app.effects.clear();
        // The cursor is on the last message, so there is nothing newer loaded to scan.
        press(&mut app, "N");
        assert!(newer_effect(&app).is_none(), "the page in flight is reused");
        deliver(
            &mut app,
            &room,
            id2,
            page_of(&room, 100, 101, "needle", false),
        );
        assert_eq!(cursor(&app, &room), 5);
        let _ = id;
    }

    #[test]
    fn an_older_page_does_not_resolve_a_newer_wait() {
        let (mut app, room, _) = seeking_newer();
        crate::app::search::on_page(&mut app, &room);
        assert!(app.search.as_ref().expect("search").seeking);
        assert!(app.search.as_ref().expect("search").seeking_newer);
    }

    #[test]
    fn n_toward_newer_in_a_room_that_is_not_detached_asks_for_nothing() {
        let mut app = app_80x24();
        let room = RoomId("general".to_string());
        app.open_room(room.clone());
        app.search = Some(crate::app::search::Search {
            query: "zzzqqq".to_string(),
            seeking: false,
            seeking_newer: false,
        });
        app.effects.clear();
        press(&mut app, "N");
        assert!(app.effects.is_empty());
        assert_eq!(status(&app), "No newer match");
        assert!(!app.search.as_ref().expect("search").seeking);
    }

    #[test]
    fn debug_prints_no_text() {
        let room = RoomId("r".to_string());
        let mut w = window(&room, 1, 0, false);
        w.messages[0].body = "secret-body".to_string();
        let text = format!(
            "{:?} {:?}",
            Action::ContextLoaded {
                room,
                context_id: 1,
                window: w
            },
            Effect::LoadContext {
                room: RoomId("r".to_string()),
                kind: crate::data::RoomKind::Channel,
                private: false,
                id: "i".to_string(),
                raw_ts: "t".to_string(),
                context_id: 1,
                before: 1,
                after: 1
            }
        );
        assert!(!text.contains("secret"), "{text}");
    }
}
