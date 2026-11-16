//! Status bar (mode badge, current room, pending keys and status message) and the command-line row with its cursor.

use crate::app::{App, Mode, StatusMsg};
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

pub fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let badge = Span::styled(format!(" {} ", app.mode.label()), app.theme.status_mode);
    let room = match app.current_room_name() {
        Some(name) => sanitize_line(name).into_owned(),
        None => "-".to_string(),
    };
    let mut spans = vec![
        badge,
        Span::styled(format!(" {room}"), app.theme.status_room),
    ];
    let pending = app.pending_display();
    if !pending.is_empty() {
        spans.push(Span::styled(
            format!(" {}", sanitize_line(&pending)),
            app.theme.status_pending,
        ));
    }
    match &app.status_msg {
        Some(StatusMsg::Info(text)) => spans.push(Span::styled(
            format!(" {}", sanitize_line(text)),
            app.theme.status_info,
        )),
        Some(StatusMsg::Error(text)) => spans.push(Span::styled(
            format!(" {}", sanitize_line(text)),
            app.theme.status_error,
        )),
        None => {}
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Where the hardware cursor goes: right after ":" and the command text, in Command mode only.
pub fn cmdline_cursor(app: &App, area: Rect) -> Option<Position> {
    if app.mode != Mode::Command || area.width == 0 || area.height == 0 {
        return None;
    }
    let text_width = u16::try_from(sanitize_line(&app.cmdline).width()).unwrap_or(u16::MAX);
    let x = area
        .x
        .saturating_add(1)
        .saturating_add(text_width)
        .min(area.right().saturating_sub(1));
    Some(Position::new(x, area.y))
}

pub fn render_cmdline(frame: &mut Frame, area: Rect, app: &App) {
    if app.mode == Mode::Command {
        let line = Line::styled(
            format!(":{}", sanitize_line(&app.cmdline)),
            app.theme.cmdline_text,
        );
        frame.render_widget(Paragraph::new(line), area);
    }
    if let Some(pos) = cmdline_cursor(app, area) {
        frame.set_cursor_position(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{app_with_config, config_from_theme};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    #[test]
    fn themed_error_style() {
        let config = config_from_theme("[status.error]\nfg = 3\n");
        let mut app = app_with_config(config, 80, 24);
        app.status_msg = Some(StatusMsg::Error("boom".to_string()));
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("backend");
        terminal
            .draw(|frame| render_status(frame, frame.area(), &app))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        let at = row.find("boom").expect("error text");
        for x in at..at + 4 {
            let x = u16::try_from(x).expect("x");
            assert_eq!(buffer[(x, 0)].fg, Color::Indexed(3), "cell {x}");
        }

        // Positive control: the default error text is red.
        let mut app = app_with_config(crate::config::Config::default(), 80, 24);
        app.status_msg = Some(StatusMsg::Error("boom".to_string()));
        terminal
            .draw(|frame| render_status(frame, frame.area(), &app))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        let at = u16::try_from(row.find("boom").expect("error text")).expect("x");
        assert_eq!(buffer[(at, 0)].fg, Color::Red);
    }

    #[test]
    fn themed_cmdline_and_room() {
        let config = config_from_theme("[cmdline.text]\nfg = 6\n[status.room]\nfg = 7\n");
        let mut app = app_with_config(config, 80, 24);
        crate::test_util::press(&mut app, "<CR>:");
        crate::test_util::press(&mut app, "qq");
        let buffer = crate::test_util::render_buffer(&app, 80, 24);
        // The command line is the last row: ":qq".
        for x in 0..3 {
            assert_eq!(buffer[(x, 23)].fg, Color::Indexed(6), "cmdline cell {x}");
        }
        // The room name sits after the badge on the status row.
        let row: String = (0..80).map(|x| buffer[(x, 22)].symbol()).collect();
        let at = u16::try_from(row.find("general").expect("room")).expect("x");
        assert_eq!(buffer[(at, 22)].fg, Color::Indexed(7));
    }
}
