//! Sending a message (MSG-02): composing the pending row, the per-room queue and the result path.
//!
//! The functions here are pure: they read and change `App` and never do IO. A send attempt is an
//! `Effect::SendMessage`; its answer arrives as `Action::MessageSent` or `Action::SendFailed`.
//!
//! Rules this module keeps:
//! - Enter in Insert mode appends a pending row at once (D-01) and clears the draft (D-02); the
//!   text goes out exactly as typed, with no trim and no normalization;
//! - at most one request is in flight per room: a message is `Queued` until the room has no
//!   `Sending` message, and every result starts the next queued message of its room (D-03);
//! - a message that failed never holds back the ones queued after it (D-16);
//! - a failure whose outcome is unknown (timeout, 5xx, lost connection) is `Unconfirmed`, not
//!   `Failed`: its retry asks the server first, so a lost answer never becomes a duplicate (D-07);
//! - a result is applied to the room the message was sent from, found by room and message id,
//!   never to the current room (the user may have opened another room meanwhile);
//! - a server answer replaces the message in place, at the same index, so nothing moves and no
//!   history is fetched again (D-04);
//! - a result for a message the room no longer holds changes nothing;
//! - the status line for a failure is one fixed string per error kind: never server text, never
//!   the message body, never a hint at `:reconnect`, which D-08 keeps separate from sending;
//! - a `Failed` or `Unconfirmed` message is recovered only by the user: `retry` puts it back in its
//!   room's queue under the same client id and `discard` removes it (D-06). Nothing resends on its
//!   own, on a timer, on `:reconnect` or when the next message is sent (D-08).

use crate::api::ApiError;
use crate::app::{App, Focus, LOCAL_AUTHOR, Mode, StatusMsg, messages};
use crate::config::settings::FollowOnSend;
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::effect::{Effect, SendText};

/// Enter in Insert mode (D-15): turn the draft into a pending message of the open room, clear the
/// draft, stay in Insert mode and start the send when the room has none in flight.
///
/// Nothing happens for a blank draft (D-09). With no room open, or an encrypted room open, the
/// draft is kept and the status bar says why; the encrypted guard sits before the draft is taken,
/// so no plaintext leaves the process (D-11). There is no check of history state, length or
/// permission here: the server decides (D-12), and a send while history is Loading or Failed goes
/// out like any other (D-19).
pub fn submit(app: &mut App) {
    if app.mode != Mode::Insert || app.input.text().trim().is_empty() {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        app.status_msg = Some(StatusMsg::Error(
            "No room open: select a room with Enter".into(),
        ));
        return;
    };
    if app.rooms.iter().any(|r| r.id == room && r.encrypted) {
        app.status_msg = Some(StatusMsg::Error("encrypted, not supported".into()));
        return;
    }
    let body = app.input.take();
    app.local_seq += 1;
    let message = Message {
        id: format!("{}{}", app.id_prefix, app.local_seq),
        room_id: room.clone(),
        author: app
            .own_username
            .clone()
            .unwrap_or_else(|| LOCAL_AUTHOR.to_string()),
        ts: app.clock_secs,
        body,
        kind: MessageKind::Normal,
        send: SendState::Queued { reconcile: false },
    };
    // Measured before the row is appended: an append shifts no existing index, so leaving
    // `msg_views` alone keeps the pane exactly where it was (D-05).
    let at_bottom = messages::is_at_bottom(app, &room);
    app.messages.entry(room.clone()).or_default().push(message);
    if app.settings.follow_on_send == FollowOnSend::Always || at_bottom {
        messages::follow_newest(app, &room);
    }
    pump(app, &room);
}

/// Start the next send of `room`: nothing while a message of the room is `Sending`; otherwise the
/// first `Queued` message in order becomes `Sending` and its effect is recorded. `Failed` and
/// `Unconfirmed` messages are skipped, so a failure never blocks the queue behind it (D-16).
pub fn pump(app: &mut App, room: &RoomId) {
    let Some(held) = app.messages.get_mut(room) else {
        return;
    };
    if held.iter().any(|m| m.send == SendState::Sending) {
        return;
    }
    let Some(next) = held
        .iter_mut()
        .find(|m| matches!(m.send, SendState::Queued { .. }))
    else {
        return;
    };
    let SendState::Queued { reconcile } = next.send else {
        return;
    };
    next.send = SendState::Sending;
    let effect = Effect::SendMessage {
        room: room.clone(),
        id: next.id.clone(),
        text: SendText::new(next.body.clone()),
        reconcile,
    };
    app.effects.push(effect);
}

/// The room and index of the message under the messages-pane cursor when it is `Failed` or
/// `Unconfirmed`: the one guard `retry` and `discard` share. `None` for any other message, for any
/// other pane and when no room is open or it holds nothing, so both keys are silent no-ops there.
/// The cursor is read from `messages::view` on every call, so a counted repeat acts on whichever
/// message sits under the cursor then.
fn unsent_under_cursor(app: &App) -> Option<(RoomId, usize)> {
    if app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.clone()?;
    let held = app.messages.get(&room)?;
    if held.is_empty() {
        return None;
    }
    let cursor = messages::view(app, &room).cursor;
    matches!(
        held.get(cursor)?.send,
        SendState::Failed | SendState::Unconfirmed
    )
    .then_some((room, cursor))
}

/// `r` (D-06): put the `Failed` or `Unconfirmed` message under the cursor back in its room's queue
/// under the same client id (D-07), at the same position (D-16: it may now land after messages typed
/// later). An `Unconfirmed` one is queued with `reconcile` set, so the request first asks the server
/// whether it already holds the message (D-15). A silent no-op on anything else. It touches no
/// connection state, no history and no other room (D-08).
pub fn retry(app: &mut App) {
    let Some((room, index)) = unsent_under_cursor(app) else {
        return;
    };
    let Some(message) = app
        .messages
        .get_mut(&room)
        .and_then(|held| held.get_mut(index))
    else {
        return;
    };
    message.send = SendState::Queued {
        reconcile: message.send == SendState::Unconfirmed,
    };
    pump(app, &room);
}

/// `dd` (D-06): remove the `Failed` or `Unconfirmed` message under the cursor from its room. A
/// silent no-op on anything else.
pub fn discard(app: &mut App) {
    let Some((room, index)) = unsent_under_cursor(app) else {
        return;
    };
    remove_at(app, &room, index);
    pump(app, &room);
}

/// Remove message `index` of `room`. A stored cursor below the removed message moves up by one, so
/// the same message stays selected (T-04-12); one on the removed message stays and now selects its
/// successor, which `messages::view` clamps at the end. A followed room has no stored view.
fn remove_at(app: &mut App, room: &RoomId, index: usize) {
    if let Some(held) = app.messages.get_mut(room)
        && index < held.len()
    {
        held.remove(index);
    }
    if let Some(view) = app.msg_views.get_mut(room)
        && view.cursor > index
    {
        view.cursor -= 1;
    }
}

/// The server accepted message `id` of `room` and answered with its stored copy: replace the held
/// message with that id by `message`, marked `Sent`, at the same index (D-04), then start the next
/// queued message of the room (D-03). When `room` holds no message with that id nothing changes.
pub fn on_sent(app: &mut App, room: RoomId, id: String, message: Message) {
    let Some(held) = app.messages.get_mut(&room) else {
        return;
    };
    if let Some(slot) = held.iter_mut().find(|m| m.id == id) {
        *slot = Message {
            send: SendState::Sent,
            ..message
        };
        pump(app, &room);
    }
}

/// The send of message `id` of `room` failed. A definite failure (the server refused it or the
/// request never left) makes the message `Failed`; an unknown outcome (a timeout, any 5xx, a lost
/// connection, an unreadable answer) makes it `Unconfirmed`, because the server may have stored it
/// (D-06, D-07, D-18). Either way it keeps its place, the fixed reason goes to the status bar, and
/// the next queued message of the room goes out (D-16). Nothing here resends it: only `retry` does.
pub fn on_failed(app: &mut App, room: RoomId, id: String, error: ApiError) {
    let unconfirmed = error.outcome_unknown();
    app.status_msg = Some(StatusMsg::Error(send_error_text(error, unconfirmed)));
    let Some(held) = app.messages.get_mut(&room) else {
        return;
    };
    if let Some(slot) = held
        .iter_mut()
        .find(|m| m.id == id && m.send == SendState::Sending)
    {
        slot.send = if unconfirmed {
            SendState::Unconfirmed
        } else {
            SendState::Failed
        };
        pump(app, &room);
    }
}

/// `message not sent: <reason>`, or `message may not have been sent: <reason>` when the outcome is
/// unknown, with one fixed reason per error kind. It is deliberately not `ApiError`'s `Display`,
/// whose hints point at `:reconnect` (D-08), and it never holds server text or the message body.
pub fn send_error_text(error: ApiError, unconfirmed: bool) -> String {
    let reason = match error {
        ApiError::Unauthorized => "unauthorized, check the user id and token".to_string(),
        ApiError::Forbidden => "not allowed in this room".to_string(),
        ApiError::NotFound => "send endpoint not found, check server_url".to_string(),
        ApiError::RateLimited => "rate limited by the server".to_string(),
        ApiError::Redirected => "server redirected, check server_url".to_string(),
        ApiError::Server(code) => format!("server error (HTTP {code})"),
        ApiError::Unreachable => "cannot connect".to_string(),
        ApiError::Timeout => "timed out".to_string(),
        ApiError::BadResponse => "unexpected response".to_string(),
        ApiError::TooLarge => "response too large".to_string(),
        ApiError::Client => "cannot build the HTTP client".to_string(),
        ApiError::Rejected => "rejected (read-only, muted or too long)".to_string(),
        ApiError::ConnectionLost => "connection lost".to_string(),
    };
    if unconfirmed {
        format!("message may not have been sent: {reason}")
    } else {
        format!("message not sent: {reason}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::Focus;
    use crate::app::update::update;
    use crate::test_util::{app_80x24, press};

    const BODY: &str = "SENTINEL-BODY-7f3a";

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn open_general_app() -> App {
        let mut app = app_80x24();
        app.open_room(general());
        app
    }

    fn msg(room: &RoomId, id: &str, author: &str, ts: i64, body: &str) -> Message {
        Message {
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    /// The send effects recorded so far, as (room id, message id, text, reconcile).
    fn sends(app: &App) -> Vec<(String, String, String, bool)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::SendMessage {
                    room,
                    id,
                    text,
                    reconcile,
                } => Some((
                    room.0.clone(),
                    id.clone(),
                    text.as_str().to_string(),
                    *reconcile,
                )),
                _ => None,
            })
            .collect()
    }

    /// The send states of the last `count` messages of the open room.
    fn states(app: &App, count: usize) -> Vec<SendState> {
        let held = &app.messages[&general()];
        held[held.len() - count..].iter().map(|m| m.send).collect()
    }

    #[test]
    fn submit_queues_and_sends() {
        let mut app = open_general_app();
        let before = app.messages[&general()].len();
        update(&mut app, Action::Clock(1_794_909_600));
        press(&mut app, "ihi there<CR>");

        let held = &app.messages[&general()];
        assert_eq!(held.len(), before + 1);
        let last = &held[before];
        assert_eq!(last.id, "local-1");
        assert_eq!(last.author, "me");
        assert_eq!(last.ts, 1_794_909_600);
        assert_eq!(last.body, "hi there");
        assert_eq!(last.send, SendState::Sending);
        assert_eq!(
            app.effects,
            vec![Effect::SendMessage {
                room: general(),
                id: "local-1".to_string(),
                text: SendText::new("hi there".to_string()),
                reconcile: false,
            }]
        );
        assert!(app.input.is_empty());
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(crate::app::messages::view(&app, &general()).cursor, before);
    }

    /// Answer the send of message `id` with the server's copy, through the reducer.
    fn answer_sent(app: &mut App, id: &str, ts: i64) {
        let message = msg(&general(), id, "smoke", ts, "answer");
        update(
            app,
            Action::MessageSent {
                room: general(),
                id: id.to_string(),
                message,
            },
        );
    }

    /// Fail the send of message `id`, through the reducer.
    fn answer_failed(app: &mut App, id: &str) {
        update(
            app,
            Action::SendFailed {
                room: general(),
                id: id.to_string(),
                error: ApiError::Forbidden,
            },
        );
    }

    #[test]
    fn guards_blank_no_room_encrypted() {
        // Blank drafts: nothing in the room, no effect, no status, still Insert (D-09).
        let mut app = open_general_app();
        let before = app.messages[&general()].len();
        press(&mut app, "i<CR>");
        press(&mut app, "   <CR>");
        press(&mut app, "<A-CR><A-CR><CR>");
        assert_eq!(app.messages[&general()].len(), before);
        assert!(app.effects.is_empty());
        assert!(app.status_msg.is_none());
        assert_eq!(app.mode, Mode::Insert);

        // No room open: the fixed status, the draft kept, nothing sent.
        let mut app = app_80x24();
        press(&mut app, "ihello<CR>");
        assert_eq!(app.input.text(), "hello");
        assert!(app.effects.is_empty());
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => {
                assert_eq!(text, "No room open: select a room with Enter");
            }
            _ => panic!("expected the no-room error"),
        }

        // An encrypted room refuses before the draft is taken (D-11).
        let secrets = RoomId("secrets".to_string());
        let mut app = app_80x24();
        assert!(app.rooms.iter().any(|r| r.id == secrets && r.encrypted));
        app.open_room(secrets.clone());
        let before = app.messages[&secrets].len();
        press(&mut app, "isecret draft<CR>");
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => assert_eq!(text, "encrypted, not supported"),
            _ => panic!("expected the encrypted-room error"),
        }
        assert_eq!(app.input.text(), "secret draft");
        assert_eq!(app.messages[&secrets].len(), before);
        assert!(app.effects.is_empty());
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn queue_one_in_flight_in_order() {
        let mut app = open_general_app();
        press(&mut app, "ia<CR>b<CR>c<CR>");
        assert_eq!(
            states(&app, 3),
            [
                SendState::Sending,
                SendState::Queued { reconcile: false },
                SendState::Queued { reconcile: false },
            ]
        );
        assert_eq!(sends(&app).len(), 1);

        answer_sent(&mut app, "local-1", 10);
        assert_eq!(
            states(&app, 3),
            [
                SendState::Sent,
                SendState::Sending,
                SendState::Queued { reconcile: false },
            ]
        );
        assert_eq!(sends(&app).len(), 2);

        // b fails: it stays at its index, marked Failed, and c goes out (D-16).
        answer_failed(&mut app, "local-2");
        assert_eq!(
            states(&app, 3),
            [SendState::Sent, SendState::Failed, SendState::Sending]
        );
        let ids: Vec<String> = sends(&app).into_iter().map(|s| s.1).collect();
        assert_eq!(ids, ["local-1", "local-2", "local-3"]);

        // A later Enter queues behind the in-flight c and goes out after it.
        press(&mut app, "d<CR>");
        assert_eq!(sends(&app).len(), 3);
        answer_sent(&mut app, "local-3", 12);
        let ids: Vec<String> = sends(&app).into_iter().map(|s| s.1).collect();
        assert_eq!(ids, ["local-1", "local-2", "local-3", "local-4"]);
    }

    #[test]
    fn fail_marks_failed_and_keeps_row() {
        let mut app = open_general_app();
        press(&mut app, &format!("i{BODY}<CR>"));
        let index = app.messages[&general()].len() - 1;
        assert_eq!(app.messages[&general()][index].send, SendState::Sending);
        let before = app.messages[&general()].len();

        answer_failed(&mut app, "local-1");
        let held = &app.messages[&general()];
        assert_eq!(held.len(), before);
        assert_eq!(held[index].id, "local-1");
        assert_eq!(held[index].send, SendState::Failed);
        assert_eq!(held[index].body, BODY, "the failed row keeps its text");
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => {
                assert!(text.starts_with("message not sent: "), "{text}");
                assert!(!text.contains("SENTINEL"), "{text}");
            }
            _ => panic!("expected a failure status"),
        }
    }

    /// `general` holding one message `local-1` that was sent and failed, the messages pane focused
    /// with the cursor on it, and no effect recorded.
    fn app_with_failed() -> App {
        let mut app = open_general_app();
        press(&mut app, "iboom<CR>");
        answer_failed(&mut app, "local-1");
        press(&mut app, "<Esc>");
        app.effects.clear();
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(
            app.messages[&general()].last().unwrap().send,
            SendState::Failed
        );
        app
    }

    /// `general` replaced by `[m0, F1, m2, m3]` with F1 `Failed` and no stored view.
    fn app_with_four() -> App {
        let mut app = open_general_app();
        let room = general();
        let mut failed = msg(&room, "F1", "me", 2, "failed text");
        failed.send = SendState::Failed;
        app.messages.insert(
            room.clone(),
            vec![
                msg(&room, "m0", "a", 1, "zero"),
                failed,
                msg(&room, "m2", "a", 3, "two"),
                msg(&room, "m3", "a", 4, "three"),
            ],
        );
        app.msg_views.clear();
        app
    }

    fn ids(app: &App) -> Vec<String> {
        app.messages[&general()]
            .iter()
            .map(|m| m.id.clone())
            .collect()
    }

    #[test]
    fn retry_failed_requeues_same_id() {
        let mut app = app_with_failed();
        press(&mut app, "r");
        assert_eq!(
            app.messages[&general()].last().unwrap().send,
            SendState::Sending
        );
        assert_eq!(
            sends(&app),
            vec![(
                "general".to_string(),
                "local-1".to_string(),
                "boom".to_string(),
                false
            )]
        );

        // An Unconfirmed message is retried with reconcile set (D-15).
        let mut app = app_with_failed();
        app.messages
            .get_mut(&general())
            .unwrap()
            .last_mut()
            .unwrap()
            .send = SendState::Unconfirmed;
        press(&mut app, "r");
        assert_eq!(
            app.messages[&general()].last().unwrap().send,
            SendState::Sending
        );
        assert_eq!(
            sends(&app),
            vec![(
                "general".to_string(),
                "local-1".to_string(),
                "boom".to_string(),
                true
            )]
        );
    }

    #[test]
    fn retry_waits_behind_a_message_in_flight() {
        let mut app = app_with_failed();
        press(&mut app, "ix<CR>");
        press(&mut app, "<Esc>");
        let last = app.messages[&general()].len() - 1;
        assert_eq!(app.messages[&general()][last].send, SendState::Sending);
        app.effects.clear();
        // Put the cursor on the Failed message (the one before the in-flight one).
        press(&mut app, "G");
        press(&mut app, "k");
        assert_eq!(app.messages[&general()][last - 1].send, SendState::Failed);
        press(&mut app, "r");
        // One request in flight per room: it is queued, not started.
        assert_eq!(
            app.messages[&general()][last - 1].send,
            SendState::Queued { reconcile: false }
        );
        assert!(sends(&app).is_empty());
        // A counted repeat does not queue anything twice.
        press(&mut app, "3r");
        assert!(sends(&app).is_empty());
    }

    #[test]
    fn retry_discard_noop_elsewhere() {
        // The cursor message is not Failed or Unconfirmed: nothing happens.
        for state in [
            SendState::Sent,
            SendState::Queued { reconcile: false },
            SendState::Sending,
        ] {
            let mut app = app_with_failed();
            app.messages
                .get_mut(&general())
                .unwrap()
                .last_mut()
                .unwrap()
                .send = state;
            let before = app.messages[&general()].clone();
            press(&mut app, "rdd");
            assert_eq!(app.messages[&general()], before, "{state:?}");
            assert!(app.effects.is_empty(), "{state:?}");
            assert!(app.status_msg.is_none(), "{state:?}");
        }

        // The pane is not the messages pane.
        for focus in [Focus::Sidebar, Focus::Input] {
            let mut app = app_with_failed();
            app.focus = focus;
            let before = app.messages[&general()].clone();
            press(&mut app, "rdd");
            assert_eq!(app.messages[&general()], before, "{focus:?}");
            assert!(app.effects.is_empty(), "{focus:?}");
            assert!(app.status_msg.is_none(), "{focus:?}");
        }

        // No room is open.
        let mut app = app_80x24();
        app.focus = Focus::Messages;
        assert!(app.current_room.is_none());
        press(&mut app, "rdd");
        assert!(app.effects.is_empty());
        assert!(app.status_msg.is_none());

        // A room that holds no message.
        let mut app = open_general_app();
        app.messages.insert(general(), Vec::new());
        press(&mut app, "rdd");
        assert!(app.effects.is_empty());
    }

    #[test]
    fn discard_removes_and_fixes_cursor() {
        // The cursor on F1 (index 1): dd removes it and the cursor stays at 1, now m2.
        let mut app = app_with_four();
        press(&mut app, "ggj");
        assert_eq!(messages::view(&app, &general()).cursor, 1);
        press(&mut app, "dd");
        assert_eq!(ids(&app), ["m0", "m2", "m3"]);
        assert_eq!(messages::view(&app, &general()).cursor, 1);
        assert_eq!(app.messages[&general()][1].id, "m2");
        assert!(app.effects.is_empty(), "a discard sends nothing");

        // A followed room (no stored view): the cursor stays on the newest message.
        let mut app = open_general_app();
        let room = general();
        let mut failed = msg(&room, "F1", "me", 2, "failed text");
        failed.send = SendState::Failed;
        app.messages
            .insert(room.clone(), vec![msg(&room, "m0", "a", 1, "zero"), failed]);
        app.msg_views.clear();
        press(&mut app, "dd");
        assert_eq!(ids(&app), ["m0"]);
        assert_eq!(messages::view(&app, &room).cursor, 0);

        // A stored cursor below the removed message moves up by one, so m3 stays selected.
        let mut app = app_with_four();
        app.msg_views.insert(
            general(),
            messages::MsgView {
                cursor: 3,
                top_row: 0,
            },
        );
        remove_at(&mut app, &general(), 1);
        assert_eq!(ids(&app), ["m0", "m2", "m3"]);
        assert_eq!(app.msg_views[&general()].cursor, 2);
        assert_eq!(app.messages[&general()][2].id, "m3");

        // A stored cursor above the removed message does not move.
        let mut app = app_with_four();
        app.msg_views.insert(
            general(),
            messages::MsgView {
                cursor: 0,
                top_row: 0,
            },
        );
        remove_at(&mut app, &general(), 1);
        assert_eq!(app.msg_views[&general()].cursor, 0);
    }

    #[test]
    fn discard_counted() {
        // Two adjacent Failed messages: 2dd removes both.
        let mut app = open_general_app();
        let room = general();
        let failed = |id: &str| {
            let mut m = msg(&room, id, "me", 2, "failed text");
            m.send = SendState::Failed;
            m
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg(&room, "m0", "a", 1, "zero"),
                failed("F1"),
                failed("F2"),
                msg(&room, "m3", "a", 4, "three"),
            ],
        );
        app.msg_views.clear();
        press(&mut app, "ggj2dd");
        assert_eq!(ids(&app), ["m0", "m3"]);

        // One Failed then a Sent message: 3dd removes only the Failed one.
        let mut app = open_general_app();
        app.messages.insert(
            room.clone(),
            vec![
                msg(&room, "m0", "a", 1, "zero"),
                failed("F1"),
                msg(&room, "m2", "a", 3, "two"),
            ],
        );
        app.msg_views.clear();
        press(&mut app, "ggj3dd");
        assert_eq!(ids(&app), ["m0", "m2"]);
    }

    #[test]
    fn edge_empty_and_text_as_is() {
        // A one-character draft is sent as one message.
        let mut app = open_general_app();
        press(&mut app, "ix<CR>");
        assert_eq!(sends(&app).len(), 1);
        assert_eq!(sends(&app)[0].2, "x");

        // Leading and trailing spaces and the line break survive byte for byte.
        let mut app = open_general_app();
        press(&mut app, "i  two  <A-CR>lines <CR>");
        let sent = sends(&app);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].2, "  two  \nlines ");
        assert_eq!(
            app.messages[&general()].last().unwrap().body,
            "  two  \nlines "
        );
    }

    #[test]
    fn send_while_history_loading() {
        use crate::app::history::HistoryState;
        for state in [HistoryState::Loading, HistoryState::Failed] {
            let mut app = open_general_app();
            app.history.insert(general(), state);
            let before = app.messages[&general()].len();
            press(&mut app, "ihello<CR>");
            assert_eq!(app.messages[&general()].len(), before + 1);
            assert_eq!(sends(&app).len(), 1);
            assert_eq!(app.messages[&general()][before].body, "hello");
        }
    }

    #[test]
    fn submit_author_is_the_own_username_when_known() {
        let mut app = open_general_app();
        app.own_username = Some("smoke".to_string());
        press(&mut app, "ihi<CR>");
        assert_eq!(app.messages[&general()].last().unwrap().author, "smoke");
    }

    #[test]
    fn sent_marks_in_place_and_pumps() {
        let mut app = open_general_app();
        update(&mut app, Action::Clock(1_794_909_600));
        press(&mut app, "ia<CR>b<CR>");
        let before = app.messages[&general()].len();
        assert_eq!(
            states(&app, 2),
            [SendState::Sending, SendState::Queued { reconcile: false }]
        );
        assert_eq!(sends(&app).len(), 1);

        // A stored cursor, to prove the swap does not move it.
        press(&mut app, "<Esc>");
        let cursor = crate::app::messages::view(&app, &general());
        app.effects.clear();

        let mut server = msg(&general(), "local-1", "smoke", 1_794_909_605, "a");
        server.send = SendState::Sending; // on_sent marks it Sent whatever the answer carries
        update(
            &mut app,
            Action::MessageSent {
                room: general(),
                id: "local-1".to_string(),
                message: server,
            },
        );
        let held = &app.messages[&general()];
        assert_eq!(held.len(), before);
        let first = &held[before - 2];
        assert_eq!(first.id, "local-1");
        assert_eq!(first.author, "smoke");
        assert_eq!(first.ts, 1_794_909_605);
        assert_eq!(first.send, SendState::Sent);
        assert_eq!(held[before - 1].send, SendState::Sending);
        assert_eq!(
            sends(&app),
            vec![(
                "general".to_string(),
                "local-2".to_string(),
                "b".to_string(),
                false
            )]
        );
        assert_eq!(app.effects.len(), 1, "no history request");
        assert_eq!(crate::app::messages::view(&app, &general()), cursor);
    }

    #[test]
    fn on_sent_other_room_and_unknown_id() {
        let mut app = open_general_app();
        let other = app
            .rooms
            .iter()
            .map(|r| r.id.clone())
            .find(|id| *id != general())
            .expect("a second room");
        assert_eq!(app.current_room, Some(general()));
        app.messages.entry(other.clone()).or_default().push(msg(
            &other,
            "rt-test-1",
            "me",
            0,
            BODY,
        ));
        let before = app.messages[&other].clone();
        let index = before.len() - 1;

        // The answer is for a room that is not the current one: it swaps there, in place.
        let server = msg(&other, "rt-test-1", "smoke", 1_794_909_600, BODY);
        on_sent(
            &mut app,
            other.clone(),
            "rt-test-1".to_string(),
            server.clone(),
        );
        assert_eq!(app.messages[&other].len(), before.len());
        assert_eq!(app.messages[&other][index], server);
        assert_eq!(app.messages[&other][..index], before[..index]);
        assert!(app.effects.is_empty());

        // The current room does not hold the id, so it is untouched.
        let general_before = app.messages[&general()].clone();
        let general_after = |app: &App| app.messages[&general()].clone();
        assert_eq!(general_after(&app), general_before);

        // An id the room does not hold is appended once through `merge_incoming` (D-13), in a room
        // that has messages and in one that has none; the existing rows are untouched.
        let snapshot = app.messages[&other].clone();
        let stray = msg(&other, "rt-unknown", "smoke", 5, BODY);
        on_sent(
            &mut app,
            other.clone(),
            "rt-unknown".to_string(),
            stray.clone(),
        );
        assert_eq!(app.messages[&other].len(), snapshot.len() + 1);
        assert_eq!(app.messages[&other][..snapshot.len()], snapshot[..]);
        assert_eq!(app.messages[&other][snapshot.len()], stray);
        let empty = RoomId("r-none".to_string());
        on_sent(
            &mut app,
            empty.clone(),
            "rt-unknown".to_string(),
            msg(&empty, "rt-unknown", "smoke", 5, BODY),
        );
        assert_eq!(app.messages[&empty].len(), 1);
    }

    /// Every `ApiError` value, with whether it leaves the outcome of a send unknown (D-07, D-18).
    /// Written out here on purpose: it is the spec `outcome_unknown` is checked against.
    fn all_errors() -> [(ApiError, bool); 13] {
        [
            (ApiError::Unauthorized, false),
            (ApiError::Forbidden, false),
            (ApiError::NotFound, false),
            (ApiError::RateLimited, false),
            (ApiError::Redirected, false),
            (ApiError::Rejected, false),
            (ApiError::Unreachable, false),
            (ApiError::Client, false),
            (ApiError::Timeout, true),
            (ApiError::Server(502), true),
            (ApiError::ConnectionLost, true),
            (ApiError::BadResponse, true),
            (ApiError::TooLarge, true),
        ]
    }

    #[test]
    fn fail_classification() {
        for (error, unknown) in all_errors() {
            let mut app = open_general_app();
            press(&mut app, &format!("i{BODY}<CR>"));
            let index = app.messages[&general()].len() - 1;
            assert_eq!(app.messages[&general()][index].send, SendState::Sending);
            let before = app.messages[&general()].len();

            on_failed(&mut app, general(), "local-1".to_string(), error);
            let held = &app.messages[&general()];
            assert_eq!(held.len(), before, "{error:?}: the message keeps its place");
            let expected = if unknown {
                SendState::Unconfirmed
            } else {
                SendState::Failed
            };
            assert_eq!(held[index].send, expected, "{error:?}");
            assert_eq!(held[index].body, BODY, "{error:?}: the row keeps its text");
            let Some(StatusMsg::Error(text)) = &app.status_msg else {
                panic!("{error:?}: no error status");
            };
            let prefix = if unknown {
                "message may not have been sent: "
            } else {
                "message not sent: "
            };
            assert!(text.starts_with(prefix), "{error:?}: {text}");
        }
    }

    #[test]
    fn fail_text_fixed() {
        for unconfirmed in [false, true] {
            let mut seen = std::collections::HashSet::new();
            for (error, _) in all_errors() {
                let text = send_error_text(error, unconfirmed);
                assert!(!text.contains(BODY), "{text}");
                assert!(!text.contains(":reconnect"), "{text}");
                assert!(!text.contains("http"), "{text}");
                assert!(seen.insert(text.clone()), "one reason per kind: {text}");
            }
        }
        assert_eq!(
            send_error_text(ApiError::Server(503), false),
            "message not sent: server error (HTTP 503)"
        );
        assert_eq!(
            send_error_text(ApiError::Server(502), true),
            "message may not have been sent: server error (HTTP 502)"
        );
        assert_eq!(
            send_error_text(ApiError::Rejected, false),
            "message not sent: rejected (read-only, muted or too long)"
        );
        assert_eq!(
            send_error_text(ApiError::ConnectionLost, true),
            "message may not have been sent: connection lost"
        );
    }

    #[test]
    fn unsent_never_resent_on_its_own() {
        // One Failed and one Unconfirmed message; nothing the app does by itself sends them again:
        // not a clock tick, not the next message, not `:reconnect`, not a result of another message.
        let mut app = open_general_app();
        let room = general();
        let mut failed = msg(&room, "F1", "me", 2, "failed text");
        failed.send = SendState::Failed;
        let mut unknown = msg(&room, "U1", "me", 3, "unknown text");
        unknown.send = SendState::Unconfirmed;
        app.messages.entry(room.clone()).or_default().push(failed);
        app.messages.entry(room.clone()).or_default().push(unknown);

        update(&mut app, Action::Clock(1_794_909_600));
        press(&mut app, ":reconnect<CR>");
        press(&mut app, "inext<CR>");
        answer_sent(&mut app, "local-1", 20);
        update(&mut app, Action::Clock(1_794_909_700));
        press(&mut app, "<Esc>:reconnect<CR>");

        let sent: Vec<String> = sends(&app).into_iter().map(|s| s.1).collect();
        assert_eq!(sent, ["local-1"], "only the new message was sent");
        let held = &app.messages[&room];
        let state_of = |id: &str| held.iter().find(|m| m.id == id).map(|m| m.send);
        assert_eq!(state_of("F1"), Some(SendState::Failed));
        assert_eq!(state_of("U1"), Some(SendState::Unconfirmed));
    }

    /// `general` with the sentinel body sent from the messages pane after `gg`, under `toml`.
    fn app_scrolled_up_and_sent(toml: &str) -> App {
        let mut app =
            crate::test_util::app_with_config(crate::test_util::config_from_toml(toml), 80, 24);
        app.open_room(general());
        press(&mut app, "gg");
        press(&mut app, &format!("i{BODY}<CR>"));
        app
    }

    #[test]
    fn follow_always_jumps() {
        let app = app_scrolled_up_and_sent("");
        assert!(app.msg_views.is_empty(), "the stored view is gone");
        let last = app.messages[&general()].len() - 1;
        assert_eq!(messages::view(&app, &general()).cursor, last);
        assert!(crate::test_util::render_to_string(&app, 80, 24).contains(BODY));
    }

    #[test]
    fn follow_if_at_bottom_stays() {
        let toml = "follow_on_send = \"if_at_bottom\"\n";
        let mut app =
            crate::test_util::app_with_config(crate::test_util::config_from_toml(toml), 80, 24);
        app.open_room(general());
        press(&mut app, "gg");
        let before = app.msg_views[&general()];
        assert_eq!((before.cursor, before.top_row), (0, 0));
        press(&mut app, &format!("i{BODY}<CR>"));
        assert_eq!(app.msg_views[&general()], before, "the pane did not move");
        assert_eq!(sends(&app).len(), 1, "the message was still sent");
        let screen = crate::test_util::render_to_string(&app, 80, 24);
        assert!(!screen.contains(BODY), "the new row is below the window");
    }

    #[test]
    fn follow_if_at_bottom_follows() {
        let toml = "follow_on_send = \"if_at_bottom\"\n";
        let mut app =
            crate::test_util::app_with_config(crate::test_util::config_from_toml(toml), 80, 24);
        app.open_room(general());
        press(&mut app, &format!("i{BODY}<CR>"));
        assert!(app.msg_views.is_empty());
        let last = app.messages[&general()].len() - 1;
        assert_eq!(messages::view(&app, &general()).cursor, last);
        assert!(crate::test_util::render_to_string(&app, 80, 24).contains(BODY));
    }

    #[test]
    fn sent_after_discard_appends() {
        let mut app = open_general_app();
        press(&mut app, "ia<CR>");
        let index = app.messages[&general()].len() - 1;
        // The row was removed meanwhile; the server's answer still carries the message.
        remove_at(&mut app, &general(), index);
        let before = app.messages[&general()].len();
        answer_sent(&mut app, "local-1", 10);
        assert_eq!(app.messages[&general()].len(), before + 1);
        assert_eq!(app.messages[&general()][before].id, "local-1");
        assert_eq!(app.messages[&general()][before].send, SendState::Sent);
        // The same answer again adds nothing.
        answer_sent(&mut app, "local-1", 10);
        assert_eq!(app.messages[&general()].len(), before + 1);
    }

    /// `general` holding Sending `local-1` (a) and Queued `local-2` (b), after `merge_incoming`
    /// confirmed a from a server copy: one SendMessage effect so far, and a's POST outstanding.
    fn app_with_confirmed_sending() -> (App, usize) {
        let mut app = open_general_app();
        press(&mut app, "ia<CR>b<CR>");
        assert_eq!(
            states(&app, 2),
            [SendState::Sending, SendState::Queued { reconcile: false }]
        );
        assert_eq!(sends(&app).len(), 1);
        let index = app.messages[&general()].len() - 2;
        let copy = msg(&general(), "local-1", "smoke", 1_794_909_605, "a");
        let outcome = messages::merge_incoming(&mut app, &general(), copy);
        assert_eq!(outcome, messages::Merge::Confirmed(index));
        (app, index)
    }

    #[test]
    fn confirm_sending_keeps_slot() {
        let (mut app, index) = app_with_confirmed_sending();
        assert_eq!(app.messages[&general()][index].send, SendState::Sent);
        assert_eq!(
            app.awaiting_answer.get(&general()).map(String::as_str),
            Some("local-1")
        );
        assert_eq!(sends(&app).len(), 1, "a confirmation records no effect");

        // A third Enter and a direct pump start nothing while a's POST is outstanding.
        press(&mut app, "c<CR>");
        pump(&mut app, &general());
        assert_eq!(sends(&app).len(), 1);

        // A Queued message of another room is not blocked by the entry.
        let other = RoomId("random".to_string());
        let mut queued = msg(&other, "local-77", "me", 0, "elsewhere");
        queued.send = SendState::Queued { reconcile: false };
        app.messages.entry(other.clone()).or_default().push(queued);
        pump(&mut app, &other);
        assert_eq!(sends(&app).len(), 2);
        assert_eq!(sends(&app)[1].0, "random");

        // a's own answer releases the slot and starts b only; c stays Queued.
        answer_sent(&mut app, "local-1", 11);
        assert!(app.awaiting_answer.is_empty());
        let started: Vec<String> = sends(&app).into_iter().map(|s| s.1).collect();
        assert_eq!(started, ["local-1", "local-77", "local-2"]);
        assert_eq!(
            states(&app, 3),
            [
                SendState::Sent,
                SendState::Sending,
                SendState::Queued { reconcile: false },
            ]
        );
    }

    #[test]
    fn confirmed_row_failure_is_silent() {
        let (mut app, index) = app_with_confirmed_sending();
        app.status_msg = None;
        answer_failed(&mut app, "local-1");
        assert_eq!(app.messages[&general()][index].send, SendState::Sent);
        assert_eq!(app.messages[&general()][index].id, "local-1");
        assert!(app.status_msg.is_none(), "no error for a delivered message");
        assert!(app.awaiting_answer.is_empty());
        let started: Vec<String> = sends(&app).into_iter().map(|s| s.1).collect();
        assert_eq!(started, ["local-1", "local-2"], "b starts");
    }

    #[test]
    fn discard_unconfirmed() {
        // An Unconfirmed message is discarded like a Failed one (D-06).
        let mut app = app_with_failed();
        app.messages
            .get_mut(&general())
            .unwrap()
            .last_mut()
            .unwrap()
            .send = SendState::Unconfirmed;
        let before = app.messages[&general()].len();
        press(&mut app, "dd");
        assert_eq!(app.messages[&general()].len(), before - 1);
        assert!(app.effects.is_empty());
    }
}
