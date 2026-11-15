//! Multiline input buffer for the input bar (D-14) with the Normal-mode motions of D-18.
// STUB-BEGIN
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_INPUT_ROWS: u16 = 5;

#[derive(Clone, Default)]
pub struct InputBuffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
    top: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        unimplemented!()
    }
    pub fn is_empty(&self) -> bool {
        unimplemented!()
    }
    pub fn text(&self) -> String {
        unimplemented!()
    }
    pub fn line_count(&self) -> usize {
        unimplemented!()
    }
    pub fn row(&self) -> usize {
        unimplemented!()
    }
    pub fn col(&self) -> usize {
        unimplemented!()
    }
    pub fn insert_char(&mut self, _c: char) -> bool {
        unimplemented!()
    }
    pub fn insert_str(&mut self, _s: &str) -> bool {
        unimplemented!()
    }
    pub fn newline(&mut self) {
        unimplemented!()
    }
    pub fn backspace(&mut self) {
        unimplemented!()
    }
    pub fn delete_word_back(&mut self) {
        unimplemented!()
    }
    pub fn delete_to_line_start(&mut self) {
        unimplemented!()
    }
    pub fn left(&mut self) {
        unimplemented!()
    }
    pub fn right(&mut self) {
        unimplemented!()
    }
    pub fn up(&mut self) {
        unimplemented!()
    }
    pub fn down(&mut self) {
        unimplemented!()
    }
    pub fn home(&mut self) {
        unimplemented!()
    }
    pub fn end(&mut self) {
        unimplemented!()
    }
    pub fn take(&mut self) -> String {
        unimplemented!()
    }
    pub fn left_in_line(&mut self, _n: usize) {
        unimplemented!()
    }
    pub fn right_in_line(&mut self, _n: usize) {
        unimplemented!()
    }
    pub fn move_line(&mut self, _delta: i64) {
        unimplemented!()
    }
    pub fn goto_line(&mut self, _idx: usize) {
        unimplemented!()
    }
    pub fn visual_rows(&self, _width: u16) -> Vec<String> {
        unimplemented!()
    }
    pub fn cursor_visual(&self, _width: u16) -> (u16, u16) {
        unimplemented!()
    }
    pub fn view_top(&self, _width: u16, _height: u16) -> usize {
        unimplemented!()
    }
    pub fn half_page(&mut self, _width: u16, _height: u16, _n: usize, _forward: bool) {
        unimplemented!()
    }
}

impl std::fmt::Debug for InputBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InputBuffer {{ {:?} }}", self.lines)
    }
}
// STUB-END

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
}
