//! Messages pane: the open room's wrapped messages with a cursor highlight. It draws only the
//! window `app::messages::view` selects, from the same row model the reducer scrolls with.

use crate::app::history::HistoryState;
use crate::app::{App, Focus, messages};
use crate::ui::sanitize::sanitize_line;
use crate::ui::{pane_block, wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// Draw the messages pane: only the rows of the window `app::messages::view` selects.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Messages;
    let title = match app.current_room_name() {
        Some(name) => format!(" {} ", sanitize_line(name)),
        None => " Messages ".to_string(),
    };
    let block = pane_block(&title, focused, &app.theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let notice = app.theme.messages_notice;
    let Some(room) = app.current_room.as_ref() else {
        frame.render_widget(Paragraph::new(Line::styled("No room open", notice)), inner);
        return;
    };
    // One placeholder line, in precedence order: an encrypted room is never read (D-12), a first
    // page in flight or failed hides any messages held for the room (D-16).
    let encrypted = app.rooms.iter().any(|r| &r.id == room && r.encrypted);
    let placeholder = if encrypted {
        Some("encrypted, not supported")
    } else {
        match app.history.get(room) {
            Some(HistoryState::Loading) => Some("Loading…"),
            Some(HistoryState::Failed) => Some("Could not load history"),
            _ => None,
        }
    };
    if let Some(text) = placeholder {
        frame.render_widget(
            Paragraph::new(Line::styled(sanitize_line(text), notice)),
            inner,
        );
        return;
    }
    let msgs = app.messages.get(room).map_or(&[][..], Vec::as_slice);
    if msgs.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No messages yet", notice)),
            inner,
        );
        return;
    }

    let v = messages::view(app, room);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor_rows = 0..0;
    let ctx = app.row_ctx(msgs);
    for (i, msg) in msgs.iter().enumerate() {
        let rows = wrap::render_rows(msg, inner.width, &ctx);
        if i == v.cursor {
            cursor_rows = lines.len()..lines.len() + rows.len();
        }
        lines.extend(rows);
    }
    // The default highlights set no foreground, so author colors survive on the cursor rows
    // (REVERSED swaps them into the background). A theme.toml fg on messages.cursor or
    // messages.cursor_unfocused is applied with set_style over the row and so overrides the
    // author colors there (RESEARCH Pitfall 13); that is documented, not prevented.
    let highlight = if focused {
        app.theme.messages_cursor
    } else {
        app.theme.messages_cursor_unfocused
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
    use crate::app::history::HistoryState;
    use crate::app::messages::{row_starts, view};
    use crate::data::{Message, MessageKind, RoomId};
    use crate::test_util::{app_80x24, press};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Rect, Size};
    use ratatui::style::{Color, Modifier};

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    /// The app's own messages pane rect: `view()` derives its viewport from `app.size`, so every
    /// render test draws on a backend of exactly this size.
    fn pane_rect(app: &App) -> Rect {
        app.areas().messages
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

    /// An 80x25 fake-data app (pane 52x20) with `name` open and `state` as its history state.
    fn open_with_history(name: &str, state: HistoryState) -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        let id = RoomId(name.to_string());
        app.open_room(id.clone());
        app.history.insert(id, state);
        assert_eq!(pane_rect(&app).width, 52, "snapshot geometry");
        app
    }

    /// The pane's inner rows (borders removed) as trimmed text, blank rows dropped.
    fn pane_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let height = terminal.backend().buffer().area.height;
        (1..height - 1)
            .map(|y| {
                row_text(terminal, y)
                    .chars()
                    .skip(1)
                    .take(50)
                    .collect::<String>()
            })
            .map(|row| row.trim().to_string())
            .filter(|row| !row.is_empty())
            .collect()
    }

    #[test]
    fn placeholder_loading_52x20() {
        let app = open_with_history("quiet", HistoryState::Loading);
        let terminal = render_pane(&app);
        assert_eq!(pane_lines(&terminal), ["Loading…"]);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn placeholder_failed_52x20() {
        let app = open_with_history("quiet", HistoryState::Failed);
        let terminal = render_pane(&app);
        assert_eq!(pane_lines(&terminal), ["Could not load history"]);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn placeholder_encrypted_52x20() {
        // The fake dataset's secrets room is encrypted and holds 4 messages.
        let mut app = App::with_fake_data(Size::new(80, 25));
        let id = RoomId("secrets".to_string());
        assert!(!app.messages[&id].is_empty());
        app.open_room(id);
        let terminal = render_pane(&app);
        assert_eq!(pane_lines(&terminal), ["encrypted, not supported"]);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn placeholder_precedence() {
        let loaded = || HistoryState::Loaded {
            oldest_ts: None,
            exhausted: true,
            loading_older: false,
        };
        let text = |app: &App| all_text(&render_pane(app));

        // Encrypted wins over Loading and over held messages.
        let app = open_with_history("secrets", HistoryState::Loading);
        let shown = text(&app);
        assert!(shown.contains("encrypted, not supported"), "{shown}");
        assert!(!shown.contains("Loading…"), "{shown}");

        // Loading and Failed win over held messages (general holds 60).
        let app = open_with_history("general", HistoryState::Loading);
        let shown = text(&app);
        assert!(shown.contains("Loading…"), "{shown}");
        assert!(!shown.contains("morning, anyone"), "{shown}");
        let app = open_with_history("general", HistoryState::Failed);
        let shown = text(&app);
        assert!(shown.contains("Could not load history"), "{shown}");

        // A Loaded room with no messages shows the Phase 1 line; one with messages shows them.
        let app = open_with_history("quiet", loaded());
        assert!(text(&app).contains("No messages yet"));
        let app = open_with_history("general", loaded());
        let shown = text(&app);
        assert!(!shown.contains("Loading…") && !shown.contains("No messages yet"));

        // No room open still says so.
        let app = App::with_fake_data(Size::new(80, 25));
        assert!(text(&app).contains("No room open"));
    }

    #[test]
    fn placeholder_style() {
        let app = open_with_history("quiet", HistoryState::Loading);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        // First inner cell: the `L` of `Loading…`.
        let cell = &buffer[(1, 1)];
        assert_eq!(cell.symbol(), "L");
        assert!(cell.modifier.contains(Modifier::DIM), "DIM by default");

        // A theme.toml messages.notice color reaches the placeholder.
        let config = crate::test_util::config_from_theme("[messages.notice]\nfg = 5\n");
        let mut themed = crate::test_util::app_with_config(config, 80, 25);
        let id = RoomId("quiet".to_string());
        themed.open_room(id.clone());
        themed.history.insert(id, HistoryState::Failed);
        let terminal = render_pane(&themed);
        assert_eq!(terminal.backend().buffer()[(1, 1)].fg, Color::Indexed(5));
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
        use crate::ui::palette::{DEFAULT_AUTHOR_PALETTE, author_color};
        let app = open_general_80x25();
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let mut colors = std::collections::HashSet::new();
        for y in 1..=18u16 {
            if buffer[(3, y)].symbol() == ":" {
                let name: String = (7..=11u16).map(|x| buffer[(x, y)].symbol()).collect();
                let name = name.trim();
                let cell = &buffer[(7, y)];
                assert_eq!(
                    cell.fg,
                    author_color(name, &DEFAULT_AUTHOR_PALETTE),
                    "row {y} {name:?}"
                );
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
    fn author_color_survives_cursor_highlight() {
        use crate::ui::palette::{DEFAULT_AUTHOR_PALETTE, author_color};
        let mut app = app_80x24();
        app.open_room(general());
        press(&mut app, "gg");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let name: String = (7..=11u16).map(|x| buffer[(x, 1)].symbol()).collect();
        let name = name.trim().to_string();
        assert_eq!(
            buffer[(7, 1)].fg,
            author_color(&name, &DEFAULT_AUTHOR_PALETTE)
        );
        assert!(buffer[(7, 1)].modifier.contains(Modifier::REVERSED));
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(7, 1)].fg,
            author_color(&name, &DEFAULT_AUTHOR_PALETTE)
        );
        assert_eq!(buffer[(7, 1)].bg, Color::DarkGray);
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
    fn themed_cursor_highlight() {
        let config = crate::test_util::config_from_theme(
            "[messages.cursor]\nbg = 6\n[messages.cursor_unfocused]\nbg = 2\n",
        );
        let mut app = crate::test_util::app_with_config(config, 80, 24);
        app.open_room(general());
        press(&mut app, "gg");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        // Row 1 is the first row of the cursor message; its whole width carries the highlight.
        for x in [2u16, 20, 40] {
            assert_eq!(buffer[(x, 1)].bg, Color::Indexed(6), "focused x {x}");
        }
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        for x in [2u16, 20, 40] {
            assert_eq!(buffer[(x, 1)].bg, Color::Indexed(2), "unfocused x {x}");
        }
    }

    #[test]
    fn themed_notices() {
        let config = crate::test_util::config_from_theme("[messages.notice]\nfg = 5\n");
        let app = crate::test_util::app_with_config(config, 80, 24);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].symbol(), "N", "No room open");
        assert_eq!(buffer[(1, 1)].fg, Color::Indexed(5));
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
            let ctx = app.row_ctx(&app.messages[&general()]);
            let expected: Vec<String> = app.messages[&general()]
                .iter()
                .flat_map(|m| wrap::render_rows(m, 50, &ctx))
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
                kind: MessageKind::Normal,
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
            kind: MessageKind::Normal,
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

    #[test]
    fn message_kinds_52x20() {
        let mut app = App::with_fake_data(Size::new(80, 25));
        let room = RoomId("quiet".to_string());
        let msg = |id: &str, author: &str, ts: i64, body: &str, kind: MessageKind| Message {
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind,
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg("1", "alice", 34_440, "hello there", MessageKind::Normal),
                msg(
                    "2",
                    "carol",
                    34_500,
                    "* joined the room",
                    MessageKind::System,
                ),
                msg(
                    "3",
                    "bob",
                    34_560,
                    "see attached [attachment: report.pdf]",
                    MessageKind::Normal,
                ),
                msg(
                    "4",
                    "bob",
                    34_620,
                    "[encrypted message]",
                    MessageKind::Normal,
                ),
            ],
        );
        app.open_room(room);
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("* joined the room"), "{text}");
        assert!(text.contains("[attachment: report.pdf]"), "{text}");
        assert!(text.contains("[encrypted message]"), "{text}");
        // The system line is dim, author included, and not bold.
        let (y, row) = (0..20u16)
            .map(|y| (y, row_text(&terminal, y)))
            .find(|(_, row)| row.contains("joined the room"))
            .expect("system row");
        let buffer = terminal.backend().buffer();
        for needle in ["carol", "joined"] {
            let x = row.find(needle).expect("text on the row");
            let cell = &buffer[(x as u16, y)];
            assert!(cell.modifier.contains(Modifier::DIM), "{needle} not dim");
            assert!(!cell.modifier.contains(Modifier::BOLD), "{needle} is bold");
        }
        insta::assert_snapshot!(terminal.backend());
    }
}
