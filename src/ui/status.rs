//! Status bar (mode badge, current room, pending keys and status message) and the command-line row with its cursor.

use crate::app::{App, Mode, StatusMsg};
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

pub fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let badge = Span::styled(format!(" {} ", app.mode.label()), app.theme.status_mode);
    let room = match app.current_room_name() {
        Some(name) => sanitize_line(name).into_owned(),
        None => "-".to_string(),
    };
    let mut spans = vec![badge, Span::raw(format!(" {room}"))];
    let pending = app.pending_display();
    if !pending.is_empty() {
        spans.push(Span::raw(format!(" {}", sanitize_line(&pending))));
    }
    match &app.status_msg {
        Some(StatusMsg::Info(text)) => {
            spans.push(Span::raw(format!(" {}", sanitize_line(text))));
        }
        Some(StatusMsg::Error(text)) => spans.push(Span::styled(
            format!(" {}", sanitize_line(text)),
            Style::default().fg(Color::Red),
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
        let line = format!(":{}", sanitize_line(&app.cmdline));
        frame.render_widget(Paragraph::new(line), area);
    }
    if let Some(pos) = cmdline_cursor(app, area) {
        frame.set_cursor_position(pos);
    }
}
