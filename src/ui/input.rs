//! Input pane: an empty box for now (plan 01-06 adds the multiline buffer).

use crate::app::{App, Focus};
use crate::ui::pane_block;
use ratatui::Frame;
use ratatui::layout::Rect;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(pane_block(" Input ", app.focus == Focus::Input), area);
}
