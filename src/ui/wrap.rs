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
    /// An `author ... time` header row (the time at the right edge), then the body indented by 2.
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
    wrap_clean(&sanitize(text), width)
}

/// `wrap_body` for text that is already sanitized, and so may hold the client's own emoji cells
/// (`ui::emoji`), which sanitising would remove.
fn wrap_clean(clean: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = textwrap::wrap(clean, width.max(1))
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
    /// The server's custom emojis and how they are drawn now (`app::emoji`): asked after the
    /// aliases and the unicode table.
    pub emoji: crate::app::emoji::EmojiView<'a>,
    /// The user's own username, to tell their reactions from the others (`None` until known).
    pub own_user: Option<&'a str>,
    /// The id of the message the `unread` divider is drawn above, when this list is the open
    /// room (`App::pane_row_ctx`); `None` for every other list.
    pub unread_before: Option<&'a str>,
    /// The ids of the threads the server lists as unread for the room this list belongs to, which
    /// give a thread marker row its `new` mark; `None` for a list with no room of its own.
    pub unread_threads: Option<&'a std::collections::HashSet<String>>,
    /// True when the list is the thread pane: every message there is of the thread, so a reply
    /// shown in the channel carries no `[thread]` tag.
    pub in_thread: bool,
    /// The user's own id, to tell whether they follow a thread (`None` until known).
    pub own_id: Option<&'a str>,
    /// The longest pause, in seconds, between two messages of one author that still share a
    /// header (`continues`); `None` draws every message with its own. Set only for the room pane
    /// in the two-line layout and for the thread pane.
    pub grouping: Option<u32>,
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
    // The thread pane always uses the two-line layout, whatever the setting.
    let layout = if ctx.in_thread {
        MessageLayout::TwoLine
    } else {
        ctx.layout
    };
    match layout {
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
            let body_width = width
                .saturating_sub(prefix_width + tag_width(msg, ctx))
                .max(MIN_BODY);
            let indent = " ".repeat(prefix_width);
            tag_reply(msg, body_rows(msg, body_width, body_style, ctx), ctx)
                .into_iter()
                .enumerate()
                .map(|(i, mut spans)| {
                    if i == 0 {
                        let prefix = vec![
                            Span::styled(time.clone(), time_style),
                            Span::raw(" "),
                            Span::styled(author.clone(), name),
                            Span::raw(pad.clone()),
                        ];
                        spans.splice(0..0, prefix);
                    } else {
                        spans.insert(0, Span::raw(indent.clone()));
                    }
                    Line::from(spans)
                })
                .collect()
        }
        MessageLayout::TwoLine => {
            let body_width = width.saturating_sub(2 + tag_width(msg, ctx)).max(MIN_BODY);
            // The author at the left and the time at the right edge, as in the thread pane. A
            // tombstone is one row: its text follows the author.
            let mut header = vec![Span::styled(author, name)];
            if deleted {
                header.push(Span::raw("  "));
                header.push(Span::styled(DELETED_TEXT, body_style));
            }
            let used: usize = header.iter().map(|s| s.content.width()).sum();
            let gap = width.saturating_sub(used + time.width()).max(2);
            header.push(Span::raw(" ".repeat(gap)));
            header.push(Span::styled(time, time_style));
            if deleted {
                return vec![Line::from(header)];
            }
            std::iter::once(Line::from(header))
                .chain(
                    tag_reply(msg, body_rows(msg, body_width, body_style, ctx), ctx)
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
    // Sanitising comes first: the emoji cells the next step writes are the client's own, and server
    // text must never be able to hold one.
    let clean = sanitize(&msg.body);
    let drawn = if msg.kind == MessageKind::Normal {
        crate::ui::emoji::draw_shortcodes(&clean, ctx.aliases, &ctx.emoji)
    } else {
        clean
    };
    // Mentions and channel references go through wrapping as zero-width markers (`ui::mention`),
    // so a token cut by a wrap keeps its style on both rows. Only an ordinary message has any.
    let drawn = if msg.kind == MessageKind::Normal {
        crate::ui::mention::mark(&drawn, msg.refs.as_ref(), ctx.own_user)
    } else {
        drawn
    };
    let (mut inside, mut open) = (false, None);
    let mut rows: Vec<Vec<Span<'static>>> = wrap_clean(&drawn, width)
        .into_iter()
        .map(|row| {
            let (row, marks) = crate::ui::mention::strip(&row, &mut open);
            let spans = highlight_row(row, body_style, query, ctx.theme, &marks);
            crate::ui::emoji::expand(spans, ctx.theme.messages_custom_emoji, &mut inside)
        })
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
    rows.extend(thread_rows(msg, width, ctx));
    rows
}

/// The text of the thread marker row of a message with replies: `-> 3 replies, last 14:02`, or
/// `-> 1 reply, last 14:02`; without a last-reply time just the count. A thread the user follows
/// ends with `, following`. Plain ASCII, so the row is
/// one cell per character.
fn thread_marker_text(msg: &Message, ctx: &RowCtx) -> String {
    let count = msg.tcount;
    let noun = if count == 1 { "reply" } else { "replies" };
    // The thread pane's title already says it, so its parent's row does not repeat it.
    let following = if !ctx.in_thread && ctx.own_id.is_some_and(|id| msg.followed_by(id)) {
        ", following"
    } else {
        ""
    };
    let new = if thread_is_unread(msg, ctx) {
        THREAD_UNREAD_WORD_SUFFIX
    } else {
        ""
    };
    match msg.tlm {
        Some(tlm) => format!(
            "-> {count} {noun}, last {}{following}{new}",
            format_ts(ctx.time_format, tlm, ctx.zone)
        ),
        None => format!("-> {count} {noun}{following}{new}"),
    }
}

/// What ends the marker row of a thread with replies the user has not read; its last word is drawn
/// in `theme.messages_thread_unread`.
const THREAD_UNREAD_WORD_SUFFIX: &str = ", new";

/// True when the server lists the thread whose parent is `msg` as unread. The state is the
/// server's: it is never worked out from the replies the app holds.
fn thread_is_unread(msg: &Message, ctx: &RowCtx) -> bool {
    ctx.unread_threads
        .is_some_and(|threads| threads.contains(&msg.id))
}

/// What starts the body of a reply that is also shown in the channel, in
/// `theme.messages_thread_reply_hint`. Its width is what the text of such a reply gives up.
const THREAD_REPLY_TAG: &str = "[thread] ";

/// True when the body of `msg` starts with the thread-reply tag: a reply that is also shown in
/// the channel.
fn shows_reply_mark(msg: &Message, ctx: &RowCtx) -> bool {
    !ctx.in_thread && msg.tmid.is_some() && msg.tshow && msg.kind != MessageKind::Deleted
}

/// A reply shown in the channel as body rows: the first row starts with `THREAD_REPLY_TAG`, the
/// others are indented by its width so the text lines up, and every text span is italic. Any other
/// message is returned as it is.
fn tag_reply(
    msg: &Message,
    rows: Vec<Vec<Span<'static>>>,
    ctx: &RowCtx,
) -> Vec<Vec<Span<'static>>> {
    if !shows_reply_mark(msg, ctx) {
        return rows;
    }
    rows.into_iter()
        .enumerate()
        .map(|(i, spans)| {
            let lead = if i == 0 {
                Span::styled(
                    THREAD_REPLY_TAG,
                    ctx.theme
                        .messages_thread_reply_hint
                        .add_modifier(ratatui::style::Modifier::ITALIC),
                )
            } else {
                Span::raw(" ".repeat(THREAD_REPLY_TAG.len()))
            };
            let mut out = vec![lead];
            out.extend(spans.into_iter().map(|mut span| {
                span.style = span.style.add_modifier(ratatui::style::Modifier::ITALIC);
                span
            }));
            out
        })
        .collect()
}

/// Columns the tag takes from the body of `msg`: 0 unless it is a reply shown in the channel.
fn tag_width(msg: &Message, ctx: &RowCtx) -> usize {
    if shows_reply_mark(msg, ctx) {
        THREAD_REPLY_TAG.len()
    } else {
        0
    }
}

/// The thread marker row of `msg` at `width` columns, as spans without any prefix, after the body
/// and the reaction rows: `theme.messages_thread` on a message that has replies, wrapped to
/// `width`. Any other message has none. (A reply shown in the channel is tagged at the start of
/// its body instead, see `tag_reply`.)
fn thread_rows(msg: &Message, width: usize, ctx: &RowCtx) -> Vec<Vec<Span<'static>>> {
    if msg.tmid.is_some() || msg.tcount == 0 {
        return Vec::new();
    }
    let unread = thread_is_unread(msg, ctx);
    // An unread thread's whole marker row takes the unread style, so it stands out in the pane.
    let style = if unread {
        ctx.theme.messages_thread_unread
    } else {
        ctx.theme.messages_thread
    };
    wrap_clean(&thread_marker_text(msg, ctx), width)
        .into_iter()
        .map(|row| vec![Span::styled(row, style)])
        .collect()
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
        // A custom emoji is written as the client's own cells (its image, a stand-in or its name,
        // see `ui::emoji`) and expanded into spans below; anything else is `resolve`.
        let label =
            match crate::ui::emoji::lookup_found(&reaction.name, ctx.aliases, ctx.emoji.custom) {
                Some(crate::ui::emoji::Found::Glyph(glyph)) => glyph,
                Some(crate::ui::emoji::Found::Custom(own)) => {
                    crate::ui::emoji::custom_cells(&own, ctx.emoji.look(&own))
                }
                None => crate::ui::emoji::resolve(&reaction.name, ctx.aliases),
            };
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
        // Every cell measures one column, like what it is drawn as, so the width is the same
        // before and after the expansion.
        let item = text.width();
        let spans = crate::ui::emoji::expand(
            vec![Span::styled(text, style)],
            ctx.theme.messages_custom_emoji,
            &mut false,
        );
        match rows.last_mut() {
            Some(row) if used + REACTION_GAP.width() + item <= width => {
                row.push(Span::raw(REACTION_GAP));
                row.extend(spans);
                used += REACTION_GAP.width() + item;
            }
            _ => {
                rows.push(spans);
                used = item;
            }
        }
    }
    rows
}

/// One wrapped body row as spans: the text outside a search match in `body_style`, each match in
/// `theme.search_match`, and each of `marks` (the mentions of the row, from `mention::strip`) in
/// its kind's style over `body_style`; a search match wins where the two meet. Without a query or
/// a mention in this row it is the one plain span. A search match that crosses a wrap boundary is
/// found in neither row and so is not highlighted (it still counts when jumping, which looks at
/// the whole body).
fn highlight_row(
    row: String,
    body_style: ratatui::style::Style,
    query: Option<&str>,
    theme: &crate::config::Theme,
    marks: &[(std::ops::Range<usize>, crate::ui::mention::Kind)],
) -> Vec<Span<'static>> {
    let ranges = query.map_or_else(Vec::new, |q| crate::app::search::find(&row, q));
    if ranges.is_empty() && marks.is_empty() {
        return vec![Span::styled(row, body_style)];
    }
    // Cut the row wherever a search match or a mention starts or ends, then style each piece: a
    // search match wins, then the kind of mention laid over the body style, else the body style.
    let mut cuts: Vec<usize> = ranges
        .iter()
        .map(|r| (r.start, r.end))
        .chain(marks.iter().map(|(r, _)| (r.start, r.end)))
        .flat_map(|(a, b)| [a, b])
        .chain([0, row.len()])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| (w[0], w[1]))
        .map(|(a, b)| {
            let style = if ranges.iter().any(|r| r.start <= a && b <= r.end) {
                theme.search_match
            } else if let Some((_, kind)) = marks.iter().find(|(r, _)| r.start <= a && b <= r.end) {
                body_style.patch(kind.style(theme))
            } else {
                body_style
            };
            Span::styled(row[a..b].to_string(), style)
        })
        .collect()
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
/// divider is one row of `width` columns: `── YYYY-MM-DD ───…` in `theme.messages_time`.
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
    Some(Line::from(Span::styled(text, ctx.theme.messages_time)))
}

/// The `unread` divider drawn above `msg`, or `None` when the divider is not above this message.
/// It is one row of `width` columns, `── unread ───…`, in `theme.messages_unread_divider`.
pub fn unread_row(msg: &Message, width: u16, ctx: &RowCtx) -> Option<Line<'static>> {
    if ctx.unread_before != Some(msg.id.as_str()) {
        return None;
    }
    let mut text = String::from("── unread ");
    let width = usize::from(width);
    let fill = width.saturating_sub(text.chars().count());
    text.extend(std::iter::repeat_n('─', fill));
    let text: String = text.chars().take(width).collect();
    Some(Line::from(Span::styled(
        text,
        ctx.theme.messages_unread_divider,
    )))
}

/// True when `msg` continues the group `prev` belongs to, whose first message is `head`: it is
/// drawn without a header row. Needs `ctx.grouping`, two `Normal`, `Sent` messages of one author
/// and thread, `msg` no more than that many seconds after `head` (the group's first message, so a
/// long burst splits where the time runs out), no divider above `msg`, and no thread marker row
/// under `prev`. `head` is `prev` itself when `prev` starts its group.
pub fn continues(
    msg: &Message,
    prev: Option<&Message>,
    head: Option<&Message>,
    ctx: &RowCtx,
) -> bool {
    let (Some(gap), Some(prev), Some(head)) = (ctx.grouping, prev, head) else {
        return false;
    };
    let plain = |m: &Message| m.kind == MessageKind::Normal && m.send == SendState::Sent;
    plain(msg)
        && plain(prev)
        && msg.author == prev.author
        && msg.tmid == prev.tmid
        && msg.ts >= prev.ts
        && (0..=i64::from(gap)).contains(&(msg.ts - head.ts))
        && divider_row(msg, Some(prev), 0, ctx).is_none()
        && ctx.unread_before != Some(msg.id.as_str())
        && !(prev.tmid.is_none() && prev.tcount > 0)
}

/// For every message of `msgs`, whether it continues the one before it (`continues`), tracking
/// where each group started. One pass, so the drawing and the row model share the answer.
pub fn continuation_flags(msgs: &[Message], ctx: &RowCtx) -> Vec<bool> {
    let mut head = 0;
    (0..msgs.len())
        .map(|i| {
            let prev = i.checked_sub(1).map(|p| &msgs[p]);
            let joins = continues(&msgs[i], prev, Some(&msgs[head]), ctx);
            if !joins {
                head = i;
            }
            joins
        })
        .collect()
}

/// The lines `msg` takes in a list: its dividers (0 to 2), then its rows, without the header row
/// when it `continues` the group before it (see `continuation_flags`). The drawing and the
/// reducer's row model both come through here, so scrolling agrees with what is drawn. Also
/// returns how many leading lines are dividers, so a caller can leave them out of the cursor
/// highlight.
pub fn block_lines(
    msg: &Message,
    prev: Option<&Message>,
    continues: bool,
    width: u16,
    ctx: &RowCtx,
) -> (usize, Vec<Line<'static>>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.extend(divider_row(msg, prev, width, ctx));
    lines.extend(unread_row(msg, width, ctx));
    let dividers = lines.len();
    let mut rows = render_rows(msg, width, ctx);
    if continues {
        rows.remove(0);
    }
    lines.extend(rows);
    (dividers, lines)
}

/// Rows `msg` takes in a list: the length of `block_lines`.
pub fn block_rows(
    msg: &Message,
    prev: Option<&Message>,
    continues: bool,
    width: u16,
    ctx: &RowCtx,
) -> usize {
    block_lines(msg, prev, continues, width, ctx).1.len()
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
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        let rows = render_rows(&m, 60, &ctx);
        let first = &rows[0].spans[0];
        assert_eq!(first.content, "2026-11-14 00:00");
        assert_eq!(first.style.fg, Some(Color::Gray));
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
        // The time is a plain light grey, not dimmed.
        assert_eq!(spans[0].style.fg, Some(Color::Gray));
        assert!(!spans[0].style.add_modifier.contains(Modifier::DIM));
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
        assert!(!spans[0].style.add_modifier.contains(Modifier::DIM));
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
        assert_eq!(text(&rows[0]), format!("alice{}00:00", " ".repeat(30 - 10)));
        let spans = &rows[0].spans;
        assert_eq!(
            spans[0].style.fg,
            Some(author_color("alice", &DEFAULT_AUTHOR_PALETTE))
        );
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].style, Style::default());
        assert_eq!(spans[2].style.fg, Some(Color::Gray), "the time");
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
        assert_eq!(rows[0], format!("alice{}00:00", " ".repeat(30 - 10)));
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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
                emoji: crate::app::emoji::EmojiView::none(),
                own_user: None,
                unread_before: None,
                unread_threads: None,
                in_thread: false,
                grouping: None,
                own_id: None,
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
                    emoji: crate::app::emoji::EmojiView::none(),
                    own_user: None,
                    unread_before: None,
                    unread_threads: None,
                    in_thread: false,
                    grouping: None,
                    own_id: None,
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
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
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
        assert_eq!(
            texts(&two),
            [
                format!("alice{}00:00", " ".repeat(60 - 10)),
                "  hello there (edited)".to_string()
            ]
        );
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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
        assert_eq!(
            texts(&two),
            [format!(
                "alice  message deleted{}00:00",
                " ".repeat(60 - "alice  message deleted".len() - "00:00".len())
            )]
        );
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
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
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
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
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

    // ---- thread rows ----

    fn parent(replies: u32, tlm: Option<i64>) -> Message {
        Message {
            tcount: replies,
            tlm,
            ..msg("alice", "hello")
        }
    }

    fn thread_text(m: &Message, width: u16, layout: MessageLayout) -> Vec<String> {
        texts(&rows_with(m, width, layout, &HashMap::new(), None))
    }

    // 14:02 UTC on 1970-01-02.
    const AT_1402: i64 = 86_400 + 14 * 3600 + 2 * 60;

    #[test]
    fn a_parent_gets_one_marker_row_with_the_count_and_the_last_time() {
        let m = parent(3, Some(AT_1402));
        let compact = thread_text(&m, 60, MessageLayout::Compact);
        assert_eq!(compact.len(), 2);
        // Aligned with the body: the time column, the author column and their gaps.
        assert_eq!(
            compact[1],
            format!("{}-> 3 replies, last 14:02", " ".repeat(5 + 1 + 5 + 2))
        );
        let two = thread_text(&m, 60, MessageLayout::TwoLine);
        assert_eq!(two.len(), 3, "header, body, marker");
        assert_eq!(two[2], "  -> 3 replies, last 14:02");
    }

    #[test]
    fn one_reply_is_singular_and_no_time_is_just_the_count() {
        let one = thread_text(&parent(1, Some(AT_1402)), 60, MessageLayout::TwoLine);
        assert_eq!(one[2], "  -> 1 reply, last 14:02");
        let bare = thread_text(&parent(2, None), 60, MessageLayout::TwoLine);
        assert_eq!(bare[2], "  -> 2 replies");
    }

    #[test]
    fn a_message_without_replies_is_drawn_as_before() {
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let plain = rows_with(&msg("alice", "hello"), 60, layout, &HashMap::new(), None);
            let none = rows_with(&parent(0, None), 60, layout, &HashMap::new(), None);
            assert_eq!(texts(&plain), texts(&none));
            assert_eq!(
                none.len(),
                if layout == MessageLayout::Compact {
                    1
                } else {
                    2
                }
            );
        }
    }

    #[test]
    fn the_marker_comes_after_the_body_and_the_reaction_row() {
        let m = Message {
            reactions: vec![reaction("+1", &["a"])],
            ..parent(2, Some(AT_1402))
        };
        let rows = thread_text(&m, 60, MessageLayout::TwoLine);
        assert_eq!(rows.len(), 4);
        assert!(rows[2].contains('1'), "{rows:?}");
        assert!(rows[3].contains("replies"), "{rows:?}");
    }

    #[test]
    fn the_marker_wraps_in_a_narrow_pane_and_stays_plain_ascii() {
        let m = parent(12, Some(AT_1402));
        for width in [10u16, 14, 20, 30] {
            let rows = thread_text(&m, width, MessageLayout::TwoLine);
            let marker = &rows[2..];
            assert!(!marker.is_empty());
            for row in &rows {
                assert!(
                    row.width() <= usize::from(width).max(MIN_BODY + 2),
                    "{width}: {rows:?}"
                );
            }
            assert!(
                marker
                    .join(" ")
                    .chars()
                    .all(|c| c.is_ascii() && !c.is_control()),
                "{marker:?}"
            );
        }
        let narrow = thread_text(&m, 14, MessageLayout::TwoLine);
        assert!(narrow.len() > 3, "wrapped onto more rows: {narrow:?}");
    }

    #[test]
    fn a_channel_visible_reply_starts_its_body_with_the_thread_tag_in_italics() {
        use ratatui::style::Modifier;
        let theme = Theme::default();
        let reply = |tshow| Message {
            tmid: Some("p".to_string()),
            tshow,
            ..msg("alice", "hello")
        };
        let italic = |line: &Line<'_>, from: usize| {
            line.spans[from..]
                .iter()
                .all(|s| s.style.add_modifier.contains(Modifier::ITALIC))
        };
        // Two-line: no extra row; the tag follows the indent and the text italic after it.
        let plain = rows_with(
            &msg("alice", "hello"),
            60,
            MessageLayout::TwoLine,
            &HashMap::new(),
            None,
        );
        let rows = rows_with(
            &reply(true),
            60,
            MessageLayout::TwoLine,
            &HashMap::new(),
            None,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(text(&rows[1]), "  [thread] hello");
        assert_eq!(text(&rows[0]), text(&plain[0]), "the header is as before");
        assert_eq!(
            rows[1].spans[1].style,
            theme
                .messages_thread_reply_hint
                .add_modifier(Modifier::ITALIC)
                .patch(ratatui::style::Style::default())
        );
        assert!(italic(&rows[1], 1), "tag and body are italic");
        assert!(
            !rows[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::ITALIC)),
            "author and time are not"
        );
        // Compact: the tag starts the body and the text moves right by its width.
        let marked = rows_with(
            &reply(true),
            60,
            MessageLayout::Compact,
            &HashMap::new(),
            None,
        );
        assert_eq!(marked.len(), 1);
        assert_eq!(text(&marked[0]), "00:00 alice  [thread] hello");
        assert!(italic(&marked[0], 4), "tag and body are italic");
        assert!(
            !marked[0].spans[..3]
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::ITALIC))
        );
        // A reply that is not shown in the channel is not drawn here at all, and a plain message
        // has no tag and no italics.
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            for m in [reply(false), msg("alice", "hello")] {
                let rows = rows_with(&m, 60, layout, &HashMap::new(), None);
                assert!(!texts(&rows).join("\n").contains("[thread]"));
                assert!(rows.iter().all(|r| {
                    !r.spans
                        .iter()
                        .any(|s| s.style.add_modifier.contains(Modifier::ITALIC))
                }));
            }
        }
    }

    #[test]
    fn a_tagged_replys_wrapped_rows_stay_under_its_text() {
        let long = Message {
            tmid: Some("p".to_string()),
            tshow: true,
            ..msg("alice", "one two three four five six seven eight nine ten")
        };
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = texts(&rows_with(&long, 36, layout, &HashMap::new(), None));
            let body: Vec<&String> = rows.iter().filter(|r| !r.starts_with("alice")).collect();
            assert!(body.len() >= 2, "{rows:?}");
            assert!(body[0].contains("[thread] one"));
            let first = body[0].split("one").next().unwrap_or("").chars().count();
            for row in &body[1..] {
                assert!(row.width() <= 36, "{row:?}");
                assert_eq!(
                    row.chars().take_while(|c| *c == ' ').count(),
                    first,
                    "continuation rows line up with the text: {rows:?}"
                );
            }
        }
    }

    #[test]
    fn the_marker_takes_its_theme_style() {
        let theme = Theme::default();
        let rows = rows_with(
            &parent(3, Some(AT_1402)),
            60,
            MessageLayout::TwoLine,
            &HashMap::new(),
            None,
        );
        assert_eq!(rows[2].spans[1].style, theme.messages_thread);
    }

    #[test]
    fn a_deleted_parent_shows_no_marker() {
        let m = Message {
            kind: MessageKind::Deleted,
            ..parent(3, Some(AT_1402))
        };
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = thread_text(&m, 60, layout);
            assert!(!rows.join("\n").contains("replies"), "{rows:?}");
        }
    }

    #[test]
    fn block_rows_counts_the_marker_row() {
        let theme = Theme::default();
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::TwoLine,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        let with = parent(3, Some(AT_1402));
        let without = msg("alice", "hello");
        assert_eq!(
            block_rows(&with, None, false, 60, &ctx),
            block_rows(&without, None, false, 60, &ctx) + 1
        );
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: own,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        render_rows(m, width, &ctx)
    }

    /// `rows_with` with the server's custom emojis loaded.
    fn rows_custom(
        m: &Message,
        width: u16,
        layout: MessageLayout,
        custom: &crate::app::emoji::CustomEmojis,
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
            aliases: &crate::ui::emoji::NO_ALIASES,
            emoji: crate::app::emoji::EmojiView {
                custom,
                ..crate::app::emoji::EmojiView::none()
            },
            own_user: Some("me"),
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        render_rows(m, width, &ctx)
    }

    fn customs(list: &[(&str, &[&str])]) -> crate::app::emoji::CustomEmojis {
        crate::app::emoji::CustomEmojis::Loaded(
            list.iter()
                .map(|(n, a)| crate::data::CustomEmoji {
                    name: n.to_string(),
                    aliases: a.iter().map(|x| x.to_string()).collect(),
                    ..Default::default()
                })
                .collect(),
        )
    }

    /// The span styles that cover `needle` in a row, to check what is drawn in the custom style.
    fn style_of(row: &Line<'_>, needle: &str) -> Option<ratatui::style::Style> {
        row.spans
            .iter()
            .find(|sp| sp.content.contains(needle))
            .map(|sp| sp.style)
    }

    fn with_refs(body: &str, mentions: &[&str], channels: &[&str]) -> Message {
        let mut m = msg("alice", body);
        m.refs = Some(crate::data::Refs {
            mentions: mentions.iter().map(|s| s.to_string()).collect(),
            channels: channels.iter().map(|s| s.to_string()).collect(),
        });
        m
    }

    #[test]
    fn a_listed_mention_is_drawn_in_the_mention_style_and_the_words_are_unchanged() {
        let theme = Theme::default();
        let m = with_refs("thanks @bob, see #dev-chat", &["bob"], &["dev-chat"]);
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            Some("me"),
        );
        assert_eq!(rows.len(), 1);
        assert!(text(&rows[0]).ends_with("thanks @bob, see #dev-chat"));
        let body = theme.messages_body;
        assert_eq!(
            style_of(&rows[0], "@bob"),
            Some(body.patch(theme.messages_mention))
        );
        assert_eq!(
            style_of(&rows[0], "#dev-chat"),
            Some(body.patch(theme.messages_channel))
        );
        assert_eq!(style_of(&rows[0], "thanks "), Some(body));
        assert_clean(&texts(&rows));
    }

    #[test]
    fn the_own_name_and_the_group_words_have_their_own_styles() {
        let theme = Theme::default();
        let m = with_refs("ping @Me and @bob, @here @all", &["me", "bob"], &[]);
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            Some("me"),
        );
        let body = theme.messages_body;
        assert_eq!(
            style_of(&rows[0], "@Me"),
            Some(body.patch(theme.messages_mention_self))
        );
        assert_eq!(
            style_of(&rows[0], "@bob"),
            Some(body.patch(theme.messages_mention))
        );
        assert_eq!(
            style_of(&rows[0], "@here"),
            Some(body.patch(theme.messages_mention_group))
        );
        assert_eq!(
            style_of(&rows[0], "@all"),
            Some(body.patch(theme.messages_mention_group))
        );
    }

    #[test]
    fn an_email_address_and_a_code_span_stay_plain() {
        let theme = Theme::default();
        let m = with_refs("mail bob@example.com or `ping @bob`", &[], &[]);
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        for row in &rows {
            for sp in &row.spans {
                if sp.content.contains("example") || sp.content.contains("ping") {
                    assert_eq!(sp.style, theme.messages_body, "{:?}", sp.content);
                }
            }
        }
        // Even by shape (no lists), the glued address is plain.
        let mut m = m;
        m.refs = None;
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        for sp in &rows[0].spans {
            assert_eq!(sp.style.bg, None, "{:?}", sp.content);
            assert_ne!(sp.style, theme.messages_mention, "{:?}", sp.content);
        }
    }

    #[test]
    fn a_message_without_lists_is_matched_by_shape() {
        let theme = Theme::default();
        let mut m = msg("alice", "cc @carol");
        m.refs = None;
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        assert_eq!(
            style_of(&rows[0], "@carol"),
            Some(theme.messages_body.patch(theme.messages_mention))
        );
    }

    #[test]
    fn a_mention_split_by_wrapping_is_styled_on_both_rows_and_adds_no_rows() {
        let theme = Theme::default();
        let name = "averyveryverylongusernamethatcannotfit";
        let body = format!("hi @{name}");
        let m = with_refs(&body, &[name], &[]);
        let plain = msg("alice", &body);
        // A pane so narrow that the token must be cut.
        let width = 22;
        let rows = rows_with(
            &m,
            width,
            MessageLayout::TwoLine,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        let base = rows_with(
            &plain_without_lists(plain),
            width,
            MessageLayout::TwoLine,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        let styled: Vec<&Line> = rows
            .iter()
            .filter(|r| {
                r.spans
                    .iter()
                    .any(|sp| sp.style == theme.messages_body.patch(theme.messages_mention))
            })
            .collect();
        assert!(styled.len() >= 2, "{:?}", texts(&rows));
        // Both are the same text, row for row, as when nothing is highlighted.
        assert_eq!(rows.len(), base.len());
        assert_eq!(texts(&rows), texts(&base));
        assert_clean(&texts(&rows));
    }

    /// `m` with empty lists: the same text, nothing highlighted.
    fn plain_without_lists(mut m: Message) -> Message {
        m.refs = Some(crate::data::Refs::default());
        m
    }

    #[test]
    fn a_system_message_and_a_deleted_line_are_not_highlighted() {
        let theme = Theme::default();
        let mut m = with_refs("@bob joined", &["bob"], &[]);
        m.kind = MessageKind::System;
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        for sp in &rows[0].spans {
            assert_ne!(
                sp.style,
                theme.messages_notice.patch(theme.messages_mention)
            );
        }
        assert!(text(&rows[0]).contains("@bob joined"));
        let mut d = with_refs("@bob", &["bob"], &[]);
        d.kind = MessageKind::Deleted;
        let rows = rows_with(
            &d,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        assert!(!text(&rows[0]).contains('@'));
    }

    #[test]
    fn server_text_cannot_forge_a_mention_marker() {
        let theme = Theme::default();
        let forged = "hi \u{2067}x\u{202c}".to_string();
        let m = with_refs(&forged, &[], &[]);
        let rows = rows_with(
            &m,
            60,
            MessageLayout::Compact,
            &crate::ui::emoji::NO_ALIASES,
            None,
        );
        assert!(text(&rows[0]).ends_with("hi x"));
        for sp in &rows[0].spans {
            assert_ne!(
                sp.style,
                theme.messages_body.patch(theme.messages_mention_self)
            );
        }
    }

    #[test]
    fn a_search_match_wins_over_a_mention_where_they_meet() {
        let theme = Theme::default();
        let m = with_refs("hello @bobby there", &["bobby"], &[]);
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::Compact,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: Some("bob"),
            aliases: &crate::ui::emoji::NO_ALIASES,
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        let rows = render_rows(&m, 60, &ctx);
        assert_eq!(style_of(&rows[0], "bob"), Some(theme.search_match));
        assert_eq!(
            style_of(&rows[0], "by"),
            Some(theme.messages_body.patch(theme.messages_mention))
        );
        assert_eq!(text(&rows[0]).trim_end().rsplit(' ').nth(1), Some("@bobby"));
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

    // ---- custom emojis (text fallback)

    #[test]
    fn a_custom_emoji_in_a_body_is_its_name_in_the_custom_style() {
        let list = customs(&[("rtshipit", &["rtship"])]);
        let custom_style = Theme::default().messages_custom_emoji;
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_custom(
                &msg("alice", "go :rtshipit: now :rtship:"),
                60,
                layout,
                &list,
            );
            let all = texts(&rows).join("\n");
            // The alias is drawn exactly as the emoji's own name is.
            assert!(
                all.contains("go :rtshipit: now :rtshipit:"),
                "{layout:?}: {all}"
            );
            let body = rows.iter().find(|r| text(r).contains("go ")).unwrap();
            assert_ne!(custom_style, Theme::default().messages_body);
            assert_eq!(
                style_of(body, ":rtshipit:").map(|s| s.add_modifier),
                Some(custom_style.add_modifier)
            );
            assert_eq!(
                style_of(body, "go ").map(|s| s.add_modifier),
                Some(Theme::default().messages_body.add_modifier),
                "the text around it keeps the body style"
            );
        }
    }

    #[test]
    fn an_unknown_name_and_a_name_in_code_stay_as_written_and_unstyled() {
        let list = customs(&[("rtshipit", &[])]);
        let rows = rows_custom(
            &msg("alice", "use `:rtshipit:` or :rt_unknown:"),
            60,
            MessageLayout::TwoLine,
            &list,
        );
        let all = texts(&rows).join("\n");
        assert!(all.contains("use `:rtshipit:` or :rt_unknown:"), "{all}");
        let body = &rows[1];
        assert_eq!(
            body.spans.len(),
            2,
            "indent and one plain span: {:?}",
            body.spans
        );
    }

    #[test]
    fn unicode_wins_over_a_custom_emoji_of_the_same_name() {
        let list = customs(&[("tada", &[])]);
        let rows = rows_custom(
            &msg("alice", "yay :tada:"),
            60,
            MessageLayout::TwoLine,
            &list,
        );
        assert!(texts(&rows).join("\n").contains("yay 🎉"));
    }

    #[test]
    fn a_custom_name_wider_than_the_pane_is_cut_across_rows_and_stays_styled() {
        let list = customs(&[("rt_a_very_long_custom_emoji_name", &[])]);
        let rows = rows_custom(
            &msg("alice", ":rt_a_very_long_custom_emoji_name:"),
            12,
            MessageLayout::TwoLine,
            &list,
        );
        let custom_style = Theme::default().messages_custom_emoji;
        let body: Vec<&Line> = rows.iter().skip(1).collect();
        assert!(body.len() >= 3);
        for row in body {
            assert!(
                row.spans
                    .iter()
                    .filter(|sp| !sp.content.trim().is_empty())
                    .all(|sp| sp.style.add_modifier == custom_style.add_modifier),
                "{:?}",
                row.spans
            );
            assert!(!text(row).chars().any(|c| c as u32 >= 0x10_8000));
        }
    }

    #[test]
    fn a_custom_reaction_is_its_name_in_the_custom_style_followed_by_the_count() {
        let list = customs(&[("rtshipit", &[])]);
        let m = reacted(
            "x",
            vec![reaction("rtshipit", &["a", "b"]), reaction("+1", &["a"])],
        );
        let rows = rows_custom(&m, 80, MessageLayout::TwoLine, &list);
        let shown = text(&rows[2]);
        assert!(shown.contains(":rtshipit: 2"), "{shown}");
        assert!(shown.contains("👍 1"), "{shown}");
        let custom = Theme::default().messages_custom_emoji;
        assert_eq!(
            style_of(&rows[2], ":rtshipit:").map(|s| s.add_modifier.contains(custom.add_modifier)),
            Some(true)
        );
        // Not loaded: the same name is just a raw name, unstyled.
        let rows = rows_custom(
            &m,
            80,
            MessageLayout::TwoLine,
            &crate::app::emoji::NO_CUSTOM,
        );
        assert!(text(&rows[2]).contains(":rtshipit: 2"));
        assert_ne!(
            style_of(&rows[2], ":rtshipit:").map(|s| s.add_modifier.contains(custom.add_modifier)),
            Some(true)
        );
    }

    #[test]
    fn server_text_cannot_carry_the_clients_emoji_cells() {
        let m = msg(
            "alice",
            "a\u{10FFF0}rtshipit\u{10FFF1}b \u{10EEEE}\u{305}\u{305}",
        );
        let list = customs(&[("rtshipit", &[])]);
        let rows = rows_custom(&m, 60, MessageLayout::TwoLine, &list);
        let all = texts(&rows[1..]).join("\n");
        assert!(all.contains("artshipitb"), "{all:?}");
        assert!(
            !all.contains(':'),
            "no colon was made from a smuggled cell: {all:?}"
        );
        assert!(!all.contains('\u{10EEEE}'));
    }

    // ---- custom emojis (images)

    use crate::app::emoji::{EmojiView, Graphics, ImageState, Images, Look, image_cell};
    use crate::emoji_image::Rgba;

    fn rows_in(
        m: &Message,
        width: u16,
        layout: MessageLayout,
        view: EmojiView<'_>,
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
            aliases: &crate::ui::emoji::NO_ALIASES,
            emoji: view,
            own_user: Some("me"),
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        render_rows(m, width, &ctx)
    }

    fn pixel() -> Rgba {
        Rgba {
            width: 1,
            height: 1,
            pixels: vec![1, 2, 3, 255].into(),
            ..Default::default()
        }
    }

    /// The image states an emoji goes through, as `Images` of a session.
    fn images_in(state: Option<ImageState>) -> Images {
        let mut images = Images::default();
        if let Some(state) = state {
            images.set_state("rtshipit", state);
        }
        images
    }

    fn ready(uploaded: bool) -> ImageState {
        ImageState::Ready {
            etag: String::new(),
            image: pixel(),
            id: 7,
            uploaded,
        }
    }

    fn failed() -> ImageState {
        ImageState::Failed {
            etag: String::new(),
        }
    }

    const IMAGE_BODY: &str = "ship it :rtshipit: and then :rtshipit: again, the quick brown fox \
                              jumps :rtshipit::rtshipit: over the lazy dog :rtshipit: ok";

    #[test]
    fn rows_do_not_change_when_the_image_arrives() {
        let list = customs(&[("rtshipit", &[])]);
        let m = reacted(
            IMAGE_BODY,
            vec![reaction("rtshipit", &["a", "b"]), reaction("tada", &["c"])],
        );
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            for width in 12..=70u16 {
                let mut shapes = Vec::new();
                for state in [None, Some(ready(false)), Some(ready(true))] {
                    let images = images_in(state);
                    let view = EmojiView {
                        custom: &list,
                        aliases: &crate::ui::emoji::NO_ALIASES,
                        images: &images,
                        graphics: Graphics::Supported,
                        now_ms: 0,
                    };
                    let rows = rows_in(&m, width, layout, view);
                    let widths: Vec<usize> = rows.iter().map(|r| r.width()).collect();
                    // Below this the Compact prefix and `MIN_BODY` alone are wider than the pane.
                    if width >= 30 {
                        assert!(
                            widths.iter().all(|w| *w <= usize::from(width)),
                            "{widths:?}"
                        );
                    }
                    shapes.push(widths);
                }
                assert_eq!(
                    shapes[0], shapes[1],
                    "{layout:?} at {width}: stand-in vs not uploaded"
                );
                assert_eq!(
                    shapes[0], shapes[2],
                    "{layout:?} at {width}: stand-in vs image"
                );
            }
        }
    }

    #[test]
    fn a_ready_image_is_drawn_as_placeholder_cells_with_its_id_as_foreground() {
        let list = customs(&[("rtshipit", &[])]);
        let images = images_in(Some(ready(true)));
        let view = EmojiView {
            custom: &list,
            aliases: &crate::ui::emoji::NO_ALIASES,
            images: &images,
            graphics: Graphics::Supported,
            now_ms: 0,
        };
        let rows = rows_in(
            &msg("alice", "go :rtshipit: now"),
            60,
            MessageLayout::TwoLine,
            view,
        );
        let row = &rows[1];
        let run = row
            .spans
            .iter()
            .find(|sp| sp.content.starts_with(crate::kitty::PLACEHOLDER))
            .expect("a placeholder span");
        assert_eq!(
            run.content,
            format!("{}{}", crate::kitty::cell(0), crate::kitty::cell(1))
        );
        assert_eq!(run.style.fg, Some(Color::Rgb(0, 0, 7)));
        // The words around it are plain body text.
        let before = row
            .spans
            .iter()
            .find(|sp| sp.content.contains("go "))
            .unwrap();
        assert_ne!(before.style.fg, Some(Color::Rgb(0, 0, 7)));
        assert!(!text(row).contains(":rtshipit:"));
        // Two columns wide, like the stand-in it replaces.
        assert_eq!(run.content.width(), 2);
    }

    #[test]
    fn the_stand_in_is_two_blank_columns_in_the_custom_style() {
        let list = customs(&[("rtshipit", &[])]);
        let images = images_in(None);
        let view = EmojiView {
            custom: &list,
            aliases: &crate::ui::emoji::NO_ALIASES,
            images: &images,
            graphics: Graphics::Supported,
            now_ms: 0,
        };
        let rows = rows_in(
            &msg("alice", "go :rtshipit: now"),
            60,
            MessageLayout::TwoLine,
            view,
        );
        let shown = text(&rows[1]);
        assert!(shown.contains("go \u{a0}\u{a0} now"), "{shown:?}");
        assert!(!shown.contains(":rtshipit:"));
        assert!(!shown.chars().any(|c| (c as u32) >= 0x10_8000));
    }

    #[test]
    fn a_failed_image_shows_its_name_and_the_message_is_measured_again_once() {
        let list = customs(&[("rtshipit", &[])]);
        let m = msg("alice", "aaaa bbbb :rtshipit: cccc dddd eeee ffff");
        let count = |state: Option<ImageState>| {
            let images = images_in(state);
            let view = EmojiView {
                custom: &list,
                aliases: &crate::ui::emoji::NO_ALIASES,
                images: &images,
                graphics: Graphics::Supported,
                now_ms: 0,
            };
            rows_in(&m, 20, MessageLayout::TwoLine, view)
        };
        let waiting = count(None);
        let ok = count(Some(ready(true)));
        let bad = count(Some(failed()));
        assert_eq!(waiting.len(), ok.len());
        assert!(texts(&bad).join("\n").contains(":rtshipit:"));
        assert!(
            bad.len() >= ok.len(),
            "the full name is wider than two columns"
        );
        // And it stays what it was: asking again gives the same rows.
        assert_eq!(texts(&count(Some(failed()))), texts(&bad));
    }

    #[test]
    fn a_terminal_without_images_draws_the_name_whatever_the_images_do() {
        let list = customs(&[("rtshipit", &[])]);
        let m = reacted(IMAGE_BODY, vec![reaction("rtshipit", &["a"])]);
        let mut all = Vec::new();
        for state in [None, Some(ready(true)), Some(failed())] {
            let images = images_in(state);
            let view = EmojiView {
                custom: &list,
                aliases: &crate::ui::emoji::NO_ALIASES,
                images: &images,
                graphics: Graphics::Unsupported,
                now_ms: 0,
            };
            let rows = rows_in(&m, 40, MessageLayout::TwoLine, view);
            assert!(texts(&rows).join("\n").contains(":rtshipit:"));
            assert!(
                !texts(&rows)
                    .join("\n")
                    .chars()
                    .any(|c| (c as u32) >= 0x10_8000 || c == crate::kitty::PLACEHOLDER)
            );
            all.push(texts(&rows));
        }
        assert_eq!(all[0], all[1]);
        assert_eq!(all[0], all[2]);
    }

    #[test]
    fn a_reaction_with_an_image_keeps_its_width_and_count() {
        let list = customs(&[("rtshipit", &[])]);
        let m = reacted("x", vec![reaction("rtshipit", &["a", "b"])]);
        let widths = |state: Option<ImageState>| {
            let images = images_in(state);
            let view = EmojiView {
                custom: &list,
                aliases: &crate::ui::emoji::NO_ALIASES,
                images: &images,
                graphics: Graphics::Supported,
                now_ms: 0,
            };
            rows_in(&m, 40, MessageLayout::TwoLine, view)
        };
        let waiting = widths(None);
        let ok = widths(Some(ready(true)));
        assert_eq!(waiting[2].width(), ok[2].width());
        assert!(text(&ok[2]).ends_with(" 2"), "{:?}", text(&ok[2]));
        let run = ok[2]
            .spans
            .iter()
            .find(|sp| sp.content.starts_with(crate::kitty::PLACEHOLDER))
            .expect("the image");
        assert_eq!(run.style.fg, Some(Color::Rgb(0, 0, 7)));
    }

    #[test]
    fn two_runs_of_the_same_image_side_by_side_are_two_runs() {
        let list = customs(&[("rtshipit", &[])]);
        let images = images_in(Some(ready(true)));
        let view = EmojiView {
            custom: &list,
            aliases: &crate::ui::emoji::NO_ALIASES,
            images: &images,
            graphics: Graphics::Supported,
            now_ms: 0,
        };
        let rows = rows_in(
            &msg("alice", ":rtshipit::rtshipit:"),
            60,
            MessageLayout::TwoLine,
            view,
        );
        let cells: String = rows[1]
            .spans
            .iter()
            .filter(|sp| sp.content.starts_with(crate::kitty::PLACEHOLDER))
            .map(|sp| sp.content.to_string())
            .collect();
        let one = format!("{}{}", crate::kitty::cell(0), crate::kitty::cell(1));
        assert_eq!(cells, format!("{one}{one}"), "columns 0,1 then 0,1 again");
        let _ = (Look::Text, image_cell(1));
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        let m = msg("alice", &vec![":tada:"; 40].join(" "));
        assert_eq!(message_rows(&m, 40, &ctx), render_rows(&m, 40, &ctx).len());
        assert_eq!(
            block_rows(&m, None, false, 40, &ctx),
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
    fn placement_characters_in_a_username_or_a_reaction_name_are_removed() {
        let cell = "\u{10EEEE}\u{305}\u{30d}\u{10FFF0}\u{108000}";
        let mut m = reacted("x", vec![reaction(&format!("na{cell}me"), &["a"])]);
        m.author = format!("al{cell}ice");
        for layout in [MessageLayout::Compact, MessageLayout::TwoLine] {
            let rows = rows_with(&m, 60, layout, &HashMap::new(), None);
            let all = texts(&rows).join("\n");
            assert!(
                !all.chars().any(|c| (c as u32) >= 0x10_8000),
                "{layout:?}: {all:?}"
            );
            assert!(all.contains("alice"), "{layout:?}: {all:?}");
            assert!(all.contains(":name:"), "{layout:?}: {all:?}");
        }
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
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: None,
            unread_threads: None,
            in_thread: false,
            grouping: None,
            own_id: None,
        };
        let plain = msg("alice", "hello");
        let m = reacted(
            "hello",
            vec![reaction("+1", &["a"]), reaction("tada", &["b"])],
        );
        assert_eq!(
            block_rows(&m, None, false, 60, &ctx),
            block_rows(&plain, None, false, 60, &ctx) + 1
        );
        assert_eq!(
            block_rows(&m, None, false, 60, &ctx),
            message_rows(&m, 60, &ctx) + 1
        );
    }
}

#[cfg(test)]
mod grouping_tests {
    use super::*;
    use crate::config::Theme;
    use crate::data::RoomId;

    /// 2026-01-01 12:00:00 UTC, so a few minutes either side stay on one day.
    const NOON: i64 = 1_767_268_800;

    fn msg(id: &str, author: &str, ts: i64) -> Message {
        Message {
            id: id.to_string(),
            room_id: RoomId("general".to_string()),
            author: author.to_string(),
            ts,
            body: format!("body {id}"),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
        }
    }

    fn with<R>(grouping: Option<u32>, unread: Option<&str>, f: impl FnOnce(&RowCtx) -> R) -> R {
        let theme = Theme::default();
        let format = TimeFormat::default();
        let ctx = RowCtx {
            layout: MessageLayout::TwoLine,
            author_col: 5,
            time_col: 5,
            time_format: &format,
            zone: Zone::Utc,
            theme: &theme,
            search: None,
            aliases: &crate::ui::emoji::NO_ALIASES,
            emoji: crate::app::emoji::EmojiView::none(),
            own_user: None,
            unread_before: unread,
            unread_threads: None,
            in_thread: false,
            grouping,
            own_id: None,
        };
        f(&ctx)
    }

    fn continues_with(m: &Message, prev: &Message) -> bool {
        with(Some(300), None, |ctx| {
            continues(m, Some(prev), Some(prev), ctx)
        })
    }

    #[test]
    fn same_author_within_the_gap_continues() {
        let a = msg("a", "bob", NOON);
        assert!(continues_with(&msg("b", "bob", NOON + 300), &a));
        assert!(!continues_with(&msg("b", "bob", NOON + 301), &a));
        // A message earlier than the one before it never continues it.
        assert!(!continues_with(&msg("b", "bob", NOON - 1), &a));
    }

    #[test]
    fn the_first_message_never_continues() {
        with(Some(300), None, |ctx| {
            assert!(!continues(&msg("a", "bob", NOON), None, None, ctx));
        });
    }

    #[test]
    fn a_group_is_measured_from_its_first_message() {
        // One message every 100 s, gap 300: 0, 100, 200 and 300 share a header; 400 is more
        // than 300 s after the first, so it starts a new group, which 500, 600 and 700 join.
        let msgs: Vec<Message> = (0..8)
            .map(|i| msg(&i.to_string(), "bob", NOON + i * 100))
            .collect();
        let flags = with(Some(300), None, |ctx| continuation_flags(&msgs, ctx));
        assert_eq!(flags, [false, true, true, true, false, true, true, true]);
    }

    #[test]
    fn another_author_breaks_it_and_the_next_one_has_a_header() {
        let a = msg("a", "bob", NOON);
        let b = msg("b", "alice", NOON + 10);
        let c = msg("c", "bob", NOON + 20);
        assert!(!continues_with(&b, &a));
        assert!(!continues_with(&c, &b));
    }

    #[test]
    fn a_day_change_breaks_it() {
        let a = msg("a", "bob", 86_400 * 100 - 10);
        let b = msg("b", "bob", 86_400 * 100 + 10);
        assert!(!continues_with(&b, &a));
    }

    #[test]
    fn the_unread_divider_breaks_it() {
        let a = msg("a", "bob", NOON);
        let b = msg("b", "bob", NOON + 10);
        assert!(with(Some(300), None, |ctx| continues(
            &b,
            Some(&a),
            Some(&a),
            ctx
        )));
        assert!(!with(Some(300), Some("b"), |ctx| continues(
            &b,
            Some(&a),
            Some(&a),
            ctx
        )));
    }

    #[test]
    fn system_deleted_and_unsent_messages_never_group() {
        let a = msg("a", "bob", NOON);
        for kind in [MessageKind::System, MessageKind::Deleted] {
            let odd = Message {
                kind,
                ..msg("b", "bob", NOON + 10)
            };
            assert!(!continues_with(&odd, &a), "{kind:?} after");
            assert!(
                !continues_with(&msg("c", "bob", NOON + 20), &odd),
                "{kind:?} before"
            );
        }
        let unsent = Message {
            send: SendState::Sending,
            ..msg("b", "bob", NOON + 10)
        };
        assert!(!continues_with(&unsent, &a));
        assert!(!continues_with(&msg("c", "bob", NOON + 20), &unsent));
    }

    #[test]
    fn another_thread_breaks_it() {
        let a = msg("a", "bob", NOON);
        let reply = Message {
            tmid: Some("p".to_string()),
            ..msg("b", "bob", NOON + 10)
        };
        assert!(!continues_with(&reply, &a));
        let reply2 = Message {
            tmid: Some("p".to_string()),
            ..msg("c", "bob", NOON + 20)
        };
        assert!(continues_with(&reply2, &reply));
        let other = Message {
            tmid: Some("q".to_string()),
            ..msg("d", "bob", NOON + 30)
        };
        assert!(!continues_with(&other, &reply2));
    }

    #[test]
    fn a_thread_marker_row_on_the_previous_message_breaks_it() {
        let parent = Message {
            tcount: 2,
            ..msg("a", "bob", NOON)
        };
        assert!(!continues_with(&msg("b", "bob", NOON + 10), &parent));
    }

    #[test]
    fn zero_and_none_turn_it_off() {
        let a = msg("a", "bob", NOON);
        let b = msg("b", "bob", NOON + 1);
        assert!(!with(None, None, |ctx| continues(
            &b,
            Some(&a),
            Some(&a),
            ctx
        )));
    }

    #[test]
    fn a_grouped_message_drops_only_its_header_row() {
        let a = msg("a", "bob", NOON);
        let b = msg("b", "bob", NOON + 10);
        with(Some(300), None, |ctx| {
            let (_, lines) = block_lines(&b, Some(&a), true, 40, ctx);
            let full = render_rows(&b, 40, ctx);
            assert_eq!(lines.len(), full.len() - 1);
            assert_eq!(lines[0], full[1]);
            assert_eq!(block_rows(&b, Some(&a), true, 40, ctx), lines.len());
        });
    }

    #[test]
    fn dividers_are_counted_and_stay_out_of_the_cursor_rows() {
        let a = msg("a", "bob", NOON);
        with(Some(300), Some("a"), |ctx| {
            let (dividers, lines) = block_lines(&a, None, false, 40, ctx);
            assert_eq!(dividers, 2);
            assert_eq!(lines.len(), 2 + render_rows(&a, 40, ctx).len());
        });
    }

    #[test]
    fn the_compact_layout_never_groups_through_a_context_with_none() {
        // pane_row_ctx leaves `grouping` None for compact; with it None nothing is dropped.
        let a = msg("a", "bob", NOON);
        let b = msg("b", "bob", NOON + 1);
        with(None, None, |ctx| {
            assert_eq!(
                block_rows(&b, Some(&a), false, 40, ctx),
                render_rows(&b, 40, ctx).len()
            );
        });
    }
}
