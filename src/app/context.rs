//! The context popup's content: what the selected object can do, with the keys of the live keymap.
//!
//! Pure: no I/O and no `Frame`; `ui::context` draws it. The object kinds use the same predicates
//! the actions themselves use, so the popup does not promise an action that would be a silent
//! no-op. Only built-in text is shown, never a message, an author or a room name.

use crate::app::{App, Focus, Mode, edit, send, sidebar};
use crate::input::actions::name_of;
use crate::input::key::format_keys;

/// What is selected, as far as the available actions are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Room,
    Folder,
    Header,
    OwnMessage,
    UnsentMessage,
    OtherMessage,
    Input,
    Nothing,
}

impl Kind {
    /// The name line of the popup.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Room => "Room",
            Kind::Folder => "Folder",
            Kind::Header => "Section header",
            Kind::OwnMessage => "Your message",
            Kind::UnsentMessage => "Your unsent message",
            Kind::OtherMessage => "Message",
            Kind::Input => "Input bar",
            Kind::Nothing => "Nothing selected",
        }
    }

    /// The action names listed for the kind, with the wording used in the popup. A test checks
    /// every name is a real action.
    pub fn actions(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Room => &[("select", "open the room")],
            Kind::Folder => &[
                ("select", "open or close the folder"),
                ("toggle_fold", "fold or unfold the folder"),
            ],
            Kind::OwnMessage => &[
                ("edit_message", "edit the message"),
                ("delete_message", "delete the message"),
            ],
            Kind::UnsentMessage => &[
                ("retry_send", "retry sending"),
                ("discard_send", "discard it"),
                ("delete_message", "discard it"),
            ],
            Kind::Input => &[
                ("enter_insert", "type a message (Insert mode)"),
                ("move_left", "move left in the draft"),
                ("move_right", "move right in the draft"),
                ("move_up", "move up in the draft"),
                ("move_down", "move down in the draft"),
            ],
            Kind::Header | Kind::OtherMessage | Kind::Nothing => &[],
        }
    }
}

/// One row of the popup: the keys bound to an action and what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextLine {
    pub keys: String,
    pub desc: String,
}

/// What the popup shows: the object name, the action rows, and the way to the full help last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub kind: Kind,
    pub rows: Vec<ContextLine>,
    pub full_help: ContextLine,
}

/// The kind of the selected object. Follows `App.focus` only, as the motions do. An unsent row is
/// classified before an own server message, mirroring `delete_message`.
pub fn classify(app: &App) -> Kind {
    match app.focus {
        Focus::Sidebar => match sidebar::selected_row(app) {
            Some(sidebar::SidebarRow::Room { .. }) => Kind::Room,
            Some(sidebar::SidebarRow::Folder { .. }) => Kind::Folder,
            Some(sidebar::SidebarRow::Header(_)) => Kind::Header,
            None => Kind::Nothing,
        },
        Focus::Input => Kind::Input,
        Focus::Messages => {
            if send::unsent_under_cursor(app).is_some() {
                Kind::UnsentMessage
            } else if edit::own_under_cursor(app).is_some() {
                Kind::OwnMessage
            } else if message_under_cursor(app) {
                Kind::OtherMessage
            } else {
                Kind::Nothing
            }
        }
    }
}

/// True when a room is open and holds a message for the cursor to be on.
fn message_under_cursor(app: &App) -> bool {
    app.current_room
        .as_ref()
        .and_then(|room| app.messages.get(room))
        .is_some_and(|held| !held.is_empty())
}

/// The Normal-mode keys bound to `name`, sorted and joined by a space. `cmd:` rows never match
/// because they have no action name.
fn keys_for(app: &App, name: &str) -> Option<String> {
    let mut keys: Vec<String> = app
        .keymap
        .bindings(Mode::Normal)
        .into_iter()
        .filter(|(_, action)| name_of(action) == Some(name))
        .map(|(chords, _)| format_keys(&chords))
        .collect();
    if keys.is_empty() {
        return None;
    }
    keys.sort();
    Some(keys.join(" "))
}

/// The popup content for the current selection, recomputed on every draw.
pub fn lines(app: &App) -> Context {
    let kind = classify(app);
    let mut rows: Vec<ContextLine> = Vec::new();
    for (name, desc) in kind.actions() {
        let Some(keys) = keys_for(app, name) else {
            continue;
        };
        // Two names can share a wording (`discard_send` and `delete_message`): keep one row per
        // wording, with every key.
        if let Some(row) = rows.iter_mut().find(|r| r.desc == *desc) {
            row.keys = format!("{} {keys}", row.keys);
        } else {
            rows.push(ContextLine {
                keys,
                desc: (*desc).to_string(),
            });
        }
    }
    let keys = keys_for(app, "show_context")
        .or_else(|| keys_for(app, "show_help"))
        .unwrap_or_else(|| ":help".to_string());
    Context {
        kind,
        rows,
        full_help: ContextLine {
            keys,
            desc: "all keys (full help)".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::SendState;
    use crate::input::actions::ACTION_NAMES;
    use crate::test_util::{app_80x24, app_with_config, config_from_keymap, press, sample_message};

    fn descs(ctx: &Context) -> Vec<&str> {
        ctx.rows.iter().map(|r| r.desc.as_str()).collect()
    }

    /// An app with the messages pane focused on a room holding one message by `author`.
    fn messages_app(author: &str, send: SendState) -> App {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = author.to_string();
        m.send = send;
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.msg_views.remove(&room);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        app
    }

    #[test]
    fn room_row() {
        let mut app = app_80x24();
        while !matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Room { .. })
        ) {
            press(&mut app, "j");
        }
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Room);
        assert_eq!(descs(&ctx), ["open the room"]);
        assert_eq!(ctx.rows[0].keys, "<CR>");
        assert_eq!(ctx.full_help.keys, "?");
    }

    #[test]
    fn folder_row() {
        let mut app = app_80x24();
        let mut n = 0;
        while !matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Folder { .. })
        ) {
            press(&mut app, "j");
            n += 1;
            assert!(n < 50, "no folder row in the fixture");
        }
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Folder);
        assert_eq!(
            descs(&ctx),
            ["open or close the folder", "fold or unfold the folder"]
        );
    }

    #[test]
    fn header_row() {
        let mut app = app_80x24();
        app.sidebar.cursor = 0;
        assert!(matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Header(_))
        ));
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Header);
        assert!(ctx.rows.is_empty());
    }

    #[test]
    fn own_message() {
        let app = messages_app("me", SendState::Sent);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OwnMessage);
        assert_eq!(descs(&ctx), ["edit the message", "delete the message"]);
        assert_eq!(ctx.rows[0].keys, "e");
        assert_eq!(ctx.rows[1].keys, "dd");
    }

    #[test]
    fn unsent_message() {
        let app = messages_app("me", SendState::Failed);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::UnsentMessage);
        assert_eq!(descs(&ctx), ["retry sending", "discard it"]);
        assert_eq!(ctx.rows[0].keys, "r");
        assert_eq!(ctx.rows[1].keys, "dd");
    }

    #[test]
    fn other_message() {
        let app = messages_app("someone-else", SendState::Sent);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OtherMessage);
        assert!(ctx.rows.is_empty());
    }

    #[test]
    fn input_bar() {
        let mut app = app_80x24();
        app.focus = Focus::Input;
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Input);
        assert!(descs(&ctx).contains(&"type a message (Insert mode)"));
    }

    #[test]
    fn nothing_selected() {
        let mut app = app_80x24();
        app.current_room = None;
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Nothing);
        assert!(ctx.rows.is_empty());
    }

    #[test]
    fn rebound_key_shown() {
        let config = config_from_keymap("[normal]\n\"E\" = \"edit_message\"\n\"e\" = \"none\"\n");
        let mut app = app_with_config(config, 80, 24);
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "me".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(ctx.rows[0].desc, "edit the message");
        assert_eq!(ctx.rows[0].keys, "E");
    }

    #[test]
    fn unbound_action_dropped() {
        let config = config_from_keymap("[normal]\n\"e\" = \"none\"\n");
        let mut app = app_with_config(config, 80, 24);
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "me".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(descs(&ctx), ["delete the message"]);
    }

    #[test]
    fn cmd_binding_not_listed() {
        let config = config_from_keymap("[normal]\n\"<C-q>\" = \"cmd:quit\"\n");
        let app = app_with_config(config, 80, 24);
        let ctx = lines(&app);
        assert!(ctx.rows.iter().all(|r| !r.keys.contains("<C-q>")));
        assert!(!ctx.full_help.keys.contains("<C-q>"));
    }

    #[test]
    fn table_names_are_valid_actions() {
        for kind in [
            Kind::Room,
            Kind::Folder,
            Kind::Header,
            Kind::OwnMessage,
            Kind::UnsentMessage,
            Kind::OtherMessage,
            Kind::Input,
            Kind::Nothing,
        ] {
            for (name, desc) in kind.actions() {
                assert!(ACTION_NAMES.contains(name), "{name}");
                assert!(!desc.is_empty());
                assert!(
                    crate::app::help::DESCRIPTIONS
                        .iter()
                        .any(|(n, _)| n == name),
                    "{name} has no help description"
                );
            }
        }
    }
}

#[cfg(test)]
mod reducer_tests {
    use crate::action::Action;
    use crate::app::update::update;
    use crate::app::{Focus, Mode, sidebar};
    use crate::test_util::{app_80x24, press, sample_message};

    #[test]
    fn opens_without_changing_mode_focus_or_selection() {
        let mut app = app_80x24();
        let (mode, focus, cursor) = (app.mode, app.focus, app.sidebar.cursor);
        let room = app.current_room.clone();
        press(&mut app, "?");
        assert!(app.context);
        assert_eq!(app.help, None);
        assert_eq!(app.mode, mode);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, focus);
        assert_eq!(app.sidebar.cursor, cursor);
        assert_eq!(app.current_room, room);
    }

    #[test]
    fn any_key_closes_without_acting() {
        for keys in ["j", "dd", "i", "<CR>", ":", "q"] {
            let mut app = app_80x24();
            let before = (
                app.sidebar.cursor,
                app.mode,
                app.focus,
                app.current_room.clone(),
                app.cmdline.clone(),
            );
            press(&mut app, "?");
            press(&mut app, keys);
            assert!(!app.context, "{keys}");
            assert_eq!(app.help, None, "{keys}");
            assert_eq!(
                before,
                (
                    app.sidebar.cursor,
                    app.mode,
                    app.focus,
                    app.current_room.clone(),
                    app.cmdline.clone()
                ),
                "{keys}"
            );
            assert!(!app.should_quit, "{keys}");
        }
    }

    #[test]
    fn second_question_mark_opens_help_and_third_closes_it() {
        let mut app = app_80x24();
        press(&mut app, "??");
        assert!(!app.context);
        assert_eq!(app.help, Some(0));
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, "?");
        assert_eq!(app.help, None);
        assert!(!app.context);
    }

    #[test]
    fn key_timeout_and_counts_do_not_act() {
        let mut app = app_80x24();
        press(&mut app, "?");
        update(&mut app, Action::KeyTimeout);
        assert!(app.context);
        press(&mut app, "3");
        assert!(!app.context);
        assert_eq!(app.help, None);
    }

    #[test]
    fn follows_the_object_after_closing() {
        let mut app = app_80x24();
        assert!(matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Room { .. }) | Some(sidebar::SidebarRow::Folder { .. })
        ));
        app.focus = Focus::Input;
        press(&mut app, "?");
        assert_eq!(super::classify(&app), super::Kind::Input);
    }

    #[test]
    fn live_messages_still_applied_underneath() {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let before = app.messages.get(&room).map_or(0, Vec::len);
        press(&mut app, "?");
        let mut live = sample_message("live-ctx");
        live.room_id = room.clone();
        live.ts = i64::MAX / 4;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(app.messages.get(&room).map_or(0, Vec::len), before + 1);
        assert!(app.context);
    }
}
