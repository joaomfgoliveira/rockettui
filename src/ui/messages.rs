//! Messages pane: the open room's wrapped messages with a cursor highlight. It draws only the
//! window `app::messages::view` selects, from the same row model the reducer scrolls with.

use crate::app::{App, Focus, messages};
use crate::ui::sanitize::sanitize_line;
use crate::ui::{pane_block, wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// `HH:MM` in UTC for a Unix timestamp in seconds (A10: until a time crate arrives in Phase 2).
pub fn hhmm(ts: i64) -> String {
    let secs = ts.rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// Draw the messages pane: only the rows of the window `app::messages::view` selects.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Messages;
    let title = match app.current_room_name() {
        Some(name) => format!(" {} ", sanitize_line(name)),
        None => " Messages ".to_string(),
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let dim = Style::new().add_modifier(Modifier::DIM);
    let Some(room) = app.current_room.as_ref() else {
        frame.render_widget(Paragraph::new(Line::styled("No room open", dim)), inner);
        return;
    };
    let msgs = app.messages.get(room).map_or(&[][..], Vec::as_slice);
    if msgs.is_empty() {
        frame.render_widget(Paragraph::new(Line::styled("No messages yet", dim)), inner);
        return;
    }

    let v = messages::view(app, room);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor_rows = 0..0;
    let author_col = wrap::author_column(msgs);
    for (i, msg) in msgs.iter().enumerate() {
        let rows = wrap::render_rows(msg, inner.width, app.message_layout, author_col);
        if i == v.cursor {
            cursor_rows = lines.len()..lines.len() + rows.len();
        }
        lines.extend(rows);
    }
    // Neither highlight sets a foreground, so author colors survive on the cursor rows (REVERSED
    // swaps them into the background).
    let highlight = if focused {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new().bg(Color::DarkGray)
    };
    // The rows are pre-wrapped, so the Paragraph does no wrapping of its own.
    let window: Vec<Line> = lines
        .into_iter()
        .skip(v.top_row)
        .take(usize::from(inner.height))
        .collect();
    frame.render_widget(Paragraph::new(window), inner);
    // Highlight the whole width of every visible row of the cursor message, trailing blanks
    // included.
    let first = cursor_rows.start.max(v.top_row);
    let end = cursor_rows.end.min(v.top_row + usize::from(inner.height));
    for row in first..end {
        let y = inner.y + u16::try_from(row - v.top_row).unwrap_or(u16::MAX);
        frame
            .buffer_mut()
            .set_style(Rect::new(inner.x, y, inner.width, 1), highlight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::messages::{row_starts, view};
    use crate::data::{Message, RoomId};
    use crate::test_util::{app_80x24, press};
    use crate::ui::layout;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Rect, Size};
    use ratatui::style::Modifier;

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    /// The app's own messages pane rect: `view()` derives its viewport from `app.size`, so every
    /// render test draws on a backend of exactly this size.
    fn pane_rect(app: &App) -> Rect {
        let area = Rect::new(0, 0, app.size.width, app.size.height);
        layout::areas(area, app.input_rows()).messages
    }

    fn render_pane(app: &App) -> Terminal<TestBackend> {
        let pane = pane_rect(app);
        let mut terminal =
            Terminal::new(TestBackend::new(pane.width, pane.height)).expect("test backend");
        terminal
            .draw(|f| render(f, f.area(), app))
            .expect("draw pane");
        terminal
    }

    fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    fn all_text(terminal: &Terminal<TestBackend>) -> String {
        let height = terminal.backend().buffer().area.height;
        (0..height)
            .map(|y| row_text(terminal, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn open_general_80x25() -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.open_room(general());
        let pane = pane_rect(&app);
        assert_eq!((pane.width, pane.height), (52, 20), "snapshot geometry");
        app
    }

    #[test]
    fn hhmm_formats_utc() {
        assert_eq!(hhmm(0), "00:00");
        assert_eq!(hhmm(3600 * 13 + 60 * 5), "13:05");
        assert_eq!(hhmm(-60), "23:59");
    }

    #[test]
    fn messages_general_newest_52x20() {
        let app = open_general_80x25();
        let terminal = render_pane(&app);
        // Inner rows are y 1..=18: the newest default window ends on the last inner row.
        let last_inner: String = row_text(&terminal, 18).chars().skip(1).take(50).collect();
        assert!(
            !last_inner.trim().is_empty(),
            "last inner row is blank: {last_inner:?}"
        );
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn messages_long_message_52x20() {
        let mut app = open_general_80x25();
        press(&mut app, "31gg");
        assert_eq!(view(&app, &general()).cursor, 30);
        let terminal = render_pane(&app);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn messages_empty_room() {
        let mut app = app_80x24();
        app.open_room(RoomId("quiet".to_string()));
        let terminal = render_pane(&app);
        assert!(all_text(&terminal).contains("No messages yet"));
    }

    #[test]
    fn messages_no_room() {
        let app = app_80x24();
        assert_eq!(app.current_room, None);
        let terminal = render_pane(&app);
        assert!(all_text(&terminal).contains("No room open"));
    }

    #[test]
    fn cursor_rows_reversed_when_focused() {
        let mut app = app_80x24();
        app.open_room(general());
        press(&mut app, "gg");
        press(&mut app, "3j");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 3);
        let starts = row_starts(&app, &general());
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let width = buffer.area.width;
        // The second cell of a wide character is painted through its first cell, so skip it.
        let hidden = |x: u16, y: u16| {
            x > 0 && unicode_width::UnicodeWidthStr::width(buffer[(x - 1, y)].symbol()) == 2
        };
        let reversed = |y: u16| {
            (1..width - 1)
                .filter(|&x| !hidden(x, y))
                .all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
        };
        let plain =
            |y: u16| (1..width - 1).all(|x| !buffer[(x, y)].modifier.contains(Modifier::REVERSED));
        let first = 1 + (starts[3] - v.top_row) as u16;
        assert!(reversed(first), "cursor message first row");
        let last = 1 + (starts[4] - 1 - v.top_row) as u16;
        assert!(reversed(last), "cursor message last row");
        assert!(plain(first - 1), "row of the previous message");
        assert!(plain(last + 1), "row of the next message");
    }

    #[test]
    fn authors_colored_in_buffer() {
        use crate::ui::palette::author_color;
        let app = open_general_80x25();
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let mut colors = std::collections::HashSet::new();
        for y in 1..=18u16 {
            if buffer[(3, y)].symbol() == ":" {
                let name: String = (7..=11u16).map(|x| buffer[(x, y)].symbol()).collect();
                let name = name.trim();
                let cell = &buffer[(7, y)];
                assert_eq!(cell.fg, author_color(name), "row {y} {name:?}");
                assert!(cell.modifier.contains(Modifier::BOLD), "row {y}");
                for x in [12u16, 13, 14] {
                    assert_eq!(buffer[(x, y)].fg, Color::Reset, "row {y} x {x}");
                }
                colors.insert(format!("{:?}", cell.fg));
            } else {
                for x in [1u16, 14] {
                    assert_eq!(buffer[(x, y)].fg, Color::Reset, "row {y} x {x}");
                }
            }
        }
        assert_eq!(colors.len(), 4, "{colors:?}");
    }

    #[test]
    fn cursor_row_not_reversed_when_unfocused() {
        let mut app = app_80x24();
        app.open_room(general());
        press(&mut app, "gg");
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        assert!(!buffer[(2, 1)].modifier.contains(Modifier::REVERSED));
        assert_eq!(buffer[(2, 1)].bg, ratatui::style::Color::DarkGray);
    }

    #[test]
    fn window_matches_view() {
        let mut app = app_80x24();
        app.open_room(general());
        for keys in ["<C-d><C-u>", "gg", "G", "20gg", "<C-d>"] {
            press(&mut app, keys);
            let pane = pane_rect(&app);
            assert_eq!((pane.width, pane.height), (52, 19));
            let v = view(&app, &general());
            let author_col = wrap::author_column(&app.messages[&general()]);
            let expected: Vec<String> = app.messages[&general()]
                .iter()
                .flat_map(|m| wrap::render_rows(m, 50, app.message_layout, author_col))
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                })
                .collect();
            let terminal = render_pane(&app);
            let inner = |y: u16| {
                // Inner columns only; the second cell of a wide character carries no text.
                let buffer = terminal.backend().buffer();
                let mut text = String::new();
                let mut skip = false;
                for x in 1..=50 {
                    let symbol = buffer[(x, y)].symbol();
                    if !skip {
                        text.push_str(symbol);
                    }
                    skip = unicode_width::UnicodeWidthStr::width(symbol) == 2 && !skip;
                }
                text.trim_end().to_string()
            };
            assert_eq!(inner(1), expected[v.top_row].trim_end(), "{keys:?} first");
            assert_eq!(
                inner(17),
                expected[v.top_row + 16].trim_end(),
                "{keys:?} last"
            );
        }
    }

    #[test]
    fn bodies_share_one_column() {
        let app = open_general_80x25();
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        for y in 1..=18u16 {
            assert_eq!(buffer[(12, y)].symbol(), " ", "row {y} x=12");
            assert_eq!(buffer[(13, y)].symbol(), " ", "row {y} x=13");
            assert_ne!(buffer[(14, y)].symbol().trim(), "", "row {y} x=14");
        }
        let text = all_text(&terminal);
        assert!(text.contains("dave   I'll take a look"), "{text}");
    }

    #[test]
    fn long_author_truncated_and_aligned() {
        let mut app = app_80x24();
        app.open_room(general());
        app.messages
            .get_mut(&general())
            .expect("general")
            .push(Message {
                id: "long-author".to_string(),
                room_id: general(),
                author: "firstname.lastname.contractor.external".to_string(),
                ts: 0,
                body: "short".to_string(),
            });
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(
            text.contains("firstname.lastname.contractor.e\u{2026}"),
            "{text}"
        );
        let buffer = terminal.backend().buffer();
        for y in 1..=17u16 {
            assert_eq!(buffer[(39, y)].symbol(), " ", "row {y} x=39");
            assert_eq!(buffer[(40, y)].symbol(), " ", "row {y} x=40");
            assert_ne!(buffer[(41, y)].symbol().trim(), "", "row {y} x=41");
        }
    }

    #[test]
    fn hostile_message_sanitized() {
        let mut app = app_80x24();
        app.open_room(general());
        let msgs = app.messages.get_mut(&general()).expect("general");
        msgs.push(Message {
            id: "hostile".to_string(),
            room_id: general(),
            author: "\u{202e}mal\u{1b}]52;c;ZXZpbA==\u{7}lory\u{9b}".to_string(),
            ts: 0,
            body: "\u{1b}]52;c;ZXZpbA==\u{7}clip\u{9b}2Jboard\u{202e}x\ty".to_string(),
        });
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                for c in symbol.chars() {
                    assert!(
                        !c.is_control(),
                        "control char U+{:04X} at {x},{y}",
                        c as u32
                    );
                    assert!(
                        !matches!(c as u32, 0x202a..=0x202e | 0x2066..=0x2069),
                        "bidi char U+{:04X} at {x},{y}",
                        c as u32
                    );
                }
                text.push_str(symbol);
            }
        }
        assert!(text.contains("clip"), "hostile body not drawn: {text}");
    }
}
