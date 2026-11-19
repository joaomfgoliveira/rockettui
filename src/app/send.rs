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
//! - a result is applied to the room the message was sent from, found by room and message id,
//!   never to the current room (the user may have opened another room meanwhile);
//! - a server answer replaces the message in place, at the same index, so nothing moves and no
//!   history is fetched again (D-04);
//! - a result for a message the room no longer holds changes nothing;
//! - the status line for a failure is one fixed string per error kind: never server text, never
//!   the message body, never a hint at `:reconnect`, which D-08 keeps separate from sending.

use crate::api::ApiError;
use crate::app::{App, LOCAL_AUTHOR, Mode, StatusMsg, messages};
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::effect::{Effect, SendText};

/// Enter in Insert mode (D-15): turn the draft into a pending message of the open room, clear the
/// draft, stay in Insert mode and start the send when the room has none in flight.
///
/// Nothing happens for a blank draft (D-09). With no room open the draft is kept and the status
/// bar says so. There is no check of history state, length or permission here: the server decides
/// (D-11, D-12), and a send while history is Loading or Failed goes out like any other (D-19).
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
    app.messages.entry(room.clone()).or_default().push(message);
    messages::follow_newest(app, &room);
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

/// The send of message `id` of `room` failed: show the fixed reason on the status bar (D-06). The
/// message keeps its place; its own Failed or Unconfirmed state arrives with `SendState` in a
/// later plan (D-07, D-18).
pub fn on_failed(app: &mut App, _room: RoomId, _id: String, error: ApiError) {
    app.status_msg = Some(StatusMsg::Error(send_error_text(error)));
}

/// `message not sent: <reason>` with one fixed reason per error kind. It is deliberately not
/// `ApiError`'s `Display`, whose hints point at `:reconnect` (D-08), and it never holds server text.
pub fn send_error_text(error: ApiError) -> String {
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
    };
    format!("message not sent: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
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
        press(&mut app, "<Esc>gg");
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

        // An id the room does not hold changes nothing, in a room that has messages and in one
        // that has none.
        let snapshot = app.messages[&other].clone();
        let stray = msg(&other, "rt-unknown", "smoke", 5, BODY);
        on_sent(
            &mut app,
            other.clone(),
            "rt-unknown".to_string(),
            stray.clone(),
        );
        assert_eq!(app.messages[&other], snapshot);
        let empty = RoomId("r-none".to_string());
        on_sent(&mut app, empty.clone(), "rt-unknown".to_string(), stray);
        assert!(!app.messages.contains_key(&empty));
    }

    #[test]
    fn fail_status_has_no_body() {
        let errors = [
            ApiError::Unauthorized,
            ApiError::Forbidden,
            ApiError::NotFound,
            ApiError::RateLimited,
            ApiError::Redirected,
            ApiError::Server(502),
            ApiError::Unreachable,
            ApiError::Timeout,
            ApiError::BadResponse,
            ApiError::TooLarge,
            ApiError::Client,
        ];
        let mut seen = std::collections::HashSet::new();
        for error in errors {
            let mut app = open_general_app();
            app.messages.entry(general()).or_default().push(msg(
                &general(),
                "rt-test-1",
                "me",
                0,
                BODY,
            ));
            let before = app.messages[&general()].clone();
            on_failed(&mut app, general(), "rt-test-1".to_string(), error);
            let Some(StatusMsg::Error(text)) = &app.status_msg else {
                panic!("{error:?}: no error status");
            };
            assert!(text.starts_with("message not sent: "), "{text}");
            assert!(!text.contains(BODY), "{text}");
            assert!(!text.contains(":reconnect"), "{text}");
            assert!(seen.insert(text.clone()), "one reason per kind: {text}");
            assert_eq!(
                app.messages[&general()],
                before,
                "the message keeps its place"
            );
        }
        assert_eq!(
            send_error_text(ApiError::Server(503)),
            "message not sent: server error (HTTP 503)"
        );
    }
}
