//! The help overlay's content, derived from the live keymap and the command table.
//!
//! Pure: no I/O and no `Frame`. `ui::help` draws the lines and the reducer scrolls them. Nothing
//! here is a hand-written key list, so the help cannot go stale (a later which-key hint can reuse
//! [`build`]).

use crate::action::Action;
use crate::app::Mode;
use crate::app::command::COMMANDS;
use crate::input::actions::name_of;
use crate::input::key::format_keys;
use crate::input::keymap::Keymap;

/// One line of the overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelpLine {
    Heading(String),
    /// A key column and a description.
    Row {
        keys: String,
        desc: String,
    },
    Blank,
}

/// The short description of each bindable action name. A test keeps it complete.
pub const DESCRIPTIONS: &[(&str, &str)] = &[
    ("move_down", "move down"),
    ("move_up", "move up"),
    ("move_left", "move left"),
    ("move_right", "move right"),
    ("goto_top", "jump to the top"),
    ("goto_bottom", "jump to the bottom"),
    ("half_page_down", "half a page down"),
    ("half_page_up", "half a page up"),
    ("select", "open the room or toggle the folder"),
    ("toggle_fold", "fold or unfold the folder"),
    ("focus_left", "focus the sidebar"),
    ("focus_down", "focus the input bar"),
    ("focus_right", "focus the messages pane"),
    ("focus_next", "focus the next pane"),
    ("focus_prev", "focus the previous pane"),
    ("enter_command", "start a : command"),
    ("enter_insert", "type a message (: for emoji)"),
    ("cancel", "cancel, or leave the mode"),
    ("delete_back", "delete the character before the cursor"),
    ("insert_newline", "insert a newline"),
    ("delete_word_back", "delete the word before the cursor"),
    ("delete_to_line_start", "delete to the start of the line"),
    ("cursor_left", "cursor left"),
    ("cursor_right", "cursor right"),
    ("cursor_up", "cursor up"),
    ("cursor_down", "cursor down"),
    ("cursor_home", "cursor to the start of the line"),
    ("cursor_end", "cursor to the end of the line"),
    ("submit_input", "send the message (or save the edit)"),
    ("retry_send", "retry a failed message"),
    ("discard_send", "discard a failed message"),
    ("edit_message", "edit your message under the cursor"),
    ("delete_message", "delete your message under the cursor"),
    (
        "show_reactions",
        "show who reacted to the message under the cursor",
    ),
    (
        "open_channel",
        "open the channel the message under the cursor names",
    ),
    (
        "react_message",
        "add or remove your reaction on the message under the cursor",
    ),
    (
        "open_thread",
        "open the thread of the message under the cursor",
    ),
    ("list_threads", "list this room's threads"),
    (
        "reply_in_thread",
        "reply in the thread of the message under the cursor",
    ),
    (
        "toggle_reply_to_channel",
        "also show the thread reply in the channel",
    ),
    (
        "toggle_follow_thread",
        "follow or unfollow the thread of the message under the cursor",
    ),
    ("close_thread", "close the thread pane"),
    ("switch_room", "switch to another room by name"),
    ("show_help", "show or close this help"),
    ("show_context", "show what the selected object can do"),
    ("search_start", "find text in the open room"),
    ("search_next", "next match, toward older messages"),
    ("search_prev", "previous match, toward newer messages"),
    ("execute_command", "run the command line"),
    ("quit", "quit"),
];

fn describe(name: &str) -> &'static str {
    DESCRIPTIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("", |(_, d)| d)
}

/// What a bound action is called in the overlay: its description, or the command it runs.
fn describe_action(action: &Action) -> String {
    match action {
        Action::RunCommand(line) => format!(":{line}"),
        other => name_of(other).map(describe).unwrap_or_default().to_string(),
    }
}

/// Every binding of `keymap` by mode, then the commands. Rows keep a stable order: by action name,
/// then by keys.
pub fn build(keymap: &Keymap) -> Vec<HelpLine> {
    let mut lines = Vec::new();
    for (mode, title) in [
        (Mode::Normal, "Normal mode"),
        (Mode::Insert, "Insert mode"),
        (Mode::Command, "Command mode"),
    ] {
        let mut rows: Vec<(String, String)> = keymap
            .bindings(mode)
            .into_iter()
            .map(|(chords, action)| (describe_action(action), format_keys(&chords)))
            .collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort();
        lines.push(HelpLine::Heading(title.to_string()));
        lines.extend(
            rows.into_iter()
                .map(|(desc, keys)| HelpLine::Row { keys, desc }),
        );
        lines.push(HelpLine::Blank);
    }
    // The thread list's motions, `select` and `/` are the Normal-mode rows above, applied to the
    // list while it is focused; `move_left` and `move_right` pick the filter there. Only `q` and
    // `Esc` are fixed keys of the list.
    let bound = |name: &str| -> String {
        let mut keys: Vec<String> = keymap
            .bindings(Mode::Normal)
            .into_iter()
            .filter(|(_, action)| name_of(action) == Some(name))
            .map(|(chords, _)| format_keys(&chords))
            .collect();
        keys.sort();
        keys.join(" ")
    };
    lines.push(HelpLine::Heading("Thread list (focused)".to_string()));
    for (keys, desc) in [
        (
            format!("{} / {}", bound("move_left"), bound("move_right")),
            "previous or next filter: all, following, unread".to_string(),
        ),
        ("q Esc".to_string(), "close the list".to_string()),
        (
            bound("search_start"),
            "search the threads (Enter keeps, Esc clears)".to_string(),
        ),
    ] {
        lines.push(HelpLine::Row { keys, desc });
    }
    lines.push(HelpLine::Blank);
    // What the thread unread marks mean. They are drawn, not bound, so the keys column shows them.
    lines.push(HelpLine::Heading("Thread unread".to_string()));
    for (keys, desc) in [
        ("new", "thread has unread replies"),
        ("T(2)", "room has 2 unread threads"),
        ("", "open thread: read after delay"),
    ] {
        lines.push(HelpLine::Row {
            keys: keys.to_string(),
            desc: desc.to_string(),
        });
    }
    lines.push(HelpLine::Blank);
    lines.push(HelpLine::Heading("Commands".to_string()));
    for def in COMMANDS {
        let keys = def
            .names
            .iter()
            .map(|n| format!(":{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(HelpLine::Row {
            keys,
            desc: def.desc.to_string(),
        });
    }
    lines
}

/// Rows of text the overlay shows: the messages pane minus its border.
pub fn viewport_height(app: &crate::app::App) -> usize {
    usize::from(app.areas().messages.height.saturating_sub(2))
}

/// Largest valid scroll offset for `total` lines in a viewport of `height` rows.
pub fn max_scroll(total: usize, height: usize) -> usize {
    total.saturating_sub(height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::actions::ACTION_NAMES;
    use crate::input::keymap::default_keymap;
    use crate::test_util::{config_from_keymap, config_from_toml};

    fn rows(lines: &[HelpLine]) -> Vec<(String, String)> {
        lines
            .iter()
            .filter_map(|l| match l {
                HelpLine::Row { keys, desc } => Some((keys.clone(), desc.clone())),
                _ => None,
            })
            .collect()
    }

    fn has(lines: &[HelpLine], keys: &str, desc: &str) -> bool {
        rows(lines).iter().any(|(k, d)| k == keys && d == desc)
    }

    #[test]
    fn every_action_name_has_a_description() {
        for name in ACTION_NAMES {
            assert!(!describe(name).is_empty(), "{name} has no description");
        }
        for (name, _) in DESCRIPTIONS {
            assert!(ACTION_NAMES.contains(name), "{name} is not an action");
        }
    }

    #[test]
    fn default_keymap_listed() {
        let lines = build(&default_keymap());
        assert!(has(&lines, "?", "show what the selected object can do"));
        assert!(has(&lines, "dd", "delete your message under the cursor"));
        assert!(has(&lines, "<C-d>", "half a page down"));
    }

    #[test]
    fn the_thread_unread_marks_are_explained() {
        let lines = build(&default_keymap());
        assert!(lines.contains(&HelpLine::Heading("Thread unread".to_string())));
        assert!(has(&lines, "new", "thread has unread replies"));
        assert!(has(&lines, "T(2)", "room has 2 unread threads"));
    }

    #[test]
    fn the_thread_list_keys_are_listed() {
        let lines = build(&default_keymap());
        assert!(has(&lines, "q Esc", "close the list"));
        assert!(rows(&lines).iter().any(|(k, d)| {
            d == "previous or next filter: all, following, unread"
                && k.contains('h')
                && k.contains('l')
        }));
    }

    #[test]
    fn the_thread_keys_are_listed_follow_the_keymap_and_close_has_no_default() {
        let lines = build(&default_keymap());
        assert!(has(
            &lines,
            "gt",
            "open the thread of the message under the cursor"
        ));
        assert!(
            !rows(&lines).iter().any(|(_, d)| d == "close the thread"),
            "close_thread has no default key; Esc is `cancel`"
        );
        let config =
            config_from_keymap("[normal]\n\"T\" = \"open_thread\"\n\"Q\" = \"close_thread\"\n");
        let lines = build(&config.keymap);
        assert!(has(
            &lines,
            "T",
            "open the thread of the message under the cursor"
        ));
        assert!(has(
            &lines,
            "gt",
            "open the thread of the message under the cursor"
        ));
        assert!(has(&lines, "Q", "close the thread pane"));
        let config = config_from_keymap("[normal]\n\"gt\" = \"none\"\n");
        assert!(!rows(&build(&config.keymap)).iter().any(|(k, _)| k == "gt"));
    }

    #[test]
    fn rebound_key_listed() {
        let config = config_from_keymap("[normal]\n\"J\" = \"move_down\"\n");
        let lines = build(&config.keymap);
        assert!(has(&lines, "J", "move down"));
    }

    #[test]
    fn removed_key_absent() {
        let config = config_from_keymap("[normal]\n\"<C-d>\" = \"none\"\n");
        let lines = build(&config.keymap);
        assert!(!rows(&lines).iter().any(|(k, _)| k == "<C-d>"));
        assert!(has(&lines, "<C-u>", "half a page up"));
    }

    #[test]
    fn vim_extras_keys_listed() {
        let config = config_from_toml("keymap_preset = \"vim-extras\"\n");
        let lines = build(&config.keymap);
        assert!(has(&lines, "<Tab>", "focus the next pane"));
        assert!(has(&lines, "<S-Tab>", "focus the previous pane"));
    }

    #[test]
    fn search_keys_listed() {
        let lines = build(&default_keymap());
        assert!(has(&lines, "/", "find text in the open room"));
        assert!(has(&lines, "n", "next match, toward older messages"));
        assert!(has(&lines, "N", "previous match, toward newer messages"));
    }

    #[test]
    fn cmd_binding_shown_as_command() {
        let config = config_from_keymap("[normal]\n\"<C-q>\" = \"cmd:quit\"\n");
        let lines = build(&config.keymap);
        assert!(has(&lines, "<C-q>", ":quit"));
    }

    #[test]
    fn commands_listed_with_aliases() {
        let lines = build(&default_keymap());
        let all = rows(&lines);
        assert!(all.iter().any(|(k, _)| k == ":q :quit"));
        assert!(all.iter().any(|(k, _)| k == ":reconnect :retry"));
        assert!(all.iter().any(|(k, _)| k == ":help"));
    }

    #[test]
    fn max_scroll_saturates() {
        assert_eq!(max_scroll(10, 4), 6);
        assert_eq!(max_scroll(3, 4), 0);
    }

    use crate::action::Action;
    use crate::app::update::update;
    use crate::app::{Focus, Mode};
    use crate::test_util::{app_80x24, app_with_config, press};

    #[test]
    fn second_question_mark_opens_without_changing_mode() {
        let mut app = app_80x24();
        press(&mut app, "??");
        assert_eq!(app.help, Some(0));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn help_command_opens_and_returns_to_normal() {
        let mut app = app_80x24();
        press(&mut app, ":help<CR>");
        assert_eq!(app.help, Some(0));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn the_help_lists_switch_room_with_its_key() {
        let lines = build(&app_80x24().keymap);
        let row = lines
            .iter()
            .find(|line| format!("{line:?}").contains("switch to another room"))
            .unwrap_or_else(|| panic!("switch_room is not listed: {lines:?}"));
        assert!(
            format!("{row:?}").contains("<C-k>") || format!("{row:?}").contains("C-k"),
            "{row:?}"
        );
    }

    #[test]
    fn every_close_key_closes() {
        for key in ["q", "<Esc>", "<C-c>", "?"] {
            let mut app = app_80x24();
            let room = app.current_room.clone();
            press(&mut app, "??");
            press(&mut app, key);
            assert_eq!(app.help, None, "{key}");
            assert_eq!(app.current_room, room, "{key}");
            assert!(!app.should_quit, "{key}");
        }
    }

    #[test]
    fn restored_question_mark_opens_directly() {
        let config = config_from_keymap("[normal]\n\"?\" = \"show_help\"\n");
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "?");
        assert_eq!(app.help, Some(0));
        assert!(!app.context);
    }

    #[test]
    fn rebound_open_key_works() {
        let config = config_from_keymap("[normal]\n\"H\" = \"show_help\"\n");
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "H");
        assert_eq!(app.help, Some(0));
    }

    #[test]
    fn motions_scroll_and_clamp() {
        let mut app = app_80x24();
        let sel = crate::app::sidebar::selected_room(&app);
        press(&mut app, "??j");
        assert_eq!(app.help, Some(1));
        press(&mut app, "5j");
        assert_eq!(app.help, Some(6));
        press(&mut app, "k");
        assert_eq!(app.help, Some(5));
        press(&mut app, "G");
        let max = max_scroll(build(&app.keymap).len(), viewport_height(&app));
        assert!(max > 0);
        assert_eq!(app.help, Some(max));
        press(&mut app, "j<C-d>");
        assert_eq!(app.help, Some(max));
        press(&mut app, "gg");
        assert_eq!(app.help, Some(0));
        press(&mut app, "k<C-u>");
        assert_eq!(app.help, Some(0));
        press(&mut app, "<C-d>");
        assert_eq!(app.help, Some(viewport_height(&app) / 2));
        assert_eq!(crate::app::sidebar::selected_room(&app), sel);
    }

    #[test]
    fn other_keys_are_ignored() {
        let mut app = app_80x24();
        app.focus = Focus::Messages;
        let before = app.messages.clone();
        press(&mut app, "??dd");
        press(&mut app, "i");
        press(&mut app, "a:");
        press(&mut app, "<C-w>j");
        assert_eq!(app.help, Some(0));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Messages);
        assert!(app.confirm.is_none());
        assert!(app.messages == before);
        assert!(app.cmdline.is_empty());
    }

    #[test]
    fn q_closes_even_when_normal_keys_are_unbound() {
        let config = config_from_keymap(
            "[normal]\n\"<Esc>\" = \"none\"\n\"<C-c>\" = \"none\"\n\"j\" = \"none\"\n",
        );
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "??");
        press(&mut app, "j");
        assert_eq!(app.help, Some(0));
        press(&mut app, "q");
        assert_eq!(app.help, None);
    }

    #[test]
    fn live_messages_still_applied_underneath() {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let before = app.messages.get(&room).map_or(0, Vec::len);
        press(&mut app, "??");
        let mut live = crate::test_util::sample_message("live-1");
        live.room_id = room.clone();
        live.ts = i64::MAX / 4;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(app.messages.get(&room).map_or(0, Vec::len), before + 1);
        assert_eq!(app.help, Some(0));
    }
}
