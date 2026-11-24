//! Status bar (mode badge, current room, connection state, persistent config warnings, pending keys and status message) and the command-line row with its cursor.

use crate::app::{App, ConnState, Mode, RtState, StatusMsg, edit};
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

pub fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let badge = Span::styled(format!(" {} ", app.mode.label()), app.theme.status_mode);
    // A confirmation prompt takes the whole row, so it is readable at any width. It is drawn from
    // `app.confirm`, not from `status_msg`, so the status line clearing of a key cannot erase it.
    if app.confirm.is_some() {
        let prompt = Span::styled(format!(" {}", edit::DELETE_PROMPT), app.theme.status_info);
        frame.render_widget(Paragraph::new(Line::from(vec![badge, prompt])), area);
        return;
    }
    let room = match app.current_room_name() {
        Some(name) => sanitize_line(name).into_owned(),
        None => "-".to_string(),
    };
    let mut spans = vec![
        badge,
        Span::styled(format!(" {room}"), app.theme.status_room),
    ];
    match app.conn {
        ConnState::Connecting => {
            spans.push(Span::styled(" Connecting…", app.theme.status_info));
        }
        ConnState::Failed(err) => spans.push(Span::styled(
            format!(" {}", sanitize_line(&err.to_string())),
            app.theme.status_error,
        )),
        ConnState::Idle | ConnState::Ready => {}
    }
    // Config warnings (D-04) stay on every frame, independent of `status_msg`.
    for warning in &app.warnings {
        spans.push(Span::styled(
            format!(" warning: {}", sanitize_line(warning)),
            app.theme.status_error,
        ));
    }
    // The realtime connection: shown only while it is down or refused, cleared when it is live.
    // After the config warnings, which stay visible whatever the connection does.
    match app.rt {
        RtState::Reconnecting => {
            spans.push(Span::styled(" Reconnecting…", app.theme.status_info));
        }
        RtState::Unavailable => spans.push(Span::styled(
            " live updates unavailable",
            app.theme.status_error,
        )),
        RtState::Off | RtState::Live => {}
    }
    let pending = app.pending_display();
    if !pending.is_empty() {
        spans.push(Span::styled(
            format!(" {}", sanitize_line(&pending)),
            app.theme.status_pending,
        ));
    }
    match &app.status_msg {
        Some(StatusMsg::Info(text)) => spans.push(Span::styled(
            format!(" {}", sanitize_line(text)),
            app.theme.status_info,
        )),
        Some(StatusMsg::Error(text)) => spans.push(Span::styled(
            format!(" {}", sanitize_line(text)),
            app.theme.status_error,
        )),
        None => {}
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// What the command row shows: `:` and the command line in Command mode, `/` and the query while
/// the search prompt is open, otherwise nothing. The text is sanitized to one row.
fn cmdline_text(app: &App) -> Option<String> {
    if let Some(query) = &app.search_prompt {
        Some(format!("/{}", sanitize_line(query)))
    } else if app.mode == Mode::Command {
        Some(format!(":{}", sanitize_line(&app.cmdline)))
    } else {
        None
    }
}

/// Where the hardware cursor goes: right after the `:` or `/` and the typed text, while the
/// command line or the search prompt is open.
pub fn cmdline_cursor(app: &App, area: Rect) -> Option<Position> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let text = cmdline_text(app)?;
    let text_width = u16::try_from(text.width()).unwrap_or(u16::MAX);
    let x = area
        .x
        .saturating_add(text_width)
        .min(area.right().saturating_sub(1));
    Some(Position::new(x, area.y))
}

pub fn render_cmdline(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(text) = cmdline_text(app) {
        frame.render_widget(
            Paragraph::new(Line::styled(text, app.theme.cmdline_text)),
            area,
        );
    }
    if let Some(pos) = cmdline_cursor(app, area) {
        frame.set_cursor_position(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{app_with_config, config_from_theme};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    #[test]
    fn themed_error_style() {
        let config = config_from_theme("[status.error]\nfg = 3\n");
        let mut app = app_with_config(config, 80, 24);
        app.status_msg = Some(StatusMsg::Error("boom".to_string()));
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("backend");
        terminal
            .draw(|frame| render_status(frame, frame.area(), &app))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        let at = row.find("boom").expect("error text");
        for x in at..at + 4 {
            let x = u16::try_from(x).expect("x");
            assert_eq!(buffer[(x, 0)].fg, Color::Indexed(3), "cell {x}");
        }

        // Positive control: the default error text is red.
        let mut app = app_with_config(crate::config::Config::default(), 80, 24);
        app.status_msg = Some(StatusMsg::Error("boom".to_string()));
        terminal
            .draw(|frame| render_status(frame, frame.area(), &app))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        let at = u16::try_from(row.find("boom").expect("error text")).expect("x");
        assert_eq!(buffer[(at, 0)].fg, Color::Red);
    }

    #[test]
    fn themed_cmdline_and_room() {
        let config = config_from_theme("[cmdline.text]\nfg = 6\n[status.room]\nfg = 7\n");
        let mut app = app_with_config(config, 80, 24);
        crate::test_util::press(&mut app, "<CR>:");
        crate::test_util::press(&mut app, "qq");
        let buffer = crate::test_util::render_buffer(&app, 80, 24);
        // The command line is the last row: ":qq".
        for x in 0..3 {
            assert_eq!(buffer[(x, 23)].fg, Color::Indexed(6), "cmdline cell {x}");
        }
        // The room name sits after the badge on the status row.
        let row: String = (0..80).map(|x| buffer[(x, 22)].symbol()).collect();
        let at = u16::try_from(row.find("general").expect("room")).expect("x");
        assert_eq!(buffer[(at, 22)].fg, Color::Indexed(7));
    }

    #[test]
    fn search_prompt_on_the_command_row() {
        let config = config_from_theme("[cmdline.text]\nfg = 6\n");
        let mut app = app_with_config(config, 80, 24);
        // Open the first room, then the prompt; hostile text can only arrive by paste.
        crate::test_util::press(&mut app, "<CR>/");
        crate::app::update::update(
            &mut app,
            crate::action::Action::Paste("x\u{1b}[31my\nz".to_string()),
        );
        let buffer = crate::test_util::render_buffer(&app, 80, 24);
        let row: String = (0..80).map(|x| buffer[(x, 23)].symbol()).collect();
        assert!(row.starts_with("/x[31my z"), "{row}");
        assert!(!row.contains('\u{1b}'));
        assert_eq!(buffer[(0, 23)].fg, Color::Indexed(6));
        let area = Rect::new(0, 23, 80, 1);
        assert_eq!(cmdline_cursor(&app, area), Some(Position::new(9, 23)));
        // The smallest supported size still draws the prompt, with the cursor inside the row.
        let small = crate::test_util::render_to_string(&app, 40, 8);
        assert!(
            small
                .lines()
                .last()
                .is_some_and(|l| l.starts_with("/x[31my z"))
        );
        assert_eq!(
            cmdline_cursor(&app, Rect::new(0, 7, 5, 1)),
            Some(Position::new(4, 7))
        );
        // The mode badge still says Normal.
        let badge: String = (0..80).map(|x| buffer[(x, 22)].symbol()).collect();
        assert!(badge.contains("NORMAL"), "{badge}");
    }

    #[test]
    fn warning_persists() {
        let config = crate::config::Config {
            warnings: vec!["mode 0644: chmod 600 it".to_string()],
            ..crate::config::Config::default()
        };
        let mut app = app_with_config(config, 80, 24);
        let status_row = |app: &App| -> (String, ratatui::buffer::Buffer) {
            let buffer = crate::test_util::render_buffer(app, 80, 24);
            let row: String = (0..80).map(|x| buffer[(x, 22)].symbol()).collect();
            (row, buffer)
        };
        let (row, buffer) = status_row(&app);
        assert!(row.contains(" warning: mode 0644: chmod 600 it"), "{row}");
        let at = u16::try_from(row.find("mode 0644").expect("text")).expect("x");
        assert_eq!(buffer[(at, 22)].fg, Color::Red);

        crate::test_util::press(&mut app, "j");
        crate::test_util::press(&mut app, ":foo<CR>");
        let (row, _) = status_row(&app);
        assert!(row.contains(" warning: mode 0644: chmod 600 it"), "{row}");

        // Positive control: an app without warnings shows none.
        let app = app_with_config(crate::config::Config::default(), 80, 24);
        assert!(!status_row(&app).0.contains("warning"));
    }
}
