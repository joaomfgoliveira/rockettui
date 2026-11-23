//! The context popup: a small bordered panel over the bottom of the messages and input column,
//! above the status row.
//!
//! The content comes from `app::context::lines`; this only lays it out. Text goes through the
//! sanitizer because the key column comes from user configuration.

use crate::app::App;
use crate::app::context::{self, ContextLine};
use crate::ui::layout::Areas;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Most rows of text the popup grows to, borders excluded.
const MAX_INNER_HEIGHT: u16 = 8;

/// Draw the popup when `app.context` is set.
pub fn render(frame: &mut Frame, areas: &Areas, app: &App) {
    if !app.context {
        return;
    }
    let ctx = context::lines(app);
    let theme = &app.theme;

    // The name line, the action rows, and the way to the full help last.
    let wanted = ctx.rows.len() + 2;
    let column_bottom = areas.input.y + areas.input.height;
    let available = column_bottom.saturating_sub(areas.messages.y);
    let inner_height = u16::try_from(wanted)
        .unwrap_or(u16::MAX)
        .min(MAX_INNER_HEIGHT)
        .min(available.saturating_sub(2));
    let height = inner_height + 2;
    let area = Rect {
        x: areas.messages.x,
        y: column_bottom.saturating_sub(height),
        width: areas.messages.width,
        height,
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.context_border)
        .title(Span::styled(" ? ", theme.context_title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Clipping keeps the name, then the full-help row, then as many action rows as still fit.
    let slots = usize::from(inner.height);
    let room_for_rows = slots.saturating_sub(2);
    let shown: Vec<&ContextLine> = ctx.rows.iter().take(room_for_rows).collect();
    let key_width = shown
        .iter()
        .copied()
        .chain(std::iter::once(&ctx.full_help))
        .map(|r| sanitize_line(&r.keys).width())
        .max()
        .unwrap_or(0);
    let row = |r: &ContextLine| {
        let keys = sanitize_line(&r.keys);
        let pad = key_width.saturating_sub(keys.width());
        Line::from(vec![
            Span::styled(format!("{keys}{}", " ".repeat(pad)), theme.context_key),
            Span::styled(format!("  {}", sanitize_line(&r.desc)), theme.context_desc),
        ])
    };
    let mut lines = vec![Line::from(Span::styled(
        ctx.kind.title().to_string(),
        theme.context_title,
    ))];
    lines.extend(shown.into_iter().map(row));
    if slots >= 2 {
        lines.push(row(&ctx.full_help));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use crate::test_util::{
        app_80x24, app_with_config, config_from_theme, press, render_buffer, render_to_string,
    };
    use ratatui::style::Color;

    #[test]
    fn context_room_80x24() {
        let mut app = app_80x24();
        press(&mut app, "?");
        insta::assert_snapshot!(render_to_string(&app, 80, 24));
    }

    #[test]
    fn context_folder_80x24() {
        let mut app = app_80x24();
        let mut n = 0;
        while !matches!(
            crate::app::sidebar::selected_row(&app),
            Some(crate::app::sidebar::SidebarRow::Folder { .. })
        ) {
            press(&mut app, "j");
            n += 1;
            assert!(n < 50, "no folder row in the fixture");
        }
        press(&mut app, "?");
        insta::assert_snapshot!(render_to_string(&app, 80, 24));
    }

    #[test]
    fn context_own_message_80x24() {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let mut m = crate::test_util::sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "me".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = crate::app::Focus::Messages;
        press(&mut app, "?");
        insta::assert_snapshot!(render_to_string(&app, 80, 24));
    }

    #[test]
    fn context_smallest_size() {
        let mut app = app_80x24();
        app.size = ratatui::layout::Size::new(40, 8);
        press(&mut app, "?");
        let out = render_to_string(&app, 40, 8);
        assert!(out.contains("Room"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn closed_draws_nothing() {
        let app = app_80x24();
        assert!(!render_to_string(&app, 80, 24).contains("open the room"));
    }

    #[test]
    fn key_column_is_themed() {
        let mut app = app_with_config(config_from_theme("[context.key]\nfg = 5\n"), 80, 24);
        press(&mut app, "?");
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let (row, line) = out
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("all keys"))
            .expect("full help row");
        let col = line.find("all keys").unwrap();
        let x = line[..col].chars().count();
        // The key sits two columns before the description.
        assert_eq!(buffer[((x - 3) as u16, row as u16)].fg, Color::Indexed(5));
    }
}
