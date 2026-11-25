//! Message wrapping: one function turns a message into styled rows, so the reducer (scroll math)
//! and the renderer (drawing) can never disagree about a message's height.

use crate::config::settings::TimeFormat;
use crate::data::{Message, MessageKind, SendState};
use crate::ui::palette::author_color;
use crate::ui::sanitize::{sanitize, sanitize_line};
use crate::ui::timefmt::{Zone, day_of, format_ts};
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

/// The marker drawn after the body of an edited message, and the text of a deleted message's line.
/// Fixed strings; the styles are the theme's `messages.edited` and `messages.deleted`.
pub const EDITED_MARKER: &str = " (edited)";
pub const DELETED_TEXT: &str = "message deleted";

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
pub(crate) fn fit_author(author: &str, col: usize) -> Cow<'_, str> {
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
    /// The query of the active search, whose matches are drawn in `theme.search_match`. Only the
    /// styling of spans depends on it: the number of rows never does.
    pub search: Option<&'a str>,
    /// The `[emoji_aliases]` of config.toml: the text drawn for a reaction by shortcode name.
    pub aliases: &'a std::collections::HashMap<String, String>,
    /// The user's own username, to tell their reactions from the others (`None` until known).
    pub own_user: Option<&'a str>,
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
/// An edited message (`msg.edited`) ends with `EDITED_MARKER` in `theme.messages_edited`: on the
/// last body row when it fits in the body width, on a row of its own otherwise. A
/// `MessageKind::Deleted` message is always one row, in both layouts: its author, its time and
/// `DELETED_TEXT`, all in `theme.messages_deleted`; it never shows a body.
///
/// The send state decides the rest (`send_look`, D-01, D-06): a `Queued` or `Sending` message draws
/// `theme.markers.sending` where its time would be and a `Failed` one `markers.failed`, an
/// `Unconfirmed` one `markers.unconfirmed`; the time cell, the author and the body then all take
/// `theme.messages_pending` (Queued, Sending) or `theme.messages_failed` (Failed, Unconfirmed).
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
    let deleted = msg.kind == MessageKind::Deleted;
    let time_style = if deleted {
        ctx.theme.messages_deleted
    } else {
        state_style.unwrap_or(ctx.theme.messages_time)
    };
    let system = msg.kind == MessageKind::System;
    let body_style = if deleted {
        ctx.theme.messages_deleted
    } else {
        state_style.unwrap_or(if system {
            ctx.theme.messages_notice
        } else {
            ctx.theme.messages_body
        })
    };
    // Keyed on the full sanitized name, before Compact truncates it, so a name keeps its color
    // at every width. An explicit fg on messages.author wins over the palette for every author.
    // A system message (D-15) draws its author dim like its text, outside the palette.
    let name = if deleted {
        ctx.theme.messages_deleted
    } else if let Some(style) = state_style {
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
            body_rows(msg, body_width, body_style, ctx)
                .into_iter()
                .enumerate()
                .map(|(i, mut spans)| {
                    if i == 0 {
                        spans.splice(
                            0..0,
                            [
                                Span::styled(time.clone(), time_style),
                                Span::raw(" "),
                                Span::styled(author.clone(), name),
                                Span::raw(pad.clone()),
                            ],
                        );
                    } else {
                        spans.insert(0, Span::raw(indent.clone()));
                    }
                    Line::from(spans)
                })
                .collect()
        }
        MessageLayout::TwoLine => {
            let body_width = width.saturating_sub(2).max(MIN_BODY);
            let mut header = vec![
                Span::styled(author, name),
                Span::raw("  "),
                Span::styled(time, time_style),
            ];
            if deleted {
                // A tombstone is one row: the text sits on the header row.
                header.push(Span::raw("  "));
                header.push(Span::styled(DELETED_TEXT, body_style));
                return vec![Line::from(header)];
            }
            std::iter::once(Line::from(header))
                .chain(
                    body_rows(msg, body_width, body_style, ctx)
                        .into_iter()
                        .map(|mut spans| {
                            spans.insert(0, Span::raw("  "));
                            Line::from(spans)
                        }),
                )
                .collect()
        }
    }
}

/// The body rows of `msg` at `width` columns, as spans without any prefix. A deleted message is
/// the one `DELETED_TEXT` row. An edited message gets `EDITED_MARKER` (in `theme.messages_edited`)
/// after its last row when it fits in `width`, on a new row of its own (without the leading space)
/// when it does not. The body is drawn with its emoji shortcodes as emoji (see `draw_shortcodes`).
/// The reaction rows (`reaction_rows`) follow the last body row.
fn body_rows(
    msg: &Message,
    width: usize,
    body_style: ratatui::style::Style,
    ctx: &RowCtx,
) -> Vec<Vec<Span<'static>>> {
    if msg.kind == MessageKind::Deleted {
        return vec![vec![Span::styled(DELETED_TEXT, body_style)]];
    }
    let query = ctx
        .search
        .filter(|_| crate::app::search::highlightable(msg));
    // The emoji shortcodes of an ordinary message are drawn as emoji before the text is wrapped, so
    // widths are measured on what is on screen and the row model agrees with the pane. The stored
    // text is not changed. A system message is fixed text the client wrote itself.
    let drawn = if msg.kind == MessageKind::Normal {
        crate::ui::emoji::draw_shortcodes(&msg.body, ctx.aliases)
    } else {
        std::borrow::Cow::Borrowed(msg.body.as_str())
    };
    let mut rows: Vec<Vec<Span<'static>>> = wrap_body(&drawn, width)
        .into_iter()
        .map(|row| highlight_row(row, body_style, query, ctx.theme.search_match))
        .collect();
    if msg.edited {
        let last_width: usize = rows
            .last()
            .map_or(0, |row| row.iter().map(|s| s.content.width()).sum());
        if last_width + EDITED_MARKER.width() <= width {
            if let Some(last) = rows.last_mut() {
                last.push(Span::styled(EDITED_MARKER, ctx.theme.messages_edited));
            }
        } else {
            rows.push(vec![Span::styled(
                EDITED_MARKER.trim_start(),
                ctx.theme.messages_edited,
            )]);
        }
    }
    rows.extend(reaction_rows(msg, width, ctx));
    rows
}

/// What sits between two reactions of a row.
const REACTION_GAP: &str = "  ";

/// The reaction rows of `msg` at `width` columns, as spans without any prefix: one item per
/// reaction, `glyph count` (the glyph from `ui::emoji::resolve`, the count the number of users),
/// in the order the server lists them, joined by `REACTION_GAP` and wrapped by whole items. An item
/// wider than `width` is cut with an ellipsis. The user's own reactions take
/// `theme.messages_reactions_own`, the others `theme.messages_reactions`. Usernames never appear
/// here (the popup shows them). No reactions, no rows.
fn reaction_rows(msg: &Message, width: usize, ctx: &RowCtx) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut used = 0;
    for reaction in &msg.reactions {
        let label = crate::ui::emoji::resolve(&reaction.name, ctx.aliases);
        let text = format!("{label} {}", reaction.users.len());
        let text = fit_author(&text, width).into_owned();
        let own = ctx
            .own_user
            .is_some_and(|me| reaction.users.iter().any(|user| user == me));
        let style = if own {
            ctx.theme.messages_reactions_own
        } else {
            ctx.theme.messages_reactions
        };
        let item = text.width();
        match rows.last_mut() {
            Some(row) if used + REACTION_GAP.width() + item <= width => {
                row.push(Span::raw(REACTION_GAP));
                row.push(Span::styled(text, style));
                used += REACTION_GAP.width() + item;
            }
            _ => {
                rows.push(vec![Span::styled(text, style)]);
                used = item;
            }
        }
    }
    rows
}

/// One wrapped body row as spans: the text outside a search match in `body_style`, each match in
/// `match_style`. Without a query, or without a match in this row, it is the one plain span. A
/// match that crosses a wrap boundary is found in neither row and so is not highlighted (it still
/// counts when jumping, which looks at the whole body).
fn highlight_row(
    row: String,
    body_style: ratatui::style::Style,
    query: Option<&str>,
    match_style: ratatui::style::Style,
) -> Vec<Span<'static>> {
    let ranges = query.map_or_else(Vec::new, |q| crate::app::search::find(&row, q));
    if ranges.is_empty() {
        return vec![Span::styled(row, body_style)];
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for r in ranges {
        if r.start > at {
            spans.push(Span::styled(row[at..r.start].to_string(), body_style));
        }
        spans.push(Span::styled(row[r.start..r.end].to_string(), match_style));
        at = r.end;
    }
    if at < row.len() {
        spans.push(Span::styled(row[at..].to_string(), body_style));
    }
    spans
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
        SendState::Failed => (
            sanitize_line(&ctx.theme.markers.failed).into_owned(),
            Some(ctx.theme.messages_failed),
        ),
        SendState::Unconfirmed => (
            sanitize_line(&ctx.theme.markers.unconfirmed).into_owned(),
            Some(ctx.theme.messages_failed),
        ),
        SendState::Sent => (format_ts(ctx.time_format, msg.ts, ctx.zone), None),
    }
}

/// Number of rows `render_rows` produces for this message.
pub fn message_rows(msg: &Message, width: u16, ctx: &RowCtx) -> usize {
    render_rows(msg, width, ctx).len()
}

/// The day divider drawn above `msg`, or `None` when it falls on the same day as `prev` (the message
/// above it in the list). The first message of a list (`prev` is `None`) gets one, so the list
/// always says what day it starts on. A message whose time cannot be represented gets none. The
/// divider is one row of `width` columns: `── YYYY-MM-DD ───…` in `theme.messages_notice`.
pub fn divider_row(
    msg: &Message,
    prev: Option<&Message>,
    width: u16,
    ctx: &RowCtx,
) -> Option<Line<'static>> {
    let day = day_of(msg.ts, ctx.zone)?;
    if prev.and_then(|p| day_of(p.ts, ctx.zone)) == Some(day) {
        return None;
    }
    let mut text = format!("── {} ", day.format("%Y-%m-%d"));
    let width = usize::from(width);
    let fill = width.saturating_sub(text.chars().count());
    text.extend(std::iter::repeat_n('─', fill));
    // Chars are all one column wide here; a width narrower than the label clips it.
    let text: String = text.chars().take(width).collect();
    Some(Line::from(Span::styled(text, ctx.theme.messages_notice)))
}

/// Rows `msg` takes in a list: its divider (0 or 1) plus `message_rows`. The reducer's row model
/// uses this, so scrolling agrees with what `divider_row` and `render_rows` draw.
pub fn block_rows(msg: &Message, prev: Option<&Message>, width: u16, ctx: &RowCtx) -> usize {
    usize::from(divider_row(msg, prev, width, ctx).is_some()) + message_rows(msg, width, ctx)
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
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
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
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
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
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
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
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
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
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
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
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
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
                search: None,
                aliases: &crate::ui::emoji::NO_ALIASES,
                own_user: None,
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
                    search: None,
                    aliases: &crate::ui::emoji::NO_ALIASES,
                    own_user: None,
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

    fn edited(author: &str, body: &str) -> Message {
        Message {
            edited: true,
            plain_text: true,
            reactions: Vec::new(),
            ..msg(author, body)
        }
    }

    fn deleted(author: &str) -> Message {
        Message {
            kind: MessageKind::Deleted,
            body: String::new(),
            ..msg(author, "")
        }
    }

    #[test]
    fn edited_marker_ends_the_last_row_in_both_layouts() {
        let theme = Theme::default();
        let m = edited("alice", "hello there");
        let compact = rows_themed(&m, 60, MessageLayout::Compact, &theme);
        assert_eq!(texts(&compact), ["00:00 alice  hello there (edited)"]);
        let marker = compact[0].spans.last().expect("marker span");
        assert_eq!(marker.content, EDITED_MARKER);
        assert_eq!(marker.style, theme.messages_edited);
        assert_ne!(marker.style, theme.messages_body, "the marker must differ");
        let two = rows_themed(&m, 60, MessageLayout::TwoLine, &theme);
        assert_eq!(texts(&two), ["alice  00:00", "  hello there (edited)"]);
        // An unedited message is unchanged.
        let plain = rows_themed(
            &msg("alice", "hello there"),
            60,
            MessageLayout::Compact,
            &theme,
        );
        assert_eq!(texts(&plain), ["00:00 alice  hello there"]);
    }

    #[test]
    fn edited_marker_moves_to_its_own_row_when_it_does_not_fit() {
        let theme = Theme::default();
        // Body width at 40 columns in Compact is 40 - (5 + 1 + 5 + 2) = 27; the body fills it.
        let body = "x".repeat(27);
        let m = edited("alice", &body);
        let rows = rows_at(&m, 40, MessageLayout::Compact, 5);
        assert_eq!(rows.len(), 2);
        assert_eq!(text(&rows[1]).trim(), "(edited)");
        assert_eq!(rows[1].spans.last().unwrap().style, theme.messages_edited);
        // The row count the scroll model uses matches what is drawn.
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
        };
        assert_eq!(message_rows(&m, 40, &ctx), 2);
    }

    #[test]
    fn deleted_line_is_one_dim_row_with_author_and_time() {
        let theme = Theme::default();
        let m = deleted("alice");
        let compact = rows_themed(&m, 60, MessageLayout::Compact, &theme);
        assert_eq!(texts(&compact), ["00:00 alice  message deleted"]);
        let two = rows_themed(&m, 60, MessageLayout::TwoLine, &theme);
        assert_eq!(texts(&two), ["alice  00:00  message deleted"]);
        for rows in [&compact, &two] {
            assert_eq!(rows.len(), 1);
            for span in rows[0]
                .spans
                .iter()
                .filter(|s| !s.content.trim().is_empty())
            {
                assert_eq!(span.style, theme.messages_deleted, "{:?}", span.content);
            }
        }
        // Whatever the model still holds, the text never shows and nor does an edited marker.
        let sneaky = Message {
            body: "SECRET-TEXT".to_string(),
            edited: true,
            plain_text: true,
            reactions: Vec::new(),
            ..deleted("alice")
        };
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_themed(&sneaky, 60, layout, &theme);
            let shown = texts(&rows).join("\n");
            assert!(!shown.contains("SECRET-TEXT"), "{shown}");
            assert!(!shown.contains("edited"), "{shown}");
        }
        // The line never says who deleted it.
        assert!(!texts(&compact).join(" ").contains("deleted by"));
    }

    #[test]
    fn edited_and_deleted_styles_follow_the_theme() {
        let config = crate::test_util::config_from_theme(
            "[messages.edited]\nfg = \"blue\"\n[messages.deleted]\nfg = \"magenta\"\n",
        );
        let theme = config.theme;
        assert_eq!(theme.messages_edited.fg, Some(Color::Blue));
        assert_eq!(theme.messages_edited.add_modifier, Modifier::DIM);
        assert_eq!(theme.messages_deleted.fg, Some(Color::Magenta));
        let rows = rows_themed(&edited("alice", "hi"), 60, MessageLayout::Compact, &theme);
        assert_eq!(rows[0].spans.last().unwrap().style.fg, Some(Color::Blue));
        let rows = rows_themed(&deleted("alice"), 60, MessageLayout::TwoLine, &theme);
        assert!(
            rows[0]
                .spans
                .iter()
                .any(|s| s.style.fg == Some(Color::Magenta))
        );
    }

    #[test]
    fn edited_and_deleted_survive_narrow_widths() {
        for width in [0u16, 1, 4, 9, 12, 19, 25] {
            for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
                for m in [edited("alice", "some words to wrap"), deleted("alice")] {
                    let rows = rows_at(&m, width, layout, 5);
                    assert!(!rows.is_empty(), "{width} {layout:?}");
                }
            }
        }
    }

    fn msg_at(ts: i64) -> Message {
        Message {
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            id: format!("m{ts}"),
            room_id: RoomId("r".to_string()),
            author: "a".to_string(),
            ts,
            body: "x".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    fn divider_text(msg: &Message, prev: Option<&Message>, width: u16) -> Option<String> {
        let theme = Theme::default();
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 1,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
        };
        divider_row(msg, prev, width, &ctx)
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
    }

    #[test]
    fn divider_on_the_first_message_and_on_each_new_day() {
        let (a, b, c) = (msg_at(100), msg_at(200), msg_at(86_400 + 5));
        let first = divider_text(&a, None, 30).expect("first");
        assert_eq!(first, "── 1970-01-01 ────────────────");
        assert_eq!(divider_text(&b, Some(&a), 30), None, "same day");
        let next = divider_text(&c, Some(&b), 30).expect("new day");
        assert!(next.starts_with("── 1970-01-02 "), "{next}");
        // Newest first lists change day going down, too.
        assert!(divider_text(&b, Some(&c), 30).is_some());
    }

    #[test]
    fn divider_clips_at_narrow_widths_and_skips_bad_times() {
        assert_eq!(divider_text(&msg_at(0), None, 6).as_deref(), Some("── 197"));
        assert_eq!(divider_text(&msg_at(i64::MAX), None, 30), None);
        let ok = msg_at(0);
        assert_eq!(divider_text(&msg_at(i64::MAX), Some(&ok), 30), None);
    }

    // ---- reaction rows ----

    use crate::data::Reaction;
    use std::collections::HashMap;

    fn reaction(name: &str, users: &[&str]) -> Reaction {
        Reaction {
            name: name.to_string(),
            users: users.iter().map(|u| u.to_string()).collect(),
        }
    }

    fn reacted(body: &str, reactions: Vec<Reaction>) -> Message {
        Message {
            reactions,
            ..msg("alice", body)
        }
    }

    /// `render_rows` with aliases and an own username, in the Phase 1 context.
    fn rows_with(
        m: &Message,
        width: u16,
        layout: MessageLayout,
        aliases: &HashMap<String, String>,
        own: Option<&str>,
    ) -> Vec<Line<'static>> {
        let format = TimeFormat::default();
        let theme = Theme::default();
        let ctx = RowCtx {
            layout,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases,
            own_user: own,
        };
        render_rows(m, width, &ctx)
    }

    fn alias_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_message_with_reactions_gets_one_row_under_its_body() {
        let plain = msg("alice", "deploy done");
        let m = reacted(
            "deploy done",
            vec![reaction("+1", &["a", "b", "c"]), reaction("tada", &["d"])],
        );
        let none = HashMap::new();
        let before = rows_with(&plain, 60, MessageLayout::Compact, &none, None);
        let rows = rows_with(&m, 60, MessageLayout::Compact, &none, None);
        assert_eq!(rows.len(), before.len() + 1);
        assert_eq!(
            text(&rows[0]),
            text(&before[0]),
            "the body row is unchanged"
        );
        // Hangs under the body: time (5) + space + author column (5) + two spaces.
        assert_eq!(text(&rows[1]), format!("{}👍 3  🎉 1", " ".repeat(13)));
    }

    #[test]
    fn a_message_without_reactions_is_drawn_as_before() {
        let m = msg("alice", "deploy done");
        let none = HashMap::new();
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            assert_eq!(
                texts(&rows_with(&m, 40, layout, &none, None)),
                texts(&rows_at(&m, 40, layout, 5))
            );
        }
        // A message with only empty reactions list is the same thing.
        assert!(m.reactions.is_empty());
    }

    #[test]
    fn two_line_puts_the_reaction_row_under_the_indented_body() {
        let m = reacted("hello", vec![reaction("+1", &["a"])]);
        let rows = rows_with(&m, 40, MessageLayout::TwoLine, &HashMap::new(), None);
        assert_eq!(texts(&rows)[1..], ["  hello", "  👍 1"]);
    }

    #[test]
    fn reactions_wrap_by_whole_items_and_never_exceed_the_width() {
        let aliases = alias_map(&[("a", "AAAA"), ("b", "BBBB"), ("c", "CCCC")]);
        let m = reacted(
            "x",
            vec![
                reaction("a", &["u"]),
                reaction("b", &["u"]),
                reaction("c", &["u"]),
            ],
        );
        // Two-line, width 16: the body column is 14, so two 6-wide items and the gap just fit.
        let rows = rows_with(&m, 16, MessageLayout::TwoLine, &aliases, None);
        assert_eq!(texts(&rows)[2..], ["  AAAA 1  BBBB 1", "  CCCC 1"]);
        // At every width the reaction rows fit, and every row holds only whole items.
        for width in [12, 14, 16, 20, 30, 52, 80] {
            let rows = rows_with(&m, width, MessageLayout::TwoLine, &aliases, None);
            for line in &rows[2..] {
                let shown = text(line);
                assert!(
                    shown.width() <= usize::from(width).max(12),
                    "{width}: {shown:?}"
                );
                for item in shown.trim().split(REACTION_GAP) {
                    assert!(
                        ["AAAA 1", "BBBB 1", "CCCC 1"].contains(&item),
                        "{width}: {item:?} in {shown:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_item_wider_than_the_row_is_cut_with_an_ellipsis() {
        let aliases = alias_map(&[("x", "WWWWWWWW")]);
        let users = vec!["u"; 100];
        let m = reacted("x", vec![reaction("x", &users)]);
        // Body column 12: "WWWWWWWW 100" is exactly 12 wide, so it fits.
        let rows = rows_with(&m, 14, MessageLayout::TwoLine, &aliases, None);
        assert_eq!(text(&rows[2]), "  WWWWWWWW 100");
        // A narrower row cannot go below MIN_BODY (10), so the item is cut to it.
        let rows = rows_with(&m, 8, MessageLayout::TwoLine, &aliases, None);
        let shown = text(&rows[2]);
        assert!(shown.trim().width() <= MIN_BODY, "{shown:?}");
        assert!(shown.trim().ends_with('\u{2026}'), "{shown:?}");
    }

    #[test]
    fn the_users_own_reactions_take_the_own_style() {
        let theme = Theme::default();
        let m = reacted(
            "x",
            vec![
                reaction("+1", &["alice", "me"]),
                reaction("tada", &["alice"]),
            ],
        );
        let rows = rows_with(&m, 40, MessageLayout::TwoLine, &HashMap::new(), Some("me"));
        let spans = &rows[2].spans;
        // indent, +1 item, gap, tada item
        assert_eq!(spans[1].style, theme.messages_reactions_own);
        assert_eq!(spans[3].style, theme.messages_reactions);
        assert_ne!(spans[1].style, spans[3].style);
        // Without an own username nothing is own.
        let rows = rows_with(&m, 40, MessageLayout::TwoLine, &HashMap::new(), None);
        assert_eq!(rows[2].spans[1].style, theme.messages_reactions);
    }

    #[test]
    fn aliases_and_raw_names_show_in_the_row_and_hostile_names_are_sanitized() {
        let aliases = alias_map(&[("partyparrot", "🦜")]);
        let m = reacted(
            "x",
            vec![
                reaction("partyparrot", &["a", "b"]),
                reaction("unknown_thing", &["a"]),
                reaction("bad\u{1b}[31mname\u{7}", &["a"]),
            ],
        );
        let rows = rows_with(&m, 80, MessageLayout::TwoLine, &aliases, None);
        let shown = text(&rows[2]);
        assert!(shown.contains("🦜 2"), "{shown:?}");
        assert!(shown.contains(":unknown_thing: 1"), "{shown:?}");
        assert!(!shown.chars().any(|c| c.is_control()), "{shown:?}");
    }

    #[test]
    fn usernames_never_appear_in_the_row() {
        let m = reacted("x", vec![reaction("+1", &["SENTINEL-USER-1", "other"])]);
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_with(&m, 80, layout, &HashMap::new(), Some("SENTINEL-USER-1"));
            assert!(!texts(&rows).join("\n").contains("SENTINEL"));
        }
    }

    // ---- emoji shortcodes in the body

    #[test]
    fn shortcodes_in_a_body_are_drawn_as_emoji_in_both_layouts() {
        let m = msg("alice", "ship it :tada: now :+1:");
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_with(&m, 60, layout, &HashMap::new(), None);
            let all = texts(&rows).join("\n");
            assert!(all.contains("ship it 🎉 now 👍"), "{layout:?}: {all}");
            assert!(
                !all.contains(":tada:") && !all.contains(":+1:"),
                "{layout:?}: {all}"
            );
        }
    }

    #[test]
    fn an_alias_applies_in_the_body() {
        let aliases = alias_map(&[("partyparrot", "🦜")]);
        let m = msg("alice", "go :partyparrot: go");
        let rows = rows_with(&m, 60, MessageLayout::TwoLine, &aliases, None);
        assert_eq!(text(&rows[1]), "  go 🦜 go");
    }

    #[test]
    fn what_is_not_a_shortcode_stays_as_written() {
        let m = msg("alice", "at 10:100:20 use `:tada:` and :nope_x: ok");
        let rows = rows_with(&m, 80, MessageLayout::TwoLine, &HashMap::new(), None);
        assert_eq!(
            text(&rows[1]),
            "  at 10:100:20 use `:tada:` and :nope_x: ok"
        );
    }

    #[test]
    fn system_messages_and_deleted_lines_are_not_drawn_with_emoji() {
        let mut system = msg("alice", "* added :tada:");
        system.kind = MessageKind::System;
        let rows = rows_with(&system, 60, MessageLayout::TwoLine, &HashMap::new(), None);
        assert!(texts(&rows).join("\n").contains(":tada:"));
        let mut deleted = msg("alice", ":tada:");
        deleted.kind = MessageKind::Deleted;
        let rows = rows_with(&deleted, 60, MessageLayout::TwoLine, &HashMap::new(), None);
        assert!(!texts(&rows).join("\n").contains('🎉'));
    }

    #[test]
    fn a_message_still_being_sent_draws_like_the_confirmed_one() {
        let mut pending = msg("me", "thanks :+1:");
        pending.send = crate::data::SendState::Sending;
        let rows = rows_with(&pending, 60, MessageLayout::TwoLine, &HashMap::new(), None);
        assert!(texts(&rows).join("\n").contains("thanks 👍"));
    }

    #[test]
    fn lines_wrap_by_the_drawn_width_and_never_exceed_the_pane() {
        // `:tada:` is six columns written and two drawn: the wrap must use the drawn width.
        let body = vec![":tada:"; 40].join(" ");
        let m = msg("alice", &body);
        for width in [12u16, 20, 31, 52, 80] {
            let rows = rows_with(&m, width, MessageLayout::TwoLine, &HashMap::new(), None);
            for row in &rows[1..] {
                let shown = text(row);
                assert!(
                    shown.width() <= usize::from(width).max(MIN_BODY + 2),
                    "{width}: {} wide: {shown:?}",
                    shown.width()
                );
            }
            assert!(!texts(&rows).join(" ").contains(":tada:"));
        }
        // Drawn, 40 emoji with their spaces take fewer rows than the written text would.
        let drawn = rows_with(&m, 40, MessageLayout::TwoLine, &HashMap::new(), None).len();
        let mut plain = m.clone();
        plain.kind = MessageKind::System;
        let written = rows_with(&plain, 40, MessageLayout::TwoLine, &HashMap::new(), None).len();
        assert!(drawn < written, "{drawn} rows drawn, {written} written");
    }

    #[test]
    fn the_row_model_counts_the_drawn_rows() {
        let format = TimeFormat::default();
        let theme = Theme::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
        };
        let m = msg("alice", &vec![":tada:"; 40].join(" "));
        assert_eq!(message_rows(&m, 40, &ctx), render_rows(&m, 40, &ctx).len());
        assert_eq!(
            block_rows(&m, None, 40, &ctx),
            message_rows(&m, 40, &ctx) + 1
        );
    }

    #[test]
    fn hostile_text_is_still_sanitized_after_the_substitution() {
        let m = msg("alice", "bad \u{1b}[31m :tada: \u{7}x");
        let rows = rows_with(&m, 60, MessageLayout::TwoLine, &HashMap::new(), None);
        let all = texts(&rows).join("\n");
        assert!(all.contains('🎉'));
        assert!(!all.chars().any(|c| c.is_control() && c != '\n'), "{all:?}");
    }

    #[test]
    fn a_reaction_named_with_a_name_the_server_uses_draws_as_its_emoji() {
        let m = reacted("x", vec![reaction("flag_af", &["a", "b"])]);
        let rows = rows_with(&m, 40, MessageLayout::TwoLine, &HashMap::new(), None);
        assert_eq!(text(&rows[2]), "  🇦🇫 2");
        assert!(!texts(&rows).join("\n").contains("flag_af"));
    }

    #[test]
    fn a_deleted_message_shows_no_reaction_row() {
        let mut m = reacted("x", vec![reaction("+1", &["a"])]);
        m.kind = MessageKind::Deleted;
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_with(&m, 40, layout, &HashMap::new(), None);
            assert_eq!(rows.len(), 1, "{:?}", texts(&rows));
        }
    }

    #[test]
    fn block_rows_counts_the_reaction_rows() {
        let format = TimeFormat::default();
        let theme = Theme::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            own_user: None,
        };
        let plain = msg("alice", "hello");
        let m = reacted(
            "hello",
            vec![reaction("+1", &["a"]), reaction("tada", &["b"])],
        );
        assert_eq!(
            block_rows(&m, None, 60, &ctx),
            block_rows(&plain, None, 60, &ctx) + 1
        );
        assert_eq!(
            block_rows(&m, None, 60, &ctx),
            message_rows(&m, 60, &ctx) + 1
        );
    }
}
