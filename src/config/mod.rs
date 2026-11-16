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
    // keymap.toml and theme.toml get real parsers in plans 02-03 and 02-04; until then their
    // syntax errors still land in the one report.
    diag::check_syntax(
        diag::ConfigFile::Keymap,
        sources.keymap.as_deref(),
        &mut problems,
    );
    diag::check_syntax(
        diag::ConfigFile::Theme,
        sources.theme.as_deref(),
        &mut problems,
    );
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

    fn sources(config: &str) -> Sources {
        Sources {
            config: Some(config.to_string()),
            ..Sources::default()
        }
    }

    #[test]
    fn multiple_errors_one_pass() {
        let sources = Sources {
            config: Some(
                "sidebar_width = 99\nhistory_size = \"x\"\n\nkey_timeout_ms = 5\n".to_string(),
            ),
            keymap: Some("[normal\n".to_string()),
            theme: Some("x = \n".to_string()),
            read_errors: Vec::new(),
        };
        let Err(report) = load_from_sources(&sources) else {
            panic!("expected Err");
        };
        assert!(report.errors.len() >= 5, "{report}");
        let order: Vec<(diag::ConfigFile, Option<usize>)> = report
            .errors
            .iter()
            .map(|p| (p.file, p.pos.map(|(line, _)| line)))
            .collect();
        assert_eq!(
            &order[..3],
            &[
                (diag::ConfigFile::Config, Some(1)),
                (diag::ConfigFile::Config, Some(2)),
                (diag::ConfigFile::Config, Some(4)),
            ]
        );
        let files: Vec<_> = order.iter().map(|(f, _)| *f).collect();
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted, "sorted by file");
        assert!(files.contains(&diag::ConfigFile::Keymap));
        assert!(files.contains(&diag::ConfigFile::Theme));
    }

    #[test]
    fn unknown_keys_warn_only() {
        let alone = "sidebar_width = 40\nbogus = 1\n[ui]\nx = 1\n";
        assert!(load_from_sources(&sources(alone)).is_ok());
        let with_error = "sidebar_width = 99\nbogus = 1\n[ui]\nx = 1\n";
        let Err(report) = load_from_sources(&sources(with_error)) else {
            panic!("expected Err");
        };
        assert_eq!(report.errors.len(), 1);
        let keys: Vec<_> = report
            .warnings
            .iter()
            .map(|p| p.key.clone().unwrap_or_default())
            .collect();
        assert_eq!(keys, ["bogus", "ui"]);
    }

    #[test]
    fn server_url_is_unknown_key() {
        let url = "server_url = \"https://chat.example\"\n";
        assert!(load_from_sources(&sources(url)).is_ok());
        let Err(report) = load_from_sources(&sources(&format!("{url}history_size = 0\n"))) else {
            panic!("expected Err");
        };
        assert!(
            report
                .warnings
                .iter()
                .any(|p| p.key.as_deref() == Some("server_url"))
        );
    }

    #[test]
    fn empty_files_are_absent() {
        for text in ["", "  \n# c\n"] {
            let all = Sources {
                config: Some(text.to_string()),
                keymap: Some(text.to_string()),
                theme: Some(text.to_string()),
                read_errors: Vec::new(),
            };
            match load_from_sources(&all) {
                Ok(config) => assert_eq!(config.settings, Settings::default()),
                Err(report) => panic!("expected Ok, got {report}"),
            }
        }
    }

    #[test]
    fn report_display_lists_all() {
        let sources = Sources {
            config: Some("sidebar_width = 99\nbogus = 1\n".to_string()),
            keymap: Some("[normal\n".to_string()),
            ..Sources::default()
        };
        let dir = temp_config_dir("display", &[]);
        let Err(mut report) = load_from_sources(&sources) else {
            panic!("expected Err");
        };
        report.dir = Some(dir.clone());
        report.errors.push(Problem {
            file: diag::ConfigFile::Theme,
            pos: Some((1, 1)),
            key: Some("k\u{1b}]0;x\u{7}ey".to_string()),
            message: "bad \u{202e}text".to_string(),
            severity: diag::Severity::Error,
        });
        let text = report.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[0].starts_with("rockettui: invalid configuration in "),
            "{text}"
        );
        let expected: Vec<String> = report
            .errors
            .iter()
            .chain(&report.warnings)
            .map(|p| p.to_string())
            .collect();
        assert_eq!(&lines[1..], expected.as_slice());
        assert_eq!(report.errors.len(), 3);
        assert_eq!(report.warnings.len(), 1);
        assert!(
            lines[1].starts_with("config.toml:1:"),
            "errors come first: {text}"
        );
        assert!(
            lines[lines.len() - 1].contains("bogus"),
            "warnings come last: {text}"
        );
        assert!(!text.contains('\u{1b}') && !text.contains('\u{7}') && !text.contains('\u{202e}'));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
