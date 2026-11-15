//! Test helpers shared by every module's tests (compiled only under `cfg(test)`).

use crate::action::Action;
use crate::app::App;
use crate::app::update::update;
use crate::input::key::parse_keys;
use crate::ui;
use crossterm::event::KeyEvent;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Size;

/// Press keys written in Vim notation: each parsed chord goes through `update` as a key event.
pub(crate) fn press(app: &mut App, keys: &str) {
    for chord in parse_keys(keys).expect("test key notation parses") {
        update(app, Action::Key(KeyEvent::new(chord.code, chord.mods)));
    }
}

/// An 80x24 app loaded with the fake data through the reducer.
pub(crate) fn app_80x24() -> App {
    App::with_fake_data(Size::new(80, 24))
}

/// Draw `ui::view` on a `TestBackend` of the given size and return the screen as text: one line
/// per row, cell symbols concatenated, no quoting (the backend's own `Display` wraps each row in
/// quote marks, which would hide a row's first characters from `starts_with`).
pub(crate) fn render_to_string(app: &App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    terminal.draw(|f| ui::view(f, app)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        if y > 0 {
            out.push('\n');
        }
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
    }
    out
}
