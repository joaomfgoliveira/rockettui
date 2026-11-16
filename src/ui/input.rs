//! Input pane: the multiline draft with its view window, an empty-state hint and the hardware
//! cursor rules (D-14, D-18).

use crate::app::{App, Focus, Mode};
use crate::ui::pane_block;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// Where the hardware cursor goes: on the draft in Insert mode and in Normal mode while the
/// input bar is focused (so the D-18 motions are visible), nowhere otherwise. Command mode keeps
/// its cursor on the command line (`status::cmdline_cursor`).
pub fn cursor_position(app: &App, area: Rect) -> Option<Position> {
    let editing =
        app.mode == Mode::Insert || (app.mode == Mode::Normal && app.focus == Focus::Input);
    let inner = pane_block("", false).inner(area);
    if !editing || inner.width == 0 || inner.height == 0 {
        return None;
    }
    let (x, y) = app.input.cursor_visual(inner.width);
    let top = u16::try_from(app.input.view_top(inner.width, inner.height)).unwrap_or(u16::MAX);
    Some(Position::new(
        inner.x + x.min(inner.width - 1),
        inner.y + y.saturating_sub(top).min(inner.height - 1),
    ))
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = pane_block(" Input ", app.focus == Focus::Input);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if app.input.is_empty() && app.mode != Mode::Insert {
        let hint = Line::styled("press i to write", Style::new().add_modifier(Modifier::DIM));
        frame.render_widget(Paragraph::new(hint), inner);
    } else {
        let top = app.input.view_top(inner.width, inner.height);
        // The buffer is already sanitized; drawing through sanitize_line keeps the every-renderer
        // rule of T-01-01 and leaves the cursor math untouched.
        let rows: Vec<Line> = app
            .input
            .visual_rows(inner.width)
            .iter()
            .skip(top)
            .take(usize::from(inner.height))
            .map(|row| Line::raw(sanitize_line(row).into_owned()))
            .collect();
        frame.render_widget(Paragraph::new(rows), inner);
    }
    if let Some(pos) = cursor_position(app, area) {
        frame.set_cursor_position(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::test_util::{app_80x24, press};
    use crate::ui;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn input_area(app: &App) -> Rect {
        app.areas().input
    }

    #[test]
    fn hardware_cursor_rules() {
        let mut app = app_80x24();
        // Normal mode, sidebar or messages focus: no cursor.
        assert_eq!(cursor_position(&app, input_area(&app)), None);
        press(&mut app, "<C-w>l");
        assert_eq!(cursor_position(&app, input_area(&app)), None);
        // Normal mode with the input bar focused: the D-18 cursor.
        press(&mut app, "<C-w>j");
        let area = input_area(&app);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1, area.y + 1))
        );
        // Insert mode: the cursor follows the draft.
        press(&mut app, "iab");
        let area = input_area(&app);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1 + 2, area.y + 1))
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend");
        terminal.draw(|f| ui::view(f, &app)).expect("draw");
        let shown = terminal.get_cursor_position().expect("cursor");
        assert_eq!(Some(shown), cursor_position(&app, area));
        // A multi-line draft moves the cursor down inside the growing bar.
        press(&mut app, "<C-j>x");
        let area = input_area(&app);
        assert_eq!(app.input_rows(), 2);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1 + 1, area.y + 1 + 1))
        );
        // Command mode: the cursor lives on the command line instead.
        press(&mut app, "<Esc>:");
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(cursor_position(&app, input_area(&app)), None);
    }
}
