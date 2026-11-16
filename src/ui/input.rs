//! Input pane: the multiline draft with its view window, an empty-state hint and the hardware
//! cursor rules (D-14, D-18).

use crate::app::{App, Focus, Mode};
use crate::ui::pane_block;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// Where the hardware cursor goes: on the draft in Insert mode and in Normal mode while the
/// input bar is focused (so the D-18 motions are visible), nowhere otherwise. Command mode keeps
/// its cursor on the command line (`status::cmdline_cursor`).
pub fn cursor_position(app: &App, area: Rect) -> Option<Position> {
    let editing =
        app.mode == Mode::Insert || (app.mode == Mode::Normal && app.focus == Focus::Input);
    let inner = pane_block("", false, &app.theme).inner(area);
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
    let block = pane_block(" Input ", app.focus == Focus::Input, &app.theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if app.input.is_empty() && app.mode != Mode::Insert {
        let hint = Line::styled("press i to write", app.theme.input_hint);
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
            .map(|row| Line::styled(sanitize_line(row).into_owned(), app.theme.input_text))
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
    use crate::test_util::{
        app_80x24, app_with_config, config_from_theme, press, render_buffer, render_to_string,
    };
    use crate::ui;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

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

    #[test]
    fn themed_hint() {
        let config = config_from_theme("[input.hint]\nfg = 5\n");
        let app = app_with_config(config, 80, 24);
        let screen = render_to_string(&app, 80, 24);
        let (y, row) = screen
            .lines()
            .enumerate()
            .find(|(_, row)| row.contains("press i to write"))
            .expect("hint row");
        let x = row[..row.find("press i to write").expect("hint")]
            .chars()
            .count();
        let buffer = render_buffer(&app, 80, 24);
        for dx in 0.."press i to write".len() {
            let cell = &buffer[(
                u16::try_from(x + dx).expect("x"),
                u16::try_from(y).expect("y"),
            )];
            assert_eq!(cell.fg, Color::Indexed(5), "cell {dx}");
        }
        // Positive control: the default hint is dim with no color.
        let app = app_80x24();
        let buffer = render_buffer(&app, 80, 24);
        let cell = &buffer[(u16::try_from(x).expect("x"), u16::try_from(y).expect("y"))];
        assert_eq!(cell.fg, Color::Reset);
    }

    #[test]
    fn themed_draft_text() {
        let config = config_from_theme("[input.text]\nfg = 2\n");
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "<C-w>jiab");
        let buffer = render_buffer(&app, 80, 24);
        let area = input_area(&app);
        for x in area.x + 1..area.x + 3 {
            assert_eq!(buffer[(x, area.y + 1)].fg, Color::Indexed(2), "cell {x}");
        }
    }
}
