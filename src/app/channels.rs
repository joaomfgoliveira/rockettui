//! Opening a referenced channel (`open_channel`, `gc`): the room a `#channel` in the message under
//! the cursor names.
//!
//! One channel opens at once. Several open a small picker, which is state on `App` like the
//! reactions popup: it names the message by room and id and holds only the selected row, so the
//! list is rebuilt from the held message on every draw, and `close_if_stale` closes it when the
//! message loses its channels or leaves the room. While it is open the reducer hands it every key.
//!
//! The room is found among the rooms the user is subscribed to, by name, ignoring case; no request
//! is made, and the client never joins a room: a name that is not one of the user's rooms gets a
//! status notice and changes nothing. A direct message is never a match, since its name is a
//! username, not a channel.
//!
//! Pure: no I/O and no `Frame`; `ui::channels` draws the picker. Text goes through the sanitizer
//! here, so the drawer can measure what it prints.

use crate::app::{App, Focus, Mode, StatusMsg, history, messages};
use crate::data::{RoomId, RoomKind};
use crate::input::key::Chord;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};

/// The message the picker was opened on, by room and id, and the selected row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelPicker {
    pub room: RoomId,
    pub id: String,
    pub selected: usize,
}

/// What the picker shows: the channel names of its message, sanitized, in the order the server
/// listed them, and the selected row (always in range).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub names: Vec<String>,
    pub selected: usize,
}

/// The channels a message references, in server order, without repeats (ignoring case) and
/// without empty names.
fn channels_of(message: &crate::data::Message) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in message.refs.iter().flat_map(|r| &r.channels) {
        if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(name.clone());
        }
    }
    out
}

/// The room, message id and channel names of the message under the cursor when the messages pane
/// is focused on a message that references a channel: what `open_channel` acts on, and what the
/// context popup checks before it lists the action. `None` for any other pane, no open room or a
/// message without a channel reference.
pub fn under_cursor(app: &App) -> Option<(RoomId, String, Vec<String>)> {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.clone()?;
    let message = app
        .messages
        .get(&room)?
        .get(messages::view(app, &room).cursor)?;
    let channels = channels_of(message);
    (!channels.is_empty()).then(|| (room, message.id.clone(), channels))
}

/// `open_channel`: one channel opens its room, several open the picker, none do nothing.
pub fn open(app: &mut App) {
    let Some((room, id, names)) = under_cursor(app) else {
        return;
    };
    if let [name] = names.as_slice() {
        open_named(app, name);
    } else {
        app.matcher.reset();
        app.channel_picker = Some(ChannelPicker {
            room,
            id,
            selected: 0,
        });
    }
}

/// Open the subscribed room named `name`, or say that it is not one of the user's rooms. Opening
/// the room that is already open changes nothing.
fn open_named(app: &mut App, name: &str) {
    let found = app
        .rooms
        .iter()
        .find(|r| r.kind != RoomKind::DirectMessage && r.name.eq_ignore_ascii_case(name))
        .map(|r| r.id.clone());
    match found {
        Some(id) if app.current_room.as_ref() == Some(&id) => {}
        Some(id) => {
            // The same two steps as `Enter` on a sidebar room.
            app.open_room(id.clone());
            history::on_open(app, &id);
        }
        None => {
            app.status_msg = Some(StatusMsg::Info(format!(
                "#{} is not one of your rooms",
                sanitize_line(name)
            )));
        }
    }
}

/// The picker's content now, `None` when it is closed or its message is gone or has no channel.
pub fn view(app: &App) -> Option<View> {
    let picker = app.channel_picker.as_ref()?;
    let message = app
        .messages
        .get(&picker.room)?
        .iter()
        .find(|m| m.id == picker.id)?;
    let names: Vec<String> = channels_of(message)
        .iter()
        .map(|n| sanitize_line(n).into_owned())
        .collect();
    if names.is_empty() {
        return None;
    }
    let selected = picker.selected.min(names.len() - 1);
    Some(View { names, selected })
}

/// Close the picker when it has nothing left to show. The reducer calls this after every action.
pub fn close_if_stale(app: &mut App) {
    if app.channel_picker.is_some() && view(app).is_none() {
        app.channel_picker = None;
    }
}

/// A key while the picker is open: it never reaches the keymap. `Esc`, `<C-c>` and `q` close it
/// without choosing, `j`, `<C-j>`, `<C-n>` and `Down` move down, `k`, `<C-k>`, `<C-p>` and `Up`
/// move up, and `Enter` opens the selected channel. Nothing else does anything.
pub fn key(app: &mut App, chord: Chord) {
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    let plain = |c: char| chord.code == KeyCode::Char(c) && chord.mods.is_empty();
    if ctrl('c') || plain('q') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        app.channel_picker = None;
    } else if plain('j') || ctrl('n') || ctrl('j') || chord.code == KeyCode::Down {
        move_selection(app, true);
    } else if plain('k') || ctrl('p') || ctrl('k') || chord.code == KeyCode::Up {
        move_selection(app, false);
    } else if chord.code == KeyCode::Enter {
        choose(app);
    }
}

/// One row down (`forward`) or up, stopping at either end.
fn move_selection(app: &mut App, forward: bool) {
    let Some(view) = view(app) else {
        return;
    };
    let next = if forward {
        (view.selected + 1).min(view.names.len() - 1)
    } else {
        view.selected.saturating_sub(1)
    };
    if let Some(picker) = app.channel_picker.as_mut() {
        picker.selected = next;
    }
}

/// `Enter`: close the picker and open the selected channel (or say it is not one of the rooms).
fn choose(app: &mut App) {
    let Some(picker) = app.channel_picker.take() else {
        return;
    };
    let name = app
        .messages
        .get(&picker.room)
        .and_then(|held| held.iter().find(|m| m.id == picker.id))
        .map(channels_of)
        .and_then(|names| names.into_iter().nth(picker.selected));
    if let Some(name) = name {
        open_named(app, &name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::data::{Message, Refs, SendState};
    use crate::test_util::{app_80x24, press, sample_message};

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    /// An app in `general` whose only message references `channels` (by name).
    fn app_with(channels: &[&str]) -> App {
        let mut app = app_80x24();
        app.open_room(general());
        let message = Message {
            room_id: general(),
            send: SendState::Sent,
            ts: i64::MAX / 4,
            body: "see them".to_string(),
            refs: Some(Refs {
                mentions: Vec::new(),
                channels: channels.iter().map(|s| s.to_string()).collect(),
            }),
            ..sample_message("m-ref")
        };
        app.messages.insert(general(), vec![message]);
        app.msg_views.remove(&general());
        app.focus = Focus::Messages;
        app
    }

    fn current(app: &App) -> Option<&str> {
        app.current_room.as_ref().map(|r| r.0.as_str())
    }

    fn room_named(app: &App, name: &str) -> RoomId {
        app.rooms
            .iter()
            .find(|r| r.name == name)
            .expect(name)
            .id
            .clone()
    }

    #[test]
    fn one_channel_opens_its_room_directly() {
        let mut app = app_with(&["random"]);
        let random = room_named(&app, "random");
        press(&mut app, "gc");
        assert_eq!(app.current_room, Some(random));
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn the_name_is_matched_ignoring_case() {
        let mut app = app_with(&["RANDOM"]);
        let random = room_named(&app, "random");
        press(&mut app, "gc");
        assert_eq!(app.current_room, Some(random));
    }

    #[test]
    fn a_message_without_a_channel_does_nothing() {
        let mut app = app_with(&[]);
        let before = app.current_room.clone();
        press(&mut app, "gc");
        assert_eq!(app.current_room, before);
        assert!(app.channel_picker.is_none());
        assert!(app.status_msg.is_none());
        // A message with no lists at all is the same.
        app.messages.get_mut(&general()).unwrap()[0].refs = None;
        press(&mut app, "gc");
        assert_eq!(app.current_room, before);
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn a_room_the_user_is_not_in_gets_a_notice_and_changes_nothing() {
        let mut app = app_with(&["secret-project"]);
        let effects = app.effects.len();
        press(&mut app, "gc");
        assert_eq!(current(&app), Some("general"));
        assert!(
            matches!(
                &app.status_msg,
                Some(StatusMsg::Info(text)) if text == "#secret-project is not one of your rooms"
            ),
            "the notice names the room"
        );
        assert_eq!(app.effects.len(), effects, "no request, no join");
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn a_direct_message_is_never_a_channel() {
        // `alice` is a direct message in the fake data: `#alice` is not that room.
        let mut app = app_with(&["alice"]);
        press(&mut app, "gc");
        assert_eq!(current(&app), Some("general"));
        assert!(matches!(app.status_msg, Some(StatusMsg::Info(_))));
    }

    #[test]
    fn the_room_that_is_already_open_stays_as_it_is() {
        let mut app = app_with(&["general"]);
        press(&mut app, "gc");
        assert_eq!(current(&app), Some("general"));
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn several_channels_open_a_picker_that_lists_only_that_messages_channels() {
        let mut app = app_with(&["random", "quiet", "RANDOM"]);
        press(&mut app, "gc");
        assert_eq!(current(&app), Some("general"), "nothing opens yet");
        let view = view(&app).expect("picker");
        assert_eq!(view.names, ["random", "quiet"], "repeats are dropped");
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn choosing_in_the_picker_opens_that_room() {
        let mut app = app_with(&["random", "quiet"]);
        let quiet = room_named(&app, "quiet");
        press(&mut app, "gcj");
        assert_eq!(view(&app).unwrap().selected, 1);
        press(&mut app, "<Enter>");
        assert_eq!(app.current_room, Some(quiet));
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn the_picker_stops_at_both_ends_and_takes_the_arrow_keys() {
        let mut app = app_with(&["random", "quiet"]);
        press(&mut app, "gckkk");
        assert_eq!(view(&app).unwrap().selected, 0);
        press(&mut app, "<Down><Down><Down>");
        assert_eq!(view(&app).unwrap().selected, 1);
        press(&mut app, "<Up>");
        assert_eq!(view(&app).unwrap().selected, 0);
    }

    #[test]
    fn cancelling_the_picker_changes_nothing() {
        for cancel in ["<Esc>", "q", "<C-c>"] {
            let mut app = app_with(&["random", "quiet"]);
            press(&mut app, "gc");
            press(&mut app, cancel);
            assert!(app.channel_picker.is_none(), "{cancel}");
            assert_eq!(current(&app), Some("general"), "{cancel}");
            assert!(app.status_msg.is_none(), "{cancel}");
        }
    }

    #[test]
    fn the_picker_takes_every_key_so_nothing_reaches_the_keymap() {
        let mut app = app_with(&["random", "quiet"]);
        press(&mut app, "gc");
        // `i` would enter Insert mode and `G` would move the cursor.
        press(&mut app, "iG");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.channel_picker.is_some());
    }

    #[test]
    fn choosing_an_unsubscribed_room_in_the_picker_gives_the_notice() {
        let mut app = app_with(&["nowhere", "random"]);
        press(&mut app, "gc<Enter>");
        assert_eq!(current(&app), Some("general"));
        assert!(matches!(app.status_msg, Some(StatusMsg::Info(_))));
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn the_picker_closes_when_its_message_is_gone() {
        let mut app = app_with(&["random", "quiet"]);
        press(&mut app, "gc");
        assert!(app.channel_picker.is_some());
        app.messages.get_mut(&general()).unwrap().clear();
        update(&mut app, Action::Clock(0));
        assert!(app.channel_picker.is_none());
    }

    #[test]
    fn it_only_acts_from_the_messages_pane_in_normal_mode() {
        let mut app = app_with(&["random"]);
        app.focus = Focus::Sidebar;
        assert!(under_cursor(&app).is_none());
        app.focus = Focus::Messages;
        assert!(under_cursor(&app).is_some());
    }
}
