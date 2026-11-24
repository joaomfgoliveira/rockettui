//! The server search results list: a bordered, titled panel over the messages pane.
//!
//! The rows are the ordinary message rows of `wrap::render_rows`, so author, time and text look as
//! they do in the pane, with a day divider above each new day. The list is read-only and has no
//! unread marker. While a result's context view is open (`app::jump`) the same panel shows that window
//! instead, titled `Context`.

use crate::app::{App, jump, results};
use crate::ui::sanitize::sanitize_line;
use crate::ui::wrap;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Draw the list over `area` when `app.results` is set.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(list) = app.results.as_ref() else {
        return;
    };
    let theme = &app.theme;
    // The context view of a result replaces the list's rows and title; the list stays underneath.
    let context = list.context.as_ref();
    let (messages, view, title) = match context {
        Some(ctx) => (
            ctx.messages.as_slice(),
            jump::view(app),
            " Context (Enter: open in chat, ?: keys, q: back) ".to_string(),
        ),
        None => (
            list.messages.as_slice(),
            results::view(app),
            format!(
                " Search: {} (Enter: open, ?: keys, q: close) ",
                sanitize_line(&list.query)
            ),
        ),
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.results_border)
        .title(Span::styled(title, theme.results_title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let ctx = app.row_ctx(messages);
    let mut lines: Vec<Line> = Vec::new();
    let mut selected_rows = 0..0;
    for (i, msg) in messages.iter().enumerate() {
        // The divider belongs to the message below it but is not part of its highlight.
        let prev = i.checked_sub(1).map(|p| &messages[p]);
        lines.extend(wrap::divider_row(msg, prev, inner.width, &ctx));
        let rows = wrap::render_rows(msg, inner.width, &ctx);
        if i == view.cursor {
            selected_rows = lines.len()..lines.len() + rows.len();
        }
        lines.extend(rows);
    }
    let window: Vec<Line> = lines
        .into_iter()
        .skip(view.top_row)
        .take(usize::from(inner.height))
        .collect();
    frame.render_widget(Paragraph::new(window), inner);
    let first = selected_rows.start.max(view.top_row);
    let end = selected_rows
        .end
        .min(view.top_row + usize::from(inner.height));
    for row in first..end {
        let y = inner.y + u16::try_from(row - view.top_row).unwrap_or(u16::MAX);
        frame.buffer_mut().set_style(
            Rect::new(inner.x, y, inner.width, 1),
            theme.results_selected,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::app::update::update;
    use crate::app::{App, StatusMsg};
    use crate::data::{Message, MessageKind, RoomId, SendState};
    use crate::test_util::{
        app_80x24, app_with_config, config_from_theme, press, render_buffer, render_to_string,
    };
    use ratatui::style::Color;

    fn open(app: &mut App, n: usize) {
        let room = app
            .rooms
            .iter()
            .find(|r| !r.encrypted)
            .expect("room")
            .id
            .clone();
        app.open_room(room.clone());
        crate::app::command::execute(app, "search dep\u{1b}loy");
        let id = app.search_seq;
        let found: Vec<Message> = (0..n)
            .map(|i| Message {
                edited: false,
                plain_text: true,
                id: format!("s{i}"),
                room_id: RoomId(room.0.clone()),
                author: "alice".to_string(),
                ts: 1_700_000_000 - 3600 * i as i64,
                body: format!("hit number {i}"),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            })
            .collect();
        update(
            app,
            Action::SearchPage {
                room,
                search_id: id,
                offset: 0,
                result: Ok(found.into()),
            },
        );
        assert!(!matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    #[test]
    fn results_80x24() {
        let mut app = app_80x24();
        open(&mut app, 5);
        let out = render_to_string(&app, 80, 24);
        assert!(
            out.contains("Search: deploy"),
            "control chars are stripped: {out}"
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn results_scrolled_smallest() {
        let mut app = app_80x24();
        app.size = ratatui::layout::Size::new(80, 10);
        open(&mut app, 12);
        press(&mut app, "G");
        let out = render_to_string(&app, 80, 10);
        assert!(out.contains("hit number 11"), "{out}");
        assert!(!out.contains("hit number 0"), "{out}");
        insta::assert_snapshot!(out);
    }

    /// 2023-11-14 22:13:20 UTC, the time of the first result `open` builds.
    const FIRST_TS: i64 = 1_700_000_000;

    #[test]
    fn rows_show_only_the_time() {
        let mut app = app_80x24();
        open(&mut app, 1);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("22:13 alice"), "{out}");
        assert!(!out.contains("2023-11-14 22:13"), "{out}");
    }

    #[test]
    fn divider_above_each_new_day() {
        let mut app = app_80x24();
        open(&mut app, 3);
        // Newest first: two on 2023-11-14 and one 43 days earlier.
        app.results.as_mut().expect("results").messages[2].ts = FIRST_TS - 86_400 * 43;
        let out = render_to_string(&app, 80, 24);
        let lines: Vec<&str> = out.lines().collect();
        let at = |needle: &str| lines.iter().position(|l| l.contains(needle)).expect(needle);
        assert!(at("── 2023-11-14") < at("hit number 0"), "{out}");
        assert_eq!(out.matches("── 2023-").count(), 2, "{out}");
        assert!(at("hit number 1") < at("── 2023-10-02"), "{out}");
        assert!(at("── 2023-10-02") < at("hit number 2"), "{out}");
    }

    #[test]
    fn divider_is_not_selected_and_scrolling_agrees() {
        let mut app = app_80x24();
        open(&mut app, 3);
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let divider = out
            .lines()
            .position(|l| l.contains("── 2023-"))
            .expect("divider");
        let hit = out
            .lines()
            .position(|l| l.contains("hit number 0"))
            .expect("hit");
        assert_eq!(hit, divider + 1, "{out}");
        assert_ne!(
            buffer[(30, divider as u16)].modifier,
            buffer[(30, hit as u16)].modifier
        );
    }

    fn open_context(app: &mut App, count: usize, selected: usize) {
        let room = app.results.as_ref().expect("list").room.clone();
        let list = app.results.as_mut().expect("list");
        list.raw_ts = vec![Some("raw".to_string()); list.messages.len()];
        press(app, "<CR>");
        let window = crate::api::ContextWindow {
            messages: (0..count)
                .map(|i| Message {
                    edited: false,
                    plain_text: true,
                    id: format!("c{i}"),
                    room_id: room.clone(),
                    author: "bob".to_string(),
                    ts: FIRST_TS - 3600 * (count - i) as i64,
                    body: format!("context line {i}"),
                    kind: MessageKind::Normal,
                    send: SendState::Sent,
                })
                .collect(),
            selected,
            newer_missing: false,
            oldest_raw: None,
        };
        let id = app.context_seq;
        update(
            app,
            Action::ContextLoaded {
                room,
                context_id: id,
                window,
            },
        );
    }

    #[test]
    fn context_view_80x24() {
        let mut app = app_80x24();
        open(&mut app, 3);
        open_context(&mut app, 5, 2);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Context"), "{out}");
        assert!(
            out.contains("context line 2") && out.contains("context line 4"),
            "{out}"
        );
        assert!(!out.contains("hit number"), "the list is underneath: {out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn context_view_highlights_the_result_and_smallest_size_draws() {
        let mut app = app_80x24();
        open(&mut app, 3);
        open_context(&mut app, 5, 2);
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let row = out
            .lines()
            .position(|l| l.contains("context line 2"))
            .expect("result row");
        let other = out
            .lines()
            .position(|l| l.contains("context line 3"))
            .expect("next row");
        assert_ne!(
            buffer[(60, row as u16)].modifier,
            buffer[(60, other as u16)].modifier
        );
        app.size = ratatui::layout::Size::new(40, 8);
        let small = render_to_string(&app, 40, 8);
        assert!(small.contains("Context"), "{small}");
        insta::assert_snapshot!("context_view_40x8", small);
    }

    #[test]
    fn question_mark_lists_the_keys_of_the_list_and_of_the_context_view() {
        let mut app = app_80x24();
        open(&mut app, 3);
        press(&mut app, "?");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Search results"), "{out}");
        assert!(out.contains("open the result in context"), "{out}");
        assert!(out.contains("q Esc"), "{out}");
        assert_eq!(app.mode, crate::app::Mode::Normal);
        // Any key closes it and does not move the selection.
        press(&mut app, "j");
        assert!(!app.context);
        assert_eq!(app.results.as_ref().expect("list").selected, 0);
        assert!(!render_to_string(&app, 80, 24).contains("Search results"));

        open_context(&mut app, 4, 1);
        press(&mut app, "?");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Search result in context"), "{out}");
        assert!(out.contains("open in the chat"), "{out}");
        assert!(out.contains("back to the results"), "{out}");
        insta::assert_snapshot!("context_view_help_80x24", out);
    }

    #[test]
    fn titles_hint_at_the_keys() {
        let mut app = app_80x24();
        open(&mut app, 2);
        assert!(render_to_string(&app, 80, 24).contains("?: keys"));
        open_context(&mut app, 3, 1);
        assert!(render_to_string(&app, 80, 24).contains("?: keys"));
    }

    #[test]
    fn closed_draws_nothing() {
        let app = app_80x24();
        assert!(!render_to_string(&app, 80, 24).contains("Search:"));
    }

    #[test]
    fn selected_row_is_themed() {
        let mut app = app_with_config(
            config_from_theme("[results.selected]\nbg = \"#334455\"\n"),
            80,
            24,
        );
        open(&mut app, 3);
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let row = out
            .lines()
            .position(|l| l.contains("hit number 0"))
            .expect("first result row");
        let hit = out.lines().nth(row).expect("row");
        let x = hit[..hit.find("hit").expect("hit")].chars().count() as u16;
        assert_eq!(buffer[(x, row as u16)].bg, Color::Rgb(0x33, 0x44, 0x55));
        let other = out
            .lines()
            .position(|l| l.contains("hit number 1"))
            .expect("second row");
        assert_ne!(buffer[(x, other as u16)].bg, Color::Rgb(0x33, 0x44, 0x55));
    }
}
