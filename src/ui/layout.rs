//! Pure pane geometry, shared by the view and (later) the reducer so scroll math and rendering
//! never disagree. Every computation is saturating: any `Rect`, including 0x0, is safe.

use ratatui::layout::{Constraint, Layout, Rect};

/// Default width of the sidebar column in cells, borders included (D-13); config.toml
/// `sidebar_width` overrides it.
pub const SIDEBAR_WIDTH: u16 = 28;

/// The five regions of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Areas {
    pub sidebar: Rect,
    pub messages: Rect,
    pub input: Rect,
    pub status: Rect,
    pub cmdline: Rect,
}

/// Split `area` into the five regions. `sidebar_width` is the sidebar column width in cells; it is
/// not clamped, so a width beyond the terminal leaves the main column narrow or empty. `input_rows`
/// is the number of content rows of the input bar; the bar itself is two rows taller (borders).
pub fn areas(area: Rect, sidebar_width: u16, input_rows: u16) -> Areas {
    let [body, status, cmdline] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let [sidebar, main] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(0)]).areas(body);
    let [messages, input] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(input_rows.saturating_add(2)),
    ])
    .areas(main);
    Areas {
        sidebar,
        messages,
        input,
        status,
        cmdline,
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
}
