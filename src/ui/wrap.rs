//! Message wrapping: one function turns a message into styled rows, so the reducer (scroll math)
//! and the renderer (drawing) can never disagree about a message's height.

use crate::config::settings::TimeFormat;
use crate::data::Message;
use crate::ui::palette::author_color;
use crate::ui::sanitize::{sanitize, sanitize_line};
use crate::ui::timefmt::{Zone, format_ts};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How a message is laid out (D-10). Compact is the built-in default; config.toml
/// `message_layout` selects it (D-12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageLayout {
    /// `time author  text`. The time is padded to the room's widest formatted time and the author
    /// to the room's shared author column, so every body starts at the same x, and continuation
    /// rows hang under the body. The author name is bold and colored by `palette::author_color`.
    #[default]
    Compact,
    /// An `author  time` header row, then the body indented by 2.
    TwoLine,
}

/// Narrowest body column the wrapper will use, however narrow the pane gets (A9). The Compact
/// author column is capped so the body keeps this many columns; below an inner width of 19 that
/// leaves no author column at all.
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

/// Fit an already-sanitized `author` into `col` display columns. An author that fits is returned
/// unchanged; `col == 0` yields an empty string; otherwise whole grapheme clusters are kept while
/// they fit in `col - 1` columns and an ellipsis is appended. A wide cluster straddling the limit
/// is dropped, never split, so the result is at most `col` wide.
fn fit_author(author: &str, col: usize) -> Cow<'_, str> {
    if author.width() <= col {
        return Cow::Borrowed(author);
    }
    if col == 0 {
        return Cow::Borrowed("");
    }
    let limit = col - 1;
    let mut out = String::new();
    let mut used = 0;
    for cluster in author.graphemes(true) {
        let w = cluster.width();
        if used + w > limit {
            break;
        }
        out.push_str(cluster);
        used += w;
    }
    out.push('\u{2026}');
    Cow::Owned(out)
}

/// Display width of the widest author among `msgs`, or 0 for an empty slice. Authors are
/// sanitized before the width is measured (threat T-01-16), so the column matches what is drawn.
pub fn author_column(msgs: &[Message]) -> usize {
    msgs.iter()
        .map(|m| sanitize_line(&m.author).width())
        .max()
        .unwrap_or(0)
}

/// Everything `render_rows` needs besides the message and the width. `App::row_ctx` is the only
/// place the app builds one, so the renderer and the reducer's row model cannot diverge.
#[derive(Clone, Copy)]
pub struct RowCtx<'a> {
    pub layout: MessageLayout,
    /// The room's author column (see `author_column`).
    pub author_col: usize,
    /// The room's time column (see `time_column`).
    pub time_col: usize,
    pub time_format: &'a TimeFormat,
    pub zone: Zone,
}

/// Display width of the widest formatted time among `msgs`, or 0 for an empty slice. Compact pads
/// every time to it, so bodies stay aligned at one x even for variable-width formats (A24).
pub fn time_column(msgs: &[Message], format: &TimeFormat, zone: Zone) -> usize {
    msgs.iter()
        .map(|m| format_ts(format, m.ts, zone).width())
        .max()
        .unwrap_or(0)
}

/// The styled rows of one message at `width` columns, laid out as `ctx` says.
///
/// Compact pads every author to `ctx.author_col` and every time to `ctx.time_col` so all bodies
/// start at the same x; TwoLine ignores both, because its body already sits at a shared column 2
/// (and its header keeps the full-length author and the unpadded time).
///
/// In both layouts the author name, and only the name, is bold and colored by
/// `palette::author_color`, keyed on the full sanitized name, so truncation never changes a
/// user's color.
///
/// Compact caps the author column at `width - (time_col + 1 + 2 + MIN_BODY)` (saturating), so the
/// body keeps at least `MIN_BODY` columns. An author wider than the capped column is cut on a
/// grapheme boundary and ends in an ellipsis. When the width leaves no room the column is 0 and
/// the author is omitted; the body keeps `MIN_BODY` and the paragraph clips any overflow. The cap
/// lives here, so the renderer and the reducer's row model always use the same value.
pub fn render_rows(msg: &Message, width: u16, ctx: &RowCtx) -> Vec<Line<'static>> {
    let time = format_ts(ctx.time_format, msg.ts, ctx.zone);
    let author = sanitize_line(&msg.author).into_owned();
    let dim = Style::new().add_modifier(Modifier::DIM);
    let bold = Style::new().add_modifier(Modifier::BOLD);
    // Keyed on the full sanitized name, before Compact truncates it, so a name keeps its color
    // at every width.
    let name = bold.fg(author_color(&author));
    let width = usize::from(width);
    match ctx.layout {
        MessageLayout::Compact => {
            // A time wider than the room's column (not possible for a room's own messages) keeps
            // its own width, so the hanging indent still matches the first row.
            let time_col = ctx.time_col.max(time.width());
            let time = format!("{time}{}", " ".repeat(time_col - time.width()));
            let col = ctx
                .author_col
                .min(width.saturating_sub(time_col + 1 + 2 + MIN_BODY));
            let author = fit_author(&author, col).into_owned();
            let pad = " ".repeat(col - author.width() + 2);
            let prefix_width = time_col + 1 + col + 2;
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
                            Span::styled(author.clone(), name),
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
                Span::styled(author, name),
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
pub fn message_rows(msg: &Message, width: u16, ctx: &RowCtx) -> usize {
    render_rows(msg, width, ctx).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::data::{RoomId, fake};
    use crate::ui::palette::author_color;

    fn msg(author: &str, body: &str) -> Message {
        Message {
            id: "m-0".to_string(),
            room_id: RoomId("general".to_string()),
            author: author.to_string(),
            ts: 0,
            body: body.to_string(),
        }
    }

    /// `render_rows` with the Phase 1 context: default format, UTC, a 5-column time.
    fn rows_at(
        m: &Message,
        width: u16,
        layout: MessageLayout,
        author_col: usize,
    ) -> Vec<Line<'static>> {
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout,
            author_col,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
        };
        render_rows(m, width, &ctx)
    }

    fn count_at(m: &Message, width: u16, layout: MessageLayout, author_col: usize) -> usize {
        rows_at(m, width, layout, author_col).len()
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
    fn time_column_alignment() {
        let format = TimeFormat::parse("%B %d").expect("valid");
        let jan = Message {
            ts: 0,
            ..msg("bob", "hi")
        };
        let may = Message {
            ts: 10_368_000,
            ..msg("bob", "hi")
        };
        let msgs = [jan, may];
        assert_eq!(time_column(&msgs, &format, Zone::Utc), 10);
        assert_eq!(time_column(&[], &format, Zone::Utc), 0);
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 3,
            time_col: 10,
            time_format: &format,
            zone: Zone::Utc,
        };
        let bodies: Vec<usize> = msgs
            .iter()
            .map(|m| {
                let t = text(&render_rows(m, 60, &ctx)[0]);
                t.find("hi").expect("body present")
            })
            .collect();
        assert_eq!(bodies, vec![16, 16]);
    }

    #[test]
    fn custom_format_compact_prefix() {
        let format = TimeFormat::parse("%Y-%m-%d %H:%M").expect("valid");
        let m = Message {
            ts: 1_794_614_400,
            ..msg("bob", "hi")
        };
        let msgs = std::slice::from_ref(&m);
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: author_column(msgs),
            time_col: time_column(msgs, &format, Zone::Utc),
            time_format: &format,
            zone: Zone::Utc,
        };
        let rows = render_rows(&m, 60, &ctx);
        let first = &rows[0].spans[0];
        assert_eq!(first.content, "2026-11-14 00:00");
        assert!(first.style.add_modifier.contains(Modifier::DIM));
        assert_eq!(text(&rows[0]), "2026-11-14 00:00 bob  hi");
    }

    #[test]
    fn compact_author_colored() {
        let rows = rows_at(&msg("bob", "hi"), 60, MessageLayout::Compact, 5);
        assert_eq!(texts(&rows), vec!["00:00 bob    hi".to_string()]);
        let spans = &rows[0].spans;
        assert_eq!(spans[2].content, "bob");
        assert_eq!(spans[2].style.fg, Some(author_color("bob")));
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[3].style, Style::default());
        assert_eq!(spans[4].style, Style::default());
        assert_eq!(spans[0].style.fg, None);
        assert!(spans[0].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn two_line_author_colored() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = rows_at(&msg("alice", body), 30, MessageLayout::TwoLine, 0);
        assert_eq!(text(&rows[0]), "alice  00:00");
        let spans = &rows[0].spans;
        assert_eq!(spans[0].style.fg, Some(author_color("alice")));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].style, Style::default());
        assert_eq!(spans[2].style.fg, None);
        for row in &rows[1..] {
            for span in &row.spans {
                assert_eq!(span.style.fg, None);
            }
        }
    }

    #[test]
    fn truncated_author_keeps_full_name_color() {
        let rows = rows_at(&msg(LONG, "x"), 30, MessageLayout::Compact, 29);
        let span = &rows[0].spans[2];
        assert_eq!(span.content, "firstname.l\u{2026}");
        assert_eq!(span.style.fg, Some(author_color(LONG)));
        assert_ne!(author_color(LONG), author_color("firstname.l\u{2026}"));
    }

    #[test]
    fn hostile_author_colored_by_sanitized_name() {
        let hostile = msg("\u{202e}mallory\u{1b}", "x");
        let rows = rows_at(&hostile, 60, MessageLayout::Compact, solo(&hostile));
        let span = &rows[0].spans[2];
        assert_eq!(span.content, "mallory");
        assert_eq!(span.style.fg, Some(author_color("mallory")));
        let plain = msg("mallory", "x");
        let plain_rows = rows_at(&plain, 60, MessageLayout::Compact, solo(&plain));
        assert_eq!(span.style.fg, plain_rows[0].spans[2].style.fg);
        assert_ne!(
            author_color("mallory"),
            author_color("\u{202e}mallory\u{1b}")
        );
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
        let bob = rows_at(&msg("bob", "hi"), 60, MessageLayout::Compact, 5);
        assert_eq!(texts(&bob), vec!["00:00 bob    hi".to_string()]);
        let alice = rows_at(&msg("alice", "hello"), 60, MessageLayout::Compact, 5);
        assert_eq!(texts(&alice), vec!["00:00 alice  hello".to_string()]);
    }

    #[test]
    fn short_author_hangs_at_shared_column() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&rows_at(&msg("bob", body), 30, MessageLayout::Compact, 5));
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
            texts(&rows_at(&m, 30, MessageLayout::TwoLine, 0)),
            texts(&rows_at(&m, 30, MessageLayout::TwoLine, 20))
        );
    }

    #[test]
    fn compact_single_line() {
        let m = msg("alice", "hello");
        let rows = rows_at(&m, 60, MessageLayout::Compact, solo(&m));
        assert_eq!(texts(&rows), vec!["00:00 alice  hello".to_string()]);
    }

    #[test]
    fn compact_hanging_indent() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let m = msg("alice", body);
        let rows = texts(&rows_at(&m, 30, MessageLayout::Compact, solo(&m)));
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
        let rows = texts(&rows_at(&m, 40, MessageLayout::Compact, solo(&m)));
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
        let rows = texts(&rows_at(&m, 30, MessageLayout::Compact, solo(&m)));
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
        let rows = rows_at(&m, 40, MessageLayout::Compact, solo(&m));
        assert_eq!(rows.len(), 1);
        assert_eq!(count_at(&m, 40, MessageLayout::Compact, solo(&m)), 1);
    }

    const LONG: &str = "firstname.lastname.contractor";

    #[test]
    fn column_capped_keeps_min_body() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&rows_at(&msg(LONG, body), 30, MessageLayout::Compact, 29));
        assert!(rows.len() > 2, "{rows:?}");
        assert!(
            rows[0].starts_with("00:00 firstname.l\u{2026}  "),
            "{:?}",
            rows[0]
        );
        for row in &rows {
            assert!(row.width() <= 30, "{row:?}");
        }
        for row in &rows[1..] {
            assert!(row.starts_with(&" ".repeat(20)), "{row:?}");
            assert!(row[20..].width() <= MIN_BODY, "{row:?}");
        }
    }

    #[test]
    fn short_author_padded_when_capped() {
        let rows = texts(&rows_at(&msg("bob", "hi"), 30, MessageLayout::Compact, 29));
        assert_eq!(rows, vec![format!("00:00 bob{}hi", " ".repeat(11))]);
    }

    #[test]
    fn wide_author_truncates_on_grapheme_boundary() {
        let rows = texts(&rows_at(
            &msg("日本語のユーザー", "x"),
            26,
            MessageLayout::Compact,
            16,
        ));
        assert_eq!(rows, vec!["00:00 日本語\u{2026}   x".to_string()]);

        let accented = "e\u{301}".repeat(12);
        let rows = texts(&rows_at(
            &msg(&accented, "x"),
            23,
            MessageLayout::Compact,
            12,
        ));
        let head = rows[0]
            .strip_prefix("00:00 ")
            .and_then(|r| r.split_once('\u{2026}'))
            .map(|(h, _)| h.to_string())
            .expect("ellipsis present");
        assert_eq!(head, "e\u{301}".repeat(4));
        assert!(head.ends_with('\u{301}'));
    }

    #[test]
    fn tiny_width_no_panic() {
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            for width in [0u16, 1, 5] {
                for author_col in [0usize, 5, 40] {
                    let m = msg("alice", "hello wide world");
                    let rows = rows_at(&m, width, layout, author_col);
                    assert!(!rows.is_empty(), "{layout:?} width {width}");
                    assert_eq!(
                        count_at(&m, width, layout, author_col),
                        rows.len(),
                        "{layout:?} width {width} col {author_col}"
                    );
                }
            }
        }
        assert!(!wrap_body("hello", 0).is_empty());
        assert!(!wrap_body("hello", 1).is_empty());
    }

    #[test]
    fn two_line_layout() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = texts(&rows_at(&msg("alice", body), 30, MessageLayout::TwoLine, 0));
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
        let rendered = texts(&rows_at(&m, 60, MessageLayout::Compact, solo(&m)));
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
        let rows = texts(&rows_at(
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
