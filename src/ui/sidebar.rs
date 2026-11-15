//! Sidebar pane: the room list (plan 01-04 replaces the plain list with sections and folders).

use crate::app::{App, Focus};
use crate::ui::pane_block;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .rooms
        .iter()
        .map(|r| ListItem::new(sanitize_line(&r.name)))
        .collect();
    let list = List::new(items)
        .block(pane_block(" Rooms ", app.focus == Focus::Sidebar))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !app.rooms.is_empty() {
        state.select(Some(app.sidebar.cursor));
    }
    frame.render_stateful_widget(list, area, &mut state);
}
