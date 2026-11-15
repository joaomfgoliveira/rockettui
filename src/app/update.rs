//! The pure reducer: the only function that changes `App`.
//!
//! Key bindings never live here; keys are resolved through `app.keymap` (D-07).

use crate::action::Action;
use crate::app::{App, Focus, MAX_CMDLINE, Mode, command, sidebar};
use crate::input::key::normalize;
use crate::input::matcher::{MAX_COUNT, Outcome};
use ratatui::layout::Size;

pub fn update(app: &mut App, action: Action) {
    match action {
        Action::Key(ev) => {
            app.status_msg = None;
            let chord = normalize(ev);
            let outcome = app.matcher.feed(app.keymap.trie(app.mode), app.mode, chord);
            dispatch(app, outcome);
        }
        Action::KeyTimeout => {
            let outcome = app.matcher.timeout(app.keymap.trie(app.mode));
            dispatch(app, outcome);
        }
        Action::Counted(n, inner) => {
            let n = n.min(MAX_COUNT);
            if inner.is_motion() {
                // Motions apply a count as one clamped jump, not n redraws.
                apply_motion(app, &inner, Some(n));
            } else {
                for _ in 0..n {
                    update(app, (*inner).clone());
                    if app.should_quit {
                        break;
                    }
                }
            }
        }
        Action::RoomsLoaded(rooms) => {
            app.rooms = rooms;
            sidebar::on_rows_changed(app);
        }
        Action::FoldersLoaded(folders) => {
            app.folders = folders;
            sidebar::on_rows_changed(app);
        }
        Action::MessagesLoaded(room_id, messages) => {
            app.messages.insert(room_id, messages);
        }
        Action::Resize(w, h) => {
            app.size = Size::new(w, h);
        }
        Action::Clock(secs) => {
            app.clock_secs = secs;
        }
        Action::MoveDown
        | Action::MoveUp
        | Action::MoveLeft
        | Action::MoveRight
        | Action::GotoTop
        | Action::GotoBottom
        | Action::HalfPageDown
        | Action::HalfPageUp
        | Action::Select
        | Action::ToggleFold => apply_motion(app, &action, None),
        // Absolute focus targets from any pane (A11).
        Action::FocusLeft => app.focus = Focus::Sidebar,
        Action::FocusRight => app.focus = Focus::Messages,
        Action::FocusDown => app.focus = Focus::Input,
        Action::EnterCommand => {
            app.mode = Mode::Command;
            app.cmdline.clear();
        }
        Action::Cancel => match app.mode {
            Mode::Command => {
                app.cmdline.clear();
                app.mode = Mode::Normal;
            }
            Mode::Normal => {
                app.status_msg = None;
            }
            Mode::Insert => {}
        },
        Action::InsertChar(c) => {
            if app.mode == Mode::Command && app.cmdline.chars().count() < MAX_CMDLINE {
                app.cmdline.push(c);
            }
        }
        Action::DeleteBack => {
            if app.mode == Mode::Command && app.cmdline.pop().is_none() {
                app.mode = Mode::Normal;
            }
        }
        Action::ExecuteCommand => {
            if app.mode == Mode::Command {
                let line = std::mem::take(&mut app.cmdline);
                app.mode = Mode::Normal;
                command::execute(app, &line);
            }
        }
        // Paste handling arrives with Insert and Command paste (plan 01-06).
        Action::Paste(_) => {}
        Action::Quit => {
            app.should_quit = true;
        }
    }
}

/// Turn what the matcher produced into reducer work.
fn dispatch(app: &mut App, outcome: Outcome) {
    match outcome {
        Outcome::Fire {
            action,
            count: None,
        } => update(app, action),
        Outcome::Fire {
            action,
            count: Some(n),
        } => update(app, Action::Counted(n, Box::new(action))),
        Outcome::Text(c) => update(app, Action::InsertChar(c)),
        Outcome::Pending | Outcome::Cancelled | Outcome::NoMatch => {}
    }
}

/// The one router every Normal-mode motion goes through: it dispatches on the focused pane.
fn apply_motion(app: &mut App, motion: &Action, count: Option<u32>) {
    match app.focus {
        Focus::Sidebar => sidebar::apply_motion(app, motion, count),
        Focus::Messages => crate::app::messages::apply_motion(app, motion, count),
        // Routed to the input-buffer motions of D-18 in plan 01-06.
        Focus::Input => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::app::{App, Focus, MAX_CMDLINE, sidebar};
    use crate::app::{Mode, update::update};
    use crate::data::{RoomId, fake};
    use crate::test_util::{app_80x24, press};
    use ratatui::layout::Size;

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    #[test]
    fn tracer_seed_keys_quit() {
        let mut app = App::new(Size::new(80, 24));
        assert!(app.rooms.is_empty());
        assert!(sidebar::selected_room(&app).is_none());
        for action in fake::seed() {
            update(&mut app, action);
        }
        assert_eq!(sidebar::selected_room(&app), Some(general()));
        press(&mut app, "j");
        assert_ne!(sidebar::selected_room(&app), Some(general()));
        press(&mut app, "k");
        assert_eq!(sidebar::selected_room(&app), Some(general()));
        press(&mut app, ":");
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(app.mode.label(), "COMMAND");
        press(&mut app, "q");
        assert_eq!(app.cmdline, "q");
        press(&mut app, "<CR>");
        assert!(app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn motion_j_k_sidebar() {
        let mut app = app_80x24();
        let first = sidebar::selected_room(&app);
        assert_eq!(first, Some(general()));
        press(&mut app, "j");
        let second = sidebar::selected_room(&app);
        assert_ne!(second, first);
        press(&mut app, "k");
        assert_eq!(sidebar::selected_room(&app), first);
        press(&mut app, "k");
        assert_eq!(sidebar::selected_room(&app), first);
        for _ in 0..50 {
            press(&mut app, "j");
        }
        let last = sidebar::selected_room(&app);
        press(&mut app, "j");
        assert_eq!(sidebar::selected_room(&app), last);
        assert_ne!(last, first);
    }

    #[test]
    fn mode_colon_enters_command() {
        let mut app = app_80x24();
        assert_eq!(app.mode.label(), "NORMAL");
        press(&mut app, ":");
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(app.mode.label(), "COMMAND");
        assert!(app.cmdline.is_empty());
    }

    #[test]
    fn mode_esc_and_ctrl_c_cancel_command() {
        for cancel in ["<Esc>", "<C-c>"] {
            let mut app = app_80x24();
            press(&mut app, ":ab");
            assert_eq!(app.cmdline, "ab");
            press(&mut app, cancel);
            assert_eq!(app.mode, Mode::Normal, "{cancel}");
            assert!(app.cmdline.is_empty(), "{cancel}");
            assert!(!app.should_quit, "{cancel}");
        }
    }

    #[test]
    fn mode_ctrl_c_and_esc_in_normal_do_not_quit() {
        let mut app = app_80x24();
        press(&mut app, "<C-c><C-c><Esc><Esc>");
        assert!(!app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn command_bs_on_empty_returns_normal() {
        let mut app = app_80x24();
        press(&mut app, ":a<BS>");
        assert_eq!(app.mode, Mode::Command);
        assert!(app.cmdline.is_empty());
        press(&mut app, "<BS>");
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn command_line_capped_at_max() {
        let mut app = app_80x24();
        press(&mut app, ":");
        press(&mut app, &"a".repeat(300));
        assert_eq!(app.cmdline.chars().count(), MAX_CMDLINE);
        assert_eq!(app.mode, Mode::Command);
    }

    #[test]
    fn motion_count_5j_equals_five_j() {
        let mut counted = app_80x24();
        let initial = sidebar::selected_room(&counted);
        press(&mut counted, "5j");
        let mut repeated = app_80x24();
        press(&mut repeated, "jjjjj");
        assert_ne!(sidebar::selected_room(&counted), initial);
        assert_eq!(
            sidebar::selected_room(&counted),
            sidebar::selected_room(&repeated)
        );
        assert!(!counted.keys_pending());
    }

    #[test]
    fn motion_huge_count_clamps() {
        let mut huge = app_80x24();
        press(&mut huge, "99999999999j");
        let mut twenty = app_80x24();
        for _ in 0..20 {
            press(&mut twenty, "j");
        }
        assert_eq!(
            sidebar::selected_room(&huge),
            sidebar::selected_room(&twenty)
        );
        assert!(!huge.keys_pending());
    }

    #[test]
    fn pending_count_tracked() {
        let mut app = app_80x24();
        assert!(!app.keys_pending());
        press(&mut app, "5");
        assert!(app.keys_pending());
        assert_eq!(app.pending_display(), "5");
        press(&mut app, "<Esc>");
        assert!(!app.keys_pending());
        assert_eq!(app.pending_display(), "");
        assert!(!app.should_quit);
    }

    #[test]
    fn pending_timeout_clears() {
        let mut app = app_80x24();
        let before = sidebar::selected_room(&app);
        press(&mut app, "5");
        assert!(app.keys_pending());
        update(&mut app, Action::KeyTimeout);
        assert!(!app.keys_pending());
        assert_eq!(sidebar::selected_room(&app), before);
        // A timeout with nothing pending is harmless.
        update(&mut app, Action::KeyTimeout);
        assert!(!app.should_quit);
    }

    #[test]
    fn counted_non_motion_repeats() {
        let mut app = app_80x24();
        update(&mut app, Action::Counted(3, Box::new(Action::EnterCommand)));
        assert_eq!(app.mode, Mode::Command);
    }

    #[test]
    fn focus_ctrl_w_moves_focus() {
        let mut app = app_80x24();
        assert_eq!(app.focus, Focus::Sidebar);
        press(&mut app, "<C-w>");
        assert!(app.keys_pending());
        assert_eq!(app.pending_display(), "<C-w>");
        press(&mut app, "l");
        assert_eq!(app.focus, Focus::Messages);
        assert!(!app.keys_pending());
        press(&mut app, "<C-w>j");
        assert_eq!(app.focus, Focus::Input);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Sidebar);
        // Absolute targets from any pane.
        press(&mut app, "<C-w>l<C-w>l");
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn focus_ctrl_w_wrong_key_drops() {
        let mut app = app_80x24();
        press(&mut app, "<C-w>x");
        assert_eq!(app.focus, Focus::Sidebar);
        assert!(!app.keys_pending());
        press(&mut app, "<C-w>");
        update(&mut app, Action::KeyTimeout);
        assert_eq!(app.focus, Focus::Sidebar);
        assert!(!app.keys_pending());
        // The dropped key is not replayed as a fresh key: no mode change, no quit.
        press(&mut app, "<C-w>:");
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn focus_open_room_focuses_messages() {
        let mut app = app_80x24();
        assert_eq!(app.current_room, None);
        app.open_room(general());
        assert_eq!(app.current_room, Some(general()));
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn resize_updates_size() {
        let mut app = app_80x24();
        press(&mut app, ":ab");
        let rooms = app.rooms.len();
        let clock = app.clock_secs;
        update(&mut app, Action::Resize(120, 40));
        assert_eq!(app.size, Size::new(120, 40));
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(app.cmdline, "ab");
        assert_eq!(app.rooms.len(), rooms);
        assert_eq!(app.clock_secs, clock);
        assert!(!app.should_quit);
    }

    #[test]
    fn clock_sets_clock_secs() {
        let mut app = app_80x24();
        update(&mut app, Action::Clock(42));
        assert_eq!(app.clock_secs, 42);
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, ":ab");
        update(&mut app, Action::Clock(43));
        assert_eq!(app.clock_secs, 43);
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(app.cmdline, "ab");
    }

    /// Draft "alpha\nbeta\ngamma" with the input bar focused in Normal mode, cursor at the end.
    fn input_app(draft: &str) -> App {
        let mut app = app_80x24();
        press(&mut app, "<C-w>j");
        app.input.insert_str(draft);
        app
    }

    fn pos(app: &App) -> (usize, usize) {
        (app.input.row(), app.input.col())
    }

    #[test]
    fn input_normal_h_l_within_line() {
        let mut app = input_app("alpha\nbeta\ngamma");
        press(&mut app, "h");
        assert_eq!(pos(&app), (2, 4));
        press(&mut app, "3h");
        assert_eq!(pos(&app), (2, 1));
        press(&mut app, "10h");
        assert_eq!(pos(&app), (2, 0));
        press(&mut app, "l");
        assert_eq!(pos(&app), (2, 1));
        press(&mut app, "99l");
        assert_eq!(pos(&app), (2, 5));
    }

    #[test]
    fn input_normal_j_k_lines() {
        let mut app = input_app("alpha\nbeta\ngamma");
        press(&mut app, "k");
        assert_eq!(pos(&app), (1, 4));
        press(&mut app, "5k");
        assert_eq!(app.input.row(), 0);
        press(&mut app, "j");
        assert_eq!(app.input.row(), 1);
        press(&mut app, "9j");
        assert_eq!(app.input.row(), 2);
        let text = app.input.text();
        let line = text.lines().nth(2).unwrap_or_default();
        assert!(line.is_char_boundary(app.input.col()));
    }

    #[test]
    fn input_normal_gg_g() {
        let mut app = input_app("alpha\nbeta\ngamma");
        press(&mut app, "gg");
        assert_eq!(pos(&app), (0, 0));
        press(&mut app, "G");
        assert_eq!(pos(&app), (2, 0));
        press(&mut app, "2gg");
        assert_eq!(pos(&app), (1, 0));
        press(&mut app, "G");
        press(&mut app, "2G");
        assert_eq!(pos(&app), (1, 0));
    }

    #[test]
    fn input_normal_half_page() {
        let mut short = input_app("a\nb\nc");
        let before = pos(&short);
        press(&mut short, "<C-d>");
        assert_eq!(pos(&short), before);
        press(&mut short, "<C-u>");
        assert_eq!(pos(&short), before);

        let draft: Vec<String> = (0..12).map(|i| format!("l{i}")).collect();
        let mut tall = input_app(&draft.join("\n"));
        press(&mut tall, "gg");
        press(&mut tall, "<C-d>");
        assert_eq!(tall.input.row(), 2);
        press(&mut tall, "<C-u>");
        assert_eq!(tall.input.row(), 0);
        press(&mut tall, "3<C-d>");
        assert_eq!(tall.input.row(), 6);
    }

    #[test]
    fn input_normal_motions_keep_state() {
        let mut app = input_app("alpha\nbeta\ngamma");
        let text = app.input.text();
        let room = app.current_room.clone();
        let selected = sidebar::selected_room(&app);
        for keys in [
            "h", "l", "j", "k", "gg", "G", "3h", "2gg", "<C-d>", "<C-u>", "5j", "9k",
        ] {
            press(&mut app, keys);
            assert_eq!(app.input.text(), text, "{keys}");
            assert_eq!(app.mode, Mode::Normal, "{keys}");
            assert_eq!(app.focus, Focus::Input, "{keys}");
            assert_eq!(app.current_room, room, "{keys}");
            assert_eq!(sidebar::selected_room(&app), selected, "{keys}");
            assert!(!app.keys_pending(), "{keys}");
        }
    }
}
