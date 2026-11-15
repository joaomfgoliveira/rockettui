//! Per-room message cursor and row-based scroll model (D-04).
//!
//! Heights come from `ui::wrap::message_rows` at the width of `layout::areas`, the same function
//! and geometry the renderer uses, so scroll math and drawing never disagree.

use crate::action::Action;
use crate::app::App;
use crate::data::RoomId;
use crate::ui::layout;
use crate::ui::wrap;
use ratatui::layout::Rect;

/// Where a room's message pane is looking: the selected message and the first visible row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MsgView {
    pub cursor: usize,
    pub top_row: usize,
}

/// Inner (width, height) of the messages pane for the app's current size, borders removed.
pub fn viewport(app: &App) -> (u16, u16) {
    let area = Rect::new(0, 0, app.size.width, app.size.height);
    let pane = layout::areas(area, app.input_rows()).messages;
    (pane.width.saturating_sub(2), pane.height.saturating_sub(2))
}

/// First row of every message of `room` at the current width, with the total row count as the
/// last element (so a room of `n` messages yields `n + 1` entries; no room data yields `[0]`).
pub fn row_starts(app: &App, room: &RoomId) -> Vec<usize> {
    let (width, _) = viewport(app);
    let mut starts = vec![0];
    let mut total = 0;
    if let Some(msgs) = app.messages.get(room) {
        for msg in msgs {
            total += wrap::message_rows(msg, width, app.message_layout);
            starts.push(total);
        }
    }
    starts
}

/// The new `top_row` that brings message `cursor` into the window.
fn scroll_into_view(
    _starts: &[usize],
    _cursor: usize,
    top_row: usize,
    _viewport_h: usize,
) -> usize {
    top_row
}

/// The effective view of `room`: the stored one (or the newest default), clamped and scrolled so
/// the cursor message is inside the window at the current size.
pub fn view(_app: &App, _room: &RoomId) -> MsgView {
    MsgView::default()
}

/// Apply a Normal-mode motion to the messages pane of the current room.
pub fn apply_motion(_app: &mut App, _motion: &Action, _count: Option<u32>) {}

/// Forget the stored view so the room shows its newest message again.
pub fn follow_newest(_app: &mut App, _room: &RoomId) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Focus;
    use crate::app::update::update;
    use crate::test_util::{app_80x24, press};

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn open_general() -> App {
        let mut app = app_80x24();
        app.open_room(general());
        app
    }

    fn viewport_h(app: &App) -> usize {
        usize::from(viewport(app).1)
    }

    /// Index of the message that holds `row`.
    fn containing(starts: &[usize], row: usize) -> usize {
        starts.partition_point(|&s| s <= row) - 1
    }

    /// The cursor message lies inside the window, or starts at the top when it is taller.
    fn assert_cursor_visible(app: &App, v: MsgView) {
        let starts = row_starts(app, &general());
        let vh = viewport_h(app);
        let first = starts[v.cursor];
        let end = starts[v.cursor + 1];
        if end - first > vh {
            assert_eq!(v.top_row, first, "tall message must start at the top");
        } else {
            assert!(v.top_row <= first, "{v:?} first {first}");
            assert!(end <= v.top_row + vh, "{v:?} end {end} vh {vh}");
        }
    }

    #[test]
    fn open_room_starts_at_newest() {
        let app = open_general();
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        let starts = row_starts(&app, &general());
        let vh = viewport_h(&app);
        let last_row = starts[60] - 1;
        assert!(v.top_row <= last_row && last_row < v.top_row + vh);
        assert_eq!(v.top_row, starts[60] - vh);
        assert!(app.msg_views.is_empty(), "the default view is not stored");
    }

    #[test]
    fn motion_edges() {
        let mut app = open_general();
        press(&mut app, "gg");
        press(&mut app, "k");
        assert_eq!(view(&app, &general()).cursor, 0);
        press(&mut app, "G");
        press(&mut app, "j");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "k");
        assert_eq!(view(&app, &general()).cursor, 58);
        press(&mut app, "j");
        assert_eq!(view(&app, &general()).cursor, 59);
    }

    #[test]
    fn motion_gg_g_capital() {
        let mut app = open_general();
        press(&mut app, "gg");
        let v = view(&app, &general());
        assert_eq!((v.cursor, v.top_row), (0, 0));
        press(&mut app, "G");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        assert_cursor_visible(&app, v);
        let starts = row_starts(&app, &general());
        assert_eq!(v.top_row, starts[60] - viewport_h(&app));
    }

    #[test]
    fn motion_counts() {
        let mut app = open_general();
        press(&mut app, "5k");
        assert_eq!(view(&app, &general()).cursor, 54);
        press(&mut app, "5gg");
        assert_eq!(view(&app, &general()).cursor, 4);
        press(&mut app, "G");
        press(&mut app, "5G");
        assert_eq!(view(&app, &general()).cursor, 4);
        press(&mut app, "5j");
        assert_eq!(view(&app, &general()).cursor, 9);
        press(&mut app, "9999j");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "9999gg");
        assert_eq!(view(&app, &general()).cursor, 59);
    }

    #[test]
    fn motion_half_page() {
        let mut app = open_general();
        let starts = row_starts(&app, &general());
        let half = (viewport_h(&app) / 2).max(1);
        assert_eq!(half, 8, "17 inner rows at 80x24");

        press(&mut app, "gg");
        press(&mut app, "<C-d>");
        let down = view(&app, &general());
        assert_eq!(down.cursor, containing(&starts, half));
        assert!(down.cursor > 0);
        // The window moved down by the half page, ending aligned to the cursor message's
        // first row when that message begins above the new top.
        assert!(down.top_row > 0 && down.top_row <= half, "{down:?}");
        assert_eq!(down.top_row, starts[down.cursor]);
        assert_cursor_visible(&app, down);

        press(&mut app, "<C-u>");
        let up = view(&app, &general());
        assert!(up.cursor < down.cursor, "{up:?} {down:?}");
        assert!(up.top_row <= down.top_row);
        assert_cursor_visible(&app, up);

        press(&mut app, "gg");
        press(&mut app, "3<C-d>");
        let three = view(&app, &general());
        assert_eq!(three.cursor, containing(&starts, 3 * half));
        assert_cursor_visible(&app, three);

        press(&mut app, "99<C-d>");
        let bottom = view(&app, &general());
        assert_eq!(bottom.cursor, 59);
        assert_cursor_visible(&app, bottom);
        press(&mut app, "<C-d>");
        assert_eq!(view(&app, &general()), bottom);

        press(&mut app, "99<C-u>");
        let top = view(&app, &general());
        assert_eq!((top.cursor, top.top_row), (0, 0));
    }

    #[test]
    fn long_message_aligns_top() {
        let mut app = open_general();
        update(&mut app, Action::Resize(80, 12));
        let starts = row_starts(&app, &general());
        let vh = viewport_h(&app);
        assert!(starts[31] - starts[30] > vh, "long message must be taller");
        press(&mut app, "31gg");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 30);
        assert_eq!(v.top_row, starts[30]);
        // Moving down keeps the long message reachable and then leaves it.
        press(&mut app, "j");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 31);
        assert_cursor_visible(&app, v);
    }

    #[test]
    fn empty_room_noop() {
        let mut app = app_80x24();
        app.open_room(RoomId("quiet".to_string()));
        for keys in ["j", "k", "gg", "G", "<C-d>", "<C-u>", "5j", "5gg", "h", "l"] {
            press(&mut app, keys);
        }
        assert!(app.msg_views.is_empty());
        let room = RoomId("quiet".to_string());
        assert_eq!(view(&app, &room), MsgView::default());
    }

    #[test]
    fn no_room_noop() {
        let mut app = app_80x24();
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(app.current_room, None);
        for keys in ["j", "k", "gg", "G", "<C-d>", "<C-u>", "5j"] {
            press(&mut app, keys);
        }
        assert!(app.msg_views.is_empty());
    }

    #[test]
    fn resize_clamps_view() {
        let mut app = open_general();
        press(&mut app, "G");
        update(&mut app, Action::Resize(200, 60));
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        assert_cursor_visible(&app, v);
        let starts = row_starts(&app, &general());
        assert!(v.top_row <= starts[60].saturating_sub(viewport_h(&app)));
    }

    #[test]
    fn resize_shrink_scrolls_into_view() {
        for (keys, expected) in [("G", 59), ("G20k", 39), ("gg", 0)] {
            for sizes in [[(80u16, 24u16), (50, 12)], [(80, 24), (40, 8)]] {
                let mut app = open_general();
                update(&mut app, Action::Resize(200, 60));
                press(&mut app, keys);
                assert_eq!(view(&app, &general()).cursor, expected, "{keys}");
                for (w, h) in sizes {
                    update(&mut app, Action::Resize(w, h));
                    let v = view(&app, &general());
                    assert_eq!(v.cursor, expected, "{keys} at {w}x{h}");
                    assert_cursor_visible(&app, v);
                }
            }
        }
    }

    #[test]
    fn motion_h_l_noop() {
        let mut app = open_general();
        let before = view(&app, &general());
        for keys in ["h", "l", "5l", "3h"] {
            press(&mut app, keys);
            assert_eq!(view(&app, &general()), before, "{keys}");
            assert!(app.msg_views.is_empty(), "{keys} stored a view");
        }
        press(&mut app, "k");
        let moved = view(&app, &general());
        assert_ne!(moved, before);
        for keys in ["h", "l", "5l"] {
            press(&mut app, keys);
            assert_eq!(view(&app, &general()), moved, "{keys}");
        }
        assert_eq!(app.msg_views.len(), 1);
    }

    #[test]
    fn follow_newest_resets() {
        let mut app = open_general();
        let newest = view(&app, &general());
        press(&mut app, "gg");
        assert_ne!(view(&app, &general()), newest);
        follow_newest(&mut app, &general());
        assert_eq!(view(&app, &general()), newest);
        assert!(app.msg_views.is_empty());
    }

    #[test]
    fn focus_routing() {
        let mut app = open_general();
        let selected = crate::app::sidebar::selected_room(&app);
        press(&mut app, "5gg");
        assert_eq!(view(&app, &general()).cursor, 4);
        assert_eq!(crate::app::sidebar::selected_room(&app), selected);

        press(&mut app, "G");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Sidebar);
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Messages);
        press(&mut app, "gg");
        assert_eq!(view(&app, &general()).cursor, 0);
        assert_eq!(crate::app::sidebar::selected_room(&app), selected);
    }
}
