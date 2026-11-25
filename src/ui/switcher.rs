//! The room switcher: a small bordered panel centred at the top of the screen, with a
//! search line and the ranked rooms under it.
//!
//! The content comes from `app::switcher::view` (the rooms, the selection, the query); this only
//! lays it out. Each row is the sidebar's own room line (`ui::sidebar::room_line`), so the kind
//! glyph, the unread marker, the count and the mention badge look the same in both places. The
//! window of rows scrolls so the selection stays visible. What does not fit is cut, never
//! overflowed. The hardware cursor sits after the typed text. The colours are the reaction
//! picker's (`picker_*` in theme.toml), so a theme styles both overlays alike.

use crate::app::App;
use crate::app::switcher;
use crate::ui::layout::Areas;
use crate::ui::sanitize::sanitize_line;
use crate::ui::sidebar::room_line;
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Rows of text the panel has, borders excluded: the search line and the rooms.
const MAX_INNER_HEIGHT: u16 = 12;

/// The widest the panel gets; on a narrower screen it takes the whole width.
const MAX_WIDTH: u16 = 60;

/// What the search line starts with.
const PROMPT: &str = "> ";

const NO_ROOMS: &str = "no rooms";
const NO_MATCHES: &str = "no matching rooms";

/// The panel and its inner area: centred across the sidebar and the messages column, starting one
/// row below the top. The height is the same whatever the list holds, so the search line stays
/// where it is while rooms come and go as the user types; it is the most that fits.
fn panel(areas: &Areas) -> (Rect, Rect) {
    let total = areas.sidebar.width + areas.messages.width;
    let width = total.min(MAX_WIDTH);
    let available = areas.messages.height.saturating_sub(1);
    let inner_height = MAX_INNER_HEIGHT.min(available.saturating_sub(2));
    let area = Rect {
        x: areas.sidebar.x + (total - width) / 2,
        y: areas.messages.y + 1,
        width,
        height: inner_height + 2,
    };
    let inner = Block::default().borders(Borders::ALL).inner(area);
    (area, inner)
}

/// First row shown: the selection is kept in the middle of the window when the list is longer than
/// it, so moving either way scrolls smoothly. Always in range.
fn window_start(selected: usize, visible: usize, total: usize) -> usize {
    if visible == 0 || total <= visible {
        return 0;
    }
    selected.saturating_sub(visible / 2).min(total - visible)
}

/// Where the hardware cursor goes: right after the typed text on the search line, kept inside the
/// panel. `None` when the switcher is closed or there is no room to draw it.
pub fn cursor_position(app: &App, areas: &Areas) -> Option<Position> {
    let view = switcher::view(app)?;
    let (_, inner) = panel(areas);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    let typed = sanitize_line(&view.query).width();
    let x = inner.x + u16::try_from(PROMPT.width() + typed).unwrap_or(u16::MAX);
    Some(Position::new(x.min(inner.x + inner.width - 1), inner.y))
}

/// Draw the switcher when it is open.
pub fn render(frame: &mut Frame, areas: &Areas, app: &App) {
    let Some(view) = switcher::view(app) else {
        return;
    };
    let theme = &app.theme;
    let (area, inner) = panel(areas);
    frame.render_widget(Clear, area);

    let title = " Switch room ";
    let hint = " C-j/C-k move  Enter open  Esc close ";
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.picker_border)
        .title(Span::styled(title, theme.picker_title));
    // The hint only when the border has room for it next to the title and the corners.
    if usize::from(area.width) >= title.width() + hint.width() + 4 {
        block =
            block.title_bottom(Line::from(Span::styled(hint, theme.picker_hint)).right_aligned());
    }
    frame.render_widget(block, area);

    let width = usize::from(inner.width);
    let visible = usize::from(inner.height).saturating_sub(1);
    let start = window_start(view.selected, visible, view.rooms.len());
    let shown = &view.rooms[start..(start + visible).min(view.rooms.len())];

    // The search line: the prompt and what was typed, cut to the width.
    let query = sanitize_line(&view.query).into_owned();
    let typed = fit_author(&query, width.saturating_sub(PROMPT.width())).into_owned();
    let mut lines: Vec<Line<'static>> = vec![Line::from(vec![
        Span::styled(PROMPT, theme.picker_item),
        Span::styled(typed, theme.picker_input),
    ])];

    if view.rooms.is_empty() {
        let text = if query.is_empty() {
            NO_ROOMS
        } else {
            NO_MATCHES
        };
        lines.push(Line::from(Span::styled(
            fit_author(text, width).into_owned(),
            theme.picker_hint,
        )));
    } else {
        for (offset, &index) in shown.iter().enumerate() {
            let selected = start + offset == view.selected;
            // One space of margin on each side; the room line cuts its own name.
            let line = room_line(app, &app.rooms[index], " ", width.saturating_sub(1));
            let mut spans: Vec<Span<'static>> = line.spans;
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < width {
                spans.push(Span::raw(" ".repeat(width - used)));
            }
            if selected {
                for span in &mut spans {
                    span.style = span.style.patch(theme.picker_selected);
                }
            }
            lines.push(Line::from(spans));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some(position) = cursor_position(app, areas) {
        frame.set_cursor_position(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Room, RoomId, RoomKind, Unread};
    use crate::test_util::{app_80x24, press, render_to_string};

    fn open(app: &mut App) {
        press(app, "<C-k>");
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, &c.to_string());
        }
    }

    fn many_rooms(app: &mut App, n: usize) {
        app.rooms = (0..n)
            .map(|i| Room {
                id: RoomId(format!("r{i}")),
                name: format!("room-{i:02}"),
                kind: RoomKind::Channel,
                encrypted: false,
                private: false,
                unread: Unread::default(),
                last_seen: None,
                last_activity: i64::try_from(n - i).unwrap_or(0),
            })
            .collect();
        app.folders.clear();
    }

    #[test]
    fn nothing_is_drawn_while_closed() {
        let app = app_80x24();
        assert!(!render_to_string(&app, 80, 24).contains("Switch room"));
    }

    #[test]
    fn switcher_empty_query_80x24() {
        let mut app = app_80x24();
        open(&mut app);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Switch room"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn switcher_query_80x24() {
        let mut app = app_80x24();
        open(&mut app);
        type_text(&mut app, "dev");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("dev-team"), "{out}");
        assert!(!out.contains("quiet ("), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn switcher_no_match_80x24() {
        let mut app = app_80x24();
        open(&mut app);
        type_text(&mut app, "zzzz");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("no matching rooms"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn no_rooms_says_so() {
        let mut app = app_80x24();
        app.rooms.clear();
        app.folders.clear();
        open(&mut app);
        assert!(render_to_string(&app, 80, 24).contains("no rooms"));
    }

    #[test]
    fn switcher_long_list_scrolled_80x24() {
        let mut app = app_80x24();
        many_rooms(&mut app, 40);
        open(&mut app);
        for _ in 0..20 {
            press(&mut app, "<C-n>");
        }
        let out = render_to_string(&app, 80, 24);
        // The selected room is on screen, with rooms before and after it.
        let view = crate::app::switcher::view(&app).expect("open");
        assert_eq!(view.selected, 20);
        let name = &app.rooms[view.rooms[20]].name;
        assert!(out.contains(name.as_str()), "{name} not visible: {out}");
        assert!(out.contains("room-18") && out.contains("room-22"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn a_room_shows_its_count_and_mention_badge_like_the_sidebar() {
        let mut app = app_80x24();
        many_rooms(&mut app, 3);
        app.rooms[1].unread = Unread {
            flagged: true,
            count: 250,
            mentions: 2,
        };
        open(&mut app);
        let out = render_to_string(&app, 80, 24);
        let row = out
            .lines()
            .find(|l| l.contains("Switch room") || l.contains("room-01 (99+) @2"))
            .unwrap_or_default();
        assert!(out.contains("room-01 (99+) @2"), "{out}\n{row}");
    }

    #[test]
    fn a_hostile_room_name_is_sanitized() {
        let mut app = app_80x24();
        many_rooms(&mut app, 2);
        app.rooms[0].name = "ev\u{1b}]0;pwn\u{7}il".to_string();
        open(&mut app);
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
    }

    #[test]
    fn the_selected_row_is_highlighted() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut app = app_80x24();
        many_rooms(&mut app, 3);
        open(&mut app);
        press(&mut app, "<C-n>");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend");
        terminal.draw(|f| crate::ui::view(f, &app)).expect("draw");
        let buffer = terminal.backend().buffer();
        let style_of = |needle: &str| {
            (0..buffer.area.height)
                .find_map(|y| {
                    let row: String = (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_string())
                        .collect();
                    let x = u16::try_from(row[..row.find(needle)?].chars().count()).ok()?;
                    Some(buffer[(x, y)].style())
                })
                .unwrap_or_else(|| panic!("{needle} not drawn"))
        };
        assert_ne!(style_of("room-00"), style_of("room-01"));
        assert_eq!(style_of("room-00"), style_of("room-02"));
    }

    #[test]
    fn the_cursor_sits_after_the_typed_text() {
        let mut app = app_80x24();
        open(&mut app);
        type_text(&mut app, "gen");
        let areas = crate::ui::layout::areas(
            Rect::new(0, 0, 80, 24),
            app.settings.sidebar_width,
            app.input_rows(),
        );
        let (_, inner) = panel(&areas);
        let position = cursor_position(&app, &areas).expect("open");
        assert_eq!(position, Position::new(inner.x + 2 + 3, inner.y));
    }

    #[test]
    fn the_panel_fits_a_small_terminal() {
        let mut app = app_80x24();
        open(&mut app);
        // 40x8 is the smallest laid-out size: nothing may panic.
        let out = render_to_string(&app, 40, 8);
        assert!(out.contains("Switch room"), "{out}");
    }
}
