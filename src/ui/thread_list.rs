//! The thread list pane: a bordered pane in the thread column, with a search line and one row per
//! thread of the open room.
//!
//! The content is `app::thread_list::ThreadList`; this only lays it out. A row is three lines: the
//! parent's author with the date and hour of the last reply at the right, the first line of the
//! parent's text cut to fit, and `following` (when the user follows the thread) with the reply
//! count at the right. Rows are told apart by a divider line; the author line is bold in the
//! author's colour, the last line dimmed, and the selected row has the cursor highlight of the
//! message panes. All server text is sanitized to one line before it is measured. The colours are
//! the picker's (`picker_*` in theme.toml).

use crate::api::{ThreadFilter, ThreadRow};
use crate::app::thread_list::{ListLoad, ThreadList};
use crate::app::{App, Focus};
use crate::ui::palette::author_color;
use crate::ui::sanitize::sanitize_line;
use crate::ui::switcher::window_start;
use crate::ui::timefmt::format_date_hour;
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

const PROMPT: &str = "> ";

/// Shown at the right of the search line while it is idle: `h` and `l` change the filter.
const FILTER_HINT: &str = "h l filter ";

/// The filter's name in the title.
fn filter_name(filter: ThreadFilter) -> &'static str {
    match filter {
        ThreadFilter::All => "all",
        ThreadFilter::Following => "following",
        ThreadFilter::Unread => "unread",
    }
}

/// `1 reply` or `N replies`.
fn replies_label(n: u32) -> String {
    if n == 1 {
        "1 reply".to_string()
    } else {
        format!("{n} replies")
    }
}

/// The date and hour of the last reply.
fn when(app: &App, row: &ThreadRow) -> String {
    row.last_reply
        .map_or_else(String::new, |ts| format_date_hour(ts, app.zone))
}

/// `left` and `right` on one line exactly `width` cells wide, a margin each side. The right part is
/// never cut; the left one gives way.
fn spread(left: &str, right: &str, width: usize) -> String {
    let left = fit_author(left, width.saturating_sub(right.width() + 3)).into_owned();
    let gap = width.saturating_sub(1 + left.width() + right.width() + 1);
    let line = format!(" {left}{}{right} ", " ".repeat(gap));
    // A very narrow pane: whatever still overflows is cut, never drawn past the border.
    fit_author(&line, width).into_owned()
}

/// One row, three lines exactly `width` cells wide: the author and the date and hour of the last
/// reply, the first line of the parent's text, and `following` (when the user follows the thread)
/// with the reply count.
fn row_lines(app: &App, row: &ThreadRow, width: usize) -> [String; 3] {
    let author = sanitize_line(&row.author);
    let text = sanitize_line(&row.first_line);
    let followed = app
        .own_user_id
        .as_deref()
        .is_some_and(|id| row.followed_by(id));
    [
        spread(&author, &when(app, row), width),
        fit_author(&format!(" {text}"), width).into_owned(),
        spread(
            if followed { "following" } else { "" },
            &replies_label(row.replies),
            width,
        ),
    ]
}

/// The area inside the pane's border.
fn inner_of(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

/// Where the hardware cursor goes: after the typed text on the search line, only while that line
/// is open. `None` otherwise or when there is no room to draw it.
pub fn cursor_position(list: &ThreadList, area: Rect) -> Option<Position> {
    let inner = inner_of(area);
    if !list.searching || inner.width == 0 || inner.height == 0 {
        return None;
    }
    let typed = sanitize_line(&list.query).width();
    let x = inner.x + u16::try_from(PROMPT.width() + typed).unwrap_or(u16::MAX);
    Some(Position::new(x.min(inner.x + inner.width - 1), inner.y))
}

/// Draw the thread list into the thread column `area`. Nothing is drawn while it is not shown.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(list) = app.thread_list.as_ref().filter(|_| app.thread_list_shown()) else {
        return;
    };
    let theme = &app.theme;
    let title = format!(" Threads: {} ", filter_name(list.filter));
    let block = crate::ui::pane_block(&title, app.focus == Focus::ThreadList, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = usize::from(inner.width);
    // Lines under the search line; a row is two of them.
    let mut visible = usize::from(inner.height).saturating_sub(1);
    // A failed page takes the last line, so the rows above it stay readable.
    let more_failed = list.more_failed.filter(|_| visible > 1);
    if more_failed.is_some() {
        visible -= 1;
    }

    let query = sanitize_line(&list.query).into_owned();
    let note = |text: String| {
        Line::from(Span::styled(
            fit_author(&sanitize_line(&text), width).into_owned(),
            theme.picker_hint,
        ))
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_lines = 0..0;
    if list.searching || !query.is_empty() {
        let typed = fit_author(&query, width.saturating_sub(PROMPT.width())).into_owned();
        lines.push(Line::from(vec![
            Span::styled(PROMPT, theme.picker_item),
            Span::styled(typed, theme.picker_input),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" / search", theme.picker_hint),
            Span::styled(
                format!(
                    "{:>pad$}",
                    FILTER_HINT,
                    pad = width.saturating_sub(" / search".width())
                ),
                theme.picker_hint,
            ),
        ]));
    }

    match list.load {
        ListLoad::Loading => lines.push(note("Loading…".to_string())),
        ListLoad::Failed(error) => lines.push(note(format!("Could not load threads: {error}"))),
        ListLoad::Loaded if list.rows.is_empty() => lines.push(note(
            if query.is_empty() {
                "No threads"
            } else {
                "No matching threads"
            }
            .to_string(),
        )),
        ListLoad::Loaded => {
            // A row is three lines and a divider between rows: n rows take 4n - 1 lines.
            let shown = ((visible + 1) / 4).max(1);
            let start = window_start(list.selected, shown, list.rows.len());
            let end = (start + shown).min(list.rows.len());
            for (index, row) in list.rows.iter().enumerate().take(end).skip(start) {
                if index > start {
                    lines.push(Line::from(Span::styled(
                        "─".repeat(width),
                        theme.picker_hint,
                    )));
                }
                if index == list.selected {
                    selected_lines = lines.len()..lines.len() + 3;
                }
                // The author line is bold in the author's colour, as in the message pane; the
                // last line is dimmed. The selected row is highlighted below like the cursor
                // message of a pane.
                let color = author_color(&sanitize_line(&row.author), &theme.author_palette);
                let [head, text, meta] = row_lines(app, row, width);
                lines.push(Line::from(Span::styled(
                    head,
                    theme.picker_item.fg(color).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(Span::styled(text, theme.picker_item)));
                lines.push(Line::from(Span::styled(
                    meta,
                    theme.picker_item.add_modifier(Modifier::DIM),
                )));
            }
        }
    }
    if let Some(error) = more_failed {
        // Pinned to the last line of the pane.
        let blank = usize::from(inner.height).saturating_sub(lines.len() + 1);
        lines.extend(std::iter::repeat_n(Line::default(), blank));
        lines.push(note(format!("Could not load more: {error}")));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    // The selected row gets the highlight of the cursor message, focused or not.
    let highlight = if app.focus == Focus::ThreadList {
        theme.messages_cursor
    } else {
        theme.messages_cursor_unfocused
    };
    for row in selected_lines {
        let y = inner.y + u16::try_from(row).unwrap_or(u16::MAX);
        if y < inner.bottom() {
            frame
                .buffer_mut()
                .set_style(Rect::new(inner.x, y, inner.width, 1), highlight);
        }
    }
    if app.focus == Focus::ThreadList
        && let Some(position) = cursor_position(list, area)
    {
        frame.set_cursor_position(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiError;
    use crate::app::thread_list::ThreadList;
    use crate::data::RoomId;
    use crate::test_util::{app_80x24, press, render_to_string};
    use crate::ui::timefmt::Zone;

    fn row(id: &str, author: &str, text: &str, replies: u32, followed: bool) -> ThreadRow {
        ThreadRow {
            id: id.to_string(),
            author: author.to_string(),
            first_line: text.to_string(),
            replies,
            last_reply: Some(1_794_614_400 + 14 * 3600 + 120),
            followers: if followed {
                vec!["me-id".to_string()]
            } else {
                Vec::new()
            },
        }
    }

    fn opened(rows: Vec<ThreadRow>, load: ListLoad) -> App {
        let mut app = app_80x24();
        app.zone = Zone::Utc;
        app.own_user_id = Some("me-id".to_string());
        let total = u32::try_from(rows.len()).unwrap_or(0);
        app.thread_list = Some(ThreadList {
            room: RoomId("r".to_string()),
            filter: ThreadFilter::All,
            query: String::new(),
            rows,
            selected: 0,
            load,
            total,
            generation: 1,
            search_pending: false,
            loading_more: false,
            more_failed: None,
            choosing: None,
            searching: false,
            opened_from: Focus::Messages,
        });
        app.focus = Focus::ThreadList;
        app
    }

    fn sample() -> Vec<ThreadRow> {
        vec![
            row("t1", "ana", "deploy plan for the week", 3, true),
            row("t2", "bob", "lunch?", 1, false),
            row(
                "t3",
                "carla",
                "a very long first line that cannot possibly fit in the row of this panel at all",
                12,
                false,
            ),
        ]
    }

    #[test]
    fn nothing_is_drawn_while_closed() {
        let app = app_80x24();
        assert!(!render_to_string(&app, 80, 24).contains("Threads:"));
    }

    #[test]
    fn rows_80x24() {
        let app = opened(sample(), ListLoad::Loaded);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Threads: all"), "{out}");
        assert!(
            out.contains("3 replies") && out.contains("1 reply"),
            "{out}"
        );
        assert!(out.contains("following"), "{out}");
        // The mode badge keeps showing the real mode.
        assert!(out.contains("NORMAL"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn rows_160x40() {
        let mut app = opened(sample(), ListLoad::Loaded);
        app.thread_list.as_mut().expect("open").filter = ThreadFilter::Following;
        let out = render_to_string(&app, 160, 40);
        assert!(out.contains("Threads: following"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn a_long_text_is_cut_and_the_counts_stay() {
        let app = opened(sample(), ListLoad::Loaded);
        let out = render_to_string(&app, 80, 24);
        let head = out.lines().find(|l| l.contains("carla")).expect("row");
        assert!(head.contains("2026-11-14 14:02"), "{head}");
        let text = out
            .lines()
            .find(|l| l.contains("a very long"))
            .expect("text");
        assert!(text.contains('…'), "{text}");
        assert!(out.lines().any(|l| l.contains("12 replies")), "{out}");
    }

    #[test]
    fn loading_empty_and_failed_lines() {
        let out = render_to_string(&opened(vec![], ListLoad::Loading), 80, 24);
        assert!(out.contains("Loading…"), "{out}");
        let out = render_to_string(&opened(vec![], ListLoad::Loaded), 80, 24);
        assert!(out.contains("No threads"), "{out}");
        let mut app = opened(vec![], ListLoad::Loaded);
        app.thread_list.as_mut().expect("open").query = "zzz".to_string();
        assert!(render_to_string(&app, 80, 24).contains("No matching threads"));
        let app = opened(vec![], ListLoad::Failed(ApiError::Rejected));
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Could not load threads"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn a_failed_page_keeps_the_rows_and_says_so() {
        let mut app = opened(sample(), ListLoad::Loaded);
        app.thread_list.as_mut().expect("open").more_failed = Some(ApiError::Server(503));
        let out = render_to_string(&app, 80, 24);
        assert!(
            out.contains("ana") && out.contains("Could not load more"),
            "{out}"
        );
    }

    #[test]
    fn the_typed_text_shows_and_hostile_server_text_is_one_clean_line() {
        let mut app = opened(
            vec![row("t1", "an\u{1b}[2Ja", "first\u{7}\nsecond", 2, false)],
            ListLoad::Loaded,
        );
        app.thread_list.as_mut().expect("open").query = "dep".to_string();
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("> dep"), "{out}");
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
    }

    #[test]
    fn the_pane_opens_from_the_key_and_the_badge_stays() {
        let mut app = app_80x24();
        press(&mut app, "<CR>");
        press(&mut app, "g");
        press(&mut app, "T");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Threads: all"), "{out}");
        assert!(out.contains("NORMAL"), "{out}");
        assert_eq!(app.focus, Focus::ThreadList);
        press(&mut app, "/");
        assert!(render_to_string(&app, 80, 24).contains("NORMAL"));
    }

    #[test]
    fn wide_terminal_shows_the_messages_beside_the_list() {
        let mut app = app_80x24();
        press(&mut app, "<CR>");
        press(&mut app, "gT");
        let out = render_to_string(&app, 160, 40);
        assert!(out.contains("Threads: all"), "{out}");
        // The message pane keeps its title beside the list, and the room's input bar stays.
        assert!(out.contains("┌ general "), "{out}");
        assert!(out.contains("Input"), "{out}");
    }

    #[test]
    fn narrow_terminal_takes_the_messages_place_and_closing_restores_them() {
        let mut app = app_80x24();
        press(&mut app, "<CR>");
        let before = render_to_string(&app, 80, 24);
        press(&mut app, "gT");
        let open = render_to_string(&app, 80, 24);
        assert!(open.contains("Threads: all"), "{open}");
        assert_ne!(before, open);
        press(&mut app, "q");
        assert_eq!(render_to_string(&app, 80, 24), before, "restored unchanged");
    }

    #[test]
    fn the_search_line_shows_while_searching_and_the_focused_border_is_thick() {
        let mut app = opened(sample(), ListLoad::Loaded);
        let idle = render_to_string(&app, 80, 24);
        assert!(idle.contains("/ search"), "{idle}");
        app.thread_list.as_mut().expect("open").searching = true;
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("> "), "{out}");
        // Focused: the thick border corner of the pane block.
        assert!(idle.contains('┏'), "{idle}");
        app.focus = Focus::Messages;
        assert!(!render_to_string(&app, 80, 24).contains("┏ Threads"));
    }
}
