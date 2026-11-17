//! Configuration: three TOML files under `$XDG_CONFIG_HOME/rockettui/` (or `~/.config/rockettui/`).
//!
//! A pure core (`load_from_sources`: strings in, `Result<Config, Report>` out, no environment, no
//! filesystem) with a thin IO shell (`load_from_dir`, `load`). Config is loaded once, before the
//! terminal starts, and held in `App`; the reducer never reads files (TEA).

pub mod connection;
pub mod diag;
pub mod keymap;
pub mod paths;
pub mod settings;
pub mod theme;

use crate::input::keymap::{Keymap, default_keymap};
use std::path::Path;

pub use diag::{Problem, Report};
pub use settings::Settings;
pub use theme::Theme;

/// Everything the app needs from the config files.
pub struct Config {
    pub settings: Settings,
    pub keymap: Keymap,
    pub theme: Theme,
    /// The validated server URL and credentials, `None` when the environment gave none (the
    /// binary requires them, `load_from_dir` does not). `App` never receives this: the token
    /// goes into `api::Client` and nowhere else.
    pub connection: Option<connection::Connection>,
    /// Runtime notices the app shows while it runs (D-04): a config.toml holding the token that
    /// others can read, or a token sent over http to a remote host. Not config `Report` warnings
    /// (those appear only next to errors, Phase 2 D-03).
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            settings: Settings::default(),
            keymap: default_keymap(),
            theme: Theme::default(),
            connection: None,
            warnings: Vec::new(),
        }
    }
}

/// The text of each file (`None` = absent) plus the problems met while reading them.
///
/// `Debug` is written by hand: `config` may hold the token, so only byte counts are printed
/// (D-07), never text.
#[derive(Clone, Default)]
pub struct Sources {
    pub config: Option<String>,
    pub keymap: Option<String>,
    pub theme: Option<String>,
    pub read_errors: Vec<Problem>,
    /// The three `ROCKETTUI_*` variables, read once by `load` (the only environment reads).
    pub env: connection::ConnEnv,
    /// True for the binary: a missing server URL, user id or token is then a config error (D-03).
    pub require_connection: bool,
    /// The permission bits of `config.toml` as `read_sources` saw them (`None` when the file was
    /// not read, or off Unix). Read once at startup together with the file (D-04).
    pub config_mode: Option<u32>,
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn size(text: &Option<String>) -> String {
            match text {
                Some(text) => format!("<{} bytes>", text.len()),
                None => "absent".to_string(),
            }
        }
        f.debug_struct("Sources")
            .field("config", &format_args!("{}", size(&self.config)))
            .field("keymap", &format_args!("{}", size(&self.keymap)))
            .field("theme", &format_args!("{}", size(&self.theme)))
            .field("read_errors", &self.read_errors.len())
            .field("env", &self.env)
            .field(
                "config_mode",
                &self.config_mode.map(|mode| format!("{mode:o}")),
            )
            .field("require_connection", &self.require_connection)
            .finish()
    }
}

/// Build a `Config` from file contents. Warnings are kept only when there is also an error (D-03).
pub fn load_from_sources(sources: &Sources) -> Result<Config, Report> {
    let mut problems = sources.read_errors.clone();
    let settings = settings::parse(sources.config.as_deref(), &mut problems);
    let keymap = keymap::build(
        settings.keymap_preset,
        sources.keymap.as_deref(),
        &mut problems,
    );
    let theme = theme::parse(sources.theme.as_deref(), &mut problems);
    let file = connection::parse_file(sources.config.as_deref(), &mut problems);
    let connection = connection::resolve(
        &sources.env,
        &file,
        sources.require_connection,
        &mut problems,
    );
    let warnings = connection::warnings(connection.as_ref(), sources.config_mode);
    let report = Report::new(problems);
    if report.errors.is_empty() {
        Ok(Config {
            settings,
            keymap,
            theme,
            connection,
            warnings,
        })
    } else {
        Err(report)
    }
}

/// Read the three files of `dir` and take the connection variables from `env`, then
/// `load_from_sources`.
pub fn load_from_dir_env(
    dir: &Path,
    env: connection::ConnEnv,
    require: bool,
) -> Result<Config, Report> {
    let mut sources = paths::read_sources(dir);
    sources.env = env;
    sources.require_connection = require;
    load_from_sources(&sources).map_err(|mut report| {
        report.dir = Some(dir.to_path_buf());
        report
    })
}

/// Read the three files of `dir` without any connection variables (nothing required).
pub fn load_from_dir(dir: &Path) -> Result<Config, Report> {
    load_from_dir_env(dir, connection::ConnEnv::default(), false)
}

/// The connection variables of the process environment. The only place they are read.
fn env_from_process() -> connection::ConnEnv {
    connection::ConnEnv {
        server_url: connection::EnvValue::from_os(std::env::var_os(connection::ENV_SERVER_URL)),
        user_id: connection::EnvValue::from_os(std::env::var_os(connection::ENV_USER_ID)),
        token: connection::EnvValue::from_os(std::env::var_os(connection::ENV_TOKEN)),
    }
}

/// Resolve the config directory from the environment and load it, requiring the connection
/// variables (D-03). With no directory known the defaults apply, but the connection is still
/// required.
pub fn load() -> Result<Config, Report> {
    let env = env_from_process();
    match paths::config_dir(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    ) {
        Some(dir) => load_from_dir_env(&dir, env, true),
        None => load_from_sources(&Sources {
            env,
            require_connection: true,
            ..Sources::default()
        }),
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
            ..Sources::default()
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

    const SENTINEL: &str = "SENTINEL-TOKEN-9c41";

    #[test]
    fn server_url_is_known_key() {
        let all = format!(
            "server_url = \"https://chat.example\"\nuser_id = \"u1\"\ntoken = \"{SENTINEL}\"\n"
        );
        let config = load_from_sources(&sources(&all)).expect("ok");
        assert!(config.connection.is_some());

        // A partial setting is an error even when nothing is required, and server_url is never
        // reported as an unknown key.
        let url = "server_url = \"https://chat.example\"\n";
        let Err(report) = load_from_sources(&sources(url)) else {
            panic!("expected Err");
        };
        let keys: Vec<_> = report
            .errors
            .iter()
            .map(|p| p.key.clone().unwrap_or_default())
            .collect();
        assert_eq!(keys, ["user_id", "token"]);
        assert!(report.warnings.is_empty(), "{report}");

        let Err(report) = load_from_sources(&sources(&format!("{url}history_size = 0\n"))) else {
            panic!("expected Err");
        };
        assert!(
            !report
                .warnings
                .iter()
                .any(|p| p.key.as_deref() == Some("server_url"))
        );
    }

    #[test]
    fn token_never_echoed() {
        for text in [
            format!("token = \"{SENTINEL}\"\nsidebar_width = 99\n"),
            format!("token = \"{SENTINEL}\n"),
        ] {
            let sources = sources(&text);
            let Err(report) = load_from_sources(&sources) else {
                panic!("expected Err for {text:?}");
            };
            assert!(!report.to_string().contains(SENTINEL), "{report}");
            assert!(!format!("{report:?}").contains(SENTINEL));
            assert!(!format!("{sources:?}").contains(SENTINEL));
        }
    }

    fn file_creds(url: &str) -> String {
        format!("server_url = \"{url}\"\nuser_id = \"u1\"\ntoken = \"{SENTINEL}\"\n")
    }

    fn warnings_of(sources: &Sources) -> Vec<String> {
        load_from_sources(sources).expect("ok").warnings
    }

    #[test]
    fn perm_warning_rules() {
        let creds = file_creds("https://chat.example");
        for (mode, shown) in [
            (0o100_644, "mode 0644"),
            (0o100_640, "mode 0640"),
            (0o100_604, "mode 0604"),
        ] {
            let warnings = warnings_of(&Sources {
                config_mode: Some(mode),
                ..sources(&creds)
            });
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(warnings[0].contains(shown), "{warnings:?}");
            assert!(warnings[0].contains("chmod 600"), "{warnings:?}");
            assert!(!warnings[0].contains(SENTINEL));
        }
        let owner_only = Sources {
            config_mode: Some(0o100_600),
            ..sources(&creds)
        };
        assert!(warnings_of(&owner_only).is_empty());

        // A token that comes from the environment gives no warning, whatever the file mode.
        let env_token = Sources {
            config_mode: Some(0o100_644),
            env: connection::ConnEnv {
                token: connection::EnvValue::Set(connection::Secret::new("env-token".to_string())),
                ..connection::ConnEnv::default()
            },
            ..sources(&creds)
        };
        assert!(warnings_of(&env_token).is_empty());

        // No connection at all: nothing to warn about.
        let none = Sources {
            config_mode: Some(0o100_644),
            ..sources("sidebar_width = 30\n")
        };
        assert!(warnings_of(&none).is_empty());
    }

    #[test]
    fn cleartext_warning() {
        let text = "server_url uses http:// to a non-local host: the token is sent unencrypted";
        for (url, warned) in [
            ("http://chat.example", true),
            ("http://localhost:3100", false),
            ("http://127.0.0.1:3100", false),
            ("http://127.5.6.7:3100", false),
            ("http://[::1]:3100", false),
            ("https://chat.example", false),
        ] {
            let warnings = warnings_of(&Sources {
                config_mode: Some(0o100_600),
                ..sources(&file_creds(url))
            });
            if warned {
                assert_eq!(warnings, [text], "{url}");
            } else {
                assert!(warnings.is_empty(), "{url}: {warnings:?}");
            }
        }
    }

    #[test]
    fn connection_env_through_sources() {
        use connection::{ConnEnv, EnvValue, Secret};
        let set = |text: &str| EnvValue::Set(Secret::new(text.to_string()));

        // The binary's path requires the connection: three errors naming each variable.
        let required = Sources {
            require_connection: true,
            ..Sources::default()
        };
        let Err(report) = load_from_sources(&required) else {
            panic!("expected Err");
        };
        assert_eq!(report.errors.len(), 3);

        // Tolerant by default: no connection, no problem.
        let config = load_from_sources(&Sources::default()).expect("ok");
        assert!(config.connection.is_none());

        // All three set: the connection is resolved and the Debug of Sources hides the token.
        let full = Sources {
            env: ConnEnv {
                server_url: set("http://127.0.0.1:3100"),
                user_id: set("u1"),
                token: set("SENTINEL-TOKEN-9c41"),
            },
            require_connection: true,
            ..Sources::default()
        };
        assert!(!format!("{full:?}").contains("SENTINEL-TOKEN-9c41"));
        let config = load_from_sources(&full).expect("ok");
        assert!(config.connection.is_some());
    }

    #[test]
    fn empty_files_are_absent() {
        for text in ["", "  \n# c\n"] {
            let all = Sources {
                config: Some(text.to_string()),
                keymap: Some(text.to_string()),
                theme: Some(text.to_string()),
                ..Sources::default()
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
