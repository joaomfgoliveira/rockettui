//! Message wrapping: one function turns a message into styled rows, so the reducer (scroll math)
//! and the renderer (drawing) can never disagree about a message's height.

use crate::data::Message;
use crate::ui::messages::hhmm;
use crate::ui::sanitize::{sanitize, sanitize_line};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::borrow::Cow;
use unicode_width::UnicodeWidthStr;

/// How a message is laid out (D-10). Compact is the built-in default; the config.toml selector
/// arrives in Phase 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageLayout {
    /// `HH:MM author  text`. The author is padded to the room's shared author column, so every
    /// body starts at the same x, and continuation rows hang under the body.
    #[default]
    Compact,
    /// An `author  HH:MM` header row, then the body indented by 2.
    TwoLine,
}

/// Narrowest body column the wrapper will use, however narrow the pane gets (A9).
pub const MIN_BODY: usize = 10;

/// Wrap `text` to `width` display columns. Sanitized first, so width math matches what is drawn.
/// Always returns at least one row.
pub fn wrap_body(text: &str, width: usize) -> Vec<String> {
    let clean = sanitize(text);
    let mut rows: Vec<String> = textwrap::wrap(&clean, width.max(1))
        .into_iter()
        .map(Cow::into_owned)
        .collect();
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

/// Display width of the widest author among `msgs`, or 0 for an empty slice. Authors are
/// sanitized before the width is measured (threat T-01-16), so the column matches what is drawn.
pub fn author_column(msgs: &[Message]) -> usize {
    msgs.iter()
        .map(|m| sanitize_line(&m.author).width())
        .max()
        .unwrap_or(0)
}

/// The styled rows of one message at `width` columns in the given layout.
///
/// `author_col` is the room's author column (see `author_column`). Compact pads every author to
/// it so all bodies start at the same x; TwoLine ignores it, because its body already sits at a
/// shared column 2.
pub fn render_rows(
    msg: &Message,
    width: u16,
    layout: MessageLayout,
    author_col: usize,
) -> Vec<Line<'static>> {
    let time = hhmm(msg.ts);
    let author = sanitize_line(&msg.author).into_owned();
    let dim = Style::new().add_modifier(Modifier::DIM);
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let width = usize::from(width);
    match layout {
        MessageLayout::Compact => {
            let col = author_col.max(author.width());
            let pad = " ".repeat(col - author.width() + 2);
            let prefix_width = time.width() + 1 + col + 2;
            let body_width = width.saturating_sub(prefix_width).max(MIN_BODY);
            let indent = " ".repeat(prefix_width);
            wrap_body(&msg.body, body_width)
                .into_iter()
                .enumerate()
                .map(|(i, row)| {
                    if i == 0 {
                        Line::from(vec![
                            Span::styled(time.clone(), dim),
                            Span::raw(" "),
                            Span::styled(author.clone(), bold),
                            Span::raw(pad.clone()),
                            Span::raw(row),
                        ])
                    } else {
                        Line::from(vec![Span::raw(indent.clone()), Span::raw(row)])
                    }
                })
                .collect()
        }
        MessageLayout::TwoLine => {
            let body_width = width.saturating_sub(2).max(MIN_BODY);
            let header = Line::from(vec![
                Span::styled(author, bold),
                Span::raw("  "),
                Span::styled(time, dim),
            ]);
            std::iter::once(header)
                .chain(
                    wrap_body(&msg.body, body_width)
                        .into_iter()
                        .map(|row| Line::from(vec![Span::raw("  "), Span::raw(row)])),
                )
                .collect()
        }
    }
}

/// Number of rows `render_rows` produces for this message.
pub fn message_rows(msg: &Message, width: u16, layout: MessageLayout, author_col: usize) -> usize {
    render_rows(msg, width, layout, author_col).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::data::{RoomId, fake};

    fn msg(author: &str, body: &str) -> Message {
        Message {
            id: "m-0".to_string(),
            room_id: RoomId("general".to_string()),
            author: author.to_string(),
            ts: 0,
            body: body.to_string(),
        }
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn texts(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(text).collect()
    }

    fn assert_clean(rows: &[String]) {
        for row in rows {
            for c in row.chars() {
                assert!(!c.is_control(), "control char U+{:04X}", c as u32);
                assert!(
                    !matches!(c as u32, 0x202a..=0x202e | 0x2066..=0x2069),
                    "bidi char U+{:04X}",
                    c as u32
                );
            }
        }
    }

    /// The author column a room holding only `m` would have.
    fn solo(m: &Message) -> usize {
        author_column(std::slice::from_ref(m))
    }

    #[test]
    fn author_column_is_widest_sanitized() {
        let authors =
            |names: &[&str]| -> Vec<Message> { names.iter().map(|n| msg(n, "x")).collect() };
        assert_eq!(author_column(&authors(&["alice", "bob", "dave"])), 5);
        assert_eq!(
            author_column(&authors(&["alice", "\u{202e}mallory\u{1b}"])),
            7
        );
        assert_eq!(author_column(&authors(&["日本語"])), 6);
        assert_eq!(author_column(&[]), 0);
    }

    #[test]
    fn short_author_padded_to_column() {
        let bob = render_rows(&msg("bob", "hi"), 60, MessageLayout::Compact, 5);
        assert_eq!(texts(&bob), vec!["00:00 bob    hi".to_string()]);
        let alice = render_rows(&msg("alice", "hello"), 60, MessageLayout::Compact, 5);
        assert_eq!(texts(&alice), vec!["00:00 alice  hello".to_string()]);
    }

    #[test]
    fn short_author_hangs_at_shared_column() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&render_rows(
            &msg("bob", body),
            30,
            MessageLayout::Compact,
            5,
        ));
        assert!(rows.len() > 2, "{rows:?}");
        assert!(rows[0].starts_with("00:00 bob    "), "{:?}", rows[0]);
        for row in &rows {
            assert!(row.width() <= 30, "{row:?}");
        }
        for row in &rows[1..] {
            assert!(row.starts_with(&" ".repeat(13)), "{row:?}");
            assert!(!row[13..].starts_with(' '), "{row:?}");
        }
    }

    #[test]
    fn two_line_ignores_author_col() {
        let m = msg("alice", "the quick brown fox jumps over the lazy dog");
        assert_eq!(
            texts(&render_rows(&m, 30, MessageLayout::TwoLine, 0)),
            texts(&render_rows(&m, 30, MessageLayout::TwoLine, 20))
        );
    }

    #[test]
    fn compact_single_line() {
        let m = msg("alice", "hello");
        let rows = render_rows(&m, 60, MessageLayout::Compact, solo(&m));
        assert_eq!(texts(&rows), vec!["00:00 alice  hello".to_string()]);
    }

    #[test]
    fn compact_hanging_indent() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let m = msg("alice", body);
        let rows = texts(&render_rows(&m, 30, MessageLayout::Compact, solo(&m)));
        assert!(rows.len() > 2, "{rows:?}");
        let prefix_width = "00:00 alice  ".len();
        for row in &rows {
            assert!(row.width() <= 30, "{row:?}");
        }
        for row in &rows[1..] {
            assert!(row.starts_with(&" ".repeat(prefix_width)), "{row:?}");
            assert!(!row[prefix_width..].starts_with(' '), "{row:?}");
        }
    }

    #[test]
    fn long_token_splits() {
        let m = msg("bob", &"x".repeat(80));
        let rows = texts(&render_rows(&m, 40, MessageLayout::Compact, solo(&m)));
        assert!(rows.len() >= 3, "{rows:?}");
        for row in &rows {
            assert!(row.width() <= 40, "{row:?}");
        }
        let joined: String = rows
            .iter()
            .map(|r| r.trim_start_matches(|c: char| c != 'x'))
            .collect();
        assert!(joined.ends_with(&"x".repeat(5)), "{joined:?}");
    }

    #[test]
    fn cjk_width() {
        let body = "これはラップの確認に使う日本語のテストメッセージです。";
        let m = msg("dave", body);
        let rows = texts(&render_rows(&m, 30, MessageLayout::Compact, solo(&m)));
        assert!(rows.len() > 1, "{rows:?}");
        for row in &rows {
            assert!(row.width() <= 30, "{row:?} is {} wide", row.width());
        }
    }

    #[test]
    fn newline_in_body() {
        assert_eq!(wrap_body("a\nb\n\nc", 20), vec!["a", "b", "", "c"]);
    }

    #[test]
    fn empty_body_one_row() {
        assert_eq!(wrap_body("", 20), vec![String::new()]);
        let m = msg("alice", "");
        let rows = render_rows(&m, 40, MessageLayout::Compact, solo(&m));
        assert_eq!(rows.len(), 1);
        assert_eq!(message_rows(&m, 40, MessageLayout::Compact, solo(&m)), 1);
    }

    #[test]
    fn tiny_width_no_panic() {
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            for width in [0u16, 1, 5] {
                let m = msg("alice", "hello wide world");
                let rows = render_rows(&m, width, layout, solo(&m));
                assert!(!rows.is_empty(), "{layout:?} width {width}");
                assert!(message_rows(&m, width, layout, solo(&m)) >= 1);
            }
        }
        assert!(!wrap_body("hello", 0).is_empty());
        assert!(!wrap_body("hello", 1).is_empty());
    }

    #[test]
    fn two_line_layout() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&render_rows(
            &msg("alice", body),
            30,
            MessageLayout::TwoLine,
            0,
        ));
        assert_eq!(rows[0], "alice  00:00");
        assert!(rows.len() > 2, "{rows:?}");
        for row in &rows[1..] {
            assert!(row.starts_with("  "), "{row:?}");
            assert!(row.width() <= 30, "{row:?}");
        }
    }

    #[test]
    fn sanitize_before_wrap() {
        let body = "red\u{1b}[31m evil\u{202e}text\there";
        let rows = wrap_body(body, 40);
        assert_clean(&rows);
        let joined = rows.join("\n");
        assert!(!joined.contains('\t'));
        assert!(joined.contains("red"), "{joined:?}");
        assert!(joined.contains("eviltext"), "{joined:?}");
        assert!(joined.contains("text    here"), "{joined:?}");
        let m = msg("\u{202e}mallory\u{1b}", body);
        let rendered = texts(&render_rows(&m, 60, MessageLayout::Compact, solo(&m)));
        assert_clean(&rendered);
    }

    #[test]
    fn fake_long_message_wraps() {
        let general: Vec<Message> = fake::seed()
            .into_iter()
            .find_map(|a| match a {
                Action::MessagesLoaded(id, m) if id.0 == "general" => Some(m),
                _ => None,
            })
            .expect("general is seeded");
        let long = &general[30];
        let rows = texts(&render_rows(
            long,
            50,
            MessageLayout::Compact,
            author_column(&general),
        ));
        assert!(rows.len() > 5, "{} rows", rows.len());
        for row in &rows {
            assert!(row.width() <= 50, "{row:?}");
        }
    }
}
