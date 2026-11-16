//! The view: a pure function of `&App` onto a ratatui `Frame`. It never mutates `App`.

pub mod error_screen;
pub mod input;
pub mod layout;
pub mod messages;
pub mod palette;
pub mod sanitize;
pub mod sidebar;
pub mod status;
pub mod timefmt;
pub mod wrap;

use crate::app::App;
use ratatui::Frame;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Narrowest terminal the panes are laid out in.
pub const MIN_WIDTH: u16 = 40;
/// Shortest terminal the panes are laid out in.
pub const MIN_HEIGHT: u16 = 8;

/// Draw the whole screen. Below `MIN_WIDTH` x `MIN_HEIGHT` a one-line notice replaces the panes,
/// so pane renderers only ever see areas of at least that size.
pub fn view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let notice = format!("terminal too small: {}x{}", area.width, area.height);
        frame.render_widget(Paragraph::new(notice), area);
        return;
    }
    let areas = layout::areas(area, app.settings.sidebar_width, app.input_rows());
    sidebar::render(frame, areas.sidebar, app);
    messages::render(frame, areas.messages, app);
    input::render(frame, areas.input, app);
    status::render_status(frame, areas.status, app);
    status::render_cmdline(frame, areas.cmdline, app);
}

/// A bordered pane block; the border is thick when the pane has focus.
pub fn pane_block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(title)
}

#[cfg(test)]
mod tests {
    use crate::test_util::{app_80x24, app_with_config, config_from_toml, press, render_to_string};
    use crate::ui::status;
    use ratatui::layout::Position;

    #[test]
    fn layout_80x24_has_all_panes() {
        let app = app_80x24();
        let out = render_to_string(&app, 80, 24);
        for needle in ["Rooms", "NORMAL", "Input", "general"] {
            assert!(out.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn hostile_text_is_sanitized_in_render() {
        use crate::data::{Message, RoomId};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = app_80x24();
        app.rooms[0].name = "\u{1b}]0;pwn\u{7}evil\u{202e}name".to_string();
        let id = app.rooms[0].id.clone();
        app.current_room = Some(id.clone());
        app.messages.entry(id.clone()).or_default().insert(
            0,
            Message {
                id: "hostile-0".to_string(),
                room_id: RoomId(id.0.clone()),
                author: "\u{202e}mallory".to_string(),
                ts: 0,
                body: "\u{1b}[2Jclear\u{9b}31m\ttab".to_string(),
            },
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend");
        terminal.draw(|f| super::view(f, &app)).expect("draw");
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..24 {
            for x in 0..80 {
                let symbol = buffer[(x, y)].symbol();
                for c in symbol.chars() {
                    assert!(!c.is_control(), "control char {:?} at {x},{y}", c as u32);
                    assert!(
                        !matches!(c as u32,
                            0x61c | 0x200b | 0x200e | 0x200f | 0x202a..=0x202e
                            | 0x2060 | 0x2066..=0x2069 | 0xfeff | 0xe0000..=0xe007f),
                        "spoofing char U+{:04X} at {x},{y}",
                        c as u32
                    );
                }
                text.push_str(symbol);
            }
        }
        assert!(text.contains("evilname"), "room name not visible");
    }

    #[test]
    fn focused_pane_has_thick_border() {
        let mut app = app_80x24();
        let top_left = |app: &crate::app::App| {
            let out = render_to_string(app, 80, 24);
            let row = out.lines().next().unwrap_or_default().to_string();
            (
                row.chars().next().unwrap_or(' '),
                row.chars().nth(28).unwrap_or(' '),
            )
        };
        // The sidebar starts focused: thick sidebar corner, plain messages corner.
        assert_eq!(top_left(&app), ('\u{250f}', '\u{250c}'));
        press(&mut app, "<C-w>l");
        assert_eq!(top_left(&app), ('\u{250c}', '\u{250f}'));
    }

    #[test]
    fn status_shows_mode_room_and_pending() {
        let mut app = app_80x24();
        let status_row = |app: &crate::app::App| {
            render_to_string(app, 80, 24)
                .lines()
                .nth(22)
                .unwrap_or_default()
                .to_string()
        };
        let row = status_row(&app);
        assert!(row.contains("NORMAL"), "{row:?}");
        assert!(row.contains(" - "), "{row:?}");
        assert!(!row.contains("general"), "{row:?}");
        app.open_room(crate::data::RoomId("general".to_string()));
        press(&mut app, "5");
        let row = status_row(&app);
        assert!(row.contains("NORMAL"), "{row:?}");
        assert!(row.contains("general"), "{row:?}");
        assert!(row.contains('5'), "{row:?}");
    }

    #[test]
    fn tiny_sizes() {
        let sizes = [
            (0u16, 0u16),
            (1, 1),
            (10, 3),
            (39, 7),
            (40, 8),
            (80, 24),
            (120, 40),
            (200, 60),
        ];
        for command_mode in [false, true] {
            for (w, h) in sizes {
                let mut app = app_80x24();
                app.current_room = Some(app.rooms[0].id.clone());
                app.size = ratatui::layout::Size::new(w, h);
                if command_mode {
                    press(&mut app, ":q");
                }
                let out = render_to_string(&app, w, h);
                if (w, h) == (39, 7) {
                    assert!(out.contains("too small"), "39x7: {out:?}");
                }
                if (w, h) == (40, 8) {
                    assert!(out.contains(if command_mode { "COMMAND" } else { "NORMAL" }));
                }
            }
        }
    }

    fn draw_80x24(app: &crate::app::App) -> ratatui::Terminal<ratatui::backend::TestBackend> {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("backend");
        terminal.draw(|f| super::view(f, app)).expect("draw");
        terminal
    }

    /// The fake-data app with the clock fixed, so timestamps in the screen are stable.
    fn fixed_clock_app() -> crate::app::App {
        let mut app = app_80x24();
        let at = crate::data::fake::FAKE_EPOCH + 43_200;
        crate::app::update::update(&mut app, crate::action::Action::Clock(at));
        app
    }

    #[test]
    fn layout_80x24_sidebar_40() {
        let mut app = app_with_config(config_from_toml("sidebar_width = 40\n"), 80, 24);
        let at = crate::data::fake::FAKE_EPOCH + 43_200;
        crate::app::update::update(&mut app, crate::action::Action::Clock(at));
        insta::assert_snapshot!(draw_80x24(&app).backend());
    }

    #[test]
    fn layout_80x24_custom_config() {
        let toml =
            "sidebar_width = 34\nmessage_layout = \"two_line\"\ntimestamp_format = \"%a %H:%M\"\n";
        let mut app = app_with_config(config_from_toml(toml), 80, 24);
        let at = crate::data::fake::FAKE_EPOCH + 43_200;
        crate::app::update::update(&mut app, crate::action::Action::Clock(at));
        press(&mut app, "<CR>");
        insta::assert_snapshot!(draw_80x24(&app).backend());
    }

    #[test]
    fn sidebar_wider_than_terminal() {
        let mut app = app_with_config(config_from_toml("sidebar_width = 60\n"), 80, 24);
        app.current_room = Some(app.rooms[0].id.clone());
        for (w, h) in [(40u16, 8u16), (61, 10), (80, 24), (39, 7)] {
            app.size = ratatui::layout::Size::new(w, h);
            let _ = render_to_string(&app, w, h);
        }
        app.size = ratatui::layout::Size::new(80, 24);
        assert_eq!(app.areas().messages.width, 20);
    }

    #[test]
    fn layout_80x24_normal() {
        let app = fixed_clock_app();
        insta::assert_snapshot!(draw_80x24(&app).backend());
    }

    #[test]
    fn layout_80x24_insert_multiline() {
        let mut app = fixed_clock_app();
        press(&mut app, "<CR>ihello<C-j>world");
        assert_eq!(app.input.line_count(), 2);
        insta::assert_snapshot!(draw_80x24(&app).backend());
    }

    #[test]
    fn layout_80x24_command() {
        let mut app = fixed_clock_app();
        press(&mut app, ":qu");
        insta::assert_snapshot!(draw_80x24(&app).backend());
    }

    #[test]
    fn input_grows_to_five_rows() {
        let mut app = fixed_clock_app();
        let one = app.areas();
        assert_eq!(one.input.height, 3);
        let draft: Vec<String> = (0..7).map(|i| format!("line {i}")).collect();
        press(&mut app, "i");
        app.input.insert_str(&draft.join("\n"));
        assert_eq!(app.input_rows(), 5);
        let seven = app.areas();
        assert_eq!(seven.input.height, 7);
        assert_eq!(one.messages.height - seven.messages.height, 4);
        // The border row of the grown input pane is drawn where the layout says.
        let out = render_to_string(&app, 80, 24);
        let top_border = out
            .lines()
            .nth(usize::from(seven.input.y))
            .unwrap_or_default();
        assert!(top_border.contains("Input"), "{top_border:?}");
        // The cursor line (the last draft line) is visible in the scrolled window.
        assert!(out.contains("line 6"));
        for (w, h) in [(40u16, 8u16), (39, 7)] {
            app.size = ratatui::layout::Size::new(w, h);
            let _ = render_to_string(&app, w, h);
        }
    }

    #[test]
    fn command_mode_renders_cmdline() {
        let mut app = app_80x24();
        let cmdline = ratatui::layout::Rect::new(0, 23, 80, 1);
        assert_eq!(status::cmdline_cursor(&app, cmdline), None);
        press(&mut app, ":q");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("COMMAND"));
        let last = out.lines().last().unwrap_or_default();
        assert!(last.starts_with(":q"), "last row: {last:?}");
        assert_eq!(
            status::cmdline_cursor(&app, cmdline),
            Some(Position::new(2, 23))
        );
    }
}
