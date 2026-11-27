//! Multiline input buffer for the input bar (D-14) with the Normal-mode motions of D-18.
//!
//! The buffer holds plain text. The layout and the motions walk *units* instead of graphemes: a
//! unit is a grapheme, or a custom emoji (`:name:`) that the bar draws as a two-column image
//! (`draft-emoji`). What is a unit depends on the `EmojiView` (the list, the images, the terminal),
//! so every function that lays out or moves takes one; `EmojiView::none()` gives plain graphemes.
use crate::app::emoji::{EmojiView, Graphics, Look};
use crate::ui::emoji::{Found, custom_cells, lookup_found, shortcode_spans};
use crate::ui::sanitize::{sanitize, sanitize_line};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Upper bound on the draft size in bytes (T-01-19).
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Most text rows the input bar shows before it scrolls (A3).
pub const MAX_INPUT_ROWS: u16 = 5;
/// Columns a drawn custom emoji takes, as in a message.
const EMOJI_WIDTH: usize = 2;

/// The draft of the input bar: at least one line, no line ever holds a line break, `col` is a byte
/// index on a grapheme boundary and `top` is the stored first visible visual row.
#[derive(Clone)]
pub struct InputBuffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
    top: usize,
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Redacting `Debug` (T-01-21): the draft is typed text, so only its size is printed.
impl std::fmt::Debug for InputBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "InputBuffer {{ lines: {}, bytes: {} }}",
            self.lines.len(),
            self.byte_len()
        )
    }
}

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// One step of the layout: a byte range of a line, the columns it is drawn in, and for a drawn
/// custom emoji the cells it is drawn as.
#[derive(Clone, Debug)]
struct Unit {
    start: usize,
    end: usize,
    width: usize,
    cells: Option<String>,
}

/// The units of `line`: its graphemes, with each custom emoji that is drawn as an image (a
/// shortcode that counts, of a custom emoji whose look is not its text) as one unit. `in_code` is
/// the backtick state at the start of the line.
fn units_of(line: &str, in_code: bool, view: &EmojiView) -> Vec<Unit> {
    let spans = if line.contains(':')
        && view.graphics == Graphics::Supported
        && !view.custom.list().is_empty()
    {
        shortcode_spans(line, &mut { in_code }, |name| {
            match lookup_found(name, view.aliases, view.custom) {
                Some(Found::Custom(own)) => match view.look(&own) {
                    Look::Text => None,
                    look => Some(custom_cells(&own, look)),
                },
                _ => None,
            }
        })
    } else {
        Vec::new()
    };
    let mut spans = spans.into_iter().peekable();
    let mut out = Vec::new();
    let mut graphemes = line.grapheme_indices(true);
    while let Some((i, g)) = graphemes.next() {
        // A span that began inside an earlier unit is not on a grapheme edge: ignore it.
        while spans.next_if(|(r, _)| r.start < i).is_some() {}
        if let Some((range, cells)) = spans.next_if(|(r, _)| r.start == i) {
            let mut end = i + g.len();
            while end < range.end {
                match graphemes.next() {
                    Some((j, g2)) => end = j + g2.len(),
                    None => break,
                }
            }
            out.push(Unit {
                start: i,
                end,
                width: EMOJI_WIDTH,
                cells: Some(cells),
            });
        } else {
            out.push(Unit {
                start: i,
                end: i + g.len(),
                width: display_width(g),
                cells: None,
            });
        }
    }
    out
}

/// The unit edge before `col` (0 when there is none).
fn prev_boundary(units: &[Unit], col: usize) -> usize {
    units
        .iter()
        .rev()
        .find(|u| u.start < col)
        .map_or(0, |u| u.start)
}

/// The unit edge after `col` (`len` when there is none).
fn next_boundary(units: &[Unit], col: usize, len: usize) -> usize {
    units.iter().find(|u| u.end > col).map_or(len, |u| u.end)
}

/// The largest unit edge at or before `col`.
fn floor_boundary(units: &[Unit], col: usize) -> usize {
    units
        .iter()
        .flat_map(|u| [u.start, u.end])
        .filter(|&edge| edge <= col)
        .max()
        .unwrap_or(0)
}

/// The columns drawn before `col`, which is on a unit edge.
fn width_before(units: &[Unit], col: usize) -> usize {
    units
        .iter()
        .take_while(|u| u.end <= col)
        .map(|u| u.width)
        .sum()
}

/// The unit edge whose display column is nearest to `target`; a tie goes left.
fn col_at_display(units: &[Unit], target: usize) -> usize {
    let mut width = 0;
    let mut best_col = 0;
    let mut best_diff = target;
    for u in units {
        width += u.width;
        let diff = width.abs_diff(target);
        if diff < best_diff {
            best_diff = diff;
            best_col = u.end;
        }
    }
    best_col
}

/// One visual row of a wrapped logical line: a byte range and its display width.
struct Span {
    start: usize,
    end: usize,
    width: usize,
}

/// Hard-wrap a line of `len` bytes by display width at unit edges. Always at least one span; a
/// unit wider than `width` gets a row of its own.
fn wrap_spans(units: &[Unit], len: usize, width: usize) -> Vec<Span> {
    let width = width.max(1);
    let mut spans = Vec::new();
    let mut start = 0;
    let mut row_width = 0;
    for u in units {
        if row_width > 0 && row_width + u.width > width {
            spans.push(Span {
                start,
                end: u.start,
                width: row_width,
            });
            start = u.start;
            row_width = 0;
        }
        row_width += u.width;
    }
    spans.push(Span {
        start,
        end: len,
        width: row_width,
    });
    spans
}

impl InputBuffer {
    /// An empty single-line buffer.
    pub fn new() -> Self {
        InputBuffer {
            lines: vec![String::new()],
            row: 0,
            col: 0,
            top: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// The draft with lines joined by LF.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The draft from its start up to the cursor, lines joined by LF.
    pub fn text_before_cursor(&self) -> String {
        let mut out = self.lines[..self.row].join("\n");
        if self.row > 0 {
            out.push('\n');
        }
        out.push_str(&self.lines[self.row][..self.col]);
        out
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn row(&self) -> usize {
        self.row
    }

    pub fn col(&self) -> usize {
        self.col
    }

    /// Size of `text()` in bytes.
    fn byte_len(&self) -> usize {
        self.lines.iter().map(String::len).sum::<usize>() + self.lines.len() - 1
    }

    /// The units of line `row`. A fenced block can open on an earlier line, so the backtick state
    /// comes from the lines above.
    fn line_units(&self, row: usize, view: &EmojiView) -> Vec<Unit> {
        let line = &self.lines[row];
        if !line.contains(':') {
            return units_of(line, false, view);
        }
        let ticks: usize = self.lines[..row]
            .iter()
            .map(|l| l.matches('`').count())
            .sum();
        units_of(line, ticks % 2 == 1, view)
    }

    /// The units of every line.
    fn all_units(&self, view: &EmojiView) -> Vec<Vec<Unit>> {
        let mut in_code = false;
        self.lines
            .iter()
            .map(|line| {
                let units = units_of(line, in_code, view);
                in_code ^= line.matches('`').count() % 2 == 1;
                units
            })
            .collect()
    }

    /// Round the column down to a unit edge.
    fn snap(&mut self, view: &EmojiView) {
        let units = self.line_units(self.row, view);
        self.col = floor_boundary(&units, self.col.min(self.lines[self.row].len()));
    }

    /// Insert one typed character. Line breaks are refused (lines are made by `newline` and
    /// `insert_str`), a character the sanitizer drops is ignored, a TAB becomes spaces.
    pub fn insert_char(&mut self, c: char) -> bool {
        if is_line_break(c) {
            return false;
        }
        let mut utf8 = [0u8; 4];
        let clean = sanitize(c.encode_utf8(&mut utf8));
        if clean.is_empty() || self.byte_len() + clean.len() > MAX_INPUT_BYTES {
            return false;
        }
        self.lines[self.row].insert_str(self.col, &clean);
        self.col += clean.len();
        true
    }

    /// Insert pasted text as one edit: sanitized, line endings normalized to LF and cut at the
    /// size cap on a char boundary. Returns true when the text was truncated.
    pub fn insert_str(&mut self, s: &str) -> bool {
        let clean = sanitize(s);
        let room = MAX_INPUT_BYTES.saturating_sub(self.byte_len());
        let mut end = clean.len().min(room);
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = end < clean.len();
        let piece = &clean[..end];
        if piece.is_empty() {
            return truncated;
        }
        let mut parts = piece.split('\n');
        let first = parts.next().unwrap_or("");
        let tail = self.lines[self.row].split_off(self.col);
        self.lines[self.row].push_str(first);
        self.col = self.lines[self.row].len();
        let rest: Vec<String> = parts.map(str::to_string).collect();
        if !rest.is_empty() {
            let last_len = rest.last().map_or(0, String::len);
            let at = self.row + 1;
            self.row += rest.len();
            self.col = last_len;
            self.lines.splice(at..at, rest);
        }
        self.lines[self.row].push_str(&tail);
        // Where the text went in is a grapheme edge; a custom emoji is not a unit to an insert.
        self.snap(&EmojiView::none());
        truncated
    }

    pub fn newline(&mut self) {
        if self.byte_len() >= MAX_INPUT_BYTES {
            return;
        }
        let tail = self.lines[self.row].split_off(self.col);
        self.row += 1;
        self.lines.insert(self.row, tail);
        self.col = 0;
    }

    /// Delete the unit before the cursor (a grapheme, or a whole drawn emoji); at column 0 join
    /// with the previous line.
    pub fn backspace(&mut self, view: &EmojiView) {
        if self.col > 0 {
            let start = prev_boundary(&self.line_units(self.row, view), self.col);
            self.lines[self.row].replace_range(start..self.col, "");
            self.col = start;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].len();
            self.lines[self.row].push_str(&current);
        }
    }

    /// Delete whitespace then a word before the cursor; at column 0 join with the previous line.
    pub fn delete_word_back(&mut self, view: &EmojiView) {
        if self.col == 0 {
            self.backspace(view);
            return;
        }
        let line = &self.lines[self.row];
        let mut start = self.col;
        let mut in_word = false;
        for (i, ch) in line[..self.col].char_indices().rev() {
            if ch.is_whitespace() {
                if in_word {
                    break;
                }
            } else {
                in_word = true;
            }
            start = i;
        }
        let start = floor_boundary(&self.line_units(self.row, view), start);
        self.lines[self.row].replace_range(start..self.col, "");
        self.col = start;
    }

    pub fn delete_to_line_start(&mut self) {
        self.lines[self.row].replace_range(..self.col, "");
        self.col = 0;
    }

    /// One unit left, crossing to the end of the previous line at column 0.
    pub fn left(&mut self, view: &EmojiView) {
        if self.col > 0 {
            self.col = prev_boundary(&self.line_units(self.row, view), self.col);
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].len();
        }
    }

    /// One unit right, crossing to the start of the next line at the line end.
    pub fn right(&mut self, view: &EmojiView) {
        let len = self.lines[self.row].len();
        if self.col < len {
            self.col = next_boundary(&self.line_units(self.row, view), self.col, len);
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self, view: &EmojiView) {
        self.move_line(-1, view);
    }

    pub fn down(&mut self, view: &EmojiView) {
        self.move_line(1, view);
    }

    pub fn home(&mut self) {
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.col = self.lines[self.row].len();
    }

    /// The whole draft; leaves an empty single-line buffer.
    pub fn take(&mut self) -> String {
        let text = self.text();
        *self = InputBuffer::new();
        text
    }

    /// Step `n` units left without leaving the line (Normal-mode `h`).
    pub fn left_in_line(&mut self, n: usize, view: &EmojiView) {
        let units = self.line_units(self.row, view);
        for _ in 0..n {
            if self.col == 0 {
                break;
            }
            self.col = prev_boundary(&units, self.col);
        }
    }

    /// Step `n` units right without leaving the line (Normal-mode `l`).
    pub fn right_in_line(&mut self, n: usize, view: &EmojiView) {
        let len = self.lines[self.row].len();
        let units = self.line_units(self.row, view);
        for _ in 0..n {
            if self.col >= len {
                break;
            }
            self.col = next_boundary(&units, self.col, len);
        }
    }

    /// Move `delta` logical lines, clamped to the first and last line, keeping the display column.
    pub fn move_line(&mut self, delta: i64, view: &EmojiView) {
        let last = self.lines.len() - 1;
        let steps = usize::try_from(delta.unsigned_abs()).unwrap_or(usize::MAX);
        let target = if delta < 0 {
            self.row.saturating_sub(steps)
        } else {
            self.row.saturating_add(steps).min(last)
        };
        if target == self.row {
            return;
        }
        let want = width_before(&self.line_units(self.row, view), self.col);
        self.row = target;
        self.col = col_at_display(&self.line_units(self.row, view), want);
    }

    /// Jump to line `idx` (clamped to the last line), column 0.
    pub fn goto_line(&mut self, idx: usize) {
        self.row = idx.min(self.lines.len() - 1);
        self.col = 0;
    }

    /// The draft hard-wrapped to `width` columns, one string per visual row, as written.
    pub fn visual_rows(&self, width: u16, view: &EmojiView) -> Vec<String> {
        self.rows(width, view, None)
    }

    /// The same rows as `visual_rows`, with each drawn custom emoji as the cells it is drawn as
    /// (`ui::emoji::expand` turns them into what is painted) and the rest cleaned for drawing.
    ///
    /// Each mention and channel reference of the draft (matched by shape, as the draft has no
    /// server lists; `own` is the user's own username) is wrapped in the zero-width markers of
    /// `ui::mention`, which `mention::strip` takes out of the rows again. Only drawing is affected:
    /// the text of the buffer, and so what is sent, is never changed.
    pub fn drawn_rows(&self, width: u16, view: &EmojiView, own: Option<&str>) -> Vec<String> {
        self.rows(width, view, Some(own))
    }

    fn rows(&self, width: u16, view: &EmojiView, drawn: Option<Option<&str>>) -> Vec<String> {
        let width = usize::from(width).max(1);
        let mut rows = Vec::new();
        let mut in_code = false;
        for (r, (line, units)) in self.lines.iter().zip(self.all_units(view)).enumerate() {
            let tokens = drawn.map_or_else(Vec::new, |own| {
                crate::ui::mention::tokens_in(line, None, own, &mut in_code)
            });
            // The token being written (an index into `tokens`), which may run over several rows.
            let mut open = false;
            let mut next = 0;
            let spans = wrap_spans(&units, line.len(), width);
            let full = spans.last().is_some_and(|s| s.width >= width);
            for s in &spans {
                if drawn.is_none() {
                    rows.push(line[s.start..s.end].to_string());
                    continue;
                }
                let mut row = String::new();
                for u in units
                    .iter()
                    .filter(|u| u.start >= s.start && u.end <= s.end)
                {
                    if !open && tokens.get(next).is_some_and(|(t, _)| t.start <= u.start) {
                        row.push(crate::ui::mention::start_marker(tokens[next].1));
                        open = true;
                    }
                    match &u.cells {
                        Some(cells) => row.push_str(cells),
                        None => row.push_str(&sanitize_line(&line[u.start..u.end])),
                    }
                    if open && tokens[next].0.end <= u.end {
                        row.push(crate::ui::mention::END);
                        open = false;
                        next += 1;
                    }
                }
                rows.push(row);
            }
            if r == self.row && self.col == line.len() && full {
                rows.push(String::new());
            }
        }
        rows
    }

    /// Number of visual rows `visual_rows(width)` yields.
    pub fn visual_row_count(&self, width: u16, view: &EmojiView) -> usize {
        let width = usize::from(width).max(1);
        let mut count = 0;
        for (r, (line, units)) in self.lines.iter().zip(self.all_units(view)).enumerate() {
            let spans = wrap_spans(&units, line.len(), width);
            count += spans.len();
            let full = spans.last().is_some_and(|s| s.width >= width);
            if r == self.row && self.col == line.len() && full {
                count += 1;
            }
        }
        count
    }

    /// The cursor as (x, y) inside the visual rows of `visual_rows(width)`.
    pub fn cursor_visual(&self, width: u16, view: &EmojiView) -> (u16, u16) {
        let width = usize::from(width).max(1);
        let all = self.all_units(view);
        let above: usize = (0..self.row)
            .map(|r| wrap_spans(&all[r], self.lines[r].len(), width).len())
            .sum();
        let line = &self.lines[self.row];
        let units = &all[self.row];
        let spans = wrap_spans(units, line.len(), width);
        let (x, dy) = if self.col >= line.len() {
            let last = &spans[spans.len() - 1];
            if last.width >= width {
                (0, spans.len())
            } else {
                (last.width, spans.len() - 1)
            }
        } else {
            // A column inside a drawn emoji (its image arrived under the cursor) is drawn at its start.
            let col = floor_boundary(units, self.col);
            let i = spans
                .iter()
                .position(|s| col < s.end)
                .unwrap_or(spans.len() - 1);
            let x = units
                .iter()
                .filter(|u| u.start >= spans[i].start && u.end <= col)
                .map(|u| u.width)
                .sum();
            (x, i)
        };
        (
            u16::try_from(x).unwrap_or(u16::MAX),
            u16::try_from(above + dy).unwrap_or(u16::MAX),
        )
    }

    /// First visible visual row: the stored top moved just far enough to show the cursor, then
    /// clamped. Pure, so the renderer and the reducer agree.
    pub fn view_top(&self, width: u16, height: u16, view: &EmojiView) -> usize {
        let rows = self.visual_row_count(width, view);
        let cursor_y = usize::from(self.cursor_visual(width, view).1);
        let height = usize::from(height);
        let mut top = self.top;
        if cursor_y < top {
            top = cursor_y;
        } else if height > 0 && cursor_y >= top + height {
            top = cursor_y + 1 - height;
        }
        top.min(rows.saturating_sub(height))
    }

    /// Store the effective view top, so scrolling stays put between keys, and put the cursor on a
    /// unit edge (a custom emoji's image may have arrived under it).
    pub fn settle(&mut self, width: u16, height: u16, view: &EmojiView) {
        self.snap(view);
        self.top = self.view_top(width, height, view);
    }

    /// Ctrl-d / Ctrl-u: scroll the view and the cursor by half the visible rows, `n` times. Does
    /// nothing while the whole draft fits in the bar.
    pub fn half_page(
        &mut self,
        width: u16,
        height: u16,
        n: usize,
        forward: bool,
        view: &EmojiView,
    ) {
        let rows = self.visual_row_count(width, view);
        let h = usize::from(height);
        if rows <= h {
            return;
        }
        let delta = (h / 2).max(1).saturating_mul(n);
        let top = self.view_top(width, height, view);
        self.top = if forward {
            top.saturating_add(delta)
        } else {
            top.saturating_sub(delta)
        }
        .min(rows - h);
        let lines = i64::try_from(delta).unwrap_or(i64::MAX);
        self.move_line(if forward { lines } else { -lines }, view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::layout::Size;

    fn nv() -> EmojiView<'static> {
        EmojiView::none()
    }

    fn buf(s: &str) -> InputBuffer {
        let mut b = InputBuffer::new();
        b.insert_str(s);
        b
    }

    fn lines_n(n: usize) -> String {
        (0..n)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn insert_and_text() {
        let mut b = InputBuffer::new();
        assert!(b.insert_char('h'));
        assert!(b.insert_char('i'));
        assert_eq!(b.text(), "hi");
        b.newline();
        b.insert_char('x');
        assert_eq!(b.text(), "hi\nx");
        assert_eq!(b.row(), 1);
        assert_eq!(b.line_count(), 2);
    }

    #[test]
    fn backspace_by_grapheme() {
        for cluster in ["\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", "e\u{301}"] {
            let mut b = buf(&format!("a{cluster}"));
            b.backspace(&nv());
            assert_eq!(b.text(), "a", "{cluster:?}");
        }
        let mut b = buf("ab\ncd");
        b.goto_line(1);
        b.backspace(&nv());
        assert_eq!(b.text(), "abcd");
        assert_eq!((b.row(), b.col()), (0, 2));
        let mut empty = InputBuffer::new();
        empty.backspace(&nv());
        assert!(empty.is_empty());
    }

    #[test]
    fn delete_word_back() {
        let mut b = buf("hello world  ");
        b.delete_word_back(&nv());
        assert_eq!(b.text(), "hello ");
        let mut j = buf("ab\ncd");
        j.goto_line(1);
        j.delete_word_back(&nv());
        assert_eq!(j.text(), "abcd");
    }

    #[test]
    fn delete_to_line_start() {
        let mut b = buf("hello");
        b.left_in_line(2, &nv());
        b.delete_to_line_start();
        assert_eq!(b.text(), "lo");
        assert_eq!(b.col(), 0);
        b.delete_to_line_start();
        assert_eq!(b.text(), "lo");
    }

    #[test]
    fn left_right_cross_lines() {
        let mut b = buf("ab\ncd");
        b.goto_line(1);
        b.left(&nv());
        assert_eq!((b.row(), b.col()), (0, 2));
        b.right(&nv());
        assert_eq!((b.row(), b.col()), (1, 0));
        let mut g = buf("e\u{301}x");
        g.home();
        g.right(&nv());
        assert_eq!(g.col(), 3);
        g.left(&nv());
        assert_eq!(g.col(), 0);
    }

    #[test]
    fn up_down_keep_display_column() {
        let mut b = buf("\u{65e5}\u{672c}\u{8a9e}\nabcdef");
        b.home();
        b.right_in_line(4, &nv());
        b.up(&nv());
        // Display column 4 is the boundary after two CJK characters (6 bytes).
        assert_eq!((b.row(), b.col()), (0, 6));
        b.down(&nv());
        assert_eq!((b.row(), b.col()), (1, 4));
        // Display column 3 sits between boundaries 2 and 4: the nearest, leftmost wins.
        b.left_in_line(1, &nv());
        b.up(&nv());
        assert_eq!((b.row(), b.col()), (0, 3));
    }

    #[test]
    fn home_end_and_take() {
        let mut b = buf("abc\ndef");
        b.home();
        assert_eq!(b.col(), 0);
        b.end();
        assert_eq!(b.col(), 3);
        assert_eq!(b.take(), "abc\ndef");
        assert!(b.is_empty());
        assert_eq!((b.row(), b.col(), b.line_count()), (0, 0, 1));
        assert_eq!(b.view_top(10, 5, &nv()), 0);
    }

    #[test]
    fn insert_str_normalizes() {
        let b = buf("a\r\nb\rc\u{1b}[2Jd\u{202e}e\tf");
        let text = b.text();
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{202e}'));
        assert!(!text.contains('\r'));
        assert_eq!(b.line_count(), 3);
        assert!(text.contains(&format!("e{}f", " ".repeat(4))), "{text:?}");
    }

    #[test]
    fn cap_enforced() {
        let mut b = InputBuffer::new();
        assert!(b.insert_str(&"a".repeat(70_000)));
        assert_eq!(b.text().len(), MAX_INPUT_BYTES);
        assert!(!b.insert_char('b'));
        assert_eq!(b.text().len(), MAX_INPUT_BYTES);
        let mut wide = InputBuffer::new();
        assert!(wide.insert_str(&"\u{e9}".repeat(40_000)));
        assert_eq!(wide.text().len(), MAX_INPUT_BYTES);
        let mut small = InputBuffer::new();
        assert!(!small.insert_str("fits"));
    }

    #[test]
    fn visual_rows_hard_wrap() {
        assert_eq!(
            buf("abcdefghijkl").visual_rows(5, &nv()),
            vec!["abcde", "fghij", "kl"]
        );
        assert_eq!(
            buf("\u{65e5}\u{672c}\u{8a9e}").visual_rows(5, &nv()),
            vec!["\u{65e5}\u{672c}", "\u{8a9e}"]
        );
        assert_eq!(buf("a\n\nb").visual_rows(5, &nv()), vec!["a", "", "b"]);
    }

    #[test]
    fn cursor_visual_exact_width() {
        let mut b = buf("abcd");
        assert_eq!(b.cursor_visual(5, &nv()), (4, 0));
        b.insert_char('e');
        assert_eq!(b.cursor_visual(5, &nv()), (0, 1));
        assert_eq!(b.visual_rows(5, &nv()), vec!["abcde", ""]);
        b.home();
        assert_eq!(b.cursor_visual(5, &nv()), (0, 0));
        assert_eq!(buf("ab\ncd").cursor_visual(5, &nv()), (2, 1));
    }

    #[test]
    fn input_rows_clamp() {
        let mut app = App::new(Size::new(80, 24));
        assert_eq!(app.input_rows(), 1);
        app.input.insert_str(&lines_n(5));
        assert_eq!(app.input_rows(), 5);
        app.input.insert_str("\nl5\nl6");
        assert_eq!(app.input.line_count(), 7);
        assert_eq!(app.input_rows(), 5);
    }

    #[test]
    fn debug_redacts() {
        let b = buf("SENTINEL-DRAFT-55aa");
        assert!(!format!("{b:?}").contains("SENTINEL-DRAFT-55aa"));
    }

    #[test]
    fn insert_char_filters() {
        for c in ['\u{1b}', '\u{9b}', '\u{202e}', '\u{200b}', '\0'] {
            let mut b = InputBuffer::new();
            assert!(!b.insert_char(c), "{:x}", c as u32);
            assert!(b.is_empty(), "{:x}", c as u32);
        }
        let mut t = InputBuffer::new();
        assert!(t.insert_char('\t'));
        assert_eq!(t.text(), " ".repeat(crate::ui::sanitize::TAB_WIDTH));
        let mut w = InputBuffer::new();
        assert!(w.insert_char('\u{e9}'));
        assert!(w.insert_char('\u{65e5}'));
        assert_eq!(w.text(), "\u{e9}\u{65e5}");
    }

    #[test]
    fn insert_char_never_breaks_line() {
        let mut b = buf("ab");
        b.left_in_line(1, &nv());
        for c in ['\n', '\r', '\u{2028}', '\u{2029}'] {
            assert!(!b.insert_char(c), "{:x}", c as u32);
            assert_eq!(b.text(), "ab");
            assert_eq!(b.line_count(), 1);
            assert_eq!(b.col(), 1);
        }
        b.insert_char('x');
        b.newline();
        b.insert_str("x\ny");
        b.insert_char('\r');
        b.insert_char('z');
        let text = b.text();
        assert!(!text.contains('\r'));
        assert_eq!(text.split('\n').count(), b.line_count());
    }

    #[test]
    fn left_right_in_line_stop_at_line_ends() {
        let mut b = buf("abc\ndef");
        b.left_in_line(2, &nv());
        assert_eq!((b.row(), b.col()), (1, 1));
        b.left_in_line(99, &nv());
        assert_eq!((b.row(), b.col()), (1, 0));
        b.right_in_line(99, &nv());
        assert_eq!((b.row(), b.col()), (1, 3));
        b.right_in_line(1, &nv());
        assert_eq!((b.row(), b.col()), (1, 3));
    }

    #[test]
    fn move_line_and_goto_line() {
        let mut b = buf("abc\nabc\nabc\nabc");
        b.goto_line(0);
        b.right_in_line(2, &nv());
        b.move_line(2, &nv());
        assert_eq!((b.row(), b.col()), (2, 2));
        b.move_line(99, &nv());
        assert_eq!(b.row(), 3);
        b.move_line(-99, &nv());
        assert_eq!(b.row(), 0);
        b.goto_line(1);
        assert_eq!((b.row(), b.col()), (1, 0));
        b.goto_line(99);
        assert_eq!((b.row(), b.col()), (3, 0));
    }

    #[test]
    fn view_top_keeps_cursor_visible() {
        let mut b = buf(&lines_n(12));
        assert_eq!(b.row(), 11);
        assert_eq!(b.view_top(10, 5, &nv()), 7);
        b.goto_line(0);
        assert_eq!(b.view_top(10, 5, &nv()), 0);
    }

    #[test]
    fn half_page_only_when_scrolling() {
        let mut short = buf("a\nb\nc");
        short.half_page(10, 5, 1, true, &nv());
        assert_eq!(short.row(), 2);
        assert_eq!(short.view_top(10, 5, &nv()), 0);
        short.goto_line(0);
        short.half_page(10, 5, 1, true, &nv());
        assert_eq!(short.row(), 0);

        let mut b = buf(&lines_n(12));
        b.goto_line(0);
        b.half_page(10, 5, 1, true, &nv());
        assert_eq!((b.row(), b.view_top(10, 5, &nv())), (2, 2));
        b.half_page(10, 5, 1, false, &nv());
        assert_eq!((b.row(), b.view_top(10, 5, &nv())), (0, 0));
        b.half_page(10, 5, 3, true, &nv());
        assert_eq!((b.row(), b.view_top(10, 5, &nv())), (6, 6));
    }

    #[test]
    fn text_before_cursor_spans_lines_up_to_the_cursor() {
        let mut b = buf("ab\ncd");
        assert_eq!(b.text_before_cursor(), "ab\ncd");
        b.left(&nv());
        assert_eq!(b.text_before_cursor(), "ab\nc");
        b.up(&nv());
        b.home();
        assert_eq!(b.text_before_cursor(), "");
    }

    // ---- custom emojis drawn as units (draft-emoji) ----

    use crate::app::emoji::{CustomEmojis, Graphics, NO_IMAGES};
    use crate::data::CustomEmoji;

    fn list() -> CustomEmojis {
        CustomEmojis::Loaded(vec![CustomEmoji {
            name: "rtshipit".to_string(),
            aliases: vec!["rtship".to_string()],
            ext: "png".to_string(),
            etag: "e".to_string(),
        }])
    }

    fn view(list: &CustomEmojis, graphics: Graphics) -> EmojiView<'_> {
        EmojiView {
            custom: list,
            aliases: &crate::ui::emoji::NO_ALIASES,
            images: &NO_IMAGES,
            graphics,
            now_ms: 0,
        }
    }

    /// The byte ranges that `line` draws as an emoji, the line opening outside code.
    fn drawn(line: &str, view: &EmojiView) -> Vec<(usize, usize)> {
        units_of(line, false, view)
            .iter()
            .filter(|u| u.cells.is_some())
            .map(|u| (u.start, u.end))
            .collect()
    }

    #[test]
    fn a_custom_name_or_alias_is_one_unit_two_columns_wide() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        assert_eq!(drawn("ship :rtshipit:", &v), [(5, 15)]);
        assert_eq!(drawn(":rtship:", &v), [(0, 8)]);
        assert_eq!(drawn("(:rtshipit:)", &v), [(1, 11)]);
        let units = units_of(":rtshipit:", false, &v);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].width, 2);
    }

    #[test]
    fn what_is_not_a_custom_emoji_stays_graphemes() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        assert!(drawn(":rt_not_an_emoji:", &v).is_empty());
        assert!(drawn(":tada:", &v).is_empty());
        assert!(drawn("a:rtshipit:", &v).is_empty());
        assert!(drawn(":rtshipit", &v).is_empty());
        assert!(drawn("`:rtshipit:`", &v).is_empty());
        // Without images the terminal draws the text, so the units are its graphemes.
        let off = view(&l, Graphics::Unsupported);
        assert!(drawn(":rtshipit:", &off).is_empty());
        assert_eq!(units_of(":rtshipit:", false, &off).len(), 10);
        assert!(drawn(":rtshipit:", &nv()).is_empty());
    }

    #[test]
    fn a_fenced_block_covers_the_lines_between_its_fences() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let b = buf("```\n:rtshipit:\n```\n:rtshipit:");
        let all = b.all_units(&v);
        let emoji = |row: usize| all[row].iter().filter(|u| u.cells.is_some()).count();
        assert_eq!((emoji(0), emoji(1), emoji(2), emoji(3)), (0, 0, 0, 1));
        assert_eq!(b.line_units(1, &v).len(), 10);
        assert_eq!(b.line_units(3, &v).len(), 1);
    }

    #[test]
    fn wrap_counts_two_columns_per_emoji_and_never_splits_one() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        // The emoji would start in the last column of the row: it goes to the next one whole.
        let b = buf("abcd,:rtshipit:");
        assert_eq!(b.visual_rows(6, &v), vec!["abcd,", ":rtshipit:"]);
        assert_eq!(b.visual_row_count(6, &v), 2);
        let c = buf("ab,:rtshipit::rtshipit::rtshipit:");
        assert_eq!(
            c.visual_rows(6, &v),
            vec!["ab,:rtshipit:", ":rtshipit::rtshipit:"]
        );
        // As text the same draft wraps by its characters.
        assert_eq!(b.visual_rows(6, &nv()), vec!["abcd,:", "rtship", "it:"]);
    }

    #[test]
    fn the_drawn_rows_hold_cells_and_are_as_many_as_the_rows() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let b = buf("ab :rtshipit:\nx");
        let rows = b.drawn_rows(20, &v, None);
        assert_eq!(rows.len(), b.visual_rows(20, &v).len());
        assert_eq!(
            rows[0],
            format!(
                "ab {}",
                crate::ui::emoji::custom_cells("rtshipit", Look::Standin)
            )
        );
        assert_eq!(rows[1], "x");
        assert_eq!(b.drawn_rows(20, &nv(), None), b.visual_rows(20, &nv()));
    }

    #[test]
    fn the_cursor_column_follows_the_drawn_width() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let b = buf("a :rtshipit:");
        assert_eq!(b.cursor_visual(20, &v), (4, 0));
        assert_eq!(b.cursor_visual(20, &nv()), (12, 0));
        // An emoji that fills the row puts the cursor on the next one.
        assert_eq!(buf("ab :rtshipit:").cursor_visual(5, &v), (0, 1));
        // A column inside an emoji (its image arrived under the cursor) is drawn at its start.
        let mut inside = buf("a :rtshipit:");
        inside.home();
        inside.right_in_line(5, &nv());
        assert_eq!(inside.cursor_visual(20, &v), (2, 0));
        inside.settle(20, 5, &v);
        assert_eq!(inside.col(), 2);
    }

    #[test]
    fn the_cursor_passes_over_an_emoji_whole() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let mut b = buf("a :rtshipit: b");
        b.left(&v);
        b.left(&v);
        assert_eq!(b.col(), 12);
        b.left(&v);
        assert_eq!(b.col(), 2);
        b.right(&v);
        assert_eq!(b.col(), 12);
        b.home();
        b.right_in_line(3, &v);
        assert_eq!(b.col(), 12);
        b.left_in_line(1, &v);
        assert_eq!(b.col(), 2);
        // As text the cursor goes one character at a time.
        let mut t = buf("a :rtshipit: b");
        t.left(&nv());
        t.left(&nv());
        t.left(&nv());
        assert_eq!(t.col(), 11);
    }

    #[test]
    fn one_backspace_deletes_the_whole_shortcode() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let mut b = buf("ship :rtshipit:");
        b.backspace(&v);
        assert_eq!(b.text(), "ship ");
        let mut t = buf("ship :rtshipit:");
        t.backspace(&nv());
        assert_eq!(t.text(), "ship :rtshipit");
        // The word before the cursor goes whole too, and never half an emoji.
        let mut w = buf("hi :rtshipit:");
        w.delete_word_back(&v);
        assert_eq!(w.text(), "hi ");
    }

    #[test]
    fn up_and_down_land_on_a_unit_edge() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        // Display column 2 is inside the emoji of the first line (columns 1 to 3): the tie goes left.
        let mut b = buf("(:rtshipit:)\nabcd");
        b.home();
        b.right_in_line(2, &v);
        assert_eq!((b.row(), b.col()), (1, 2));
        b.up(&v);
        assert_eq!((b.row(), b.col()), (0, 1));
        b.right(&v);
        b.down(&v);
        assert_eq!((b.row(), b.col()), (1, 3));
    }

    #[test]
    fn typing_the_closing_colon_makes_the_unit_and_a_deletion_undoes_it() {
        let l = list();
        let v = view(&l, Graphics::Supported);
        let mut b = buf(":rtshipit");
        assert!(drawn(&b.text(), &v).is_empty());
        b.insert_char(':');
        assert_eq!(drawn(&b.text(), &v), [(0, 10)]);
        assert_eq!(b.col(), 10);
        b.backspace(&v);
        assert_eq!(b.text(), "");
    }
}
