//! Message wrapping: one function turns a message into styled rows, so the reducer (scroll math)
//! and the renderer (drawing) can never disagree about a message's height.

use crate::data::Message;
use ratatui::text::Line;

/// How a message is laid out (D-10). Compact is the built-in default; the config.toml selector
/// arrives in Phase 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageLayout {
    /// `HH:MM author  text`, continuation rows hang under the text.
    #[default]
    Compact,
    /// An `author  HH:MM` header row, then the body indented by 2.
    TwoLine,
}

/// Narrowest body column the wrapper will use, however narrow the pane gets (A9).
pub const MIN_BODY: usize = 10;

/// Wrap `text` to `width` display columns. Sanitized first, so width math matches what is drawn.
/// Always returns at least one row.
pub fn wrap_body(_text: &str, _width: usize) -> Vec<String> {
    Vec::new()
}

/// The styled rows of one message at `width` columns in the given layout.
pub fn render_rows(_msg: &Message, _width: u16, _layout: MessageLayout) -> Vec<Line<'static>> {
    Vec::new()
}

/// Number of rows `render_rows` produces for this message.
pub fn message_rows(_msg: &Message, _width: u16, _layout: MessageLayout) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::data::{RoomId, fake};
    use unicode_width::UnicodeWidthStr;

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

    #[test]
    fn compact_single_line() {
        let rows = render_rows(&msg("alice", "hello"), 60, MessageLayout::Compact);
        assert_eq!(texts(&rows), vec!["00:00 alice  hello".to_string()]);
    }

    #[test]
    fn compact_hanging_indent() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&render_rows(
            &msg("alice", body),
            30,
            MessageLayout::Compact,
        ));
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
        let rows = texts(&render_rows(
            &msg("bob", &"x".repeat(80)),
            40,
            MessageLayout::Compact,
        ));
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
        let rows = texts(&render_rows(&msg("dave", body), 30, MessageLayout::Compact));
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
        let rows = render_rows(&msg("alice", ""), 40, MessageLayout::Compact);
        assert_eq!(rows.len(), 1);
        assert_eq!(message_rows(&msg("alice", ""), 40, MessageLayout::Compact), 1);
    }

    #[test]
    fn tiny_width_no_panic() {
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            for width in [0u16, 1, 5] {
                let rows = render_rows(&msg("alice", "hello wide world"), width, layout);
                assert!(!rows.is_empty(), "{layout:?} width {width}");
                assert!(message_rows(&msg("alice", "hello wide world"), width, layout) >= 1);
            }
        }
        assert!(!wrap_body("hello", 0).is_empty());
        assert!(!wrap_body("hello", 1).is_empty());
    }

    #[test]
    fn two_line_layout() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&render_rows(&msg("alice", body), 30, MessageLayout::TwoLine));
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
        let rendered = texts(&render_rows(
            &msg("\u{202e}mallory\u{1b}", body),
            60,
            MessageLayout::Compact,
        ));
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
        let rows = texts(&render_rows(long, 50, MessageLayout::Compact));
        assert!(rows.len() > 5, "{} rows", rows.len());
        for row in &rows {
            assert!(row.width() <= 50, "{row:?}");
        }
    }
}
