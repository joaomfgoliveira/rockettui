//! Editing and deleting the user's own messages (`message-editing`).
//!
//! The functions here are pure: they read and change `App` and never do IO. An edit is an
//! `Effect::EditMessage` and a delete an `Effect::DeleteMessage`; their answers arrive as
//! `Action::EditDone`, `EditFailed`, `DeleteDone` and `DeleteFailed`. Neither uses the send queue
//! or changes any `SendState` (`message-sending`).
//!
//! Rules this module keeps:
//! - one predicate, `is_own_plain`, decides what may be edited or deleted: a held (`Sent`),
//!   non-deleted message of the open, non-encrypted room, written by the known own username, with
//!   plain text. It runs when `e` or `dd` is pressed and again when a delete is confirmed. Anything
//!   else is a silent no-op: no state, no effect and no status line changes;
//! - an edit loads the text into the input bar, stashing the room's draft, and Enter saves it. The
//!   old text stays on screen until the server accepts: the answer or the live stream, whichever
//!   comes first, replaces the row in place and the second is a no-op;
//! - a failed edit keeps the editing state and the typed text; a failed delete changes nothing;
//! - a delete asks for one key first (`App.confirm`), drawn in the status bar and consumed by the
//!   top of `Action::Key`, so no other command can run while it is open;
//! - every status line is one fixed string per error kind: never server text, never message text.

use crate::api::ApiError;
use crate::app::input_buf::InputBuffer;
use crate::app::{App, Focus, Mode, StatusMsg, messages, send};
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::effect::{Effect, SendText};
use crate::input::key::Chord;
use crossterm::event::{KeyCode, KeyModifiers};

/// The message being edited. While it exists the input bar holds the edit text and `stash` holds
/// the draft the room had before. No `Debug`: it holds text.
pub struct Editing {
    pub room: RoomId,
    pub id: String,
    /// The text the edit started from; saving it unchanged sends nothing.
    original: String,
    /// The room's draft before the edit, put back when the edit ends.
    stash: InputBuffer,
    /// True from the save until its answer: a second Enter does not send a second request.
    in_flight: bool,
}

/// A question waiting for one key. No `Debug`: nothing in it is text, but it stays out of logs.
#[derive(Clone, PartialEq)]
pub enum Confirm {
    /// Delete message `id` of `room`.
    Delete { room: RoomId, id: String },
}

/// The text of the delete prompt, drawn in the status bar while `App.confirm` is set.
pub const DELETE_PROMPT: &str = "Delete this message? y = yes, any other key = no";

/// True when `message` of `room` may be edited or deleted: held as `Sent`, not a tombstone, written
/// by the known own username (exact match), plain text, in a room that is known and not encrypted.
pub fn is_own_plain(app: &App, room: &RoomId, message: &Message) -> bool {
    let Some(own) = app.own_username.as_deref() else {
        return false;
    };
    message.send == SendState::Sent
        && message.kind == MessageKind::Normal
        && message.plain_text
        && message.author == own
        && app.rooms.iter().any(|r| &r.id == room && !r.encrypted)
}

/// The id of the message under the messages-pane cursor when `is_own_plain` allows it. `None` for
/// any other pane or mode, with no room open, an empty room, or any message the rule excludes.
fn own_under_cursor(app: &App) -> Option<(RoomId, String)> {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.clone()?;
    let held = app.messages.get(&room)?;
    let message = held.get(messages::view(app, &room).cursor)?;
    is_own_plain(app, &room, message).then(|| (room, message.id.clone()))
}

/// `e` (`edit_message`): start editing the own message under the cursor. The room's draft is
/// stashed, the message text goes into the input bar and the app enters Insert mode on it.
pub fn start(app: &mut App) {
    if app.editing.is_some() {
        return;
    }
    let Some((room, id)) = own_under_cursor(app) else {
        return;
    };
    let Some(body) = app
        .messages
        .get(&room)
        .and_then(|held| held.iter().find(|m| m.id == id))
        .map(|m| m.body.clone())
    else {
        return;
    };
    let mut text = InputBuffer::new();
    text.insert_str(&body);
    let stash = std::mem::replace(&mut app.input, text);
    app.editing = Some(Editing {
        room,
        id,
        // What the buffer holds after its own sanitizing, so "unchanged" compares like with like.
        original: app.input.text(),
        stash,
        in_flight: false,
    });
    app.prev_focus = app.focus;
    app.focus = Focus::Input;
    app.mode = Mode::Insert;
    let (width, height) = app.input_viewport();
    app.input.settle(width, height);
}

/// Leave the editing state: the stashed draft comes back into the input bar. The mode and the
/// focus are the caller's business. A no-op when nothing is being edited.
pub fn cancel(app: &mut App) {
    if let Some(editing) = app.editing.take() {
        app.input = editing.stash;
        let (width, height) = app.input_viewport();
        app.input.settle(width, height);
    }
}

/// End the edit and go back to Normal mode on the pane the edit started from.
fn finish(app: &mut App) {
    cancel(app);
    app.mode = Mode::Normal;
    app.focus = app.prev_focus;
}

/// Enter while editing: blank or unchanged text ends the edit with no request; otherwise the
/// request is recorded and the old text stays on screen until the answer.
pub fn save(app: &mut App) {
    let Some(editing) = app.editing.as_mut() else {
        return;
    };
    if editing.in_flight {
        return;
    }
    let text = app.input.text();
    if text.trim().is_empty() || text == editing.original {
        finish(app);
        return;
    }
    editing.in_flight = true;
    let effect = Effect::EditMessage {
        room: editing.room.clone(),
        id: editing.id.clone(),
        text: SendText::new(text),
    };
    app.effects.push(effect);
}

/// The server accepted the edit of message `id` of `room`: show the stored copy in place (a no-op
/// when the live stream already did) and end the edit when it is the one being edited.
pub fn on_edit_done(app: &mut App, room: RoomId, id: String, message: Message) {
    messages::replace_edited(app, &room, message);
    if app
        .editing
        .as_ref()
        .is_some_and(|e| e.room == room && e.id == id)
    {
        finish(app);
    }
}

/// The edit failed: one fixed status line, the editing state and the typed text stay, the
/// displayed message stays as it was.
pub fn on_edit_failed(app: &mut App, room: &RoomId, id: &str, error: ApiError) {
    if let Some(editing) = app.editing.as_mut()
        && editing.room == *room
        && editing.id == id
    {
        editing.in_flight = false;
    }
    app.status_msg = Some(StatusMsg::Error(failure_text(
        error,
        "edit not saved",
        "edit may not have been saved",
    )));
}

/// `dd` (`delete_message`): discard a `Failed` or `Unconfirmed` local row as before, or open the
/// confirmation of a delete on an own server message. A second `dd` (or a count) while a prompt is
/// open opens no second one. Anything else is a silent no-op.
pub fn delete(app: &mut App) {
    if app.confirm.is_some() {
        return;
    }
    if send::unsent_under_cursor(app).is_some() {
        send::discard(app);
        return;
    }
    if let Some((room, id)) = own_under_cursor(app) {
        app.confirm = Some(Confirm::Delete { room, id });
    }
}

/// The one key after a prompt: `y` or Enter confirms, anything else cancels. The key is consumed
/// either way. Confirming checks the predicate again against the row as it is now.
pub fn on_confirm_key(app: &mut App, confirm: Confirm, chord: Chord) {
    let yes = chord.mods == KeyModifiers::NONE
        && matches!(chord.code, KeyCode::Char('y') | KeyCode::Enter);
    if !yes {
        return;
    }
    let Confirm::Delete { room, id } = confirm;
    let still_own = app
        .messages
        .get(&room)
        .and_then(|held| held.iter().find(|m| m.id == id))
        .is_some_and(|m| is_own_plain(app, &room, m));
    if still_own {
        app.effects.push(Effect::DeleteMessage { room, id });
    }
}

/// The server accepted the delete: the row becomes the deleted line at once (the later stream
/// event finds nothing to do).
pub fn on_delete_done(app: &mut App, room: &RoomId, id: &str) {
    messages::tombstone(app, room, id);
}

/// The delete failed: one fixed status line, the row stays as it was.
pub fn on_delete_failed(app: &mut App, error: ApiError) {
    app.status_msg = Some(StatusMsg::Error(failure_text(
        error,
        "message not deleted",
        "message may not have been deleted",
    )));
}

/// `<known> : <reason>` or `<unknown outcome> : <reason>`, one fixed reason per error kind.
fn failure_text(error: ApiError, known: &str, unknown: &str) -> String {
    let lead = if error.outcome_unknown() {
        unknown
    } else {
        known
    };
    let reason = match error {
        ApiError::Rejected => {
            "refused by the server (editing or deleting off, time limit, or not allowed)"
                .to_string()
        }
        ApiError::Forbidden => "not allowed".to_string(),
        other => send::error_reason(other),
    };
    format!("{lead}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::messages::MsgView;
    use crate::app::update::update;
    use crate::test_util::{app_80x24, press, render_to_string};

    const OWN: &str = "dev";
    const SENTINEL: &str = "SENTINEL-EDIT-TEXT-61c3";

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn row(id: &str, author: &str, body: &str) -> Message {
        Message {
            id: id.to_string(),
            room_id: general(),
            author: author.to_string(),
            ts: 100,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
        }
    }

    /// `general` open with the messages pane focused, holding `rows`, the cursor on the last one
    /// and `dev` as the own username.
    fn app_with(rows: Vec<Message>) -> App {
        let mut app = app_80x24();
        app.open_room(general());
        app.own_username = Some(OWN.to_string());
        let last = rows.len().saturating_sub(1);
        app.messages.insert(general(), rows);
        cursor_at(&mut app, last);
        app
    }

    fn cursor_at(app: &mut App, cursor: usize) {
        app.msg_views
            .insert(general(), MsgView { cursor, top_row: 0 });
    }

    fn own_app() -> App {
        app_with(vec![
            row("m0", "alice", "older"),
            row("m1", OWN, "my text"),
            row("m2", "alice", "newer"),
        ])
        .tap(|app| cursor_at(app, 1))
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for App {}

    fn body(app: &App, id: &str) -> String {
        app.messages[&general()]
            .iter()
            .find(|m| m.id == id)
            .expect("message")
            .body
            .clone()
    }

    /// True when nothing observable changed: no editing, no prompt, no effect, no status line, the
    /// input and the mode as given.
    fn untouched(app: &App, input: &str) -> bool {
        app.editing.is_none()
            && app.confirm.is_none()
            && app.effects.is_empty()
            && app.status_msg.is_none()
            && app.input.text() == input
            && app.mode == Mode::Normal
            && app.focus == Focus::Messages
    }

    fn edit_effects(app: &App) -> Vec<(String, String)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::EditMessage { id, text, .. } => {
                    Some((id.clone(), text.as_str().to_string()))
                }
                _ => None,
            })
            .collect()
    }

    fn delete_effects(app: &App) -> Vec<String> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::DeleteMessage { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn status_text(app: &App) -> Option<String> {
        match &app.status_msg {
            Some(StatusMsg::Error(t) | StatusMsg::Info(t)) => Some(t.clone()),
            None => None,
        }
    }

    fn stored_copy(id: &str, body: &str) -> Message {
        Message {
            edited: true,
            ..row(id, OWN, body)
        }
    }

    // ---- 4.1: the one predicate

    #[test]
    fn exclusions_do_nothing() {
        let tombstone = Message {
            kind: MessageKind::Deleted,
            body: String::new(),
            ..row("x", OWN, "")
        };
        let attachment = Message {
            plain_text: false,
            ..row("x", OWN, "see [attachment: a.pdf]")
        };
        let cases: Vec<(&str, Message)> = vec![
            ("someone else", row("x", "alice", "theirs")),
            ("case differs", row("x", "Dev", "mine?")),
            ("tombstone", tombstone),
            ("attachment", attachment),
            (
                "queued",
                Message {
                    send: SendState::Queued { reconcile: false },
                    ..row("x", OWN, "mine")
                },
            ),
            (
                "sending",
                Message {
                    send: SendState::Sending,
                    ..row("x", OWN, "mine")
                },
            ),
        ];
        for (label, message) in cases {
            let mut app = app_with(vec![message]);
            press(&mut app, "e");
            assert!(untouched(&app, ""), "e on {label}");
            press(&mut app, "dd");
            assert!(untouched(&app, ""), "dd on {label}");
            assert_eq!(app.messages[&general()].len(), 1, "{label}");
        }
    }

    #[test]
    fn failed_and_unconfirmed_rows_are_not_editable_but_dd_still_discards() {
        for state in [SendState::Failed, SendState::Unconfirmed] {
            let mut app = app_with(vec![Message {
                send: state,
                ..row("x", OWN, "mine")
            }]);
            press(&mut app, "e");
            assert!(untouched(&app, ""), "e on {state:?}");
            press(&mut app, "dd");
            assert!(app.messages[&general()].is_empty(), "{state:?} discarded");
            assert!(app.confirm.is_none(), "no prompt for a local row");
            assert!(app.effects.is_empty());
        }
    }

    #[test]
    fn unknown_own_username_does_nothing() {
        let mut app = own_app();
        app.own_username = None;
        press(&mut app, "e");
        assert!(untouched(&app, ""));
        press(&mut app, "dd");
        assert!(untouched(&app, ""));
    }

    #[test]
    fn encrypted_room_does_nothing() {
        let secrets = RoomId("secrets".to_string());
        let mut app = app_80x24();
        app.own_username = Some(OWN.to_string());
        app.open_room(secrets.clone());
        let message = Message {
            room_id: secrets.clone(),
            ..row("s0", OWN, "mine")
        };
        app.messages.insert(secrets.clone(), vec![message]);
        app.msg_views.insert(
            secrets,
            MsgView {
                cursor: 0,
                top_row: 0,
            },
        );
        press(&mut app, "e");
        assert!(untouched(&app, ""));
        press(&mut app, "dd");
        assert!(untouched(&app, ""));
    }

    #[test]
    fn other_panes_and_empty_room_do_nothing() {
        let mut app = own_app();
        app.focus = Focus::Sidebar;
        press(&mut app, "e");
        press(&mut app, "dd");
        assert!(app.editing.is_none() && app.confirm.is_none() && app.effects.is_empty());
        assert_eq!(app.focus, Focus::Sidebar);

        let mut app = app_with(Vec::new());
        press(&mut app, "e");
        press(&mut app, "dd");
        assert!(untouched(&app, ""));
    }

    #[test]
    fn keymap_names_defaults_and_rebinding() {
        use crate::input::actions::{action_from_name, name_of};
        assert_eq!(action_from_name("edit_message"), Some(Action::EditMessage));
        assert_eq!(
            action_from_name("delete_message"),
            Some(Action::DeleteMessage)
        );
        assert_eq!(name_of(&Action::EditMessage), Some("edit_message"));
        assert_eq!(name_of(&Action::DeleteMessage), Some("delete_message"));

        // `E` rebinds the edit; `dd` rebound to `discard_send` only discards.
        let config = crate::test_util::config_from_keymap(
            "[normal]\n\"E\" = \"edit_message\"\n\"dd\" = \"discard_send\"\n",
        );
        let mut app = App::with_config(ratatui::layout::Size::new(80, 24), config);
        for action in crate::data::fake::seed() {
            update(&mut app, action);
        }
        app.open_room(general());
        app.own_username = Some(OWN.to_string());
        app.messages
            .insert(general(), vec![row("m1", OWN, "my text")]);
        cursor_at(&mut app, 0);
        press(&mut app, "dd");
        assert!(
            untouched(&app, ""),
            "discard_send does not delete a server message"
        );
        press(&mut app, "E");
        assert!(app.editing.is_some());
        assert_eq!(app.input.text(), "my text");
    }

    // ---- 5.1: starting and cancelling

    #[test]
    fn start_loads_text_and_enters_insert() {
        let mut app = own_app();
        press(&mut app, "e");
        assert_eq!(app.input.text(), "my text");
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.focus, Focus::Input);
        assert!(app.effects.is_empty());
        let screen = render_to_string(&app, 80, 24);
        assert!(
            screen.contains("Editing: Enter saves, Esc cancels"),
            "{screen}"
        );
        assert!(screen.contains("my text"));
    }

    #[test]
    fn cancel_restores_draft_or_empty() {
        let mut app = own_app();
        press(&mut app, "e<Esc>");
        assert!(app.editing.is_none());
        assert_eq!(app.input.text(), "");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Messages);
        assert!(app.effects.is_empty());
        assert_eq!(body(&app, "m1"), "my text");

        let mut app = own_app();
        press(&mut app, "ihalf typed<Esc>");
        assert_eq!(app.input.text(), "half typed");
        app.focus = Focus::Messages;
        press(&mut app, "e");
        assert_eq!(app.input.text(), "my text");
        press(&mut app, "<C-c>");
        assert!(app.editing.is_none());
        assert_eq!(app.input.text(), "half typed");
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("Input"), "{screen}");
        assert!(!screen.contains("Editing"), "{screen}");
    }

    #[test]
    fn room_switch_cancels_and_keeps_drafts() {
        let mut app = own_app();
        press(&mut app, "ihalf typed<Esc>");
        app.focus = Focus::Messages;
        press(&mut app, "e");
        app.open_room(RoomId("random".to_string()));
        assert!(app.editing.is_none());
        assert!(app.input.is_empty(), "random has no draft");
        assert_eq!(app.drafts[&general()].text(), "half typed");
        app.open_room(general());
        assert_eq!(app.input.text(), "half typed");
    }

    #[test]
    fn e_elsewhere_does_nothing() {
        let mut app = own_app();
        press(&mut app, "ihello");
        press(&mut app, "e");
        assert_eq!(app.input.text(), "helloe", "typing, not a command");
        assert!(app.editing.is_none());
    }

    // ---- 5.2: saving

    #[test]
    fn unchanged_or_blank_ends_without_request() {
        let mut app = own_app();
        press(&mut app, "e<CR>");
        assert!(app.editing.is_none());
        assert!(app.effects.is_empty());
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.input.text(), "");

        let mut app = own_app();
        press(&mut app, "ekeep<Esc>");
        press(&mut app, "e<C-u><CR>");
        assert!(app.effects.is_empty());
        assert!(app.editing.is_none());
        assert_eq!(body(&app, "m1"), "my text");
    }

    #[test]
    fn save_records_one_effect_and_keeps_old_text() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR>");
        assert_eq!(
            edit_effects(&app),
            [("m1".to_string(), "new words".to_string())]
        );
        assert_eq!(body(&app, "m1"), "my text", "old text stays until accepted");
        assert!(app.editing.is_some());
        // Two saves in a row: the second sends nothing.
        press(&mut app, "<CR>");
        assert_eq!(edit_effects(&app).len(), 1);
        // The send queue and every SendState are untouched.
        assert!(
            app.messages[&general()]
                .iter()
                .all(|m| m.send == SendState::Sent)
        );
        assert!(app.awaiting_answer.is_empty());
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. }))
        );
    }

    #[test]
    fn done_from_the_answer_first() {
        let mut app = own_app();
        press(&mut app, "ihalf<Esc>");
        app.focus = Focus::Messages;
        press(&mut app, "e<C-u>new words<CR>");
        update(
            &mut app,
            Action::EditDone {
                room: general(),
                id: "m1".to_string(),
                message: stored_copy("m1", "new words"),
            },
        );
        assert_eq!(body(&app, "m1"), "new words");
        assert!(app.messages[&general()][1].edited);
        assert!(app.editing.is_none());
        assert_eq!(app.input.text(), "half", "the draft is back");
        assert_eq!(app.mode, Mode::Normal);
        // The stream's copy afterwards is a no-op: one row, same text, not appended.
        update(&mut app, Action::RtMessage(stored_copy("m1", "new words")));
        assert_eq!(app.messages[&general()].len(), 3);
        assert_eq!(body(&app, "m1"), "new words");
        let screen = render_to_string(&app, 80, 24);
        assert_eq!(screen.matches("new words (edited)").count(), 1, "{screen}");
    }

    #[test]
    fn done_from_the_stream_first() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR>");
        update(&mut app, Action::RtMessage(stored_copy("m1", "new words")));
        assert_eq!(body(&app, "m1"), "new words");
        assert!(app.editing.is_some(), "the request has not answered yet");
        update(
            &mut app,
            Action::EditDone {
                room: general(),
                id: "m1".to_string(),
                message: stored_copy("m1", "new words"),
            },
        );
        assert!(app.editing.is_none());
        assert_eq!(app.messages[&general()].len(), 3);
        assert_eq!(app.messages[&general()][1].id, "m1");
    }

    #[test]
    fn refusal_keeps_the_edit_open() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR>");
        update(
            &mut app,
            Action::EditFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Rejected,
            },
        );
        assert!(app.editing.is_some());
        assert_eq!(app.input.text(), "new words");
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(body(&app, "m1"), "my text");
        let line = status_text(&app).expect("a status line");
        assert!(
            line.starts_with("edit not saved: refused by the server"),
            "{line}"
        );
        assert!(
            !line.contains("new words") && !line.contains("my text"),
            "{line}"
        );
        // Another try is possible.
        press(&mut app, "<CR>");
        assert_eq!(edit_effects(&app).len(), 2);
    }

    #[test]
    fn timeout_says_it_may_have_saved() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR>");
        update(
            &mut app,
            Action::EditFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Timeout,
            },
        );
        assert_eq!(
            status_text(&app).as_deref(),
            Some("edit may not have been saved: timed out")
        );
        assert!(app.editing.is_some());
        assert_eq!(app.input.text(), "new words");
    }

    #[test]
    fn message_disappeared_meanwhile() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR>");
        app.messages.get_mut(&general()).expect("room").remove(1);
        update(
            &mut app,
            Action::EditDone {
                room: general(),
                id: "m1".to_string(),
                message: stored_copy("m1", "new words"),
            },
        );
        assert_eq!(app.messages[&general()].len(), 2);
        assert!(app.editing.is_none());
    }

    #[test]
    fn done_after_cancel_still_applies() {
        let mut app = own_app();
        press(&mut app, "e<C-u>new words<CR><Esc>");
        assert!(app.editing.is_none());
        update(
            &mut app,
            Action::EditDone {
                room: general(),
                id: "m1".to_string(),
                message: stored_copy("m1", "new words"),
            },
        );
        assert_eq!(body(&app, "m1"), "new words");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn edit_does_not_touch_the_send_queue() {
        let mut app = own_app();
        press(&mut app, "ihello<CR><Esc>");
        let sends_before = app
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::SendMessage { .. }))
            .count();
        assert_eq!(sends_before, 1);
        app.focus = Focus::Messages;
        cursor_at(&mut app, 1);
        press(&mut app, "e<C-u>other<CR>");
        let sends_after = app
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::SendMessage { .. }))
            .count();
        assert_eq!(sends_after, 1, "no new send");
        assert_eq!(edit_effects(&app).len(), 1, "the edit went out at once");
        let states: Vec<SendState> = app.messages[&general()].iter().map(|m| m.send).collect();
        assert_eq!(states.last(), Some(&SendState::Sending));
    }

    // ---- 6.1 and 6.2: deleting

    #[test]
    fn dd_opens_a_prompt_and_sends_nothing() {
        let mut app = own_app();
        press(&mut app, "dd");
        assert!(app.confirm.is_some());
        assert!(app.effects.is_empty());
        assert_eq!(app.mode, Mode::Normal);
        let screen = render_to_string(&app, 80, 24);
        let status = screen.lines().nth(22).expect("status row").to_string();
        assert!(status.contains("Delete this message?"), "{status}");
    }

    #[test]
    fn confirm_with_y_and_enter() {
        for key in ["y", "<CR>"] {
            let mut app = own_app();
            press(&mut app, "dd");
            press(&mut app, key);
            assert!(app.confirm.is_none(), "{key}");
            assert_eq!(delete_effects(&app), ["m1"], "{key}");
        }
    }

    #[test]
    fn any_other_key_cancels_and_is_consumed() {
        for key in ["j", "n", "<Esc>", "k", "Y", "<C-y>", "i", ":"] {
            let mut app = own_app();
            press(&mut app, "dd");
            press(&mut app, key);
            assert!(app.confirm.is_none(), "{key}");
            assert!(app.effects.is_empty(), "{key}");
            assert_eq!(app.messages[&general()][1].id, "m1");
            assert_eq!(app.mode, Mode::Normal, "{key}");
            assert_eq!(app.focus, Focus::Messages, "{key}");
            assert_eq!(
                crate::app::messages::view(&app, &general()).cursor,
                1,
                "{key}"
            );
        }
    }

    #[test]
    fn row_changed_between_prompt_and_confirm() {
        // It became someone else's, a tombstone or was removed: confirming sends nothing.
        type Change = Box<dyn Fn(&mut App)>;
        let mut variants: Vec<Change> = Vec::new();
        variants.push(Box::new(|app| {
            app.messages.get_mut(&general()).expect("room")[1].author = "alice".to_string();
        }));
        variants.push(Box::new(|app| {
            crate::app::messages::tombstone(app, &general(), "m1");
        }));
        variants.push(Box::new(|app| {
            app.messages.get_mut(&general()).expect("room").remove(1);
        }));
        variants.push(Box::new(|app| app.own_username = None));
        for (n, change) in variants.iter().enumerate() {
            let mut app = own_app();
            press(&mut app, "dd");
            change(&mut app);
            press(&mut app, "y");
            assert!(app.effects.is_empty(), "variant {n}");
            assert!(app.confirm.is_none(), "variant {n}");
        }
    }

    #[test]
    fn counted_dd_opens_one_prompt() {
        let mut app = own_app();
        press(&mut app, "3dd");
        assert!(app.confirm.is_some());
        press(&mut app, "y");
        assert_eq!(delete_effects(&app), ["m1"], "one delete, not three");
        // A second dd while a prompt is open does not open a second one either.
        let mut app = own_app();
        press(&mut app, "dd");
        update(&mut app, Action::DeleteMessage);
        assert!(app.confirm.is_some());
        press(&mut app, "y");
        assert_eq!(delete_effects(&app).len(), 1);
    }

    #[test]
    fn prompt_survives_the_status_clearing() {
        let mut app = own_app();
        press(&mut app, "dd");
        // Clock and realtime actions do not touch it; a key is the only thing that closes it.
        update(&mut app, Action::Clock(5));
        update(
            &mut app,
            Action::RtDeleted {
                room: general(),
                id: "other".to_string(),
            },
        );
        app.status_msg = None;
        assert!(app.confirm.is_some());
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("Delete this message?"), "{screen}");
    }

    #[test]
    fn delete_done_tombstones_at_once() {
        let mut app = own_app();
        press(&mut app, "dd");
        press(&mut app, "y");
        update(
            &mut app,
            Action::DeleteDone {
                room: general(),
                id: "m1".to_string(),
            },
        );
        let held = &app.messages[&general()][1];
        assert_eq!(held.kind, MessageKind::Deleted);
        assert_eq!(held.id, "m1");
        assert_eq!(app.messages[&general()].len(), 3);
        // The later stream event finds nothing to do.
        update(
            &mut app,
            Action::RtDeleted {
                room: general(),
                id: "m1".to_string(),
            },
        );
        assert_eq!(app.messages[&general()].len(), 3);
        assert_eq!(app.messages[&general()][1].kind, MessageKind::Deleted);
        // And `e` and `dd` on the tombstone do nothing.
        press(&mut app, "e");
        press(&mut app, "dd");
        assert!(app.editing.is_none() && app.confirm.is_none());
    }

    #[test]
    fn delete_refused_leaves_the_row() {
        let mut app = own_app();
        press(&mut app, "dd");
        press(&mut app, "y");
        update(
            &mut app,
            Action::DeleteFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Rejected,
            },
        );
        assert_eq!(body(&app, "m1"), "my text");
        assert_eq!(app.messages[&general()][1].kind, MessageKind::Normal);
        let line = status_text(&app).expect("status");
        assert!(
            line.starts_with("message not deleted: refused by the server"),
            "{line}"
        );
        assert!(!line.contains("my text"), "{line}");
    }

    #[test]
    fn status_lines_never_hold_message_text() {
        for error in [
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
            ApiError::Rejected,
            ApiError::ConnectionLost,
        ] {
            let mut app = app_with(vec![row("m1", OWN, SENTINEL)]);
            press(&mut app, "e<C-u>other text<CR>");
            update(
                &mut app,
                Action::EditFailed {
                    room: general(),
                    id: "m1".to_string(),
                    error,
                },
            );
            let line = status_text(&app).expect("status");
            assert!(
                !line.contains(SENTINEL) && !line.contains("other text"),
                "{line}"
            );
            update(
                &mut app,
                Action::DeleteFailed {
                    room: general(),
                    id: "m1".to_string(),
                    error,
                },
            );
            let line = status_text(&app).expect("status");
            assert!(!line.contains(SENTINEL), "{line}");
        }
    }

    #[test]
    fn prompt_snapshots() {
        let mut app = own_app();
        press(&mut app, "dd");
        insta::assert_snapshot!("delete_prompt_80x24", render_to_string(&app, 80, 24));
        app.size = ratatui::layout::Size::new(40, 12);
        insta::assert_snapshot!("delete_prompt_40x12", render_to_string(&app, 40, 12));
    }

    #[test]
    fn editing_snapshot() {
        let mut app = own_app();
        press(&mut app, "e");
        insta::assert_snapshot!("editing_80x24", render_to_string(&app, 80, 24));
    }
}
