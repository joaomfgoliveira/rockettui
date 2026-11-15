//! The view: a pure function of `&App` onto a ratatui `Frame`. It never mutates `App`.

pub mod input;
pub mod layout;
pub mod messages;
pub mod sidebar;
pub mod status;

use crate::app::App;
use ratatui::Frame;
use ratatui::widgets::{Block, BorderType, Borders};

/// Draw the whole screen.
pub fn view(frame: &mut Frame, app: &App) {
    let areas = layout::areas(frame.area(), app.input_rows());
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
    use crate::test_util::{app_80x24, press, render_to_string};
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
