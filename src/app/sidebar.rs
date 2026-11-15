//! Sidebar cursor state (D-02). Plan 01-04 replaces the row model with sections behind these
//! same functions.

use crate::action::Action;
use crate::app::App;
use crate::data::RoomId;

#[derive(Debug, Default)]
pub struct SidebarState {
    pub cursor: usize,
}

/// The room under the cursor, in `app.rooms` order; `None` when there are no rooms.
pub fn selected_room(app: &App) -> Option<RoomId> {
    app.rooms.get(app.sidebar.cursor).map(|r| r.id.clone())
}

/// Apply a motion to the sidebar cursor. `MoveDown` / `MoveUp` move by `count` (default 1),
/// saturating at the ends of the room list; other motions are ignored.
pub fn apply_motion(app: &mut App, motion: &Action, count: Option<u32>) {
    let step = count.unwrap_or(1) as usize;
    let last = app.rooms.len().saturating_sub(1);
    match motion {
        Action::MoveDown => {
            app.sidebar.cursor = app.sidebar.cursor.saturating_add(step).min(last);
        }
        Action::MoveUp => {
            app.sidebar.cursor = app.sidebar.cursor.saturating_sub(step);
        }
        _ => {}
    }
}

/// Keep the cursor inside the room list after the rooms changed.
pub fn on_rows_changed(app: &mut App) {
    let last = app.rooms.len().saturating_sub(1);
    app.sidebar.cursor = app.sidebar.cursor.min(last);
}
