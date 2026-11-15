//! rockettui: a modal terminal client for Rocket.Chat. This crate is the headless core
//! (Elm/TEA: one `App`, one `Action` enum, one reducer); the terminal layer sits above it.

pub mod action;
pub mod app;
pub mod data;
pub mod input;

#[cfg(test)]
pub(crate) mod test_util {
    use crate::action::Action;
    use crate::app::App;
    use crate::app::update::update;
    use crate::input::key::parse_keys;
    use crossterm::event::KeyEvent;
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
}
