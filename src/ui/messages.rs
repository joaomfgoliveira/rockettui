//! Messages pane: the open room's wrapped messages with a cursor highlight. It draws only the
//! window `app::messages::view` selects, from the same row model the reducer scrolls with.

use crate::app::history::HistoryState;
use crate::app::{App, Focus, messages};
use crate::ui::sanitize::sanitize_line;
use crate::ui::{pane_block, wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

/// How many threads of the open room the server lists as unread.
fn unread_threads(app: &App) -> usize {
    app.current_room
        .as_ref()
        .and_then(|id| app.rooms.iter().find(|r| &r.id == id))
        .map_or(0, |r| r.threads.count())
}

/// The right end of the pane's top border: ` Threads (<key>) unread (<n>) `, the key being the one
/// bound to `list_threads` (left out when none is). Nothing while no room is open.
fn threads_title(app: &App) -> Option<String> {
    app.current_room.as_ref()?;
    let key = crate::app::context::keys_for(app, "list_threads")
        .map(|keys| format!(" ({})", keys.split(' ').next().unwrap_or_default()))
        .unwrap_or_default();
    Some(format!(" Threads{key} unread ({}) ", unread_threads(app)))
}

/// Draw the messages pane: only the rows of the window `app::messages::view` selects.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Messages;
    let title = match app.current_room_name() {
        Some(name) => format!(" {} ", sanitize_line(name)),
        None => " Messages ".to_string(),
    };
    let mut block = pane_block(&title, focused, &app.theme);
    if let Some(threads) = threads_title(app)
        && title.width() + threads.width() + 4 <= usize::from(area.width)
    {
        let style = if unread_threads(app) > 0 {
            app.theme.messages_thread_unread
        } else {
            app.theme.border_unfocused
        };
        block = block.title(Line::styled(threads, style).right_aligned());
    }
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
    let ctx = app.pane_row_ctx(room, msgs);
    let joins = wrap::continuation_flags(msgs, &ctx);
    for (i, msg) in msgs.iter().enumerate() {
        // The dividers belong to the message below them but are not part of its highlight.
        let prev = i.checked_sub(1).map(|p| &msgs[p]);
        let (dividers, block) = wrap::block_lines(msg, prev, joins[i], inner.width, &ctx);
        if i == v.cursor {
            cursor_rows = lines.len() + dividers..lines.len() + block.len();
        }
        lines.extend(block);
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
    // Where the matches of an active search sit on the visible cursor rows: the cursor highlight
    // below would reverse them, so they are drawn again afterwards.
    let match_cells = if app.search.is_some() {
        cursor_match_cells(&window, &cursor_rows, v.top_row, &app.theme)
    } else {
        Vec::new()
    };
    let first = cursor_rows.start.max(v.top_row);
    let end = cursor_rows.end.min(v.top_row + usize::from(inner.height));
    frame.render_widget(Paragraph::new(window), inner);
    // Highlight the whole width of every visible row of the cursor message, trailing blanks
    // included.
    for row in first..end {
        let y = inner.y + u16::try_from(row - v.top_row).unwrap_or(u16::MAX);
        frame
            .buffer_mut()
            .set_style(Rect::new(inner.x, y, inner.width, 1), highlight);
    }
    // An emoji image keeps its id in the cell's foreground colour: the cursor row must not reverse it.
    for row in first..end {
        let y = inner.y + u16::try_from(row - v.top_row).unwrap_or(u16::MAX);
        crate::ui::emoji::keep_image_ids(frame.buffer_mut(), Rect::new(inner.x, y, inner.width, 1));
    }
    // A match keeps its own style on the cursor row: no reversing, its colors as themed.
    let match_style = app.theme.search_match.remove_modifier(Modifier::REVERSED);
    for (row, x, width) in match_cells {
        let y = inner.y + u16::try_from(row - v.top_row).unwrap_or(u16::MAX);
        let x = inner.x.saturating_add(x);
        let width = width.min(inner.right().saturating_sub(x));
        frame
            .buffer_mut()
            .set_style(Rect::new(x, y, width, 1), match_style);
    }
}

/// `(row, x, width)` of every span in the match style on the visible rows of the cursor message,
/// with `x` relative to the pane.
fn cursor_match_cells(
    window: &[Line<'_>],
    cursor_rows: &std::ops::Range<usize>,
    top_row: usize,
    theme: &crate::config::Theme,
) -> Vec<(usize, u16, u16)> {
    let mut cells = Vec::new();
    for (i, line) in window.iter().enumerate() {
        let row = top_row + i;
        if !cursor_rows.contains(&row) {
            continue;
        }
        let mut x = 0usize;
        for span in &line.spans {
            let width = span.content.width();
            if span.style == theme.search_match && width > 0 {
                cells.push((
                    row,
                    u16::try_from(x).unwrap_or(u16::MAX),
                    u16::try_from(width).unwrap_or(u16::MAX),
                ));
            }
            x += width;
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::history::HistoryState;
    use crate::app::messages::{row_starts, view};
    use crate::app::update::update;
    use crate::data::{Message, MessageKind, RoomId, SendState};
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
        let name: String = (7..=11u16).map(|x| buffer[(x, 2)].symbol()).collect();
        let name = name.trim().to_string();
        assert_eq!(
            buffer[(7, 2)].fg,
            author_color(&name, &DEFAULT_AUTHOR_PALETTE)
        );
        assert!(buffer[(7, 2)].modifier.contains(Modifier::REVERSED));
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(7, 2)].fg,
            author_color(&name, &DEFAULT_AUTHOR_PALETTE)
        );
        assert_eq!(buffer[(7, 2)].bg, Color::DarkGray);
    }

    #[test]
    fn cursor_row_not_reversed_when_unfocused() {
        let mut app = app_80x24();
        app.open_room(general());
        press(&mut app, "gg");
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        assert!(!buffer[(2, 2)].modifier.contains(Modifier::REVERSED));
        assert_eq!(buffer[(2, 2)].bg, ratatui::style::Color::DarkGray);
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
        // Row 1 is the day divider, row 2 the first row of the cursor message; its whole width carries the highlight.
        for x in [2u16, 20, 40] {
            assert_eq!(buffer[(x, 2)].bg, Color::Indexed(6), "focused x {x}");
        }
        press(&mut app, "<C-w>h");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        for x in [2u16, 20, 40] {
            assert_eq!(buffer[(x, 2)].bg, Color::Indexed(2), "unfocused x {x}");
        }
    }

    #[test]
    fn an_emoji_image_on_the_cursor_row_is_not_reversed() {
        use crate::app::emoji::{CustomEmojis, Graphics, ImageState};
        let mut app = app_80x24();
        app.open_room(general());
        app.graphics = Graphics::Supported;
        app.custom_emojis = CustomEmojis::Loaded(vec![crate::data::CustomEmoji {
            name: "rtshipit".to_string(),
            ext: "png".to_string(),
            ..Default::default()
        }]);
        app.images.set_state(
            "rtshipit",
            ImageState::Ready {
                etag: String::new(),
                image: crate::emoji_image::Rgba {
                    width: 1,
                    height: 1,
                    pixels: vec![0, 0, 0, 255].into(),
                    ..Default::default()
                },
                id: 9,
                uploaded: true,
            },
        );
        let room = general();
        app.messages.get_mut(&room).expect("messages")[0].body = "ship :rtshipit: it".to_string();
        press(&mut app, "gg");
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let mut found = 0;
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol().starts_with(crate::kitty::PLACEHOLDER) {
                    found += 1;
                    assert!(!cell.modifier.contains(Modifier::REVERSED), "x {x} y {y}");
                    assert_eq!(cell.fg, Color::Rgb(0, 0, 9), "the id stays the foreground");
                }
            }
        }
        assert_eq!(found, 2, "the two cells of the emoji");
        // The rest of the cursor row is still highlighted.
        assert!(buffer[(2, 2)].modifier.contains(Modifier::REVERSED));
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
            let msgs = &app.messages[&general()];
            let expected: Vec<String> = msgs
                .iter()
                .enumerate()
                .flat_map(|(i, m)| {
                    let prev = i.checked_sub(1).map(|p| &msgs[p]);
                    wrap::divider_row(m, prev, 50, &ctx)
                        .into_iter()
                        .chain(wrap::render_rows(m, 50, &ctx))
                })
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
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                refs: None,
                reactions: Vec::new(),
                id: "long-author".to_string(),
                room_id: general(),
                author: "firstname.lastname.contractor.external".to_string(),
                ts: 0,
                body: "short".to_string(),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            });
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(
            text.contains("firstname.lastname.contractor.e\u{2026}"),
            "{text}"
        );
        let buffer = terminal.backend().buffer();
        for y in 1..=17u16 {
            if buffer[(1, y)].symbol() == "\u{2500}" {
                continue; // a day divider spans the whole width
            }
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
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: "hostile".to_string(),
            room_id: general(),
            author: "\u{202e}mal\u{1b}]52;c;ZXZpbA==\u{7}lory\u{9b}".to_string(),
            ts: 0,
            body: "\u{1b}]52;c;ZXZpbA==\u{7}clip\u{9b}2Jboard\u{202e}x\ty".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
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
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind,
            send: SendState::Sent,
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

    #[test]
    fn send_pending_52x20() {
        let mut app = open_general_80x25();
        press(&mut app, "ihello pending<CR>");
        let sent = app.messages[&general()].last().expect("pending row");
        assert!(sent.send.is_pending(), "{:?}", sent.send);
        let terminal = render_pane(&app);
        let last = row_text(&terminal, 18);
        assert!(last.contains("\u{2026}"), "{last:?}");
        assert!(last.contains("hello pending"), "{last:?}");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn pending_theme_end_to_end() {
        let config = crate::test_util::config_from_theme(
            "[markers]\nsending = \"~\"\n[messages.pending]\nfg = \"blue\"\n",
        );
        let mut app = crate::test_util::app_with_config(config, 80, 25);
        app.open_room(general());
        press(&mut app, "ihi<CR>");
        let id = app.messages[&general()].last().expect("row").id.clone();

        let terminal = render_pane(&app);
        let row = row_text(&terminal, 18);
        assert!(row.starts_with("\u{2502}~"), "{row:?}");
        let buffer = terminal.backend().buffer();
        let x = row.chars().position(|c| c == 'h').expect("body") as u16;
        assert_eq!(buffer[(x, 18)].symbol(), "h");
        assert_eq!(buffer[(x, 18)].fg, Color::Blue);
        assert_eq!(buffer[(1, 18)].fg, Color::Blue, "marker");

        // The server's copy replaces the row: its time is back and the body is plain.
        let mut server = app.messages[&general()].last().expect("row").clone();
        server.ts = 34_440;
        server.author = "me".to_string();
        update(
            &mut app,
            Action::MessageSent {
                room: general(),
                id,
                message: server,
            },
        );
        let terminal = render_pane(&app);
        let row = row_text(&terminal, 18);
        assert!(row.contains("09:34"), "{row:?}");
        assert!(!row.contains('~'), "{row:?}");
        let buffer = terminal.backend().buffer();
        let x = row.chars().position(|c| c == 'h').expect("body") as u16;
        assert_eq!(buffer[(x, 18)].fg, Color::Reset);
        assert_eq!(buffer[(x, 18)].modifier, Modifier::empty());
    }

    /// A room holding one message in each send state, in this order.
    fn app_with_every_state(layout: wrap::MessageLayout) -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.message_layout = layout;
        let room = RoomId("quiet".to_string());
        let msg = |id: &str, author: &str, ts: i64, body: &str, send: SendState| Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send,
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg("1", "alice", 34_440, "delivered message", SendState::Sent),
                msg("2", "me", 34_500, "in flight", SendState::Sending),
                msg(
                    "3",
                    "me",
                    34_560,
                    "waiting its turn",
                    SendState::Queued { reconcile: false },
                ),
                msg(
                    "4",
                    "me",
                    34_620,
                    "refused by the server",
                    SendState::Failed,
                ),
                msg(
                    "5",
                    "me",
                    34_680,
                    "outcome not known",
                    SendState::Unconfirmed,
                ),
            ],
        );
        app.open_room(room);
        app
    }

    /// In Compact each message of `app_with_every_state` is one row, at y 1 to 5.
    fn assert_state_styles(terminal: &Terminal<TestBackend>, body_x: u16) {
        let buffer = terminal.backend().buffer();
        let dim = |x: u16, y: u16| buffer[(x, y)].modifier.contains(Modifier::DIM);
        for (y, name) in [(3u16, "Sending"), (4, "Queued")] {
            assert!(dim(body_x, y), "{name} body not dim");
            assert_ne!(buffer[(body_x, y)].fg, Color::Red, "{name}");
        }
        for (y, name) in [(5u16, "Failed"), (6, "Unconfirmed")] {
            assert_eq!(buffer[(body_x, y)].fg, Color::Red, "{name} body not red");
            assert!(!dim(body_x, y), "{name}");
        }
        assert!(!dim(body_x, 2), "Sent body is not dim");
        assert_eq!(buffer[(body_x, 2)].fg, Color::Reset);
    }

    #[test]
    fn send_states_compact_52x20() {
        let app = app_with_every_state(wrap::MessageLayout::Compact);
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        for needle in ["\u{2026}", "!", "?"] {
            assert!(text.contains(needle), "{needle}: {text}");
        }
        assert_state_styles(&terminal, 14);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn send_states_two_line_52x20() {
        let app = app_with_every_state(wrap::MessageLayout::TwoLine);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        // Header rows are y 2, 4, 6, 8, 10 (y 1 is the day divider); each body sits at x 3 on the row below.
        let marker_at = |y: u16| row_text(&terminal, y).contains(['\u{2026}', '!', '?']);
        assert!(!marker_at(2), "a Sent header has a time, not a marker");
        for y in [4u16, 6, 8, 10] {
            assert!(marker_at(y), "row {y} {}", row_text(&terminal, y));
        }
        assert_eq!(buffer[(3, 3)].fg, Color::Reset, "Sent body");
        assert!(
            buffer[(3, 5)].modifier.contains(Modifier::DIM),
            "Sending body"
        );
        assert!(
            buffer[(3, 7)].modifier.contains(Modifier::DIM),
            "Queued body"
        );
        assert_eq!(buffer[(3, 9)].fg, Color::Red, "Failed body");
        assert_eq!(buffer[(3, 11)].fg, Color::Red, "Unconfirmed body");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn cursor_over_failed_row() {
        let mut app = app_with_every_state(wrap::MessageLayout::Compact);
        press(&mut app, "4gg");
        assert_eq!(view(&app, &RoomId("quiet".to_string())).cursor, 3);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        // Row 5 is the Failed message on the cursor: the highlight covers it, the red stays.
        for x in [1u16, 14, 30] {
            assert!(
                buffer[(x, 5)].modifier.contains(Modifier::REVERSED),
                "x {x} not highlighted"
            );
        }
        assert_eq!(
            buffer[(14, 5)].fg,
            Color::Red,
            "failed style under the cursor"
        );
        assert!(!buffer[(14, 6)].modifier.contains(Modifier::REVERSED));
    }

    // ---- search highlighting ----

    fn msg_with(id: usize, body: &str, kind: MessageKind) -> Message {
        Message {
            id: format!("s{id}"),
            room_id: general(),
            author: "alice".to_string(),
            ts: 1000 + id as i64,
            body: body.to_string(),
            kind,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
        }
    }

    fn search_app(config: crate::config::Config, bodies: &[(&str, MessageKind)]) -> App {
        let mut app = crate::test_util::app_with_config(config, 80, 24);
        app.open_room(general());
        let msgs = bodies
            .iter()
            .enumerate()
            .map(|(i, (b, k))| msg_with(i, b, *k))
            .collect();
        app.messages.insert(general(), msgs);
        app.msg_views.remove(&general());
        app
    }

    fn searching(app: &mut App, query: &str) {
        app.search = Some(crate::app::search::Search {
            query: query.to_string(),
            seeking: false,
            seeking_newer: false,
        });
    }

    /// Cells of the pane (inside the border) whose background is `bg`.
    fn cells_with_bg(terminal: &Terminal<TestBackend>, bg: Color) -> Vec<(u16, u16)> {
        let buffer = terminal.backend().buffer();
        let mut out = Vec::new();
        for y in 1..buffer.area.height - 1 {
            for x in 1..buffer.area.width - 1 {
                if buffer[(x, y)].bg == bg {
                    out.push((x, y));
                }
            }
        }
        out
    }

    const N: MessageKind = MessageKind::Normal;

    #[test]
    fn every_occurrence_is_drawn_in_the_match_style() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[("foo bar foo", N), ("middle", N), ("last foo", N)],
        );
        searching(&mut app, "foo");
        // Put the cursor on the message with no match, so no row is reversed over a match.
        press(&mut app, "gg");
        press(&mut app, "j");
        let terminal = render_pane(&app);
        let cells = cells_with_bg(&terminal, Color::Yellow);
        assert_eq!(cells.len(), 9, "three matches of three columns: {cells:?}");
        let buffer = terminal.backend().buffer();
        assert!(
            cells
                .iter()
                .all(|&(x, y)| buffer[(x, y)].fg == Color::Black)
        );
        let text: String = cells
            .iter()
            .map(|&(x, y)| buffer[(x, y)].symbol())
            .collect();
        assert_eq!(text, "foofoofoo");
    }

    #[test]
    fn matching_is_smart_case_in_the_highlight() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[("Foo foo", N), ("x", N)],
        );
        searching(&mut app, "Foo");
        let terminal = render_pane(&app);
        assert_eq!(cells_with_bg(&terminal, Color::Yellow).len(), 3);
        searching(&mut app, "foo");
        let terminal = render_pane(&app);
        assert_eq!(cells_with_bg(&terminal, Color::Yellow).len(), 6);
    }

    #[test]
    fn themed_match_style_applies() {
        let config = crate::test_util::config_from_theme("[search.match]\nbg = 5\n");
        let mut app = search_app(config, &[("foo", N), ("x", N)]);
        searching(&mut app, "foo");
        let terminal = render_pane(&app);
        assert_eq!(cells_with_bg(&terminal, Color::Indexed(5)).len(), 3);
        assert!(cells_with_bg(&terminal, Color::Yellow).is_empty());
    }

    #[test]
    fn match_on_the_cursor_row_is_not_reversed() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[("older", N), ("say foo now", N)],
        );
        searching(&mut app, "foo");
        // The cursor starts on the newest message, which holds the match.
        assert_eq!(view(&app, &general()).cursor, 1);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let cells = cells_with_bg(&terminal, Color::Yellow);
        assert_eq!(cells.len(), 3);
        for &(x, y) in &cells {
            assert!(!buffer[(x, y)].modifier.contains(Modifier::REVERSED));
            assert_eq!(buffer[(x, y)].fg, Color::Black);
        }
        // The rest of the cursor row is still the reversed cursor highlight.
        let y = cells[0].1;
        assert!(buffer[(2, y)].modifier.contains(Modifier::REVERSED));
    }

    /// The cells of the pane whose symbol is `@`: where a mention starts.
    fn at_signs(terminal: &Terminal<TestBackend>) -> Vec<(u16, u16)> {
        let buffer = terminal.backend().buffer();
        let mut out = Vec::new();
        for y in 1..buffer.area.height - 1 {
            for x in 1..buffer.area.width - 1 {
                if buffer[(x, y)].symbol() == "@" {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn a_mention_on_the_cursor_row_stays_distinguishable_from_the_text_around_it() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[("older @bob here", N), ("say @bob now", N)],
        );
        // The cursor starts on the newest message; the older one is a control without the cursor.
        assert_eq!(view(&app, &general()).cursor, 1);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let cyan = app.theme.messages_mention.fg;
        let signs = at_signs(&terminal);
        assert_eq!(signs.len(), 2, "{signs:?}");
        for (x, y) in signs {
            let cursor_row = buffer[(2, y)].modifier.contains(Modifier::REVERSED);
            // The mention keeps its colour whether or not the cursor row reverses it, and the
            // text next to it does not have that colour.
            assert_eq!(
                Some(buffer[(x, y)].fg),
                cyan,
                "{x},{y} (cursor row: {cursor_row})"
            );
            assert_ne!(Some(buffer[(2, y)].fg), cyan, "{x},{y}");
        }
        // Unfocused, the cursor sets a background over the row: the mention is still its colour.
        app.focus = crate::app::Focus::Sidebar;
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        for (x, y) in at_signs(&terminal) {
            assert_eq!(Some(buffer[(x, y)].fg), cyan, "{x},{y} unfocused");
        }
    }

    #[test]
    fn an_own_mention_stays_prominent_on_the_cursor_row() {
        let mut app = search_app(crate::config::Config::default(), &[("say @me now", N)]);
        app.own_username = Some("me".to_string());
        let own = app.theme.messages_mention_self;
        for focus in [crate::app::Focus::Messages, crate::app::Focus::Sidebar] {
            app.focus = focus;
            let terminal = render_pane(&app);
            let buffer = terminal.backend().buffer();
            let (x, y) = at_signs(&terminal)[0];
            assert_eq!(Some(buffer[(x, y)].fg), own.fg, "{focus:?}");
            assert!(
                buffer[(x, y)].modifier.contains(own.add_modifier),
                "{focus:?}: bold and underlined survive the cursor"
            );
        }
    }

    #[test]
    fn deleted_lines_and_encrypted_text_are_not_highlighted() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[
                ("foo", MessageKind::Deleted),
                (crate::api::map::ENCRYPTED_TEXT, N),
                ("plain", N),
            ],
        );
        searching(&mut app, "message");
        let terminal = render_pane(&app);
        // "message deleted" and "[encrypted message]" contain the query but are not matches.
        assert!(cells_with_bg(&terminal, Color::Yellow).is_empty());
    }

    #[test]
    fn a_hostile_body_is_matched_as_drawn() {
        let mut app = search_app(
            crate::config::Config::default(),
            &[("a\u{1b}[31mfoo\u{7}b", N), ("x", N)],
        );
        searching(&mut app, "foo");
        let terminal = render_pane(&app);
        assert_eq!(cells_with_bg(&terminal, Color::Yellow).len(), 3);
        assert!(!all_text(&terminal).contains('\u{1b}'));
    }

    #[test]
    fn a_search_never_changes_row_heights() {
        let long = "foo ".repeat(40);
        let mut app = search_app(
            crate::config::Config::default(),
            &[(long.as_str(), N), ("foo", N), ("tail", N)],
        );
        let without = row_starts(&app, &general());
        searching(&mut app, "foo");
        assert_eq!(row_starts(&app, &general()), without);
    }

    /// A 52x20 pane in `layout` with a room of three messages: one with two reactions, one without
    /// and one whose many reactions (an alias, a skin tone, a custom name, the user's own) wrap.
    fn app_with_reactions(layout: wrap::MessageLayout) -> App {
        use crate::data::Reaction;
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.message_layout = layout;
        app.own_username = Some("me".to_string());
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        let room = RoomId("quiet".to_string());
        let reaction = |name: &str, users: &[&str]| Reaction {
            name: name.to_string(),
            users: users.iter().map(|u| u.to_string()).collect(),
        };
        let msg = |id: &str, author: &str, ts: i64, body: &str, reactions: Vec<Reaction>| Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions,
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg(
                    "1",
                    "alice",
                    34_440,
                    "deploy finished",
                    vec![
                        reaction("+1", &["dev", "alice", "bob"]),
                        reaction("tada", &["alice"]),
                    ],
                ),
                msg("2", "bob", 34_500, "no reactions here", Vec::new()),
                msg(
                    "3",
                    "alice",
                    34_560,
                    "ship it",
                    vec![
                        reaction("partyparrot", &["bob"]),
                        reaction("thumbsup_tone2", &["me"]),
                        reaction("rt_custom_name", &["alice", "bob"]),
                        reaction("heart", &["a"]),
                        reaction("eyes", &["a"]),
                        reaction("rocket", &["a"]),
                        reaction("fire", &["a"]),
                        reaction("100", &["a"]),
                    ],
                ),
            ],
        );
        app.open_room(room);
        app
    }

    #[test]
    fn reactions_compact_52x20() {
        let app = app_with_reactions(wrap::MessageLayout::Compact);
        let terminal = render_pane(&app);
        // A wide glyph leaves a placeholder cell after it in the test buffer; squeeze it out.
        let text = all_text(&terminal).replace("  ", " ");
        assert!(text.contains("👍 3"), "{text}");
        assert!(text.contains("🦜 1"), "{text}");
        assert!(text.contains(":rt_custom_name: 2"), "{text}");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn reactions_two_line_52x20() {
        let app = app_with_reactions(wrap::MessageLayout::TwoLine);
        let terminal = render_pane(&app);
        assert!(all_text(&terminal).replace("  ", " ").contains("👍 3"));
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn the_cursor_highlight_covers_the_reaction_row() {
        let mut app = app_with_reactions(wrap::MessageLayout::Compact);
        press(&mut app, "gg");
        let room = RoomId("quiet".to_string());
        let v = view(&app, &room);
        assert_eq!(v.cursor, 0);
        let starts = row_starts(&app, &room);
        // The first message of the room: the day divider, its body row and one reaction row.
        assert_eq!(starts[1] - starts[0], 3);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let width = buffer.area.width;
        let hidden = |x: u16, y: u16| {
            x > 0 && unicode_width::UnicodeWidthStr::width(buffer[(x - 1, y)].symbol()) == 2
        };
        let reversed = |y: u16| {
            (1..width - 1)
                .filter(|&x| !hidden(x, y))
                .all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
        };
        // The divider row is not part of the highlight (the cursor skips dividers).
        let body = 2 + (starts[0] - v.top_row) as u16;
        assert!(!reversed(body - 1), "the divider row");
        assert!(reversed(body), "body row of the cursor message");
        assert!(reversed(body + 1), "reaction row of the cursor message");
        assert!(!reversed(body + 2), "the next message is not highlighted");
    }

    /// A 52x20 pane in `layout` with a room of four messages: a parent with 3 replies, a parent
    /// with 1 reply, a reply also shown in the channel and a plain message. The time of the last
    /// reply is 00:11 UTC on 1970-01-01.
    fn app_with_threads(layout: wrap::MessageLayout) -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.message_layout = layout;
        let room = RoomId("quiet".to_string());
        let msg = |id: &str, ts: i64, body: &str| Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: id.to_string(),
            room_id: room.clone(),
            author: "alice".to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        app.messages.insert(
            room.clone(),
            vec![
                Message {
                    tcount: 3,
                    tlm: Some(660),
                    followers: None,
                    refs: None,
                    ..msg("1", 60, "deploy plan")
                },
                Message {
                    tcount: 1,
                    tlm: Some(700),
                    followers: None,
                    refs: None,
                    ..msg("2", 120, "who owns the rollback")
                },
                Message {
                    tmid: Some("1".to_string()),
                    tshow: true,
                    ..msg("3", 660, "done, also here")
                },
                msg("4", 720, "plain message"),
            ],
        );
        app.open_room(room);
        app
    }

    #[test]
    fn thread_marker_compact_52x20() {
        let app = app_with_threads(wrap::MessageLayout::Compact);
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("-> 3 replies, last 00:11"), "{text}");
        assert!(text.contains("-> 1 reply, last 00:11"), "{text}");
        assert!(text.contains("[thread] done, also here"), "{text}");
        assert_eq!(
            text.matches("[thread]").count(),
            1,
            "only the channel-visible reply is tagged"
        );
        assert_eq!(
            text.matches("-> ").count(),
            2,
            "only the parents have a marker"
        );
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn thread_marker_two_line_52x20() {
        let app = app_with_threads(wrap::MessageLayout::TwoLine);
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("-> 3 replies, last 00:11"), "{text}");
        assert!(text.contains("[thread] done, also here"), "{text}");
        assert_eq!(
            text.matches("[thread]").count(),
            1,
            "only the channel-visible reply is tagged"
        );
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn the_thread_marker_wraps_in_a_narrow_pane() {
        let mut app = app_with_threads(wrap::MessageLayout::TwoLine);
        app.settings.sidebar_width = 56;
        let pane = pane_rect(&app);
        assert!(pane.width < 32, "narrow pane: {}", pane.width);
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("-> 3 replies,"), "{text}");
        assert!(
            !text.contains("-> 3 replies, last 00:11"),
            "wrapped onto two rows: {text}"
        );
        insta::assert_snapshot!(terminal.backend());
    }

    fn mark_thread_unread(app: &mut App, id: &str) {
        app.rooms
            .iter_mut()
            .find(|r| r.id.0 == "quiet")
            .expect("room")
            .threads
            .unread
            .insert(id.to_string());
    }

    #[test]
    fn an_unread_thread_ends_its_marker_row_with_new() {
        let mut app = app_with_threads(wrap::MessageLayout::Compact);
        // Not listed: nothing extra.
        let text = all_text(&render_pane(&app));
        assert!(!text.contains("new"), "{text}");
        mark_thread_unread(&mut app, "1");
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("-> 3 replies, last 00:11, new"), "{text}");
        assert_eq!(text.matches(", new").count(), 1, "only thread 1: {text}");
        assert!(
            text.contains("-> 1 reply, last 00:11\n") || text.contains("-> 1 reply, last 00:11 ")
        );
        // An unread thread's whole marker row is drawn in the unread style; a read one's in the thread style.
        let height = terminal.backend().buffer().area.height;
        let y = (0..height)
            .find(|&y| row_text(&terminal, y).contains(", new"))
            .unwrap();
        let line = row_text(&terminal, y);
        let x = u16::try_from(line[..line.find("new").unwrap()].chars().count()).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(x, y)].fg,
            app.theme.messages_thread_unread.fg.unwrap()
        );
        assert_eq!(
            buffer[(x - 3, y)].fg,
            app.theme.messages_thread_unread.fg.unwrap()
        );
        let read_y = (0..height)
            .find(|&y| row_text(&terminal, y).contains("-> 1 reply"))
            .unwrap();
        let read_x = u16::try_from(row_text(&terminal, read_y).find("->").unwrap()).unwrap();
        assert_eq!(
            buffer[(read_x, read_y)].fg,
            app.theme.messages_thread.fg.unwrap()
        );
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn the_top_border_names_the_thread_list_key_and_the_unread_count() {
        let mut app = app_with_threads(wrap::MessageLayout::TwoLine);
        let top = |app: &App| {
            crate::test_util::render_to_string(app, 100, 25)
                .lines()
                .find(|l| l.contains("Threads (gT)"))
                .map(str::to_string)
        };
        let before = top(&app).expect("title");
        assert!(before.contains("unread (0)"), "{before}");
        let room = app.current_room.clone().unwrap();
        let held = app.rooms.iter_mut().find(|r| r.id == room).unwrap();
        held.threads.unread.insert("t1".to_string());
        held.threads.unread.insert("t2".to_string());
        assert!(top(&app).unwrap().contains("unread (2)"));
    }

    #[test]
    fn the_new_mark_wraps_with_the_marker_row_in_a_narrow_pane() {
        let mut app = app_with_threads(wrap::MessageLayout::TwoLine);
        app.settings.sidebar_width = 56;
        mark_thread_unread(&mut app, "1");
        let terminal = render_pane(&app);
        let text = all_text(&terminal);
        assert!(text.contains("new"), "{text}");
        assert!(
            !text.contains("-> 3 replies, last 00:11, new"),
            "wrapped: {text}"
        );
    }

    #[test]
    fn the_cursor_highlight_covers_the_marker_row() {
        let mut app = app_with_threads(wrap::MessageLayout::TwoLine);
        press(&mut app, "gg");
        let room = RoomId("quiet".to_string());
        let v = view(&app, &room);
        assert_eq!(v.cursor, 0);
        let starts = row_starts(&app, &room);
        // The first message: the day divider, header, body and the marker row.
        assert_eq!(starts[1] - starts[0], 4);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let width = buffer.area.width;
        let reversed =
            |y: u16| (1..width - 1).all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED));
        let body = 2 + (starts[0] - v.top_row) as u16;
        assert!(!reversed(body - 1), "the divider row");
        assert!(reversed(body), "header row");
        assert!(reversed(body + 2), "marker row of the cursor message");
        assert!(!reversed(body + 3), "the next message is not highlighted");
    }

    /// The 52x20 fake-data app with `general` open in `layout`, last seen just before its fifth
    /// newest message, so the `unread` divider is above that message and four follow it.
    fn app_with_unread_divider(layout: wrap::MessageLayout) -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.message_layout = layout;
        let seen = app.messages[&general()][app.messages[&general()].len() - 5].ts - 1;
        app.rooms
            .iter_mut()
            .find(|r| r.id == general())
            .expect("general")
            .last_seen = Some(seen);
        app.open_room(general());
        assert_eq!(pane_rect(&app).width, 52, "snapshot geometry");
        app
    }

    /// The y of the row of the `unread` divider, if one is drawn.
    fn divider_y(terminal: &Terminal<TestBackend>) -> Option<u16> {
        let height = terminal.backend().buffer().area.height;
        (0..height).find(|&y| row_text(terminal, y).contains("── unread "))
    }

    #[test]
    fn unread_divider_compact_52x20() {
        let app = app_with_unread_divider(wrap::MessageLayout::Compact);
        let terminal = render_pane(&app);
        let y = divider_y(&terminal).expect("divider is drawn");
        // It spans the whole inner width and sits above the first unread message's row.
        let row = row_text(&terminal, y);
        assert!(row.contains("── unread ─"), "{row}");
        let body = &app.messages[&general()][app.messages[&general()].len() - 5];
        assert!(row_text(&terminal, y + 1).contains(&body.author), "{row}");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn unread_divider_two_line_52x20() {
        let app = app_with_unread_divider(wrap::MessageLayout::TwoLine);
        let terminal = render_pane(&app);
        assert!(divider_y(&terminal).is_some());
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn the_unread_divider_is_red_and_takes_one_row_without_moving_columns() {
        let with = app_with_unread_divider(wrap::MessageLayout::Compact);
        let mut without = App::with_fake_data(Size::new(80, 25));
        without.open_room(general());
        let room = general();
        let rows_with = *row_starts(&with, &room).last().unwrap();
        let rows_without = *row_starts(&without, &room).last().unwrap();
        assert_eq!(rows_with, rows_without + 1, "one row for the divider");
        let terminal = render_pane(&with);
        let y = divider_y(&terminal).expect("divider");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, y)].fg, Color::Red);
        // The date divider keeps its own dim style.
        assert_ne!(buffer[(2, y)].modifier, Modifier::DIM);
    }

    #[test]
    fn the_cursor_highlight_skips_the_unread_divider() {
        let mut app = app_with_unread_divider(wrap::MessageLayout::Compact);
        let room = general();
        // The cursor goes to the first unread message, the one just below the divider.
        let first_unread = app.messages[&room].len() - 5;
        press(&mut app, &format!("{}gg", first_unread + 1));
        assert_eq!(view(&app, &room).cursor, first_unread);
        let terminal = render_pane(&app);
        let y = divider_y(&terminal).expect("divider");
        let buffer = terminal.backend().buffer();
        let reversed = |y: u16| {
            (2..buffer.area.width - 2).all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
        };
        assert!(!reversed(y), "the divider row is not highlighted");
        assert!(reversed(y + 1), "the message below it is");
    }

    /// A 52x20 pane in `layout` with messages that hold emoji shortcodes in the ways that matter.
    fn app_with_shortcodes(layout: wrap::MessageLayout) -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        app.message_layout = layout;
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        let room = RoomId("quiet".to_string());
        let msg = |id: &str, author: &str, ts: i64, body: &str| Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: id.to_string(),
            room_id: room.clone(),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg("1", "alice", 34_440, "deploy done :tada: thanks :+1:"),
                msg("2", "bob", 34_500, "meeting at 10:100:20 or use `:smile:` here"),
                msg("3", "alice", 34_560, ":partyparrot: :flag_af: :rt_custom_name: :thumbsup_tone2:"),
                msg(
                    "4",
                    "bob",
                    34_620,
                    "a long line with several emoji :rocket: :fire: :100: :tada: that has to wrap to the next row",
                ),
            ],
        );
        app.open_room(room);
        app
    }

    #[test]
    fn shortcodes_compact_52x20() {
        let app = app_with_shortcodes(wrap::MessageLayout::Compact);
        let terminal = render_pane(&app);
        // A wide glyph leaves a placeholder cell after it in the test buffer; squeeze it out.
        let text = all_text(&terminal).replace("  ", " ");
        assert!(text.contains("deploy done 🎉 thanks 👍"), "{text}");
        assert!(
            text.contains("10:100:20") && text.contains("`:smile:`"),
            "{text}"
        );
        assert!(
            text.contains("🦜") && text.contains("🇦🇫") && text.contains(":rt_custom_name:"),
            "{text}"
        );
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn shortcodes_two_line_52x20() {
        let app = app_with_shortcodes(wrap::MessageLayout::TwoLine);
        let terminal = render_pane(&app);
        assert!(
            all_text(&terminal)
                .replace("  ", " ")
                .contains("deploy done 🎉")
        );
        insta::assert_snapshot!(terminal.backend());
    }

    // --- message grouping -------------------------------------------------------------------

    const NOON: i64 = 1_767_268_800;

    fn gmsg(id: &str, author: &str, ts: i64) -> Message {
        Message {
            id: id.to_string(),
            room_id: RoomId("quiet".to_string()),
            author: author.to_string(),
            ts,
            body: format!("text {id}"),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
        }
    }

    /// `quiet` open in the two-line layout with a burst by bob (3 messages, a minute apart), the
    /// same author 20 minutes later, then alice.
    fn grouped_app() -> App {
        let mut app = App::with_fake_data(Size::new(80, 25));
        let room = RoomId("quiet".to_string());
        app.message_layout = crate::ui::wrap::MessageLayout::TwoLine;
        app.own_username = Some("bob".to_string());
        app.messages.insert(
            room.clone(),
            vec![
                gmsg("1", "bob", NOON),
                gmsg("2", "bob", NOON + 60),
                gmsg("3", "bob", NOON + 120),
                gmsg("4", "bob", NOON + 120 + 1200),
                gmsg("5", "alice", NOON + 120 + 1210),
            ],
        );
        app.open_room(room);
        app
    }

    fn count_in(lines: &[String], needle: &str) -> usize {
        lines.iter().filter(|l| l.contains(needle)).count()
    }

    #[test]
    fn a_burst_shares_one_header_and_a_gap_or_author_starts_another() {
        let app = grouped_app();
        let lines = pane_lines(&render_pane(&app));
        // Headers: bob (burst), bob (after the gap), alice. Every body is drawn.
        assert_eq!(count_in(&lines, "bob"), 2, "{lines:#?}");
        assert_eq!(count_in(&lines, "alice"), 1, "{lines:#?}");
        for id in 1..=5 {
            assert_eq!(count_in(&lines, &format!("text {id}")), 1, "{lines:#?}");
        }
        // The first header carries the first message's time; grouped bodies carry none.
        let first = lines
            .iter()
            .position(|l| l.contains("bob"))
            .expect("header");
        assert!(lines[first].ends_with("12:00"), "{lines:#?}");
        assert!(lines[first + 1].starts_with("text 1"), "{lines:#?}");
        assert!(lines[first + 2].starts_with("text 2"), "{lines:#?}");
        assert!(lines[first + 3].starts_with("text 3"), "{lines:#?}");
    }

    #[test]
    fn row_starts_agrees_with_the_rows_drawn() {
        let app = grouped_app();
        let room = RoomId("quiet".to_string());
        let starts = row_starts(&app, &room);
        let msgs = &app.messages[&room];
        let ctx = app.pane_row_ctx(&room, msgs);
        let (width, _) = crate::app::messages::viewport(&app);
        let joins = crate::ui::wrap::continuation_flags(msgs, &ctx);
        let mut drawn = 0;
        for (i, m) in msgs.iter().enumerate() {
            assert_eq!(starts[i], drawn, "message {i}");
            let prev = i.checked_sub(1).map(|p| &msgs[p]);
            drawn += crate::ui::wrap::block_lines(m, prev, joins[i], width, &ctx)
                .1
                .len();
        }
        assert_eq!(starts[msgs.len()], drawn);
        // Grouping saved the two headers of the burst.
        let off = {
            let mut app = grouped_app();
            app.settings.message_grouping_seconds = 0;
            row_starts(&app, &room)
        };
        assert_eq!(off[msgs.len()], drawn + 2);
    }

    #[test]
    fn the_cursor_steps_over_a_group_and_highlights_only_its_rows() {
        let mut app = grouped_app();
        let room = RoomId("quiet".to_string());
        press(&mut app, "3gg");
        assert_eq!(view(&app, &room).cursor, 2);
        press(&mut app, "k");
        assert_eq!(view(&app, &room).cursor, 1);
        let terminal = render_pane(&app);
        let buffer = terminal.backend().buffer();
        let reversed = |y: u16| buffer[(1, y)].modifier.contains(Modifier::REVERSED);
        let marked: Vec<String> = (1..buffer.area.height - 1)
            .filter(|&y| reversed(y))
            .map(|y| row_text(&terminal, y).trim().to_string())
            .collect();
        assert_eq!(marked.len(), 1, "{marked:?}");
        assert!(marked[0].contains("text 2"), "{marked:?}");
    }

    #[test]
    fn delete_acts_on_the_message_under_the_cursor_in_a_group() {
        let mut app = grouped_app();
        press(&mut app, "2gg");
        press(&mut app, "dd");
        match &app.confirm {
            Some(crate::app::edit::Confirm::Delete { id, .. }) => assert_eq!(id, "2"),
            other => panic!("no delete prompt: {:?}", other.is_some()),
        }
    }

    #[test]
    fn compact_layout_is_not_grouped() {
        let mut app = grouped_app();
        app.message_layout = crate::ui::wrap::MessageLayout::Compact;
        let lines = pane_lines(&render_pane(&app));
        assert_eq!(count_in(&lines, "bob"), 4, "{lines:#?}");
    }

    #[test]
    fn a_live_message_continues_the_newest_group() {
        let mut app = grouped_app();
        let room = RoomId("quiet".to_string());
        app.messages
            .get_mut(&room)
            .expect("room")
            .push(gmsg("6", "alice", NOON + 120 + 1220));
        let lines = pane_lines(&render_pane(&app));
        assert_eq!(count_in(&lines, "alice"), 1, "{lines:#?}");
        assert_eq!(count_in(&lines, "text 6"), 1, "{lines:#?}");
    }
}
