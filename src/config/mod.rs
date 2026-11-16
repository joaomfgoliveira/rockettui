//! Configuration: three TOML files under `$XDG_CONFIG_HOME/rockettui/` (or `~/.config/rockettui/`).
//!
//! A pure core (`load_from_sources`: strings in, `Result<Config, Report>` out, no environment, no
//! filesystem) with a thin IO shell (`load_from_dir`, `load`). Config is loaded once, before the
//! terminal starts, and held in `App`; the reducer never reads files (TEA).

pub mod diag;
pub mod paths;
pub mod settings;

use crate::input::keymap::{Keymap, default_keymap};
use std::path::Path;

pub use diag::{Problem, Report};
pub use settings::Settings;

/// Everything the app needs from the config files.
pub struct Config {
    pub settings: Settings,
    pub keymap: Keymap,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            settings: Settings::default(),
            keymap: default_keymap(),
        }
    }
}

/// The text of each file (`None` = absent) plus the problems met while reading them.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub config: Option<String>,
    pub keymap: Option<String>,
    pub theme: Option<String>,
    pub read_errors: Vec<Problem>,
}

/// Build a `Config` from file contents. Warnings are kept only when there is also an error (D-03).
pub fn load_from_sources(sources: &Sources) -> Result<Config, Report> {
    let mut problems = sources.read_errors.clone();
    let settings = settings::parse(sources.config.as_deref(), &mut problems);
    let report = Report::new(problems);
    if report.errors.is_empty() {
        Ok(Config {
            settings,
            keymap: default_keymap(),
        })
    } else {
        Err(report)
    }
}

/// Read the three files of `dir`, then `load_from_sources`.
pub fn load_from_dir(dir: &Path) -> Result<Config, Report> {
    load_from_sources(&paths::read_sources(dir)).map_err(|mut report| {
        report.dir = Some(dir.to_path_buf());
        report
    })
}

/// Resolve the config directory from the environment and load it. No directory known: defaults.
pub fn load() -> Result<Config, Report> {
    match paths::config_dir(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    ) {
        Some(dir) => load_from_dir(&dir),
        None => Ok(Config::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{app_with_config, press, render_to_string, temp_config_dir};
    use crate::ui::wrap::MessageLayout;

    /// True when a pane border is followed by `HH:MM`, one space, then a lowercase letter: the
    /// Compact layout. The border anchors the match, so a time inside a message body is ignored.
    fn has_compact_prefix(row: &str) -> bool {
        let chars: Vec<char> = row.chars().collect();
        chars.windows(8).any(|w| {
            matches!(w[0], '\u{2503}' | '\u{2502}')
                && w[1].is_ascii_digit()
                && w[2].is_ascii_digit()
                && w[3] == ':'
                && w[4].is_ascii_digit()
                && w[5].is_ascii_digit()
                && w[6] == ' '
                && w[7].is_ascii_lowercase()
        })
    }

    /// True when `row` holds `alice`, two spaces, then `HH:MM`: the TwoLine header.
    fn has_two_line_header(row: &str) -> bool {
        let Some(at) = row.find("alice  ") else {
            return false;
        };
        let rest: Vec<char> = row[at + "alice  ".len()..].chars().take(5).collect();
        rest.len() == 5
            && rest[0].is_ascii_digit()
            && rest[1].is_ascii_digit()
            && rest[2] == ':'
            && rest[3].is_ascii_digit()
            && rest[4].is_ascii_digit()
    }

    #[test]
    fn tracer_two_line_from_dir() {
        let dir = temp_config_dir(
            "tracer",
            &[("config.toml", "message_layout = \"two_line\"\n")],
        );
        let config = match load_from_dir(&dir) {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        };
        let mut app = app_with_config(config, 80, 24);
        assert_eq!(app.message_layout, MessageLayout::TwoLine);
        press(&mut app, "<CR>");
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.lines().any(has_two_line_header), "{screen}");
        assert!(!screen.lines().any(has_compact_prefix), "{screen}");
        let _ = std::fs::remove_dir_all(&dir);

        // Positive control: the defaults draw the Compact layout the helpers above look for.
        let mut app = app_with_config(Config::default(), 80, 24);
        press(&mut app, "<CR>");
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.lines().any(has_compact_prefix), "{screen}");
        assert!(!screen.lines().any(has_two_line_header), "{screen}");
    }

    #[test]
    fn absent_files_default() {
        match load_from_sources(&Sources::default()) {
            Ok(config) => {
                assert_eq!(config.settings, Settings::default());
                assert_eq!(config.settings.message_layout, MessageLayout::Compact);
            }
            Err(report) => panic!("expected Ok, got {report}"),
        }
        let missing =
            std::env::temp_dir().join(format!("rockettui-test-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        match load_from_dir(&missing) {
            Ok(config) => assert_eq!(config.settings, Settings::default()),
            Err(report) => panic!("expected Ok, got {report}"),
        }
        assert!(!missing.exists(), "loading must not create the directory");
    }
}
