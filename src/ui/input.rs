//! Input pane: an empty box for now (plan 01-06 adds the multiline buffer).

use crate::app::{App, Focus};
use crate::ui::pane_block;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};

/// Where the hardware cursor goes (stub).
pub fn cursor_position(_app: &App, _area: Rect) -> Option<Position> {
    None
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(pane_block(" Input ", app.focus == Focus::Input), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::test_util::{app_80x24, press};
    use crate::ui::{self, layout};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn input_area(app: &App) -> Rect {
        layout::areas(
            Rect::new(0, 0, app.size.width, app.size.height),
            app.input_rows(),
        )
        .input
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
