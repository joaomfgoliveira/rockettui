//! The reactions popup (`show_reactions`, `gr`): who gave each reaction of the message under the
//! cursor.
//!
//! State, not a mode: like the context popup it leaves the mode badge, the focus and the cursor
//! alone, and any key closes it and is swallowed (`app::update`). The state names the message by
//! room and id and holds no copy of its reactions, so `content` is rebuilt from the held message
//! on every draw and a live reaction shows up without any plumbing. `close_if_stale` closes the
//! popup when its message loses its last reaction or leaves the room.
//!
//! Pure: no I/O and no `Frame`; `ui::reactions` draws it. Text goes through the sanitizer here, so
//! the drawer can measure what it prints.

use crate::app::{App, Focus, Mode, messages};
use crate::data::RoomId;
use crate::ui::emoji::resolve;
use crate::ui::sanitize::sanitize_line;

/// What the user's own entry in a user list says instead of their username.
pub const YOU: &str = "you";

/// The message the popup was opened on, by room and id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionsPopup {
    pub room: RoomId,
    pub id: String,
}

/// One reaction as the popup shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What the row of the message draws for it (the glyph, an alias, or the raw `:name:`).
    pub glyph: String,
    /// The `:shortcode:` as the server named it, sanitized.
    pub shortcode: String,
    /// The usernames that gave it, sanitized, in server order. The user's own is `you` and comes
    /// first.
    pub users: Vec<String>,
    /// True when the user's own username is among them.
    pub own: bool,
}

/// What the popup shows: one entry per reaction, in the order of the message's reaction row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    pub entries: Vec<Entry>,
}

/// The room and id of the message under the cursor when the messages pane is focused on one that
/// has reactions: what `show_reactions` acts on, and what the context popup checks before it lists
/// the action. `None` for any other pane, no open room or a message without reactions.
pub fn under_cursor(app: &App) -> Option<(RoomId, String)> {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.clone()?;
    let message = app
        .messages
        .get(&room)?
        .get(messages::view(app, &room).cursor)?;
    (!message.reactions.is_empty()).then(|| (room, message.id.clone()))
}

/// `show_reactions`: open the popup for the message under the cursor. A silent no-op unless the
/// messages pane is focused on a message that has reactions.
pub fn open(app: &mut App) {
    if let Some((room, id)) = under_cursor(app) {
        app.reactions_popup = Some(ReactionsPopup { room, id });
    }
}

/// The popup's content now, `None` when it is closed or its message is gone or has no reactions.
pub fn content(app: &App) -> Option<Content> {
    let popup = app.reactions_popup.as_ref()?;
    let message = app
        .messages
        .get(&popup.room)?
        .iter()
        .find(|m| m.id == popup.id)?;
    if message.reactions.is_empty() {
        return None;
    }
    let me = app.own_username.as_deref();
    let entries = message
        .reactions
        .iter()
        .map(|reaction| {
            let own = me.is_some_and(|me| reaction.users.iter().any(|user| user == me));
            let mut users: Vec<String> = Vec::new();
            if own {
                users.push(YOU.to_string());
            }
            users.extend(
                reaction
                    .users
                    .iter()
                    .filter(|user| Some(user.as_str()) != me)
                    .map(|user| sanitize_line(user).into_owned()),
            );
            Entry {
                glyph: resolve(&reaction.name, &app.settings.emoji_aliases),
                shortcode: format!(":{}:", sanitize_line(&reaction.name)),
                users,
                own,
            }
        })
        .collect();
    Some(Content { entries })
}

/// Close the popup when it has nothing left to show: its message lost its last reaction, or is no
/// longer held. The reducer calls this after every action, so a live change closes it at once.
pub fn close_if_stale(app: &mut App) {
    if app.reactions_popup.is_some() && content(app).is_none() {
        app.reactions_popup = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::data::{Message, Reaction, SendState};
    use crate::test_util::{app_80x24, press, sample_message};

    fn reaction(name: &str, users: &[&str]) -> Reaction {
        Reaction {
            name: name.to_string(),
            users: users.iter().map(|u| u.to_string()).collect(),
        }
    }

    fn message(room: &RoomId, id: &str, reactions: Vec<Reaction>) -> Message {
        Message {
            room_id: room.clone(),
            reactions,
            send: SendState::Sent,
            ts: i64::MAX / 4,
            ..sample_message(id)
        }
    }

    /// An app with the messages pane focused on the middle one of three messages, the middle one
    /// carrying `reactions`. The user is `me`.
    fn app_on_reactions(reactions: Vec<Reaction>) -> (App, RoomId) {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        app.messages.insert(
            room.clone(),
            vec![
                message(&room, "m0", Vec::new()),
                message(&room, "m1", reactions),
                message(&room, "m2", Vec::new()),
            ],
        );
        app.msg_views.remove(&room);
        app.current_room = Some(room.clone());
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        press(&mut app, "gg");
        press(&mut app, "j");
        assert_eq!(messages::view(&app, &room).cursor, 1);
        (app, room)
    }

    fn some_reactions() -> Vec<Reaction> {
        vec![
            reaction("+1", &["bob", "me", "ana"]),
            reaction("tada", &["ana"]),
        ]
    }

    #[test]
    fn gr_opens_the_popup_without_touching_mode_focus_or_cursor() {
        let (mut app, room) = app_on_reactions(some_reactions());
        let mode = app.mode;
        press(&mut app, "gr");
        assert_eq!(
            app.reactions_popup,
            Some(ReactionsPopup {
                room: room.clone(),
                id: "m1".to_string()
            })
        );
        assert_eq!(app.mode, mode);
        assert_eq!(app.mode.label(), "NORMAL");
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(messages::view(&app, &room).cursor, 1);
    }

    #[test]
    fn content_lists_each_reaction_with_you_first_and_own_marked() {
        let (mut app, _) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        let content = content(&app).expect("open");
        assert_eq!(content.entries.len(), 2);
        let plus = &content.entries[0];
        assert_eq!(plus.glyph, "👍");
        assert_eq!(plus.shortcode, ":+1:");
        // The server listed bob, me, ana; the user's own entry comes first, as `you`.
        assert_eq!(plus.users, ["you", "bob", "ana"]);
        assert!(plus.own);
        let tada = &content.entries[1];
        assert_eq!(tada.glyph, "🎉");
        assert_eq!(tada.users, ["ana"]);
        assert!(!tada.own);
    }

    #[test]
    fn without_an_own_username_nobody_is_you() {
        let (mut app, _) = app_on_reactions(some_reactions());
        app.own_username = None;
        press(&mut app, "gr");
        let content = content(&app).expect("open");
        assert_eq!(content.entries[0].users, ["bob", "me", "ana"]);
        assert!(content.entries.iter().all(|e| !e.own));
    }

    #[test]
    fn aliases_and_raw_names_show_in_the_popup() {
        let (mut app, _) = app_on_reactions(vec![
            reaction("partyparrot", &["bob"]),
            reaction("rt_unknown", &["bob"]),
        ]);
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        press(&mut app, "gr");
        let content = content(&app).expect("open");
        assert_eq!(content.entries[0].glyph, "🦜");
        assert_eq!(content.entries[0].shortcode, ":partyparrot:");
        // A name that resolves to nothing is drawn raw, and is still listed with its users.
        assert_eq!(content.entries[1].glyph, ":rt_unknown:");
        assert_eq!(content.entries[1].users, ["bob"]);
    }

    #[test]
    fn a_reaction_named_with_a_name_the_server_uses_shows_its_emoji_in_the_popup() {
        let (mut app, _) = app_on_reactions(vec![reaction("flag_af", &["bob"])]);
        press(&mut app, "gr");
        let content = content(&app).expect("open");
        assert_eq!(content.entries[0].glyph, "🇦🇫");
        assert_eq!(content.entries[0].shortcode, ":flag_af:");
    }

    #[test]
    fn gr_on_a_message_without_reactions_does_nothing() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "k");
        assert_eq!(messages::view(&app, &room).cursor, 0);
        press(&mut app, "gr");
        assert_eq!(app.reactions_popup, None);
    }

    #[test]
    fn gr_with_another_focus_or_no_room_does_nothing() {
        let (mut app, _) = app_on_reactions(some_reactions());
        app.focus = Focus::Sidebar;
        press(&mut app, "gr");
        assert_eq!(app.reactions_popup, None);
        app.focus = Focus::Input;
        press(&mut app, "gr");
        assert_eq!(app.reactions_popup, None);
        app.focus = Focus::Messages;
        app.current_room = None;
        press(&mut app, "gr");
        assert_eq!(app.reactions_popup, None);
    }

    #[test]
    fn any_key_closes_the_popup_and_is_swallowed() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        assert!(app.reactions_popup.is_some());
        // `j` would move the cursor; it only closes the popup.
        press(&mut app, "j");
        assert_eq!(app.reactions_popup, None);
        assert_eq!(
            messages::view(&app, &room).cursor,
            1,
            "the cursor did not move"
        );
        assert_eq!(app.mode.label(), "NORMAL");
        // And the key after that is an ordinary key again.
        press(&mut app, "j");
        assert_eq!(messages::view(&app, &room).cursor, 2);
        // `:` does not open the command line from the popup either.
        press(&mut app, "k");
        press(&mut app, "gr");
        press(&mut app, ":");
        assert_eq!(app.reactions_popup, None);
        assert_eq!(app.mode.label(), "NORMAL");
    }

    #[test]
    fn a_reaction_arriving_while_open_shows_without_a_key() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        let mut live = message(
            &room,
            "m1",
            vec![
                reaction("+1", &["bob", "me", "ana"]),
                reaction("tada", &["ana"]),
                reaction("rocket", &["zed"]),
            ],
        );
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert!(app.reactions_popup.is_some(), "still open");
        let content = content(&app).expect("open");
        assert_eq!(content.entries.len(), 3);
        assert_eq!(content.entries[2].glyph, "🚀");
        assert_eq!(content.entries[2].users, ["zed"]);
    }

    #[test]
    fn removing_the_last_reaction_closes_the_popup() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        // One of two taken back: still open, one entry.
        let mut live = message(&room, "m1", vec![reaction("tada", &["ana"])]);
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(content(&app).expect("open").entries.len(), 1);
        // The last one: the server sends the message with the key absent.
        let mut live = message(&room, "m1", Vec::new());
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(app.reactions_popup, None);
        assert_eq!(app.mode.label(), "NORMAL");
        // A later key is an ordinary key, not a swallowed one.
        press(&mut app, "j");
        assert_eq!(messages::view(&app, &room).cursor, 2);
    }

    #[test]
    fn deleting_the_message_closes_the_popup() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        update(
            &mut app,
            Action::RtDeleted {
                room: room.clone(),
                id: "m1".to_string(),
            },
        );
        assert_eq!(app.reactions_popup, None);
        // A tombstone keeps nothing of the message, so `gr` on it does nothing either.
        assert!(app.messages[&room][1].reactions.is_empty());
        press(&mut app, "gr");
        assert_eq!(app.reactions_popup, None);
    }

    #[test]
    fn the_room_updating_underneath_does_not_close_it() {
        let (mut app, room) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        let mut newer = message(&room, "m3", Vec::new());
        newer.ts = i64::MAX / 4 + 10;
        update(&mut app, Action::RtMessage(newer));
        assert_eq!(app.messages[&room].len(), 4, "the message was added");
        assert!(app.reactions_popup.is_some());
    }

    #[test]
    fn hostile_names_and_users_are_sanitized() {
        let (mut app, _) = app_on_reactions(vec![reaction(
            "bad\u{1b}[31mname\u{7}",
            &["ev\u{1b}]0;il\u{7}", "me"],
        )]);
        press(&mut app, "gr");
        let content = content(&app).expect("open");
        let all = format!(
            "{} {} {}",
            content.entries[0].glyph,
            content.entries[0].shortcode,
            content.entries[0].users.join(" ")
        );
        assert!(!all.chars().any(|c| c.is_control()), "{all:?}");
    }

    #[test]
    fn key_timeout_while_open_does_not_close_it() {
        let (mut app, _) = app_on_reactions(some_reactions());
        press(&mut app, "gr");
        update(&mut app, Action::KeyTimeout);
        assert!(app.reactions_popup.is_some());
    }
}
