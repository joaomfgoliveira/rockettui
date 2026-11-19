//! Message wrapping: one function turns a message into styled rows, so the reducer (scroll math)
//! and the renderer (drawing) can never disagree about a message's height.

use crate::config::settings::TimeFormat;
use crate::data::{Message, MessageKind, SendState};
use crate::ui::palette::author_color;
use crate::ui::sanitize::{sanitize, sanitize_line};
use crate::ui::timefmt::{Zone, format_ts};
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
    /// The theme: the author palette now, the messages element styles next (D-09, D-10).
    pub theme: &'a crate::config::Theme,
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
/// In both layouts the author name, and only the name, takes `theme.messages_author` (bold by
/// default) with its color from `palette::author_color` over `theme.author_palette`, keyed on the
/// full sanitized name, so truncation never changes a user's color. The time takes
/// `theme.messages_time` and body text `theme.messages_body`; padding and indent stay raw spans.
/// A `MessageKind::System` message takes `theme.messages_notice` for its author and its text.
///
/// The send state decides the rest (`send_look`, D-01): a `Queued` or `Sending` message draws
/// `theme.markers.sending` where its time would be, and the time cell, the author and the body all
/// take `theme.messages_pending`. Other states keep the confirmed look here until they get their own.
/// The marker is padded and measured exactly like the time, so no body moves.
///
/// Compact caps the author column at `width - (time_col + 1 + 2 + MIN_BODY)` (saturating), so the
/// body keeps at least `MIN_BODY` columns. An author wider than the capped column is cut on a
/// grapheme boundary and ends in an ellipsis. When the width leaves no room the column is 0 and
/// the author is omitted; the body keeps `MIN_BODY` and the paragraph clips any overflow. The cap
/// lives here, so the renderer and the reducer's row model always use the same value.
pub fn render_rows(msg: &Message, width: u16, ctx: &RowCtx) -> Vec<Line<'static>> {
    let (time, state_style) = send_look(msg, ctx);
    let author = sanitize_line(&msg.author).into_owned();
    let time_style = state_style.unwrap_or(ctx.theme.messages_time);
    let system = msg.kind == MessageKind::System;
    let body_style = state_style.unwrap_or(if system {
        ctx.theme.messages_notice
    } else {
        ctx.theme.messages_body
    });
    // Keyed on the full sanitized name, before Compact truncates it, so a name keeps its color
    // at every width. An explicit fg on messages.author wins over the palette for every author.
    // A system message (D-15) draws its author dim like its text, outside the palette.
    let name = if let Some(style) = state_style {
        style
    } else if system {
        ctx.theme.messages_notice
    } else if ctx.theme.messages_author.fg.is_some() {
        ctx.theme.messages_author
    } else {
        ctx.theme
            .messages_author
            .fg(author_color(&author, &ctx.theme.author_palette))
    };
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
                            Span::styled(time.clone(), time_style),
                            Span::raw(" "),
                            Span::styled(author.clone(), name),
                            Span::raw(pad.clone()),
                            Span::styled(row, body_style),
                        ])
                    } else {
                        Line::from(vec![
                            Span::raw(indent.clone()),
                            Span::styled(row, body_style),
                        ])
                    }
                })
                .collect()
        }
        MessageLayout::TwoLine => {
            let body_width = width.saturating_sub(2).max(MIN_BODY);
            let header = Line::from(vec![
                Span::styled(author, name),
                Span::raw("  "),
                Span::styled(time, time_style),
            ]);
            std::iter::once(header)
                .chain(
                    wrap_body(&msg.body, body_width).into_iter().map(|row| {
                        Line::from(vec![Span::raw("  "), Span::styled(row, body_style)])
                    }),
                )
                .collect()
        }
    }
}

/// What takes the time's place in a message's first row, and the style of the time cell, the
/// author and the body when the send state overrides the normal look (D-01). One match on
/// `msg.send`, so every row of every state is chosen in one place.
fn send_look(msg: &Message, ctx: &RowCtx) -> (String, Option<ratatui::style::Style>) {
    match msg.send {
        SendState::Queued { .. } | SendState::Sending => (
            sanitize_line(&ctx.theme.markers.sending).into_owned(),
            Some(ctx.theme.messages_pending),
        ),
        SendState::Sent | SendState::Failed | SendState::Unconfirmed => {
            (format_ts(ctx.time_format, msg.ts, ctx.zone), None)
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
    use crate::config::Theme;
    use crate::data::{RoomId, fake};
    use crate::ui::palette::{DEFAULT_AUTHOR_PALETTE, author_color};
    use ratatui::style::{Color, Modifier, Style};

    fn msg(author: &str, body: &str) -> Message {
        Message {
            id: "m-0".to_string(),
            room_id: RoomId("general".to_string()),
            author: author.to_string(),
            ts: 0,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: crate::data::SendState::Sent,
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
        let theme = Theme::default();
        let ctx = RowCtx {
            layout,
            author_col,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
        };
        render_rows(m, width, &ctx)
    }

    /// `render_rows` with `theme` in place of the default one.
    fn rows_themed(
        m: &Message,
        width: u16,
        layout: MessageLayout,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout,
            author_col: solo(m),
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme,
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
        let theme = Theme::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 3,
            time_col: 10,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
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
        let theme = Theme::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: author_column(msgs),
            time_col: time_column(msgs, &format, Zone::Utc),
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
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
        assert_eq!(
            spans[2].style.fg,
            Some(author_color("bob", &DEFAULT_AUTHOR_PALETTE))
        );
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[3].style, Style::default());
        assert_eq!(spans[4].style, Style::default());
        assert_eq!(spans[0].style.fg, None);
        assert!(spans[0].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn themed_time_author_body() {
        let theme = crate::test_util::config_from_theme(
            "[messages.time]\nfg = 1\n[messages.body]\nfg = 2\n[messages.author]\nfg = 3\n",
        )
        .theme;
        let m = msg("bob", "hi");
        let rows = rows_themed(&m, 60, MessageLayout::Compact, &theme);
        let spans = &rows[0].spans;
        assert_eq!(spans[0].style.fg, Some(Color::Indexed(1)), "time");
        assert!(spans[0].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(spans[2].style.fg, Some(Color::Indexed(3)), "author");
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[4].style.fg, Some(Color::Indexed(2)), "body");
        // An explicit author fg colors every author the same.
        let other = rows_themed(&msg("alice", "hi"), 60, MessageLayout::Compact, &theme);
        assert_eq!(other[0].spans[2].style.fg, Some(Color::Indexed(3)));
        // Without messages.author fg the author keeps the palette color.
        let theme = crate::test_util::config_from_theme("[messages.time]\nfg = 1\n").theme;
        let rows = rows_themed(&m, 60, MessageLayout::Compact, &theme);
        assert_eq!(
            rows[0].spans[2].style.fg,
            Some(author_color("bob", &DEFAULT_AUTHOR_PALETTE))
        );

        // The continuation and TwoLine body rows take the body style too, padding stays raw.
        let theme = crate::test_util::config_from_theme("[messages.body]\nfg = 2\n").theme;
        let long = msg(
            "bob",
            "the quick brown fox jumps over the lazy dog and keeps on running far away",
        );
        let rows = rows_themed(&long, 30, MessageLayout::Compact, &theme);
        assert!(rows.len() > 2);
        assert_eq!(rows[1].spans[0].style, Style::default(), "indent");
        assert_eq!(rows[1].spans[1].style.fg, Some(Color::Indexed(2)));
        let rows = rows_themed(&long, 30, MessageLayout::TwoLine, &theme);
        assert_eq!(rows[1].spans[0].style, Style::default(), "indent");
        assert_eq!(rows[1].spans[1].style.fg, Some(Color::Indexed(2)));
    }

    #[test]
    fn two_line_author_colored() {
        let body = "the quick brown fox jumps over the lazy dog and keeps on running far away";
        let rows = rows_at(&msg("alice", body), 30, MessageLayout::TwoLine, 0);
        assert_eq!(text(&rows[0]), "alice  00:00");
        let spans = &rows[0].spans;
        assert_eq!(
            spans[0].style.fg,
            Some(author_color("alice", &DEFAULT_AUTHOR_PALETTE))
        );
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
        assert_eq!(
            span.style.fg,
            Some(author_color(LONG, &DEFAULT_AUTHOR_PALETTE))
        );
        assert_ne!(
            author_color(LONG, &DEFAULT_AUTHOR_PALETTE),
            author_color("firstname.l\u{2026}", &DEFAULT_AUTHOR_PALETTE)
        );
    }

    #[test]
    fn hostile_author_colored_by_sanitized_name() {
        let hostile = msg("\u{202e}mallory\u{1b}", "x");
        let rows = rows_at(&hostile, 60, MessageLayout::Compact, solo(&hostile));
        let span = &rows[0].spans[2];
        assert_eq!(span.content, "mallory");
        assert_eq!(
            span.style.fg,
            Some(author_color("mallory", &DEFAULT_AUTHOR_PALETTE))
        );
        let plain = msg("mallory", "x");
        let plain_rows = rows_at(&plain, 60, MessageLayout::Compact, solo(&plain));
        assert_eq!(span.style.fg, plain_rows[0].spans[2].style.fg);
        assert_ne!(
            author_color("mallory", &DEFAULT_AUTHOR_PALETTE),
            author_color("\u{202e}mallory\u{1b}", &DEFAULT_AUTHOR_PALETTE)
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

    #[test]
    fn system_row_style() {
        let theme = Theme::default();
        let notice = theme.messages_notice;
        assert_ne!(notice, theme.messages_body, "the notice style must differ");
        let m = Message {
            kind: MessageKind::System,
            ..msg("carol", "* joined the room")
        };
        for (layout, author_span, body_span) in [
            (MessageLayout::Compact, 2, 4),
            (MessageLayout::TwoLine, 0, 1),
        ] {
            let rows = rows_themed(&m, 60, layout, &theme);
            assert_eq!(
                rows.len(),
                if layout == MessageLayout::Compact {
                    1
                } else {
                    2
                }
            );
            let line = &rows[rows.len() - 1];
            let body = line.spans[body_span].clone();
            assert_eq!(body.content, "* joined the room");
            assert_eq!(body.style, notice, "{layout:?} body");
            let head = &rows[0].spans[author_span];
            assert_eq!(head.content, "carol");
            assert_eq!(head.style, notice, "{layout:?} author");
        }
    }

    fn with_state(send: crate::data::SendState, author: &str, body: &str) -> Message {
        Message {
            send,
            ..msg(author, body)
        }
    }

    #[test]
    fn pending_row_compact() {
        let theme = Theme::default();
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
        };
        let sent = render_rows(&msg("alice", "hello there"), 60, &ctx);
        for state in [
            crate::data::SendState::Sending,
            crate::data::SendState::Queued { reconcile: false },
        ] {
            let rows = render_rows(&with_state(state, "alice", "hello there"), 60, &ctx);
            assert_eq!(rows.len(), 1);
            let first = text(&rows[0]);
            assert_eq!(first, "\u{2026}     alice  hello there", "{state:?}");
            // The body starts at the same column as a Sent message's body.
            let body_x = |line: &Line<'_>| {
                let t = text(line);
                t[..t.find("hello").expect("body")].width()
            };
            assert_eq!(body_x(&rows[0]), body_x(&sent[0]));
            // Marker, author and body all carry messages_pending.
            for span in [0usize, 2, 4] {
                assert_eq!(rows[0].spans[span].style, theme.messages_pending, "{span}");
            }
        }
    }

    #[test]
    fn pending_rows_count_equals_render() {
        let theme = Theme::default();
        let format = TimeFormat::default();
        let long = "word ".repeat(30);
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let ctx = RowCtx {
                layout,
                author_col: 5,
                time_col: 5,
                time_format: &format,
                zone: Zone::Utc,
                theme: &theme,
            };
            for state in [
                crate::data::SendState::Sending,
                crate::data::SendState::Queued { reconcile: true },
            ] {
                let m = with_state(state, "alice", &long);
                assert_eq!(message_rows(&m, 40, &ctx), render_rows(&m, 40, &ctx).len());
                assert!(message_rows(&m, 40, &ctx) > 2, "{layout:?}");
            }
        }
    }

    /// Failed and Unconfirmed rows: the state's marker where the time would be, and author, marker
    /// and body in messages_failed, in both layouts.
    #[test]
    fn failed_and_unconfirmed_rows() {
        let theme = Theme::default();
        assert_ne!(theme.messages_failed, theme.messages_pending);
        let format = TimeFormat::default();
        let long = "word ".repeat(30);
        for (state, marker) in [
            (crate::data::SendState::Failed, "!"),
            (crate::data::SendState::Unconfirmed, "?"),
        ] {
            for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
                let ctx = RowCtx {
                    layout,
                    author_col: 5,
                    time_col: 5,
                    time_format: &format,
                    zone: Zone::Utc,
                    theme: &theme,
                };
                let m = with_state(state, "alice", "hello there");
                let rows = render_rows(&m, 60, &ctx);
                let (marker_span, author_span, body_span) = match layout {
                    MessageLayout::Compact => (0, 2, 4),
                    MessageLayout::TwoLine => (2, 0, 1),
                };
                let body_line = match layout {
                    MessageLayout::Compact => &rows[0],
                    MessageLayout::TwoLine => &rows[1],
                };
                let head = &rows[0];
                assert_eq!(head.spans[marker_span].content.trim(), marker, "{layout:?}");
                assert_eq!(head.spans[marker_span].style, theme.messages_failed);
                assert_eq!(head.spans[author_span].content, "alice");
                assert_eq!(head.spans[author_span].style, theme.messages_failed);
                let body = &body_line.spans[body_span];
                assert_eq!(body.content, "hello there");
                assert_eq!(body.style, theme.messages_failed, "{layout:?}");
                assert!(!text(&rows[0]).contains("00:00"), "{layout:?}");
                let m = with_state(state, "alice", &long);
                assert_eq!(message_rows(&m, 40, &ctx), render_rows(&m, 40, &ctx).len());
            }
        }
    }

    /// A marker wider than the formatted time widens the room's time column, so every body
    /// still starts at the same x.
    #[test]
    fn wide_marker_keeps_body_column() {
        use crate::data::SendState;
        let sources = crate::config::Sources {
            config: Some("timestamp_format = \"%H\"\n".to_string()),
            theme: Some("[markers]\nfailed = \"FAIL\"\n".to_string()),
            ..Default::default()
        };
        let Ok(config) = crate::config::load_from_sources(&sources) else {
            panic!("test config is invalid");
        };
        let app = crate::app::App::with_config(ratatui::layout::Size::new(80, 24), config);
        let msgs = [
            msg("alice", "sent body"),
            with_state(SendState::Failed, "alice", "failed body"),
        ];
        let ctx = app.row_ctx(&msgs);
        assert_eq!(ctx.time_col, 4);
        let at = |m: &Message, needle: &str| {
            let rows = render_rows(m, 60, &ctx);
            let t = text(&rows[0]);
            t[..t.find(needle).expect("body")].width()
        };
        assert_eq!(at(&msgs[0], "sent"), at(&msgs[1], "failed"));
    }
}
