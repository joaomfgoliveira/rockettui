//! The pure reducer: the only function that changes `App`.
//!
//! Key bindings never live here; keys are resolved through `app.keymap` (D-07).

use crate::action::Action;
use crate::app::{App, Focus, MAX_CMDLINE, Mode, command, sidebar};
use crate::input::key::normalize;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Size;

pub fn update(app: &mut App, action: Action) {
    match action {
        Action::Key(ev) => {
            app.status_msg = None;
            let chord = normalize(ev);
            if let Some(bound) = app.keymap.lookup(app.mode, &[chord]).cloned() {
                update(app, bound);
            } else if app.mode != Mode::Normal
                && let KeyCode::Char(c) = chord.code
                && !chord
                    .mods
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                // Text-entry fallback: unbound plain characters are typed.
                update(app, Action::InsertChar(c));
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
        Action::MoveDown | Action::MoveUp => {
            if app.focus == Focus::Sidebar {
                sidebar::apply_motion(app, &action, None);
            }
        }
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

#[cfg(test)]
mod tests {
    use crate::app::{App, sidebar};
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
}
