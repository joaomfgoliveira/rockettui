//! What the reducer does with realtime events (`realtime-ddp`). Pure: no IO, no clock.
//!
//! The realtime task owns the socket and the timing; it only sends `Action`s. The decisions
//! about state live here:
//! - a live message is appended only to a room whose history is Loaded; for any other room the
//!   history fetch on first open brings it, and only the unread flag (from the subscription
//!   update) changes, so a live message never makes an unopened room look loaded;
//! - after a reconnect the open room refetches its newest page and every other loaded room is
//!   marked stale (`App.stale`), refetched when it is next opened (`history::on_open`).

use crate::app::history::{self, HistoryState};
use crate::app::{App, RtState, messages, sidebar};
use crate::data::{Message, RoomId};

/// A new message arrived live. It joins the room only when the room's history is Loaded, through
/// `messages::merge_incoming` (a held id is never duplicated, an echo of a pending send confirms
/// it in place). A reader at the bottom keeps following; a reader scrolled up stays on the same
/// message, because an append shifts no index.
pub fn on_message(app: &mut App, message: Message) {
    let room = message.room_id.clone();
    if !matches!(app.history.get(&room), Some(HistoryState::Loaded { .. })) {
        return;
    }
    // A held server message whose text or edited state changed is an edit: replace it in place.
    if messages::is_edit(app, &room, &message) {
        messages::replace_edited(app, &room, message);
        return;
    }
    // A detached room shows older history: a new message would sit after a gap (`app::jump`).
    if super::jump::is_detached(app, &room) {
        return;
    }
    let at_bottom = messages::is_at_bottom(app, &room);
    if messages::merge_incoming(app, &room, message) == messages::Merge::Appended && at_bottom {
        messages::follow_newest(app, &room);
    }
}

/// A message of `room` was deleted: a held server message becomes a tombstone. A room that is not
/// loaded, an unknown id and a pending local row are ignored.
pub fn on_deleted(app: &mut App, room: &RoomId, id: &str) {
    messages::tombstone(app, room, id);
}

/// The server reported the unread state of one subscription (set, or cleared by another client).
/// The room moves between the sidebar sections, so the cursor is put back on the room it was on.
/// An open room stays flagged until the server says otherwise.
pub fn on_subscription(app: &mut App, room: &RoomId, unread: bool) {
    let selected = sidebar::selected_room(app);
    let Some(held) = app.rooms.iter_mut().find(|r| &r.id == room) else {
        return;
    };
    if held.unread == unread {
        return;
    }
    held.unread = unread;
    let moved = selected.and_then(|id| {
        sidebar::rows(app).iter().position(
            |row| matches!(row, sidebar::SidebarRow::Room { id: row_id, .. } if *row_id == id),
        )
    });
    match moved {
        Some(index) => app.sidebar.cursor = index,
        None => sidebar::on_rows_changed(app),
    }
}

/// The connection is live. Messages may have been posted while it was not (an outage, `:reconnect`,
/// or the moment between a room's first fetch and the first connection), so every connection runs
/// gap recovery: the open room refetches its newest page now and every other loaded room is marked
/// stale. At startup no room is loaded yet, so that costs nothing.
pub fn on_connected(app: &mut App) {
    app.rt = RtState::Live;
    let loaded: Vec<RoomId> = app
        .history
        .iter()
        .filter(|(_, state)| matches!(state, HistoryState::Loaded { .. }))
        .map(|(id, _)| id.clone())
        .collect();
    for id in loaded {
        if app.current_room.as_ref() == Some(&id) {
            history::refetch_newest(app, &id);
        } else {
            app.stale.insert(id);
        }
    }
}

pub fn on_disconnected(app: &mut App) {
    app.rt = RtState::Reconnecting;
}

pub fn on_rejected(app: &mut App) {
    app.rt = RtState::Unavailable;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::api::map;
    use crate::api::wire::{RoomsResp, SubscriptionsResp};
    use crate::app::history::HistoryState;
    use crate::app::update::update;
    use crate::data::{MessageKind, SendState};
    use crate::effect::Effect;
    use crate::test_util::{press, render_to_string};
    use ratatui::layout::Size;

    const SUBS: &str = include_str!("../api/fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("../api/fixtures/rooms.json");

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

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

    fn msg(room: &str, id: &str, ts: i64) -> Message {
        Message {
            edited: false,
            plain_text: true,
            id: id.to_string(),
            room_id: rid(room),
            author: "alice".to_string(),
            ts,
            body: format!("body of {id}"),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    /// Deliver the first page of `room` as the server would, so the room is Loaded.
    fn load(app: &mut App, room: &str, page: Vec<Message>) {
        update(
            app,
            Action::HistoryLoaded {
                room: rid(room),
                messages: page,
                oldest_ts: Some("2026-11-16T09:00:00.000Z".to_string()),
                older: false,
            },
        );
    }

    fn ids(app: &App, room: &str) -> Vec<String> {
        app.messages
            .get(&rid(room))
            .map(|v| v.iter().map(|m| m.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Open `r-general` and load a page of `count` messages `m0..`, ts 100, 110, ...
    fn open_general(count: usize) -> App {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        let page = (0..count)
            .map(|i| msg("r-general", &format!("m{i}"), 100 + 10 * i as i64))
            .collect();
        load(&mut app, "r-general", page);
        app.effects.clear();
        app
    }

    fn loads(app: &App) -> Vec<&Effect> {
        app.effects
            .iter()
            .filter(|e| matches!(e, Effect::LoadHistory { .. }))
            .collect()
    }

    // ---- live messages (2.1, 2.3)

    #[test]
    fn live_message_for_an_unloaded_room_is_not_stored_as_history() {
        let mut app = fixture_app();
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 10)));
        assert!(
            app.messages
                .get(&rid("r-general"))
                .is_none_or(Vec::is_empty),
            "an unloaded room must not hold a live message"
        );
        // The first open still fetches history, as if no live message had arrived.
        press(&mut app, "<CR>");
        assert!(
            matches!(
                loads(&app).as_slice(),
                [Effect::LoadHistory { room, before: None, .. }] if room == &rid("r-general")
            ),
            "{:?}",
            app.effects
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loading)
        );
        // The page that brings the message shows it once.
        load(
            &mut app,
            "r-general",
            vec![
                msg("r-general", "live-1", 10),
                msg("r-general", "live-2", 11),
            ],
        );
        assert_eq!(ids(&app, "r-general"), ["live-1", "live-2"]);
    }

    #[test]
    fn live_message_in_the_open_room_appears_and_follows() {
        let mut app = open_general(3);
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 200)));
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2", "live-1"]);
        assert!(app.messages[&rid("r-general")][3].send == SendState::Sent);
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("body of live-1"), "{screen}");
    }

    #[test]
    fn live_message_in_another_loaded_room_is_held() {
        let mut app = open_general(2);
        let other = app
            .rooms
            .iter()
            .map(|r| r.id.clone())
            .find(|id| {
                id != &rid("r-general") && !app.rooms.iter().any(|r| &r.id == id && r.encrypted)
            })
            .expect("another room");
        load(&mut app, &other.0, vec![msg(&other.0, "o0", 50)]);
        update(&mut app, Action::RtMessage(msg(&other.0, "o1", 60)));
        assert_eq!(ids(&app, &other.0), ["o0", "o1"]);
        assert_eq!(ids(&app, "r-general"), ["m0", "m1"]);
    }

    #[test]
    fn duplicate_live_message_is_held_once() {
        let mut app = open_general(2);
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 200)));
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 200)));
        // A message already in the fetched page is not added again either.
        update(&mut app, Action::RtMessage(msg("r-general", "m1", 110)));
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "live-1"]);
    }

    #[test]
    fn echo_of_a_pending_send_confirms_it_in_place() {
        let mut app = open_general(1);
        app.messages
            .get_mut(&rid("r-general"))
            .unwrap()
            .push(Message {
                send: SendState::Sending,
                ..msg("r-general", "rt-mine-1", 300)
            });
        let mut echo = msg("r-general", "rt-mine-1", 301);
        echo.body = "stored copy".to_string();
        update(&mut app, Action::RtMessage(echo));
        assert_eq!(ids(&app, "r-general"), ["m0", "rt-mine-1"]);
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!(held.send, SendState::Sent);
        assert_eq!(held.body, "stored copy");
    }

    #[test]
    fn reader_scrolled_up_keeps_the_cursor_message() {
        let mut app = open_general(30);
        press(&mut app, "gg");
        let before = crate::app::messages::view(&app, &rid("r-general"));
        assert_eq!(before.cursor, 0);
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 900)));
        let after = crate::app::messages::view(&app, &rid("r-general"));
        assert_eq!(after, before, "the pane did not move");
        assert_eq!(ids(&app, "r-general").last().unwrap(), "live-1");
    }

    // ---- live edits and deletes

    fn body_of(app: &App, room: &str, id: &str) -> String {
        app.messages[&rid(room)]
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.body.clone())
            .expect("held message")
    }

    fn edited_copy(room: &str, id: &str, ts: i64, body: &str) -> Message {
        Message {
            body: body.to_string(),
            edited: true,
            plain_text: true,
            ..msg(room, id, ts)
        }
    }

    /// Cursor message id and the screen row it sits on, for the open `r-general`.
    fn anchor_of(app: &App) -> (String, isize) {
        let room = rid("r-general");
        let view = crate::app::messages::view(app, &room);
        let starts = crate::app::messages::row_starts(app, &room);
        (
            app.messages[&room][view.cursor].id.clone(),
            starts[view.cursor] as isize - view.top_row as isize,
        )
    }

    #[test]
    fn edit_in_the_open_room_replaces_in_place() {
        let mut app = open_general(3);
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "m1", 110, "new words")),
        );
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2"]);
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!(held.body, "new words");
        assert!(held.edited);
        assert_eq!(held.send, SendState::Sent);
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("new words"), "{screen}");
        assert!(!screen.contains("body of m1"), "{screen}");
    }

    #[test]
    fn edit_while_scrolled_up_keeps_cursor_message_and_screen_row() {
        let mut app = open_general(30);
        let room = rid("r-general");
        let starts = crate::app::messages::row_starts(&app, &room);
        app.msg_views.insert(
            room.clone(),
            crate::app::messages::MsgView {
                cursor: 20,
                top_row: starts[20] - 2,
            },
        );
        let before = anchor_of(&app);
        assert_eq!(before.0, "m20");
        // An edit above the cursor that grows to several rows shifts everything below it.
        let long = "word ".repeat(80);
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "m5", 150, &long)),
        );
        assert_eq!(body_of(&app, "r-general", "m5"), long);
        let after_starts = crate::app::messages::row_starts(&app, &room);
        assert!(
            after_starts[20] > starts[20],
            "the edit must shift the rows below it"
        );
        assert_eq!(anchor_of(&app), before, "same message at the same row");
    }

    #[test]
    fn redelivery_without_a_change_is_ignored() {
        let mut app = open_general(2);
        let before = app.messages[&rid("r-general")].clone();
        // A reaction or a pin: the same message again.
        update(&mut app, Action::RtMessage(msg("r-general", "m1", 110)));
        assert_eq!(app.messages[&rid("r-general")], before);
        // An edited message delivered twice changes only the first time.
        let edit = edited_copy("r-general", "m1", 110, "edited once");
        update(&mut app, Action::RtMessage(edit.clone()));
        let once = app.messages[&rid("r-general")].clone();
        update(&mut app, Action::RtMessage(edit));
        assert_eq!(app.messages[&rid("r-general")], once);
    }

    #[test]
    fn edit_of_a_message_not_held_is_appended_like_any_live_message() {
        let mut app = open_general(2);
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "unseen", 500, "late edit")),
        );
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "unseen"]);
        // And for a room that is not loaded, nothing is stored.
        let mut app = fixture_app();
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "unseen", 500, "late edit")),
        );
        assert!(ids(&app, "r-general").is_empty());
    }

    #[test]
    fn edit_of_a_pending_local_row_confirms_it_as_before() {
        let mut app = open_general(1);
        app.messages
            .get_mut(&rid("r-general"))
            .unwrap()
            .push(Message {
                send: SendState::Failed,
                ..msg("r-general", "rt-mine-1", 300)
            });
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "rt-mine-1", 301, "stored")),
        );
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!(held.send, SendState::Sent);
        assert_eq!(held.body, "stored");
        assert_eq!(ids(&app, "r-general"), ["m0", "rt-mine-1"]);
    }

    #[test]
    fn delete_becomes_a_tombstone_that_keeps_author_and_time() {
        let mut app = open_general(3);
        update(
            &mut app,
            Action::RtDeleted {
                room: rid("r-general"),
                id: "m1".to_string(),
            },
        );
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2"]);
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!(held.kind, MessageKind::Deleted);
        assert_eq!(held.body, "");
        assert_eq!(held.author, "alice");
        assert_eq!(held.ts, 110);
        assert_eq!(held.send, SendState::Sent);
    }

    #[test]
    fn screen_shows_the_edited_marker_and_the_tombstone() {
        let mut app = open_general(3);
        update(
            &mut app,
            Action::RtMessage(edited_copy("r-general", "m0", 100, "second thoughts")),
        );
        update(
            &mut app,
            Action::RtDeleted {
                room: rid("r-general"),
                id: "m1".to_string(),
            },
        );
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("second thoughts (edited)"), "{screen}");
        assert!(screen.contains("alice  message deleted"), "{screen}");
        assert!(!screen.contains("body of m1"), "{screen}");
        assert!(screen.contains("body of m2"), "{screen}");
    }

    #[test]
    fn live_rm_update_becomes_a_tombstone_keeping_the_author() {
        let mut app = open_general(3);
        update(
            &mut app,
            Action::RtMessage(Message {
                kind: MessageKind::Deleted,
                body: String::new(),
                author: "the-deleter".to_string(),
                ts: 999,
                ..msg("r-general", "m1", 110)
            }),
        );
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!(held.kind, MessageKind::Deleted);
        assert_eq!((held.author.as_str(), held.ts), ("alice", 110));
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2"]);
    }

    #[test]
    fn delete_in_another_loaded_room_is_applied() {
        let mut app = open_general(2);
        let other = app
            .rooms
            .iter()
            .map(|r| r.id.clone())
            .find(|id| {
                id != &rid("r-general") && !app.rooms.iter().any(|r| &r.id == id && r.encrypted)
            })
            .expect("another room");
        load(&mut app, &other.0, vec![msg(&other.0, "o0", 50)]);
        update(
            &mut app,
            Action::RtDeleted {
                room: other.clone(),
                id: "o0".to_string(),
            },
        );
        assert_eq!(app.messages[&other][0].kind, MessageKind::Deleted);
        assert_eq!(app.messages[&rid("r-general")][0].kind, MessageKind::Normal);
    }

    #[test]
    fn delete_of_unknown_pending_or_repeated_changes_nothing_more() {
        let mut app = open_general(2);
        app.messages
            .get_mut(&rid("r-general"))
            .unwrap()
            .push(Message {
                send: SendState::Sending,
                ..msg("r-general", "rt-mine-1", 300)
            });
        let before = app.messages[&rid("r-general")].clone();
        for (room, id) in [
            ("r-general", "nope"),
            ("r-general", "rt-mine-1"),
            ("nowhere", "m0"),
        ] {
            update(
                &mut app,
                Action::RtDeleted {
                    room: rid(room),
                    id: id.to_string(),
                },
            );
            assert_eq!(app.messages[&rid("r-general")], before, "{room}/{id}");
        }
        assert!(!app.messages.contains_key(&rid("nowhere")));
        let delete = || Action::RtDeleted {
            room: rid("r-general"),
            id: "m0".to_string(),
        };
        update(&mut app, delete());
        let once = app.messages[&rid("r-general")].clone();
        update(&mut app, delete());
        assert_eq!(app.messages[&rid("r-general")], once);
        // A live copy of a deleted message does not bring it back.
        update(&mut app, Action::RtMessage(msg("r-general", "m0", 100)));
        assert_eq!(app.messages[&rid("r-general")], once);
    }

    #[test]
    fn delete_keeps_the_cursor_and_the_stored_view() {
        let mut app = open_general(30);
        let room = rid("r-general");
        let starts = crate::app::messages::row_starts(&app, &room);
        app.msg_views.insert(
            room.clone(),
            crate::app::messages::MsgView {
                cursor: 20,
                top_row: starts[20] - 2,
            },
        );
        let before = anchor_of(&app);
        // Delete one above and the cursor message itself.
        for id in ["m5", "m20"] {
            update(
                &mut app,
                Action::RtDeleted {
                    room: room.clone(),
                    id: id.to_string(),
                },
            );
            assert_eq!(anchor_of(&app), before, "after deleting {id}");
        }
        assert_eq!(app.msg_views[&room].cursor, 20);
        assert_eq!(ids(&app, "r-general").len(), 30);
    }

    // ---- live unread (2.4)

    #[test]
    fn subscription_update_sets_and_clears_unread() {
        let mut app = fixture_app();
        let quiet = app
            .rooms
            .iter()
            .find(|r| !r.unread)
            .map(|r| r.id.clone())
            .expect("a read room");
        update(
            &mut app,
            Action::RtSubscription {
                room: quiet.clone(),
                unread: true,
            },
        );
        assert!(app.rooms.iter().find(|r| r.id == quiet).unwrap().unread);
        assert!(sidebar::rows(&app).iter().any(|row| matches!(
            row,
            sidebar::SidebarRow::Room { id, .. } if id == &quiet
        )));
        update(
            &mut app,
            Action::RtSubscription {
                room: quiet.clone(),
                unread: false,
            },
        );
        assert!(!app.rooms.iter().find(|r| r.id == quiet).unwrap().unread);
        // An unknown room is ignored.
        update(
            &mut app,
            Action::RtSubscription {
                room: rid("nope"),
                unread: true,
            },
        );
    }

    #[test]
    fn subscription_update_keeps_the_sidebar_cursor_on_its_room() {
        let mut app = fixture_app();
        let before = sidebar::selected_room(&app).expect("selected");
        let other = app
            .rooms
            .iter()
            .find(|r| !r.unread && r.id != before)
            .map(|r| r.id.clone())
            .expect("a read room");
        update(
            &mut app,
            Action::RtSubscription {
                room: other,
                unread: true,
            },
        );
        assert_eq!(sidebar::selected_room(&app), Some(before));
    }

    #[test]
    fn open_room_stays_flagged_and_opening_does_not_mark_read() {
        let mut app = open_general(1);
        update(
            &mut app,
            Action::RtSubscription {
                room: rid("r-general"),
                unread: true,
            },
        );
        update(&mut app, Action::RtMessage(msg("r-general", "live-1", 200)));
        assert!(
            app.rooms
                .iter()
                .find(|r| r.id == rid("r-general"))
                .unwrap()
                .unread
        );
        // Every effect of opening is a GET: nothing marks the room read.
        assert!(
            app.effects
                .iter()
                .all(|e| matches!(e, Effect::LoadHistory { .. }))
        );
    }

    // ---- stale rooms and gap recovery (2.5, 2.6)

    fn second_room(app: &App) -> RoomId {
        app.rooms
            .iter()
            .find(|r| r.id != rid("r-general") && !r.encrypted)
            .map(|r| r.id.clone())
            .expect("another room")
    }

    #[test]
    fn connecting_with_nothing_loaded_needs_no_recovery() {
        let mut app = fixture_app();
        update(&mut app, Action::RtConnected);
        assert_eq!(app.rt, RtState::Live);
        assert!(app.effects.is_empty());
        assert!(app.stale.is_empty());
    }

    #[test]
    fn reconnect_refetches_the_open_room_and_marks_others_stale() {
        let mut app = open_general(2);
        let other = second_room(&app);
        load(&mut app, &other.0, vec![msg(&other.0, "o0", 50)]);
        update(&mut app, Action::RtDisconnected);
        assert_eq!(app.rt, RtState::Reconnecting);
        update(&mut app, Action::RtConnected);
        assert_eq!(app.rt, RtState::Live);
        assert!(matches!(
            loads(&app).as_slice(),
            [Effect::LoadHistory { room, before: None, .. }] if room == &rid("r-general")
        ));
        assert_eq!(app.stale.iter().collect::<Vec<_>>(), [&other]);
        // A room never loaded is not marked stale.
        assert!(!app.stale.contains(&rid("r-never")));
    }

    #[test]
    fn stale_room_refetches_once_on_reopen_and_merges_by_id() {
        let mut app = open_general(2);
        let other = second_room(&app);
        load(&mut app, &other.0, vec![msg(&other.0, "o0", 50)]);
        app.stale.insert(other.clone());
        app.effects.clear();
        // Not stale: no refetch.
        app.open_room(rid("r-general"));
        history::on_open(&mut app, &rid("r-general"));
        assert!(app.effects.is_empty());
        // Stale: one refetch, state stays Loaded, flag cleared.
        app.open_room(other.clone());
        history::on_open(&mut app, &other);
        assert!(matches!(
            loads(&app).as_slice(),
            [Effect::LoadHistory { room, before: None, .. }] if room == &other
        ));
        assert!(matches!(
            app.history.get(&other),
            Some(HistoryState::Loaded { .. })
        ));
        assert!(app.stale.is_empty());
        app.effects.clear();
        load(
            &mut app,
            &other.0,
            vec![msg(&other.0, "o0", 50), msg(&other.0, "o1", 60)],
        );
        assert_eq!(ids(&app, &other.0), ["o0", "o1"]);
        history::on_open(&mut app, &other);
        assert!(app.effects.is_empty(), "reopening again does not refetch");
    }

    #[test]
    fn failed_refetch_keeps_the_room_loaded_and_stale() {
        let mut app = open_general(2);
        app.stale.insert(rid("r-general"));
        history::on_open(&mut app, &rid("r-general"));
        update(
            &mut app,
            Action::HistoryFailed {
                room: rid("r-general"),
                older: false,
                error: crate::api::ApiError::Timeout,
            },
        );
        assert!(matches!(
            app.history.get(&rid("r-general")),
            Some(HistoryState::Loaded { .. })
        ));
        assert!(app.stale.contains(&rid("r-general")));
        assert_eq!(ids(&app, "r-general"), ["m0", "m1"]);
    }

    #[test]
    fn short_gap_is_merged_without_duplicates() {
        let mut app = open_general(3);
        load(
            &mut app,
            "r-general",
            (0..6)
                .map(|i| msg("r-general", &format!("m{i}"), 100 + 10 * i))
                .collect(),
        );
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2", "m3", "m4", "m5"]);
    }

    #[test]
    fn no_gap_changes_nothing() {
        let mut app = open_general(3);
        load(
            &mut app,
            "r-general",
            (0..3)
                .map(|i| msg("r-general", &format!("m{i}"), 100 + 10 * i))
                .collect(),
        );
        assert_eq!(ids(&app, "r-general"), ["m0", "m1", "m2"]);
    }

    #[test]
    fn refetch_keeps_the_cursor_message() {
        let mut app = open_general(30);
        press(&mut app, "gg");
        press(&mut app, "5j");
        let room = rid("r-general");
        let cursor_id = |app: &App| {
            let v = crate::app::messages::view(app, &room);
            app.messages[&room][v.cursor].id.clone()
        };
        assert_eq!(cursor_id(&app), "m5");
        // The page also brings a message older than everything held (inserted before the cursor).
        let mut page: Vec<Message> = (25..30)
            .map(|i| msg("r-general", &format!("m{i}"), 100 + 10 * i))
            .collect();
        page.insert(0, msg("r-general", "early", 99));
        page.push(msg("r-general", "late", 1000));
        // Not overlapping the held ids would reload; m25.. overlap.
        load(&mut app, "r-general", page);
        assert_eq!(cursor_id(&app), "m5");
        assert_eq!(ids(&app, "r-general").last().unwrap(), "late");
        assert_eq!(
            ids(&app, "r-general")
                .iter()
                .filter(|i| *i == "early")
                .count(),
            1
        );
    }

    /// The newest `count` messages of a room that held `m0..`, as a refetched page: from `from`.
    fn page_from(from: i64, to: i64) -> Vec<Message> {
        (from..to)
            .map(|i| msg("r-general", &format!("m{i}"), 100 + 10 * i))
            .collect()
    }

    fn kind_of(app: &App, id: &str) -> MessageKind {
        app.messages[&rid("r-general")]
            .iter()
            .find(|m| m.id == id)
            .expect("held")
            .kind
    }

    #[test]
    fn refetch_takes_edits_made_while_offline() {
        let mut app = open_general(10);
        let mut page = page_from(4, 10);
        page[2] = edited_copy("r-general", "m6", 160, "edited offline");
        load(&mut app, "r-general", page);
        assert_eq!(ids(&app, "r-general").len(), 10);
        let held = &app.messages[&rid("r-general")][6];
        assert_eq!(held.id, "m6");
        assert_eq!(held.body, "edited offline");
        assert!(held.edited);
        // Messages the page did not edit stay as they were.
        assert_eq!(body_of(&app, "r-general", "m7"), "body of m7");
    }

    #[test]
    fn refetch_tombstones_held_messages_the_page_lacks_inside_its_range() {
        let mut app = open_general(10);
        // m6 was deleted while offline; the page covers m4..m9 without it.
        let mut page = page_from(4, 10);
        page.remove(2);
        load(&mut app, "r-general", page);
        assert_eq!(ids(&app, "r-general").len(), 10);
        assert_eq!(kind_of(&app, "m6"), MessageKind::Deleted);
        assert_eq!(body_of(&app, "r-general", "m6"), "");
        for id in ["m4", "m5", "m7", "m8", "m9"] {
            assert_eq!(kind_of(&app, id), MessageKind::Normal, "{id}");
        }
    }

    #[test]
    fn refetch_leaves_messages_older_than_the_page_alone() {
        let mut app = open_general(10);
        // m1 and m2 were deleted and m3 edited while offline, but all are older than the page.
        load(&mut app, "r-general", page_from(6, 10));
        for i in 0..6 {
            assert_eq!(kind_of(&app, &format!("m{i}")), MessageKind::Normal);
            assert!(!app.messages[&rid("r-general")][i].edited);
        }
    }

    #[test]
    fn refetch_does_not_judge_the_page_edges_or_unsent_rows() {
        let mut app = open_general(10);
        app.messages
            .get_mut(&rid("r-general"))
            .unwrap()
            .push(Message {
                send: SendState::Failed,
                ..msg("r-general", "rt-mine-1", 150)
            });
        // A live message in the newest second that the request did not see, and one in the oldest
        // second that lies beyond the page: neither is judged. Nor is the unsent row inside it.
        app.messages.get_mut(&rid("r-general")).unwrap().extend([
            msg("r-general", "same-second-new", 190),
            msg("r-general", "same-second-old", 140),
        ]);
        let mut page = page_from(4, 10);
        page.retain(|m| m.id != "m9");
        load(&mut app, "r-general", page);
        assert_eq!(kind_of(&app, "same-second-old"), MessageKind::Normal);
        assert_eq!(kind_of(&app, "rt-mine-1"), MessageKind::Normal);
        assert_eq!(
            app.messages[&rid("r-general")]
                .iter()
                .find(|m| m.id == "rt-mine-1")
                .unwrap()
                .send,
            SendState::Failed
        );
        assert_eq!(kind_of(&app, "same-second-new"), MessageKind::Normal);
    }

    #[test]
    fn refetch_reconcile_keeps_the_cursor_message_and_row() {
        let mut app = open_general(30);
        let room = rid("r-general");
        let starts = crate::app::messages::row_starts(&app, &room);
        app.msg_views.insert(
            room.clone(),
            crate::app::messages::MsgView {
                cursor: 20,
                top_row: starts[20].saturating_sub(2),
            },
        );
        let before = anchor_of(&app);
        let mut page = page_from(10, 30);
        page[3] = edited_copy("r-general", "m13", 230, &"word ".repeat(80));
        page.remove(1); // m11 deleted offline
        load(&mut app, "r-general", page);
        assert_eq!(kind_of(&app, "m11"), MessageKind::Deleted);
        assert_eq!(anchor_of(&app), before);
    }

    #[test]
    fn gap_larger_than_a_page_reloads_the_room() {
        let mut app = open_general(3);
        press(&mut app, "gg");
        // A pending send must survive the reload.
        app.messages
            .get_mut(&rid("r-general"))
            .unwrap()
            .push(Message {
                send: SendState::Failed,
                ..msg("r-general", "rt-mine-1", 150)
            });
        // The newest page shares no id with what is held.
        let page: Vec<Message> = (100..103)
            .map(|i| msg("r-general", &format!("n{i}"), 1000 + i))
            .collect();
        load(&mut app, "r-general", page);
        assert_eq!(
            ids(&app, "r-general"),
            ["n100", "n101", "n102", "rt-mine-1"]
        );
        assert!(
            !app.msg_views.contains_key(&rid("r-general")),
            "shows the newest again"
        );
        assert!(matches!(
            app.history.get(&rid("r-general")),
            Some(HistoryState::Loaded { .. })
        ));
    }

    #[test]
    fn reconnect_command_restarts_realtime() {
        let mut app = open_general(1);
        update(&mut app, Action::RtRejected);
        assert_eq!(app.rt, RtState::Unavailable);
        press(&mut app, ":reconnect<CR>");
        assert!(app.restart_realtime);
        assert_eq!(app.rt, RtState::Reconnecting);
    }

    // ---- status line (2.7)

    #[test]
    fn status_line_shows_each_connection_state() {
        let mut app = open_general(1);
        let status = |app: &App| {
            render_to_string(app, 100, 24)
                .lines()
                .nth(22)
                .unwrap()
                .to_string()
        };
        assert!(!status(&app).contains("Reconnecting"));
        update(&mut app, Action::RtConnected);
        assert!(!status(&app).contains("Reconnecting"));
        update(&mut app, Action::RtDisconnected);
        assert!(status(&app).contains("Reconnecting"), "{}", status(&app));
        update(&mut app, Action::RtConnected);
        assert!(!status(&app).contains("Reconnecting"));
        update(&mut app, Action::RtRejected);
        let line = status(&app);
        assert!(line.contains("live updates unavailable"), "{line}");
    }
}
