//! Messages pane: the open room's messages, one line each (plan 01-05 adds wrapping, a cursor
//! and scrolling).

use crate::app::{App, Focus};
use crate::ui::pane_block;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// `HH:MM` in UTC for a Unix timestamp in seconds (A10: until a time crate arrives in Phase 2).
pub fn hhmm(ts: i64) -> String {
    let secs = ts.rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let title = match app.current_room_name() {
        Some(name) => format!(" {name} "),
        None => " Messages ".to_string(),
    };
    let block = pane_block(&title, app.focus == Focus::Messages);
    let lines: Vec<Line> = match app.current_room.as_ref() {
        None => vec![Line::from("No room open")],
        Some(id) => app
            .messages
            .get(id)
            .map(|msgs| {
                msgs.iter()
                    .map(|m| Line::from(format!("{} {}  {}", hhmm(m.ts), m.author, m.body)))
                    .collect()
            })
            .unwrap_or_default(),
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hhmm_formats_utc() {
        assert_eq!(hhmm(0), "00:00");
        assert_eq!(hhmm(3600 * 13 + 60 * 5), "13:05");
        assert_eq!(hhmm(-60), "23:59");
    }
}
