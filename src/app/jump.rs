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
use crate::api::{ApiError, ContextWindow};
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
const NEWER_HIDDEN: &str = "Newer messages are not shown: more than 100 follow this one";
const UNSENT: &str = "Resolve the unsent message first (r to retry, dd to discard)";
const OLDER: &str = "Showing older messages: press G on the last message to return to the newest";
const OLDER_HIDDEN: &str =
    "Showing older messages that end before the newest: press G on the last message to load them";

/// The read-only window around one search result, oldest first.
pub struct ContextView {
    pub messages: Vec<Message>,
    pub selected: usize,
    pub top_row: usize,
    /// More than 100 newer messages exist, so none of the ones after the result are shown.
    pub newer_missing: bool,
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
    let newer_missing = window.newer_missing;
    list.context = Some(ContextView {
        messages: window.messages,
        selected: window.selected,
        top_row: 0,
        newer_missing,
        oldest_raw: window.oldest_raw,
    });
    if newer_missing {
        info(app, NEWER_HIDDEN);
    }
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
    let newer_missing = ctx.newer_missing;
    app.messages.insert(room.clone(), ctx.messages);
    app.history.insert(
        room.clone(),
        HistoryState::Loaded {
            oldest_ts: ctx.oldest_raw,
            exhausted: false,
            loading_older: false,
        },
    );
    app.detached.insert(room.clone());
    crate::app::search::clear(app);
    crate::app::results::close_all(app);
    app.focus = Focus::Messages;
    let (_, height) = messages::viewport(app);
    let starts = messages::row_starts(app, &room);
    let top_row = messages::scroll_into_view(&starts, selected, 0, usize::from(height));
    app.msg_views.insert(
        room,
        MsgView {
            cursor: selected,
            top_row,
        },
    );
    info(app, if newer_missing { OLDER_HIDDEN } else { OLDER });
}

/// True while `room` shows a window of older history instead of its newest messages.
pub fn is_detached(app: &App, room: &RoomId) -> bool {
    app.detached.contains(room)
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
    if app.detached.remove(room) {
        app.messages.remove(room);
        app.history.remove(room);
        app.msg_views.remove(room);
    }
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

    fn window(room: &RoomId, count: usize, selected: usize, newer_missing: bool) -> ContextWindow {
        ContextWindow {
            messages: (0..count).map(|i| msg(room, i, "ctx")).collect(),
            selected,
            newer_missing,
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
    fn the_window_opens_with_the_result_selected_and_notes_hidden_newer() {
        let (mut app, room) = with_list(2);
        press(&mut app, "<CR>");
        loaded(&mut app, &room, window(&room, 6, 3, true));
        let ctx = app
            .results
            .as_ref()
            .expect("list")
            .context
            .as_ref()
            .expect("view");
        assert_eq!((ctx.messages.len(), ctx.selected), (6, 3));
        assert_eq!(view(&app).cursor, 3);
        assert!(status(&app).contains("Newer messages are not shown"));
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
        assert!(app.detached.contains(&room));
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
    fn jump_with_hidden_newer_says_the_view_ends_early() {
        let (mut app, room) = with_list(1);
        press(&mut app, "<CR>");
        loaded(&mut app, &room, window(&room, 4, 3, true));
        press(&mut app, "<CR>");
        assert!(status(&app).contains("before the newest"));
    }

    #[test]
    fn jump_is_refused_while_a_message_is_unsent() {
        let (mut app, room) = with_list(1);
        let mut unsent = msg(&room, 5000, "pending");
        unsent.send = SendState::Failed;
        app.messages.entry(room.clone()).or_default().push(unsent);
        open_view(&mut app, &room, 3, 1);
        press(&mut app, "<CR>");
        assert!(!app.detached.contains(&room));
        assert!(app.results.as_ref().expect("still open").context.is_some());
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    fn jumped(count: usize, selected: usize) -> (App, RoomId) {
        let (mut app, room) = with_list(1);
        open_view(&mut app, &room, count, selected);
        press(&mut app, "<CR>");
        assert!(app.detached.contains(&room));
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
        assert!(app.detached.contains(&room));
    }

    #[test]
    fn g_on_the_last_message_returns_to_the_newest() {
        let (mut app, room) = jumped(4, 1);
        press(&mut app, "G");
        assert!(app.detached.contains(&room), "first G only moves");
        assert_eq!(messages::view(&app, &room).cursor, 3);
        app.effects.clear();
        press(&mut app, "G");
        assert!(!app.detached.contains(&room));
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
        assert!(!app.detached.contains(&room));
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
