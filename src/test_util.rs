//! Test helpers shared by every module's tests (compiled only under `cfg(test)`).

use crate::action::Action;
use crate::app::App;
use crate::app::update::update;
use crate::config::Config;
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

/// Draw `ui::view` on a `TestBackend` of the given size and return the cell buffer, for tests that
/// assert styles and colors rather than text.
pub(crate) fn render_buffer(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    terminal.draw(|f| ui::view(f, app)).expect("draw");
    terminal.backend().buffer().clone()
}

/// A fresh directory `<temp>/rockettui-test-<tag>-<pid>` holding the given `(name, contents)`
/// files. Any earlier directory of that name is removed first; the caller removes it at the end.
pub(crate) fn temp_config_dir(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rockettui-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp config dir");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write temp config file");
    }
    dir
}

/// The `Config` parsed from the text of a config.toml; panics with the report if it is invalid.
pub(crate) fn config_from_toml(toml: &str) -> Config {
    let sources = crate::config::Sources {
        config: Some(toml.to_string()),
        ..Default::default()
    };
    match crate::config::load_from_sources(&sources) {
        Ok(config) => config,
        Err(report) => panic!("test config is invalid: {} problem(s)", report.errors.len()),
    }
}

/// The `Config` parsed from the text of a theme.toml; panics with the problem count if invalid.
pub(crate) fn config_from_theme(toml: &str) -> Config {
    let sources = crate::config::Sources {
        theme: Some(toml.to_string()),
        ..Default::default()
    };
    match crate::config::load_from_sources(&sources) {
        Ok(config) => config,
        Err(report) => panic!("test theme is invalid: {} problem(s)", report.errors.len()),
    }
}

/// An app built from `config`, with the fake data loaded through the reducer.
pub(crate) fn app_with_config(config: Config, w: u16, h: u16) -> App {
    let mut app = App::with_config(Size::new(w, h), config);
    for action in crate::data::fake::seed() {
        update(&mut app, action);
    }
    app
}

/// A plain `Sent` message of room `r-general` by `alice`, for tests that need some message.
pub(crate) fn sample_message(id: &str) -> crate::data::Message {
    crate::data::Message {
        id: id.to_string(),
        room_id: crate::data::RoomId("r-general".to_string()),
        author: "alice".to_string(),
        ts: 0,
        body: "sample".to_string(),
        kind: crate::data::MessageKind::Normal,
        send: crate::data::SendState::Sent,
        edited: false,
        plain_text: true,
    }
}

/// The `Config` parsed from the text of a keymap.toml; panics with the problem count if invalid.
pub(crate) fn config_from_keymap(toml: &str) -> Config {
    let sources = crate::config::Sources {
        keymap: Some(toml.to_string()),
        ..Default::default()
    };
    match crate::config::load_from_sources(&sources) {
        Ok(config) => config,
        Err(report) => panic!("test keymap is invalid: {} problem(s)", report.errors.len()),
    }
}
