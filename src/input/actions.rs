//! The curated names a `keymap.toml` value may use (D-07).
//!
//! Only the unit variants of [`Action`] that make sense as a key press have a name. Typed text,
//! raw keys, resize, clock, paste, loaded data, the key timeout and counted actions are produced by
//! the program, never by a keymap row, so they have none (T-02-11). The names are the user-facing
//! vocabulary: renaming one breaks a user's `keymap.toml`.

use crate::action::Action;

/// Every bindable action name, snake_case. `none` and `cmd:<command>` are handled by the loader.
pub const ACTION_NAMES: &[&str] = &[
    "move_down",
    "move_up",
    "move_left",
    "move_right",
    "goto_top",
    "goto_bottom",
    "half_page_down",
    "half_page_up",
    "select",
    "toggle_fold",
    "focus_left",
    "focus_down",
    "focus_right",
    "focus_next",
    "focus_prev",
    "enter_command",
    "enter_insert",
    "cancel",
    "delete_back",
    "insert_newline",
    "delete_word_back",
    "delete_to_line_start",
    "cursor_left",
    "cursor_right",
    "cursor_up",
    "cursor_down",
    "cursor_home",
    "cursor_end",
    "submit_input",
    "execute_command",
    "quit",
];

/// The action a name stands for. `none`, `cmd:<command>` and every unknown name give `None`.
pub fn action_from_name(name: &str) -> Option<Action> {
    Some(match name {
        "move_down" => Action::MoveDown,
        "move_up" => Action::MoveUp,
        "move_left" => Action::MoveLeft,
        "move_right" => Action::MoveRight,
        "goto_top" => Action::GotoTop,
        "goto_bottom" => Action::GotoBottom,
        "half_page_down" => Action::HalfPageDown,
        "half_page_up" => Action::HalfPageUp,
        "select" => Action::Select,
        "toggle_fold" => Action::ToggleFold,
        "focus_left" => Action::FocusLeft,
        "focus_down" => Action::FocusDown,
        "focus_right" => Action::FocusRight,
        "focus_next" => Action::FocusNext,
        "focus_prev" => Action::FocusPrev,
        "enter_command" => Action::EnterCommand,
        "enter_insert" => Action::EnterInsert,
        "cancel" => Action::Cancel,
        "delete_back" => Action::DeleteBack,
        "insert_newline" => Action::InsertNewline,
        "delete_word_back" => Action::DeleteWordBack,
        "delete_to_line_start" => Action::DeleteToLineStart,
        "cursor_left" => Action::CursorLeft,
        "cursor_right" => Action::CursorRight,
        "cursor_up" => Action::CursorUp,
        "cursor_down" => Action::CursorDown,
        "cursor_home" => Action::CursorHome,
        "cursor_end" => Action::CursorEnd,
        "submit_input" => Action::SubmitInput,
        "execute_command" => Action::ExecuteCommand,
        "quit" => Action::Quit,
        _ => return None,
    })
}

/// The name of a bindable action. The match is exhaustive on purpose (no wildcard arm): every
/// `Action` variant added later must choose whether it is bindable, as in the `Debug` impl.
pub fn name_of(action: &Action) -> Option<&'static str> {
    match action {
        Action::MoveDown => Some("move_down"),
        Action::MoveUp => Some("move_up"),
        Action::MoveLeft => Some("move_left"),
        Action::MoveRight => Some("move_right"),
        Action::GotoTop => Some("goto_top"),
        Action::GotoBottom => Some("goto_bottom"),
        Action::HalfPageDown => Some("half_page_down"),
        Action::HalfPageUp => Some("half_page_up"),
        Action::Select => Some("select"),
        Action::ToggleFold => Some("toggle_fold"),
        Action::FocusLeft => Some("focus_left"),
        Action::FocusDown => Some("focus_down"),
        Action::FocusRight => Some("focus_right"),
        Action::FocusNext => Some("focus_next"),
        Action::FocusPrev => Some("focus_prev"),
        Action::EnterCommand => Some("enter_command"),
        Action::EnterInsert => Some("enter_insert"),
        Action::Cancel => Some("cancel"),
        Action::DeleteBack => Some("delete_back"),
        Action::InsertNewline => Some("insert_newline"),
        Action::DeleteWordBack => Some("delete_word_back"),
        Action::DeleteToLineStart => Some("delete_to_line_start"),
        Action::CursorLeft => Some("cursor_left"),
        Action::CursorRight => Some("cursor_right"),
        Action::CursorUp => Some("cursor_up"),
        Action::CursorDown => Some("cursor_down"),
        Action::CursorHome => Some("cursor_home"),
        Action::CursorEnd => Some("cursor_end"),
        Action::SubmitInput => Some("submit_input"),
        Action::ExecuteCommand => Some("execute_command"),
        Action::Quit => Some("quit"),
        // `cmd:<command>` is a form of the value, not a name of its own.
        Action::RunCommand(_)
        | Action::InsertChar(_)
        | Action::Key(_)
        | Action::Resize(..)
        | Action::Clock(_)
        | Action::Paste(_)
        | Action::RoomsLoaded(_)
        | Action::FoldersLoaded(_)
        | Action::MessagesLoaded(..)
        | Action::LoadFailed(_)
        | Action::HistoryLoaded { .. }
        | Action::HistoryFailed { .. }
        | Action::KeyTimeout
        | Action::Counted(..) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::HashSet;

    #[test]
    fn names_round_trip() {
        let unique: HashSet<&&str> = ACTION_NAMES.iter().collect();
        assert_eq!(unique.len(), ACTION_NAMES.len(), "duplicate name");
        for name in ACTION_NAMES {
            let action = action_from_name(name).unwrap_or_else(|| panic!("{name} has no action"));
            assert_eq!(name_of(&action), Some(*name));
        }
    }

    #[test]
    fn internal_actions_unnamed() {
        let key = Action::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        for action in [
            Action::InsertChar('x'),
            key,
            Action::Resize(1, 1),
            Action::Clock(0),
            Action::Paste("p".to_string()),
            Action::RunCommand("quit".to_string()),
            Action::KeyTimeout,
            Action::Counted(2, Box::new(Action::MoveDown)),
        ] {
            assert_eq!(name_of(&action), None, "{action:?}");
        }
        assert!(action_from_name("none").is_none());
        assert!(action_from_name("insert_char").is_none());
        assert!(action_from_name("").is_none());
        assert!(action_from_name("MOVE_DOWN").is_none());
    }
}
