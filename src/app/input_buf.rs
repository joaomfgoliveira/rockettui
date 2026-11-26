//! Multiline input buffer for the input bar (D-14) with the Normal-mode motions of D-18.
use crate::ui::sanitize::sanitize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Upper bound on the draft size in bytes (T-01-19).
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Most text rows the input bar shows before it scrolls (A3).
pub const MAX_INPUT_ROWS: u16 = 5;

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

/// The grapheme boundary before `col` (0 when there is none).
fn prev_boundary(line: &str, col: usize) -> usize {
    line[..col]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}

/// The grapheme boundary after `col` (the line end when there is none).
fn next_boundary(line: &str, col: usize) -> usize {
    line[col..]
        .graphemes(true)
        .next()
        .map_or(line.len(), |g| col + g.len())
}

/// The largest grapheme boundary at or before `col`.
fn floor_boundary(line: &str, col: usize) -> usize {
    let col = col.min(line.len());
    line.grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(line.len()))
        .take_while(|&i| i <= col)
        .last()
        .unwrap_or(0)
}

/// The grapheme boundary of `line` whose display column is nearest to `target`; a tie goes left.
fn col_at_display(line: &str, target: usize) -> usize {
    let mut width = 0;
    let mut best_col = 0;
    let mut best_diff = target;
    for (i, g) in line.grapheme_indices(true) {
        width += display_width(g);
        let diff = width.abs_diff(target);
        if diff < best_diff {
            best_diff = diff;
            best_col = i + g.len();
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

/// Hard-wrap `line` by display width at grapheme boundaries. Always at least one span; a grapheme
/// wider than `width` gets a row of its own.
fn wrap_spans(line: &str, width: usize) -> Vec<Span> {
    let width = width.max(1);
    let mut spans = Vec::new();
    let mut start = 0;
    let mut row_width = 0;
    for (i, g) in line.grapheme_indices(true) {
        let gw = display_width(g);
        if row_width > 0 && row_width + gw > width {
            spans.push(Span {
                start,
                end: i,
                width: row_width,
            });
            start = i;
            row_width = 0;
        }
        row_width += gw;
    }
    spans.push(Span {
        start,
        end: line.len(),
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

    fn snap(&mut self) {
        self.col = floor_boundary(&self.lines[self.row], self.col);
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
        self.snap();
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

    /// Delete the grapheme before the cursor; at column 0 join with the previous line.
    pub fn backspace(&mut self) {
        if self.col > 0 {
            let line = &mut self.lines[self.row];
            let start = prev_boundary(line, self.col);
            line.replace_range(start..self.col, "");
            self.col = start;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].len();
            self.lines[self.row].push_str(&current);
        }
    }

    /// Delete whitespace then a word before the cursor; at column 0 join with the previous line.
    pub fn delete_word_back(&mut self) {
        if self.col == 0 {
            self.backspace();
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
        let start = floor_boundary(line, start);
        self.lines[self.row].replace_range(start..self.col, "");
        self.col = start;
    }

    pub fn delete_to_line_start(&mut self) {
        self.lines[self.row].replace_range(..self.col, "");
        self.col = 0;
    }

    /// One grapheme left, crossing to the end of the previous line at column 0.
    pub fn left(&mut self) {
        if self.col > 0 {
            self.col = prev_boundary(&self.lines[self.row], self.col);
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].len();
        }
    }

    /// One grapheme right, crossing to the start of the next line at the line end.
    pub fn right(&mut self) {
        if self.col < self.lines[self.row].len() {
            self.col = next_boundary(&self.lines[self.row], self.col);
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self) {
        self.move_line(-1);
    }

    pub fn down(&mut self) {
        self.move_line(1);
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

    /// Step `n` graphemes left without leaving the line (Normal-mode `h`).
    pub fn left_in_line(&mut self, n: usize) {
        for _ in 0..n {
            if self.col == 0 {
                break;
            }
            self.col = prev_boundary(&self.lines[self.row], self.col);
        }
    }

    /// Step `n` graphemes right without leaving the line (Normal-mode `l`).
    pub fn right_in_line(&mut self, n: usize) {
        let len = self.lines[self.row].len();
        for _ in 0..n {
            if self.col >= len {
                break;
            }
            self.col = next_boundary(&self.lines[self.row], self.col);
        }
    }

    /// Move `delta` logical lines, clamped to the first and last line, keeping the display column.
    pub fn move_line(&mut self, delta: i64) {
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
        let want = display_width(&self.lines[self.row][..self.col]);
        self.row = target;
        self.col = col_at_display(&self.lines[self.row], want);
    }

    /// Jump to line `idx` (clamped to the last line), column 0.
    pub fn goto_line(&mut self, idx: usize) {
        self.row = idx.min(self.lines.len() - 1);
        self.col = 0;
    }

    /// The draft hard-wrapped to `width` columns, one string per visual row.
    pub fn visual_rows(&self, width: u16) -> Vec<String> {
        let width = usize::from(width).max(1);
        let mut rows = Vec::new();
        for (r, line) in self.lines.iter().enumerate() {
            let spans = wrap_spans(line, width);
            let full = spans.last().is_some_and(|s| s.width >= width);
            rows.extend(spans.iter().map(|s| line[s.start..s.end].to_string()));
            if r == self.row && self.col == line.len() && full {
                rows.push(String::new());
            }
        }
        rows
    }

    /// Number of visual rows `visual_rows(width)` yields.
    pub fn visual_row_count(&self, width: u16) -> usize {
        let width = usize::from(width).max(1);
        let mut count = 0;
        for (r, line) in self.lines.iter().enumerate() {
            let spans = wrap_spans(line, width);
            count += spans.len();
            let full = spans.last().is_some_and(|s| s.width >= width);
            if r == self.row && self.col == line.len() && full {
                count += 1;
            }
        }
        count
    }

    /// The cursor as (x, y) inside the visual rows of `visual_rows(width)`.
    pub fn cursor_visual(&self, width: u16) -> (u16, u16) {
        let width = usize::from(width).max(1);
        let above: usize = self.lines[..self.row]
            .iter()
            .map(|l| wrap_spans(l, width).len())
            .sum();
        let line = &self.lines[self.row];
        let spans = wrap_spans(line, width);
        let (x, dy) = if self.col >= line.len() {
            let last = &spans[spans.len() - 1];
            if last.width >= width {
                (0, spans.len())
            } else {
                (last.width, spans.len() - 1)
            }
        } else {
            let i = spans
                .iter()
                .position(|s| self.col < s.end)
                .unwrap_or(spans.len() - 1);
            (display_width(&line[spans[i].start..self.col]), i)
        };
        (
            u16::try_from(x).unwrap_or(u16::MAX),
            u16::try_from(above + dy).unwrap_or(u16::MAX),
        )
    }

    /// First visible visual row: the stored top moved just far enough to show the cursor, then
    /// clamped. Pure, so the renderer and the reducer agree.
    pub fn view_top(&self, width: u16, height: u16) -> usize {
        let rows = self.visual_row_count(width);
        let cursor_y = usize::from(self.cursor_visual(width).1);
        let height = usize::from(height);
        let mut top = self.top;
        if cursor_y < top {
            top = cursor_y;
        } else if height > 0 && cursor_y >= top + height {
            top = cursor_y + 1 - height;
        }
        top.min(rows.saturating_sub(height))
    }

    /// Store the effective view top, so scrolling stays put between keys.
    pub fn settle(&mut self, width: u16, height: u16) {
        self.top = self.view_top(width, height);
    }

    /// Ctrl-d / Ctrl-u: scroll the view and the cursor by half the visible rows, `n` times. Does
    /// nothing while the whole draft fits in the bar.
    pub fn half_page(&mut self, width: u16, height: u16, n: usize, forward: bool) {
        let rows = self.visual_row_count(width);
        let h = usize::from(height);
        if rows <= h {
            return;
        }
        let delta = (h / 2).max(1).saturating_mul(n);
        let top = self.view_top(width, height);
        self.top = if forward {
            top.saturating_add(delta)
        } else {
            top.saturating_sub(delta)
        }
        .min(rows - h);
        let lines = i64::try_from(delta).unwrap_or(i64::MAX);
        self.move_line(if forward { lines } else { -lines });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::layout::Size;

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
            b.backspace();
            assert_eq!(b.text(), "a", "{cluster:?}");
        }
        let mut b = buf("ab\ncd");
        b.goto_line(1);
        b.backspace();
        assert_eq!(b.text(), "abcd");
        assert_eq!((b.row(), b.col()), (0, 2));
        let mut empty = InputBuffer::new();
        empty.backspace();
        assert!(empty.is_empty());
    }

    #[test]
    fn delete_word_back() {
        let mut b = buf("hello world  ");
        b.delete_word_back();
        assert_eq!(b.text(), "hello ");
        let mut j = buf("ab\ncd");
        j.goto_line(1);
        j.delete_word_back();
        assert_eq!(j.text(), "abcd");
    }

    #[test]
    fn delete_to_line_start() {
        let mut b = buf("hello");
        b.left_in_line(2);
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
        b.left();
        assert_eq!((b.row(), b.col()), (0, 2));
        b.right();
        assert_eq!((b.row(), b.col()), (1, 0));
        let mut g = buf("e\u{301}x");
        g.home();
        g.right();
        assert_eq!(g.col(), 3);
        g.left();
        assert_eq!(g.col(), 0);
    }

    #[test]
    fn up_down_keep_display_column() {
        let mut b = buf("\u{65e5}\u{672c}\u{8a9e}\nabcdef");
        b.home();
        b.right_in_line(4);
        b.up();
        // Display column 4 is the boundary after two CJK characters (6 bytes).
        assert_eq!((b.row(), b.col()), (0, 6));
        b.down();
        assert_eq!((b.row(), b.col()), (1, 4));
        // Display column 3 sits between boundaries 2 and 4: the nearest, leftmost wins.
        b.left_in_line(1);
        b.up();
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
        assert_eq!(b.view_top(10, 5), 0);
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
            buf("abcdefghijkl").visual_rows(5),
            vec!["abcde", "fghij", "kl"]
        );
        assert_eq!(
            buf("\u{65e5}\u{672c}\u{8a9e}").visual_rows(5),
            vec!["\u{65e5}\u{672c}", "\u{8a9e}"]
        );
        assert_eq!(buf("a\n\nb").visual_rows(5), vec!["a", "", "b"]);
    }

    #[test]
    fn cursor_visual_exact_width() {
        let mut b = buf("abcd");
        assert_eq!(b.cursor_visual(5), (4, 0));
        b.insert_char('e');
        assert_eq!(b.cursor_visual(5), (0, 1));
        assert_eq!(b.visual_rows(5), vec!["abcde", ""]);
        b.home();
        assert_eq!(b.cursor_visual(5), (0, 0));
        assert_eq!(buf("ab\ncd").cursor_visual(5), (2, 1));
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
        b.left_in_line(1);
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
        b.left_in_line(2);
        assert_eq!((b.row(), b.col()), (1, 1));
        b.left_in_line(99);
        assert_eq!((b.row(), b.col()), (1, 0));
        b.right_in_line(99);
        assert_eq!((b.row(), b.col()), (1, 3));
        b.right_in_line(1);
        assert_eq!((b.row(), b.col()), (1, 3));
    }

    #[test]
    fn move_line_and_goto_line() {
        let mut b = buf("abc\nabc\nabc\nabc");
        b.goto_line(0);
        b.right_in_line(2);
        b.move_line(2);
        assert_eq!((b.row(), b.col()), (2, 2));
        b.move_line(99);
        assert_eq!(b.row(), 3);
        b.move_line(-99);
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
        assert_eq!(b.view_top(10, 5), 7);
        b.goto_line(0);
        assert_eq!(b.view_top(10, 5), 0);
    }

    #[test]
    fn half_page_only_when_scrolling() {
        let mut short = buf("a\nb\nc");
        short.half_page(10, 5, 1, true);
        assert_eq!(short.row(), 2);
        assert_eq!(short.view_top(10, 5), 0);
        short.goto_line(0);
        short.half_page(10, 5, 1, true);
        assert_eq!(short.row(), 0);

        let mut b = buf(&lines_n(12));
        b.goto_line(0);
        b.half_page(10, 5, 1, true);
        assert_eq!((b.row(), b.view_top(10, 5)), (2, 2));
        b.half_page(10, 5, 1, false);
        assert_eq!((b.row(), b.view_top(10, 5)), (0, 0));
        b.half_page(10, 5, 3, true);
        assert_eq!((b.row(), b.view_top(10, 5)), (6, 6));
    }

    #[test]
    fn text_before_cursor_spans_lines_up_to_the_cursor() {
        let mut b = buf("ab\ncd");
        assert_eq!(b.text_before_cursor(), "ab\ncd");
        b.left();
        assert_eq!(b.text_before_cursor(), "ab\nc");
        b.up();
        b.home();
        assert_eq!(b.text_before_cursor(), "");
    }
}
