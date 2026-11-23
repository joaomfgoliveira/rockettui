//! Per-room history state and the decisions about when to fetch (D-11, D-12, D-13, D-16).
//!
//! The functions here are pure: they read and change `App` and push `Effect`s onto `app.effects`,
//! and never do IO. The run loop turns the effects into requests and the answers come back as
//! `Action::HistoryLoaded` and `Action::HistoryFailed`.
//!
//! Rules this module keeps:
//! - an encrypted room is never fetched and never decrypted (D-12, ROOM-03);
//! - a room is fetched on its first open only; a Loading or Loaded room is not fetched again
//!   (D-13), and nothing is prefetched;
//! - opening a room never marks it read on the server: every effect is a GET (D-11);
//! - only a Failed room is fetched again when it is reopened;
//! - moving the messages cursor onto the oldest loaded message fetches the next older page, one
//!   request in flight per room, and an empty page ends the history (D-14).

use crate::api::ApiError;
use crate::app::messages::{self, MsgView};
use crate::app::{App, StatusMsg};
use crate::data::{Message, RoomId};
use crate::effect::Effect;
use std::collections::HashSet;

/// Where a room's history stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryState {
    /// The first page was requested and has not arrived.
    Loading,
    /// The first page arrived. `oldest_ts` is the raw server timestamp of the oldest message held,
    /// the `latest` value of the next older page; `exhausted` is true once the server has no
    /// older message; `loading_older` is true while an older page is in flight.
    Loaded {
        oldest_ts: Option<String>,
        exhausted: bool,
        loading_older: bool,
    },
    /// The first page failed; reopening the room tries again.
    Failed,
}

/// A room was opened: decide whether its first page must be fetched. An unknown room does
/// nothing, an encrypted room does nothing (D-12), a room that is Loading or Loaded does nothing
/// (D-13), and a room with no state or a Failed one becomes Loading and records one
/// `Effect::LoadHistory` for the newest `history_size` messages.
pub fn on_open(app: &mut App, id: &RoomId) {
    let Some(room) = app.rooms.iter().find(|r| &r.id == id) else {
        return;
    };
    if room.encrypted {
        return;
    }
    match app.history.get(id) {
        // A loaded room that missed messages during a realtime outage refetches its newest page
        // once; the page is merged by id and the room stays Loaded (see `on_refetched`).
        Some(HistoryState::Loaded { .. }) => {
            if app.stale.remove(id) {
                refetch_newest(app, id);
            }
            return;
        }
        Some(HistoryState::Loading) => return,
        Some(HistoryState::Failed) | None => {}
    }
    let effect = Effect::LoadHistory {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        count: app.settings.history_size,
        before: None,
    };
    app.history.insert(id.clone(), HistoryState::Loading);
    app.effects.push(effect);
}

/// Record one request for the newest page of the loaded room `id`, without changing its state: the
/// answer arrives as a first-page `HistoryLoaded` for a room that is already Loaded, which
/// `on_loaded` merges by id. An unknown or encrypted room does nothing.
pub fn refetch_newest(app: &mut App, id: &RoomId) {
    let Some(room) = app.rooms.iter().find(|r| &r.id == id) else {
        return;
    };
    if room.encrypted {
        return;
    }
    let effect = Effect::LoadHistory {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        count: app.settings.history_size,
        before: None,
    };
    app.effects.push(effect);
}

/// `:reconnect` (D-16): forget every room whose first page failed, and fetch the open room again
/// now if its page was one of them. A closed room that failed is fetched when it is next opened.
/// Loading and Loaded rooms are untouched (D-13).
pub fn retry_failed(app: &mut App) {
    let reopen = app
        .current_room
        .clone()
        .filter(|id| matches!(app.history.get(id), Some(HistoryState::Failed)));
    app.history
        .retain(|_, state| !matches!(state, HistoryState::Failed));
    if let Some(id) = reopen {
        on_open(app, &id);
    }
}

/// The older-page fetch (D-14). Called after every messages-pane motion: when the cursor sits on
/// the oldest loaded message of a Loaded room it records one `Effect::LoadHistory` with `before`
/// set to the raw timestamp of that message and marks the page in flight. Nothing happens while a
/// page is in flight (one request per room), and an exhausted room shows `Beginning of history`
/// instead of asking again. No automatic retry: a failed page is retried by moving onto the top
/// message again.
pub fn maybe_load_older(app: &mut App) {
    let Some(room_id) = app.current_room.clone() else {
        return;
    };
    if app.messages.get(&room_id).is_none_or(Vec::is_empty) {
        return;
    }
    let (exhausted, loading_older, oldest_ts) = match app.history.get(&room_id) {
        Some(HistoryState::Loaded {
            oldest_ts,
            exhausted,
            loading_older,
        }) => (*exhausted, *loading_older, oldest_ts.clone()),
        _ => return,
    };
    if messages::view(app, &room_id).cursor != 0 || loading_older {
        return;
    }
    if exhausted {
        app.status_msg = Some(StatusMsg::Info(BEGINNING.into()));
        return;
    }
    let Some(before) = oldest_ts else {
        return;
    };
    let Some(room) = app.rooms.iter().find(|r| r.id == room_id) else {
        return;
    };
    let effect = Effect::LoadHistory {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        count: app.settings.history_size,
        before: Some(before),
    };
    if let Some(HistoryState::Loaded { loading_older, .. }) = app.history.get_mut(&room_id) {
        *loading_older = true;
    }
    app.effects.push(effect);
}

/// The status line shown at the top of a room with no older message.
const BEGINNING: &str = "Beginning of history";

/// A page of history arrived (`page` oldest first). The first page is merged in front of any
/// messages the room already holds (local sends); an empty first page marks the room exhausted,
/// since no older page can follow. An older page is prepended and keeps the cursor on the same
/// message at the same screen row; an empty page or one with nothing new marks the room exhausted
/// (D-14).
///
/// Held ids follow D-21 in both cases, through `messages::confirm_held`: a page message whose id is
/// held as `Sent` is ignored, and one held as a pending copy (Queued, Sending, Failed, Unconfirmed)
/// replaces it in place and marks it `Sent`, keeping its position and any stored cursor. Only the
/// messages the room does not hold are prepended.
pub fn on_loaded(
    app: &mut App,
    room: RoomId,
    page: Vec<Message>,
    oldest_ts: Option<String>,
    older: bool,
) {
    if older {
        on_older_page(app, room, page, oldest_ts);
        return;
    }
    // A first page for a room that is already Loaded is the answer to a refetch after a realtime
    // outage: merge it by id instead of treating it as the room's first page.
    if matches!(app.history.get(&room), Some(HistoryState::Loaded { .. })) {
        on_refetched(app, room, page, oldest_ts);
        return;
    }

    let exhausted = page.is_empty();
    let mut merged: Vec<Message> = page
        .into_iter()
        .filter(|m| messages::confirm_held(app, &room, m).is_none())
        .collect();
    let added = merged.len();
    let held = app.messages.entry(room.clone()).or_default();
    merged.append(held);
    *held = merged;
    // A stored view points at an index; the new messages sit in front of it.
    if let Some(view) = app.msg_views.get_mut(&room) {
        view.cursor += added;
        view.top_row = 0;
    }
    app.history.insert(
        room,
        HistoryState::Loaded {
            oldest_ts,
            exhausted,
            loading_older: false,
        },
    );
}

/// The newest page of an already Loaded room (`page` oldest first), after a realtime outage.
///
/// When the page shares a message id with what the room holds (or the room holds no server message
/// at all), each message it brings that the room lacks is inserted by time, held ids follow D-21
/// through `messages::confirm_held`, and a stored view keeps the same message at the same screen
/// row. When it shares none, the gap is larger than one page and merging cannot repair it: the
/// room is reloaded from this page instead (messages newer than the page and unsent messages are
/// kept) and shows its newest message again, so no hole is left.
fn on_refetched(app: &mut App, room: RoomId, page: Vec<Message>, oldest_ts: Option<String>) {
    let Some(newest) = page.last().map(|m| m.ts) else {
        return;
    };
    let overlaps = app.messages.get(&room).is_none_or(|held| {
        !held.iter().any(|m| !m.send.is_unsent())
            || page.iter().any(|p| held.iter().any(|m| m.id == p.id))
    });
    if !overlaps {
        let held = app.messages.remove(&room).unwrap_or_default();
        let mut reloaded = page;
        reloaded.extend(
            held.into_iter()
                .filter(|m| m.send.is_unsent() || m.ts > newest),
        );
        app.messages.insert(room.clone(), reloaded);
        app.msg_views.remove(&room);
        app.history.insert(
            room,
            HistoryState::Loaded {
                oldest_ts,
                exhausted: false,
                loading_older: false,
            },
        );
        return;
    }

    // The cursor message and its row inside the window, read before anything moves.
    let anchor = app.msg_views.contains_key(&room).then(|| {
        let view = messages::view(app, &room);
        let starts = messages::row_starts(app, &room);
        let id = app.messages[&room][view.cursor].id.clone();
        (id, starts[view.cursor] as isize - view.top_row as isize)
    });
    // What the page says about messages the room already holds: their current text and edited
    // state, and (by absence) which ones were deleted. Only the span strictly between the page's
    // oldest and newest second is judged: a message in the oldest second may simply lie beyond the
    // page, and one in the newest second may have arrived live after the request left.
    let page_ids: HashSet<String> = page.iter().map(|m| m.id.clone()).collect();
    let span = page.first().map(|m| m.ts).zip(page.last().map(|m| m.ts));
    let mut seen: HashSet<String> = HashSet::new();
    for message in page {
        if !seen.insert(message.id.clone()) {
            continue;
        }
        if messages::is_edit(app, &room, &message) {
            messages::apply_edit(app, &room, message);
            continue;
        }
        if messages::confirm_held(app, &room, &message).is_some() {
            continue;
        }
        let held = app.messages.entry(room.clone()).or_default();
        let at = held
            .iter()
            .position(|m| m.ts > message.ts)
            .unwrap_or(held.len());
        held.insert(
            at,
            Message {
                send: crate::data::SendState::Sent,
                ..message
            },
        );
    }
    if let Some((oldest, newest)) = span {
        let gone: Vec<String> = app.messages[&room]
            .iter()
            .filter(|m| {
                m.send == crate::data::SendState::Sent
                    && m.kind != crate::data::MessageKind::Deleted
                    && m.ts > oldest
                    && m.ts < newest
                    && !page_ids.contains(&m.id)
            })
            .map(|m| m.id.clone())
            .collect();
        for id in gone {
            messages::tombstone_in_place(app, &room, &id);
        }
    }
    if let Some(HistoryState::Loaded {
        oldest_ts: stored, ..
    }) = app.history.get_mut(&room)
        && stored.is_none()
    {
        *stored = oldest_ts;
    }
    if let Some((id, offset)) = anchor
        && let Some(cursor) = app.messages[&room].iter().position(|m| m.id == id)
    {
        let starts = messages::row_starts(app, &room);
        let top_row = (starts[cursor] as isize - offset).max(0) as usize;
        app.msg_views.insert(room, MsgView { cursor, top_row });
    }
}

/// The older-page branch of `on_loaded`: prepend what is not held yet (held ids follow D-21 through
/// `messages::confirm_held`; the page's own repeats are dropped), keep a stored view on the same message and the same screen row, and
/// move `oldest_ts` back. A page with nothing new ends the history, so a misbehaving server
/// cannot cause a request loop.
fn on_older_page(app: &mut App, room: RoomId, page: Vec<Message>, oldest_ts: Option<String>) {
    if !matches!(app.history.get(&room), Some(HistoryState::Loaded { .. })) {
        return;
    }
    // The cursor message and its row inside the window, read before anything moves.
    let anchor = app.msg_views.contains_key(&room).then(|| {
        let view = messages::view(app, &room);
        let starts = messages::row_starts(app, &room);
        (
            view.cursor,
            starts[view.cursor] as isize - view.top_row as isize,
        )
    });

    // `seen` holds the ids already taken from this page, so a page that repeats an id adds it once.
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<Message> = page
        .into_iter()
        .filter(|m| messages::confirm_held(app, &room, m).is_none() && seen.insert(m.id.clone()))
        .collect();
    let added = merged.len();
    let held = app.messages.entry(room.clone()).or_default();
    merged.append(held);
    *held = merged;

    if let Some(HistoryState::Loaded {
        oldest_ts: stored,
        exhausted,
        loading_older,
    }) = app.history.get_mut(&room)
    {
        *loading_older = false;
        if added == 0 {
            *exhausted = true;
        } else if oldest_ts.is_some() {
            *stored = oldest_ts;
        }
    }
    if added == 0 {
        app.status_msg = Some(StatusMsg::Info(BEGINNING.into()));
        return;
    }
    if let Some((cursor, offset)) = anchor {
        let cursor = cursor + added;
        let starts = messages::row_starts(app, &room);
        let top_row = (starts[cursor] as isize - offset).max(0) as usize;
        app.msg_views.insert(room, MsgView { cursor, top_row });
    }
}

/// A history request failed. The first page leaves the room Failed (reopening retries); an older
/// page only clears `loading_older`. Both show the fixed error line in the status bar.
pub fn on_failed(app: &mut App, room: RoomId, older: bool, error: ApiError) {
    if older {
        if let Some(HistoryState::Loaded { loading_older, .. }) = app.history.get_mut(&room) {
            *loading_older = false;
        }
    } else if matches!(app.history.get(&room), Some(HistoryState::Loaded { .. })) {
        // A failed refetch of a Loaded room leaves it Loaded, with what it holds, and stale again.
        app.stale.insert(room);
    } else {
        app.history.insert(room, HistoryState::Failed);
    }
    app.status_msg = Some(StatusMsg::Error(error.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::api::map;
    use crate::api::wire::{HistoryResp, RoomsResp, SubscriptionsResp};
    use crate::app::update::update;
    use crate::data::{MessageKind, RoomKind, SendState};
    use crate::test_util::{app_80x24, press, render_to_string};
    use ratatui::layout::Size;

    const SUBS: &str = include_str!("../api/fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("../api/fixtures/rooms.json");
    const HISTORY: &str = include_str!("../api/fixtures/history.json");

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    /// An app with the fixture rooms loaded through the reducer, as the server would deliver them.
    fn fixture_app() -> App {
        let subs = serde_json::from_str::<SubscriptionsResp>(SUBS)
            .expect("subscriptions")
            .update;
        let records = serde_json::from_str::<RoomsResp>(ROOMS)
            .expect("rooms")
            .update;
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(map::rooms(&subs, &records)));
        app
    }

    fn history_messages(room: &str) -> Vec<Message> {
        let msgs = serde_json::from_str::<HistoryResp>(HISTORY)
            .expect("history")
            .messages;
        map::history(&rid(room), msgs).messages
    }

    fn loaded_action(room: &str) -> Action {
        Action::HistoryLoaded {
            room: rid(room),
            messages: history_messages(room),
            oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
            older: false,
        }
    }

    /// Put the sidebar cursor on `name` and open it with Enter.
    fn open(app: &mut App, name: &str) {
        press(app, "<C-w>hgg");
        let mut moves = 0;
        loop {
            if crate::app::sidebar::selected_room(app) == Some(rid(name)) {
                break;
            }
            press(app, "j");
            moves += 1;
            assert!(moves < 40, "{name} is not in the sidebar");
        }
        press(app, "<CR>");
    }

    #[test]
    fn first_open_issues_one_load() {
        let mut app = fixture_app();
        // general is the newest unread room, so it is the first selectable row.
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        assert_eq!(
            app.effects,
            [Effect::LoadHistory {
                room: rid("r-general"),
                kind: RoomKind::Channel,
                private: false,
                count: 50,
                before: None,
            }]
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loading)
        );

        app.effects.clear();
        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        assert!(
            app.effects.is_empty(),
            "a Loading room is not fetched again"
        );
    }

    #[test]
    fn effect_follows_room_kind_and_history_size() {
        let mut app = fixture_app();
        app.settings.history_size = 20;
        open(&mut app, "r-devteam");
        open(&mut app, "r-dm-alice");
        let kinds: Vec<(&str, RoomKind, bool, u16)> = app
            .effects
            .iter()
            .map(|e| match e {
                Effect::LoadHistory {
                    room,
                    kind,
                    private,
                    count,
                    ..
                } => (room.0.as_str(), *kind, *private, *count),
                other => panic!("unexpected effect {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                ("r-devteam", RoomKind::PrivateGroup, true, 20),
                ("r-dm-alice", RoomKind::DirectMessage, false, 20),
            ]
        );
    }

    #[test]
    fn encrypted_never_loads() {
        let mut app = fixture_app();
        open(&mut app, "r-secrets");
        assert_eq!(app.current_room, Some(rid("r-secrets")));
        assert!(app.effects.is_empty(), "{:?}", app.effects);
        assert!(!app.history.contains_key(&rid("r-secrets")));
    }

    #[test]
    fn loaded_then_cached() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        app.effects.clear();
        update(&mut app, loaded_action("r-general"));
        let held: Vec<&str> = app.messages[&rid("r-general")]
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(held, ["m1", "m2", "m3", "m4"]);
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
                exhausted: false,
                loading_older: false,
            })
        );
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("fourth and newest message"), "{screen}");

        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert!(app.effects.is_empty(), "a Loaded room is not fetched again");
    }

    #[test]
    fn failed_reopen_retries() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        app.effects.clear();
        update(
            &mut app,
            Action::HistoryFailed {
                room: rid("r-general"),
                older: false,
                error: ApiError::Timeout,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Failed)
        );
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => assert_eq!(text, &ApiError::Timeout.to_string()),
            _ => panic!("expected a status error"),
        }

        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loading)
        );
        assert_eq!(app.effects.len(), 1, "{:?}", app.effects);
    }

    #[test]
    fn empty_first_page() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: Vec::new(),
                oldest_ts: None,
                older: false,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: None,
                exhausted: true,
                loading_older: false,
            })
        );
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("No messages yet"), "{screen}");
    }

    #[test]
    fn fake_rooms_never_load() {
        let mut app = app_80x24();
        press(&mut app, "<CR>");
        assert!(app.current_room.is_some());
        assert!(app.effects.is_empty(), "{:?}", app.effects);
    }

    #[test]
    fn first_page_merges_before_local_messages() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        // A local send while the page is in flight, then the page arrives.
        press(&mut app, "ihello<CR><Esc>");
        update(&mut app, loaded_action("r-general"));
        let held: Vec<&str> = app.messages[&rid("r-general")]
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(held, ["m1", "m2", "m3", "m4", "local-1"]);
        // The same page twice never duplicates a message.
        update(&mut app, loaded_action("r-general"));
        assert_eq!(app.messages[&rid("r-general")].len(), 5);
    }

    #[test]
    fn older_page_only_clears_the_flag() {
        let mut app = fixture_app();
        app.history.insert(
            rid("r-general"),
            HistoryState::Loaded {
                oldest_ts: Some("x".to_string()),
                exhausted: false,
                loading_older: true,
            },
        );
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: Vec::new(),
                oldest_ts: None,
                older: true,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: Some("x".to_string()),
                exhausted: true,
                loading_older: false,
            })
        );
    }

    const NEWEST_TS: &str = "2026-11-16T09:59:00.300Z";

    /// `count` messages with ids `{prefix}{i}` for i in `from..from + count`, oldest first.
    fn run(prefix: &str, from: usize, count: usize) -> Vec<Message> {
        (from..from + count)
            .map(|i| Message {
                edited: false,
                plain_text: true,
                id: format!("{prefix}{i:03}"),
                room_id: rid("r-general"),
                author: "alice".to_string(),
                ts: i as i64,
                body: format!("body {prefix}{i:03}"),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            })
            .collect()
    }

    fn older_action(messages: Vec<Message>, oldest_ts: Option<&str>) -> Action {
        Action::HistoryLoaded {
            room: rid("r-general"),
            messages,
            oldest_ts: oldest_ts.map(str::to_string),
            older: true,
        }
    }

    /// r-general open with focus on the messages pane and 50 messages held (`n000`..`n049`),
    /// no effect pending.
    fn loaded_general() -> App {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: run("n", 0, 50),
                oldest_ts: Some(NEWEST_TS.to_string()),
                older: false,
            },
        );
        app.effects.clear();
        app
    }

    fn state(app: &App) -> &HistoryState {
        app.history.get(&rid("r-general")).expect("history state")
    }

    fn info_text(app: &App) -> Option<&str> {
        match &app.status_msg {
            Some(StatusMsg::Info(text)) => Some(text.as_str()),
            _ => None,
        }
    }

    fn older_effect() -> Effect {
        Effect::LoadHistory {
            room: rid("r-general"),
            kind: RoomKind::Channel,
            private: false,
            count: 50,
            before: Some(NEWEST_TS.to_string()),
        }
    }

    #[test]
    fn older_page_trigger() {
        let mut app = loaded_general();
        press(&mut app, "gg");
        assert_eq!(app.effects, [older_effect()]);
        assert!(matches!(
            state(&app),
            HistoryState::Loaded {
                loading_older: true,
                ..
            }
        ));
        press(&mut app, "k");
        press(&mut app, "<C-u>");
        assert_eq!(
            app.effects.len(),
            1,
            "one request in flight: {:?}",
            app.effects
        );
    }

    #[test]
    fn older_prepend_keeps_cursor() {
        let mut app = loaded_general();
        press(&mut app, "gg");
        let id = app.messages[&rid("r-general")][0].id.clone();
        let starts = messages::row_starts(&app, &rid("r-general"));
        let before = messages::view(&app, &rid("r-general"));
        let offset = starts[before.cursor] as isize - before.top_row as isize;

        update(
            &mut app,
            older_action(run("a", 0, 50), Some("2026-11-16T09:58:00.100Z")),
        );
        let held = &app.messages[&rid("r-general")];
        assert_eq!(held.len(), 100);
        let after = messages::view(&app, &rid("r-general"));
        assert_eq!(after.cursor, 50);
        assert_eq!(held[after.cursor].id, id);
        let starts = messages::row_starts(&app, &rid("r-general"));
        assert_eq!(
            starts[after.cursor] as isize - after.top_row as isize,
            offset
        );
        assert_eq!(
            state(&app),
            &HistoryState::Loaded {
                oldest_ts: Some("2026-11-16T09:58:00.100Z".to_string()),
                exhausted: false,
                loading_older: false,
            }
        );
    }

    #[test]
    fn older_prepend_keeps_screen_row_when_cursor_moved_down() {
        let mut app = loaded_general();
        press(&mut app, "gg");
        // Moving down while the page is in flight leaves the window on the first rows.
        press(&mut app, "3j");
        let room = rid("r-general");
        let id = app.messages[&room][3].id.clone();
        let before = messages::view(&app, &room);
        let starts = messages::row_starts(&app, &room);
        let offset = starts[before.cursor] as isize - before.top_row as isize;

        update(&mut app, older_action(run("a", 0, 50), Some("x")));
        let after = messages::view(&app, &room);
        assert_eq!(after.cursor, 53);
        assert_eq!(app.messages[&room][after.cursor].id, id);
        let starts = messages::row_starts(&app, &room);
        assert_eq!(
            starts[after.cursor] as isize - after.top_row as isize,
            offset
        );
    }

    #[test]
    fn older_dedupe() {
        let mut app = loaded_general();
        press(&mut app, "gg");
        let room = rid("r-general");
        // The page ends with a message the room already holds, and repeats one of its own.
        let mut page = run("a", 0, 10);
        page.push(page[0].clone());
        page.push(app.messages[&room][0].clone());
        update(&mut app, older_action(page, Some("x")));
        let ids: Vec<&str> = app.messages[&room].iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), 60);
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), 60, "no id appears twice");
        assert_eq!(ids[0], "a000");
        assert_eq!(ids[10], "n000");
    }

    #[test]
    fn exhausted_stops() {
        for page in [Vec::new(), run("n", 0, 3)] {
            let mut app = loaded_general();
            press(&mut app, "gg");
            let room = rid("r-general");
            update(&mut app, older_action(page, None));
            assert_eq!(app.messages[&room].len(), 50);
            assert!(matches!(
                state(&app),
                HistoryState::Loaded {
                    exhausted: true,
                    loading_older: false,
                    ..
                }
            ));
            assert_eq!(info_text(&app), Some("Beginning of history"));

            app.effects.clear();
            app.status_msg = None;
            press(&mut app, "gg");
            assert!(app.effects.is_empty(), "{:?}", app.effects);
            assert_eq!(info_text(&app), Some("Beginning of history"));
        }
    }

    #[test]
    fn older_failure_allows_retry() {
        let mut app = loaded_general();
        press(&mut app, "gg");
        app.effects.clear();
        update(
            &mut app,
            Action::HistoryFailed {
                room: rid("r-general"),
                older: true,
                error: ApiError::Timeout,
            },
        );
        assert!(matches!(
            state(&app),
            HistoryState::Loaded {
                loading_older: false,
                exhausted: false,
                ..
            }
        ));
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => assert_eq!(text, &ApiError::Timeout.to_string()),
            _ => panic!("expected a status error"),
        }
        assert!(app.effects.is_empty(), "no automatic retry");
        press(&mut app, "gg");
        assert_eq!(app.effects, [older_effect()]);
    }

    #[test]
    fn no_trigger_away_from_top() {
        let mut app = loaded_general();
        for keys in ["j", "G", "<C-d>", "k", "5k", "<C-u>"] {
            press(&mut app, keys);
            assert!(app.effects.is_empty(), "{keys}: {:?}", app.effects);
        }
        assert!(matches!(
            state(&app),
            HistoryState::Loaded {
                loading_older: false,
                ..
            }
        ));
    }

    /// A message of `r-general` as the server would return it: author `alice`, time `ts`.
    fn server(id: &str, ts: i64, body: &str) -> Message {
        Message {
            edited: false,
            plain_text: true,
            id: id.to_string(),
            room_id: rid("r-general"),
            author: "alice".to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    /// A message this client composed, in `send` state.
    fn local(id: &str, send: SendState) -> Message {
        Message {
            author: "me".to_string(),
            ..server(id, 1, "typed")
        }
        .with_send(send)
    }

    trait WithSend {
        fn with_send(self, send: SendState) -> Self;
    }

    impl WithSend for Message {
        fn with_send(mut self, send: SendState) -> Self {
            self.send = send;
            self
        }
    }

    fn send_effects(app: &App) -> usize {
        app.effects
            .iter()
            .filter(|e| matches!(e, Effect::SendMessage { .. }))
            .count()
    }

    #[test]
    fn first_page_confirms_held_pending() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        let room = rid("r-general");
        app.messages.insert(
            room.clone(),
            vec![
                local("u1", SendState::Unconfirmed),
                local("f1", SendState::Failed),
                local("s1", SendState::Sending),
            ],
        );
        app.msg_views.insert(
            room.clone(),
            MsgView {
                cursor: 0,
                top_row: 0,
            },
        );
        app.effects.clear();

        let page = vec![
            server("old1", 10, "older one"),
            server("old2", 11, "older two"),
            server("u1", 20, "typed"),
            server("f1", 21, "typed"),
            server("s1", 22, "typed"),
        ];
        update(
            &mut app,
            Action::HistoryLoaded {
                room: room.clone(),
                messages: page,
                oldest_ts: Some("x".to_string()),
                older: false,
            },
        );
        let held = &app.messages[&room];
        let ids: Vec<&str> = held.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["old1", "old2", "u1", "f1", "s1"]);
        for (index, ts) in [(2, 20), (3, 21), (4, 22)] {
            assert_eq!(held[index].send, SendState::Sent, "{}", held[index].id);
            assert_eq!(held[index].author, "alice");
            assert_eq!(held[index].ts, ts);
        }
        assert_eq!(app.msg_views[&room].cursor, 2, "still selects u1");
        assert_eq!(held[app.msg_views[&room].cursor].id, "u1");
        assert_eq!(
            app.awaiting_answer.get(&room).map(String::as_str),
            Some("s1")
        );
        assert_eq!(send_effects(&app), 0, "a confirmation records no send");
    }

    #[test]
    fn first_page_ignores_held_sent() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        let room = rid("r-general");
        let mut mine = local("h1", SendState::Sent);
        mine.body = "mine".to_string();
        app.messages.insert(room.clone(), vec![mine.clone()]);
        update(
            &mut app,
            Action::HistoryLoaded {
                room: room.clone(),
                messages: vec![
                    server("old1", 10, "older"),
                    server("h1", 99, "a different body"),
                ],
                oldest_ts: Some("x".to_string()),
                older: false,
            },
        );
        let held = &app.messages[&room];
        let ids: Vec<&str> = held.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["old1", "h1"], "no id twice");
        assert_eq!(held[1], mine, "the held Sent row is left exactly as it was");
        assert!(app.awaiting_answer.is_empty());
    }

    #[test]
    fn older_page_confirms_held_pending() {
        let mut app = loaded_general();
        let room = rid("r-general");
        app.messages
            .get_mut(&room)
            .expect("room")
            .push(local("f1", SendState::Failed));
        press(&mut app, "gg");
        let id = app.messages[&room][0].id.clone();
        let starts = messages::row_starts(&app, &room);
        let before = messages::view(&app, &room);
        let offset = starts[before.cursor] as isize - before.top_row as isize;

        update(
            &mut app,
            older_action(
                vec![server("p000", 0, "body p000"), server("f1", 77, "typed")],
                Some("x"),
            ),
        );
        let held = &app.messages[&room];
        assert_eq!(held.len(), 52);
        assert_eq!(held.iter().filter(|m| m.id == "f1").count(), 1);
        assert_eq!(held.iter().filter(|m| m.id == "p000").count(), 1);
        assert_eq!(held[0].id, "p000");
        let f1 = held.last().expect("last");
        assert_eq!(f1.id, "f1", "replaced in place at the end");
        assert_eq!(f1.send, SendState::Sent);
        assert_eq!(f1.ts, 77);
        let after = messages::view(&app, &room);
        assert_eq!(held[after.cursor].id, id, "same message selected");
        let starts = messages::row_starts(&app, &room);
        assert_eq!(
            starts[after.cursor] as isize - after.top_row as isize,
            offset,
            "same screen row"
        );
    }

    #[test]
    fn older_page_ignores_held_sent() {
        let mut app = loaded_general();
        let room = rid("r-general");
        press(&mut app, "gg");
        let held_before = app.messages[&room][0].clone();
        let mut repeated = held_before.clone();
        repeated.body = "different".to_string();
        repeated.ts += 1000;
        update(
            &mut app,
            older_action(vec![repeated, server("b000", 0, "body b000")], Some("x")),
        );
        let held = &app.messages[&room];
        assert_eq!(held.len(), 51);
        assert_eq!(held[0].id, "b000");
        assert_eq!(held[1], held_before, "the held Sent row is unchanged");
        assert_eq!(held.iter().filter(|m| m.id == held_before.id).count(), 1);
    }

    #[test]
    fn no_trigger_outside_a_loaded_room() {
        // Loading, Failed, no state and no room never request an older page.
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        app.effects.clear();
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: run("n", 0, 5),
                oldest_ts: Some(NEWEST_TS.to_string()),
                older: false,
            },
        );
        app.history.insert(rid("r-general"), HistoryState::Loading);
        press(&mut app, "gg");
        app.history.insert(rid("r-general"), HistoryState::Failed);
        press(&mut app, "gg");
        app.history.remove(&rid("r-general"));
        press(&mut app, "gg");
        assert!(app.effects.is_empty(), "{:?}", app.effects);
    }
}
