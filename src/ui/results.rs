//! The server search results list: a bordered, titled panel over the messages pane.
//!
//! The rows are the ordinary message rows of `wrap::render_rows`, so author, time and text look as
//! they do in the pane. The list is read-only and has no day separators or unread marker.

use crate::app::{App, results};
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
    let title = format!(" Search: {} (q to close) ", sanitize_line(&list.query));
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.results_border)
        .title(Span::styled(title, theme.results_title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let view = results::view(app);
    let ctx = app.row_ctx(&list.messages);
    let mut lines: Vec<Line> = Vec::new();
    let mut selected_rows = 0..0;
    for (i, msg) in list.messages.iter().enumerate() {
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
        let found = (0..n)
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
                result: Ok(found),
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
