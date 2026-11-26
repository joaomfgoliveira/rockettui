//! Pure pane geometry, shared by the view and (later) the reducer so scroll math and rendering
//! never disagree. Every computation is saturating: any `Rect`, including 0x0, is safe.

use ratatui::layout::{Constraint, Layout, Rect};

/// Default width of the sidebar column in cells, borders included (D-13); config.toml
/// `sidebar_width` overrides it.
pub const SIDEBAR_WIDTH: u16 = 28;

/// Narrowest terminal (in columns) that shows the sidebar, the message pane and the thread pane
/// side by side. Below it an open thread takes the place of the message pane.
pub const THREAD_SPLIT_MIN_WIDTH: u16 = 100;

/// Share of the area right of the sidebar given to the message pane when a thread is shown beside
/// it; the thread pane gets the rest.
const MESSAGES_PERCENT: u16 = 60;

/// The regions of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Areas {
    pub sidebar: Rect,
    pub messages: Rect,
    pub input: Rect,
    pub status: Rect,
    pub cmdline: Rect,
    /// Where the thread pane is drawn, `None` while no thread is open. Beside the message pane
    /// and the input bar (the full height of the body) on a wide terminal; on a narrow one it is
    /// the rect of `messages`, which then is not drawn but keeps its size, so closing the thread
    /// brings it back unchanged.
    pub thread: Option<Rect>,
    /// The thread's own input bar, under the thread pane. `None` unless the thread sits beside the
    /// messages: when it takes the messages' place there is one input bar, the room's.
    pub thread_input: Option<Rect>,
}

impl Areas {
    /// The whole thread column, the pane and its input bar together: what the thread list takes,
    /// since it has no input bar. `None` while the column is closed.
    pub fn column(&self) -> Option<Rect> {
        self.thread
            .map(|pane| self.thread_input.map_or(pane, |bar| pane.union(bar)))
    }

    /// True when the thread pane replaces the message pane instead of sitting beside it.
    pub fn thread_takes_over(&self) -> bool {
        self.thread == Some(self.messages)
    }
}

/// Split `area` into the five regions. `sidebar_width` is the sidebar column width in cells; it is
/// not clamped, so a width beyond the terminal leaves the main column narrow or empty. `input_rows`
/// is the number of content rows of the input bar; the bar itself is two rows taller (borders).
pub fn areas(area: Rect, sidebar_width: u16, input_rows: u16) -> Areas {
    areas_with_thread(area, sidebar_width, input_rows, 1, false)
}

/// Like `areas`, with room for the thread pane when `thread` is true: beside the messages when the
/// terminal is at least `THREAD_SPLIT_MIN_WIDTH` columns wide, over them otherwise. Beside them the
/// thread has an input bar of its own, `thread_rows` content rows tall.
pub fn areas_with_thread(
    area: Rect,
    sidebar_width: u16,
    input_rows: u16,
    thread_rows: u16,
    thread: bool,
) -> Areas {
    let [body, status, cmdline] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let [sidebar, main] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(0)]).areas(body);
    let split = thread && area.width >= THREAD_SPLIT_MIN_WIDTH;
    let (left, beside) = if split {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(MESSAGES_PERCENT), Constraint::Min(0)])
                .areas(main);
        let [right, right_input] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(thread_rows.saturating_add(2)),
        ])
        .areas(right);
        (left, Some((right, right_input)))
    } else {
        (main, None)
    };
    let [messages, input] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(input_rows.saturating_add(2)),
    ])
    .areas(left);
    Areas {
        sidebar,
        messages,
        input,
        status,
        cmdline,
        thread: beside.map(|b| b.0).or(thread.then_some(messages)),
        thread_input: beside.map(|b| b.1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_80x24() {
        let a = areas(Rect::new(0, 0, 80, 24), SIDEBAR_WIDTH, 1);
        assert_eq!((a.sidebar.x, a.sidebar.width), (0, 28));
        assert_eq!((a.status.y, a.status.height), (22, 1));
        assert_eq!((a.cmdline.y, a.cmdline.height), (23, 1));
        assert_eq!(a.input.height, 3);
        for r in [a.messages, a.input] {
            assert_eq!(r.x, 28);
            assert_eq!(r.x + r.width, 80);
        }
    }

    #[test]
    fn areas_tiny() {
        for (w, h) in [(0u16, 0u16), (1, 1), (10, 3)] {
            let outer = Rect::new(0, 0, w, h);
            for rows in [1u16, 5] {
                let a = areas(outer, SIDEBAR_WIDTH, rows);
                for r in [a.sidebar, a.messages, a.input, a.status, a.cmdline] {
                    assert!(r.x >= outer.x && r.y >= outer.y, "{w}x{h} rows {rows}");
                    assert!(r.right() <= outer.right(), "{w}x{h} rows {rows}");
                    assert!(r.bottom() <= outer.bottom(), "{w}x{h} rows {rows}");
                }
            }
        }
    }

    #[test]
    fn areas_custom_sidebar_width() {
        let a = areas(Rect::new(0, 0, 80, 24), 40, 1);
        assert_eq!((a.sidebar.x, a.sidebar.width), (0, 40));
        assert_eq!(a.messages.x, 40);
        assert_eq!(a.messages.width, 40);
        let outer = Rect::new(0, 0, 40, 8);
        let a = areas(outer, 60, 1);
        for r in [a.sidebar, a.messages, a.input, a.status, a.cmdline] {
            assert!(r.x >= outer.x && r.y >= outer.y);
            assert!(r.right() <= outer.right());
            assert!(r.bottom() <= outer.bottom());
        }
    }

    #[test]
    fn a_wide_terminal_puts_the_thread_beside_the_messages() {
        let a = areas_with_thread(Rect::new(0, 0, 160, 40), SIDEBAR_WIDTH, 1, 1, true);
        let thread = a.thread.expect("open");
        assert!(!a.thread_takes_over());
        assert_eq!(a.messages.right(), thread.x, "side by side");
        assert_eq!(thread.right(), 160);
        let input = a.thread_input.expect("the thread has its own input");
        assert_eq!(
            thread.height + input.height,
            a.sidebar.height,
            "the thread and its input fill the body"
        );
        assert_eq!((input.x, input.right()), (thread.x, thread.right()));
        assert_eq!(thread.bottom(), input.y);
        assert_eq!(
            a.input.right(),
            thread.x,
            "the room input stays under the messages"
        );
        assert!(a.messages.width > thread.width, "about 60/40");
    }

    #[test]
    fn a_narrow_terminal_gives_the_message_pane_to_the_thread() {
        let open = areas_with_thread(Rect::new(0, 0, 99, 30), SIDEBAR_WIDTH, 1, 1, true);
        let closed = areas(Rect::new(0, 0, 99, 30), SIDEBAR_WIDTH, 1);
        assert!(open.thread_takes_over());
        assert_eq!(
            open.messages, closed.messages,
            "the message pane keeps its size"
        );
        let edge = areas_with_thread(Rect::new(0, 0, 100, 30), SIDEBAR_WIDTH, 1, 1, true);
        assert!(
            !edge.thread_takes_over(),
            "100 columns is the first split width"
        );
    }

    #[test]
    fn no_thread_leaves_the_layout_as_it_was() {
        let a = areas_with_thread(Rect::new(0, 0, 160, 40), SIDEBAR_WIDTH, 1, 1, false);
        assert_eq!(a, areas(Rect::new(0, 0, 160, 40), SIDEBAR_WIDTH, 1));
        assert_eq!(a.thread, None);
    }

    #[test]
    fn every_size_stays_inside_the_terminal_with_a_thread() {
        for w in [0u16, 1, 10, 40, 99, 100, 101, 160, 300] {
            for h in [0u16, 1, 8, 24, 60] {
                let outer = Rect::new(0, 0, w, h);
                let a = areas_with_thread(outer, SIDEBAR_WIDTH, 3, 2, true);
                let thread = a.thread.expect("open");
                for r in [
                    a.sidebar,
                    a.messages,
                    a.input,
                    a.status,
                    a.cmdline,
                    thread,
                    a.thread_input.unwrap_or_default(),
                ] {
                    assert!(
                        r.right() <= outer.right() && r.bottom() <= outer.bottom(),
                        "{w}x{h}"
                    );
                }
            }
        }
    }
}
