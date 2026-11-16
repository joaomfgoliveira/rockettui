//! The config error screen (D-02): every problem found in the config files, drawn before any
//! `App` exists. Styles are fixed and built in, never the user's theme, which may be the very
//! file that is invalid. Like `ui::view` it is a pure function of its input.

use crate::config::Report;
use crate::config::diag::Problem;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{Event, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// Rows above the problem list: title, counts, a blank row.
const HEADER_ROWS: u16 = 3;
/// Rows reserved at the bottom: the quit hint.
const FOOTER_ROWS: u16 = 1;

/// True for the events that quit the screen: a key press or repeat. A key release, a resize,
/// a paste and focus events do not (Pitfall 15; same filter as `event::spawn_input_task`).
pub fn quits_on(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release)
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn problem_line(problem: &Problem, label: &'static str, color: Color) -> Line<'static> {
    let mut text = format!("{}  ", problem.location());
    if let Some(key) = &problem.key {
        text.push_str(&sanitize_line(key));
        text.push_str(": ");
    }
    text.push_str(&sanitize_line(&problem.message));
    Line::from(vec![
        Span::styled(format!("{label:<8}"), Style::default().fg(color)),
        Span::raw(text),
    ])
}

/// Draw the report. Rows are clipped, not wrapped. When the problems do not fit, as many as fit
/// are shown plus a `... and N more` line: nothing scrolls. Any area size, 0x0 included, is safe.
pub fn render(frame: &mut Frame, report: &Report) {
    let area = frame.area();
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        "rockettui: invalid configuration",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    ));
    let mut counts = format!(
        "{}, {}",
        plural(report.errors.len(), "error"),
        plural(report.warnings.len(), "warning")
    );
    if let Some(dir) = &report.dir {
        counts.push_str(" in ");
        counts.push_str(&sanitize_line(&dir.to_string_lossy()));
    }
    lines.push(Line::raw(counts));
    lines.push(Line::raw(""));

    let problems: Vec<Line<'static>> = report
        .errors
        .iter()
        .map(|p| problem_line(p, "error", Color::Red))
        .chain(
            report
                .warnings
                .iter()
                .map(|p| problem_line(p, "warning", Color::Yellow)),
        )
        .collect();
    let room = usize::from(
        area.height
            .saturating_sub(HEADER_ROWS)
            .saturating_sub(FOOTER_ROWS),
    );
    if problems.len() <= room {
        lines.extend(problems);
    } else if room > 0 {
        let shown = room - 1;
        let hidden = problems.len() - shown;
        lines.extend(problems.into_iter().take(shown));
        lines.push(Line::raw(format!(
            "... and {hidden} more (full list printed on exit)"
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);

    if area.height > 0 {
        let hint = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        frame.render_widget(Clear, hint);
        frame.render_widget(
            Paragraph::new(Line::styled(
                "press any key to quit",
                Style::default().add_modifier(Modifier::REVERSED),
            )),
            hint,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diag::{ConfigFile, Severity};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn problem(
        file: ConfigFile,
        pos: Option<(usize, usize)>,
        key: &str,
        message: &str,
        severity: Severity,
    ) -> Problem {
        Problem {
            file,
            pos,
            key: Some(key.to_string()),
            message: message.to_string(),
            severity,
        }
    }

    fn sample_report() -> Report {
        let mut report = Report::new(vec![
            problem(
                ConfigFile::Config,
                Some((1, 17)),
                "sidebar_width",
                "must be between 10 and 60, got 99",
                Severity::Error,
            ),
            problem(
                ConfigFile::Config,
                Some((4, 20)),
                "timestamp_format",
                "invalid strftime format",
                Severity::Error,
            ),
            Problem {
                key: None,
                ..problem(
                    ConfigFile::Keymap,
                    Some((1, 7)),
                    "",
                    "TOML syntax error: unclosed table, expected `]`",
                    Severity::Error,
                )
            },
            problem(
                ConfigFile::Config,
                Some((2, 1)),
                "bogus",
                "unknown key, ignored",
                Severity::Warning,
            ),
        ]);
        report.dir = Some(PathBuf::from("/home/u/.config/rockettui"));
        report
    }

    fn draw(report: &Report, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("backend");
        terminal
            .draw(|f| render(f, report))
            .expect("draw never fails on a test backend");
        terminal
    }

    fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn error_screen_80x24() {
        insta::assert_snapshot!(draw(&sample_report(), 80, 24).backend());
    }

    #[test]
    fn truncates_with_more_line() {
        let errors = (1..=50)
            .map(|n| {
                problem(
                    ConfigFile::Config,
                    Some((n, 1)),
                    "history_size",
                    "must be between 1 and 1000, got 0",
                    Severity::Error,
                )
            })
            .collect();
        let screen = rows(&draw(&Report::new(errors), 80, 12));
        let more = screen
            .iter()
            .find(|r| r.contains("... and") && r.contains("more"))
            .expect("a more line");
        assert!(more.contains("43 more"), "{more}");
        assert!(screen[11].contains("press any key to quit"), "{screen:?}");
    }

    #[test]
    fn hostile_text_sanitized() {
        let report = Report::new(vec![problem(
            ConfigFile::Theme,
            Some((2, 3)),
            "k\u{1b}]0;x\u{7}ey",
            "bad \u{202e}text\u{1b}[2J",
            Severity::Error,
        )]);
        let terminal = draw(&report, 80, 12);
        for cell in terminal.backend().buffer().content() {
            for c in cell.symbol().chars() {
                assert!(!c.is_control(), "control char in {:?}", cell.symbol());
                assert_ne!(c, '\u{202e}');
            }
        }
        assert!(rows(&terminal).iter().any(|r| r.contains("key")));
    }

    #[test]
    fn quits_only_on_key_press() {
        let key = |kind| {
            let mut event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
            event.kind = kind;
            event.state = KeyEventState::NONE;
            Event::Key(event)
        };
        assert!(quits_on(&key(KeyEventKind::Press)));
        assert!(quits_on(&key(KeyEventKind::Repeat)));
        assert!(!quits_on(&key(KeyEventKind::Release)));
        assert!(!quits_on(&Event::Resize(80, 24)));
        assert!(!quits_on(&Event::Paste("x".to_string())));
        assert!(!quits_on(&Event::FocusGained));
    }

    #[test]
    fn tiny_sizes_no_panic() {
        let report = sample_report();
        for (w, h) in [(0, 0), (1, 1), (10, 3), (40, 8), (200, 60)] {
            draw(&report, w, h);
        }
    }
}
