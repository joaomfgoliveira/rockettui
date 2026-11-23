//! The help overlay: a bordered, titled panel drawn over the messages pane.
//!
//! The lines come from `app::help::build`; this only lays them out and applies the scroll offset.
//! Text goes through the sanitizer even though it is built in, because `cmd:` bindings are user
//! configuration.

use crate::app::App;
use crate::app::help::{self, HelpLine};
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Draw the overlay over `area` when `app.help` is set.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(offset) = app.help else {
        return;
    };
    let theme = &app.theme;
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.help_border)
        .title(Span::styled(" Help (q to close) ", theme.help_title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = help::build(&app.keymap);
    let key_width = lines
        .iter()
        .filter_map(|l| match l {
            HelpLine::Row { keys, .. } => Some(sanitize_line(keys).width()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let height = usize::from(inner.height);
    let start = offset.min(help::max_scroll(lines.len(), height));
    let visible: Vec<Line> = lines
        .iter()
        .skip(start)
        .take(height)
        .map(|line| match line {
            HelpLine::Heading(text) => Line::from(Span::styled(
                sanitize_line(text).into_owned(),
                theme.help_heading,
            )),
            HelpLine::Row { keys, desc } => {
                let keys = sanitize_line(keys);
                let pad = key_width.saturating_sub(keys.width());
                Line::from(vec![
                    Span::styled(format!("{keys}{}", " ".repeat(pad)), theme.help_key),
                    Span::styled(format!("  {}", sanitize_line(desc)), theme.help_desc),
                ])
            }
            HelpLine::Blank => Line::default(),
        })
        .collect();
    frame.render_widget(Paragraph::new(visible), inner);
}

#[cfg(test)]
mod tests {
    use crate::test_util::render_to_string;
    use crate::test_util::{app_80x24, app_with_config, config_from_theme, press, render_buffer};
    use ratatui::style::Color;

    #[test]
    fn help_80x24() {
        let mut app = app_80x24();
        press(&mut app, "??");
        insta::assert_snapshot!(render_to_string(&app, 80, 24));
    }

    #[test]
    fn help_smallest_size() {
        let mut app = app_80x24();
        app.size = ratatui::layout::Size::new(40, 8);
        press(&mut app, "??");
        insta::assert_snapshot!(render_to_string(&app, 40, 8));
    }

    #[test]
    fn help_scrolled_to_end() {
        let mut app = app_80x24();
        press(&mut app, "??G");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains(":help"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn help_closed_draws_nothing() {
        let app = app_80x24();
        assert!(!render_to_string(&app, 80, 24).contains("Help"));
    }

    #[test]
    fn heading_is_themed() {
        let mut app = app_with_config(config_from_theme("[help.heading]\nfg = 5\n"), 80, 24);
        press(&mut app, "??");
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let row = out
            .lines()
            .position(|l| l.contains("Normal mode"))
            .expect("heading row");
        let col = out
            .lines()
            .nth(row)
            .and_then(|l| l.find("Normal"))
            .expect("col");
        let x = out
            .lines()
            .nth(row)
            .map(|l| l[..col].chars().count())
            .unwrap();
        assert_eq!(buffer[(x as u16, row as u16)].fg, Color::Indexed(5));
    }
}
