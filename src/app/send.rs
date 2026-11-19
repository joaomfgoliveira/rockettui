//! The result half of sending a message (MSG-02, D-04, D-06, D-07).
//!
//! The functions here are pure: they read and change `App` and never do IO. A send attempt is an
//! `Effect::SendMessage`; its answer arrives as `Action::MessageSent` or `Action::SendFailed`.
//!
//! Rules this module keeps:
//! - a result is applied to the room the message was sent from, found by room and message id,
//!   never to the current room (the user may have opened another room meanwhile);
//! - a server answer replaces the message in place, at the same index, so nothing moves and no
//!   history is fetched again (D-04);
//! - a result for a message the room no longer holds changes nothing;
//! - the status line for a failure is one fixed string per error kind: never server text, never
//!   the message body, never a hint at `:reconnect`, which D-08 keeps separate from sending.

use crate::api::ApiError;
use crate::app::{App, StatusMsg};
use crate::data::{Message, RoomId};

/// The server accepted message `id` of `room` and answered with its stored copy: replace the held
/// message with that id by `message` at the same index (D-04). When `room` holds no message with
/// that id nothing changes.
pub fn on_sent(app: &mut App, room: RoomId, id: String, message: Message) {
    let Some(held) = app.messages.get_mut(&room) else {
        return;
    };
    if let Some(slot) = held.iter_mut().find(|m| m.id == id) {
        *slot = message;
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
    use crate::data::MessageKind;
    use crate::test_util::app_80x24;

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
        }
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
