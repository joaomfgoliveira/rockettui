//! `keymap.toml`: per-mode tables of `"<keys>" = "action_name"` rows layered on a preset (D-04).
//!
//! Key notation is Vim notation (Phase 1 D-08); action names are the curated vocabulary of
//! [`crate::input::actions`] (D-07). Every problem found is collected with its file, line:col and
//! key path; nothing here panics on user data.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use super::settings::KeymapPreset;
use crate::action::Action;
use crate::app::Mode;
use crate::app::command::COMMANDS;
use crate::input::actions::action_from_name;
use crate::input::key::parse_keys;
use crate::input::keymap::{Keymap, default_bindings, default_keymap, vim_extras_bindings};
use toml::Spanned;
use toml::de::DeValue;

/// The mode tables of the file, in the order their rows are applied.
const MODES: [(&str, Mode); 3] = [
    ("normal", Mode::Normal),
    ("insert", Mode::Insert),
    ("command", Mode::Command),
];

/// Longest piece of user-written text echoed in a problem (T-02-01).
const ECHO_CAP: usize = 32;

/// One valid `"<keys>" = "<action>"` row of the user's file.
struct UserRow {
    mode: Mode,
    notation: String,
    action: Action,
}

/// At most `ECHO_CAP` characters of `text`, with `...` when it was cut.
fn cap(text: &str) -> String {
    let mut shown: String = text.chars().take(ECHO_CAP).collect();
    if text.chars().count() > ECHO_CAP {
        shown.push_str("...");
    }
    shown
}

/// The key path of a row in problem output: `normal."<C-w>h"`.
fn key_path(table: &str, notation: &str) -> String {
    format!("{table}.\"{}\"", cap(notation))
}

/// The rows of the chosen preset (D-14): `vim` is the Phase 1 defaults, `vim-extras` adds Tab and
/// Shift-Tab pane focus cycling.
fn preset_rows(preset: KeymapPreset) -> Vec<(Mode, &'static str, Action)> {
    match preset {
        KeymapPreset::Vim => default_bindings(),
        KeymapPreset::VimExtras => vim_extras_bindings(),
    }
}

/// The action a value stands for: a curated name, or `cmd:<command>` (D-07). A `cmd:` command
/// must exist in the in-app command table, so a typo fails at load and not at the key press (A5).
fn resolve_value(text: &str) -> Result<Action, String> {
    let Some(rest) = text.strip_prefix("cmd:") else {
        return action_from_name(text).ok_or_else(|| format!("unknown action '{}'", cap(text)));
    };
    let line = rest.trim();
    let Some(name) = line.split_whitespace().next() else {
        return Err("empty command after cmd:".to_string());
    };
    if COMMANDS.iter().any(|def| def.names.contains(&name)) {
        Ok(Action::RunCommand(line.to_string()))
    } else {
        Err(format!("unknown command '{}'", cap(name)))
    }
}

/// Build the keymap: the preset, then the user's rows on top. Every problem goes to `out`; when
/// the file is absent, empty or has syntax errors the result is the plain preset.
pub fn build(preset: KeymapPreset, src: Option<&str>, out: &mut Vec<Problem>) -> Keymap {
    let preset_rows = preset_rows(preset);
    let Some(src) = src else {
        return preset_keymap(&preset_rows);
    };
    let Some(root) = parse_table(ConfigFile::Keymap, src, out) else {
        return preset_keymap(&preset_rows);
    };
    let mut reader = Reader {
        file: ConfigFile::Keymap,
        src,
        out,
    };
    let table = root.get_ref();
    let mut rows: Vec<UserRow> = Vec::new();
    for (name, mode) in MODES {
        if let Some(value) = table.get(name) {
            read_mode_table(&mut reader, name, mode, value, &mut rows);
        }
    }
    // D-03: any other top-level key or table is a warning only.
    let known: Vec<&str> = MODES.iter().map(|(name, _)| *name).collect();
    reader.warn_unknown(table, &known, "");

    let mut all: Vec<(Mode, &str, Action)> = preset_rows;
    all.extend(
        rows.iter()
            .map(|row| (row.mode, row.notation.as_str(), row.action.clone())),
    );
    match Keymap::from_bindings(&all) {
        Ok(keymap) => keymap,
        Err(_) => {
            // Every notation parsed already; if this still fails, say so instead of panicking.
            reader.out.push(Problem {
                file: ConfigFile::Keymap,
                pos: None,
                key: None,
                message: "internal error: a validated key binding failed to load".to_string(),
                severity: super::diag::Severity::Error,
            });
            default_keymap()
        }
    }
}

/// The keymap of the preset rows alone. Built-in data: a parse failure is a bug in the preset.
fn preset_keymap(rows: &[(Mode, &str, Action)]) -> Keymap {
    Keymap::from_bindings(rows).expect("built-in keymap parses")
}

/// Read one `[normal]` / `[insert]` / `[command]` table into `rows`.
fn read_mode_table(
    reader: &mut Reader<'_>,
    table_name: &str,
    mode: Mode,
    value: &Spanned<DeValue<'_>>,
    rows: &mut Vec<UserRow>,
) {
    let DeValue::Table(entries) = value.get_ref() else {
        reader.error(
            value.span().start,
            table_name,
            "expected a table of key bindings",
        );
        return;
    };
    for (key, entry) in entries.iter() {
        let notation = key.get_ref().to_string();
        let path = key_path(table_name, &notation);
        if let Err(e) = parse_keys(&notation) {
            reader.error(key.span().start, &path, cap_message(&e.to_string()));
            continue;
        }
        let DeValue::String(text) = entry.get_ref() else {
            reader.error(
                entry.span().start,
                &path,
                format!(
                    "expected an action name string, found {}",
                    entry.get_ref().type_str()
                ),
            );
            continue;
        };
        match resolve_value(text) {
            Ok(action) => rows.push(UserRow {
                mode,
                notation,
                action,
            }),
            Err(message) => reader.error(entry.span().start, &path, message),
        }
    }
}

/// A parser message with any echoed user text cut to a sane length.
fn cap_message(message: &str) -> String {
    message.chars().take(96).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diag::Severity;
    use crate::config::{Config, Sources, load_from_sources};
    use crate::input::key::parse_keys;
    use crate::input::keymap::default_bindings;
    use crate::input::trie::Lookup;
    use crate::test_util::{app_with_config, press, render_to_string};

    fn load(keymap: &str) -> Result<Config, crate::config::Report> {
        load_from_sources(&Sources {
            keymap: Some(keymap.to_string()),
            ..Sources::default()
        })
    }

    fn problems(src: &str) -> Vec<Problem> {
        let mut out = Vec::new();
        let _ = build(KeymapPreset::Vim, Some(src), &mut out);
        out
    }

    fn errors(src: &str) -> Vec<Problem> {
        problems(src)
            .into_iter()
            .filter(|p| p.severity == Severity::Error)
            .collect()
    }

    fn lookup_kind(keymap: &Keymap, mode: Mode, keys: &str) -> &'static str {
        match keymap.lookup(mode, &parse_keys(keys).unwrap()) {
            Lookup::None => "none",
            Lookup::Prefix => "prefix",
            Lookup::Exact(_) => "exact",
            Lookup::ExactAndPrefix(_) => "exact+prefix",
        }
    }

    #[test]
    fn tracer_rebind_semicolon() {
        let config = match load("[normal]\n\";\" = \"enter_command\"\n") {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        };
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, ";q<CR>");
        assert!(app.should_quit);
        // The preset row is kept: a fresh app still quits with :q.
        let mut fresh = app_with_config(Config::default(), 80, 24);
        press(&mut fresh, ":q<CR>");
        assert!(fresh.should_quit);
    }

    #[test]
    fn unknown_action_error() {
        let found = errors("[normal]\nj = \"mov_down\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].file, ConfigFile::Keymap);
        assert_eq!(found[0].pos.map(|(line, _)| line), Some(2));
        assert!(found[0].message.contains("mov_down"));
    }

    #[test]
    fn bad_notation_error() {
        let found = errors("[normal]\n\"<C-\" = \"move_down\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("unclosed '<'"), "{found:?}");
        assert_eq!(found[0].pos.map(|(line, _)| line), Some(2));
    }

    #[test]
    fn unknown_mode_table_warns() {
        assert!(load("[visual]\nj = \"move_down\"\n").is_ok());
        let Err(report) = load("[visual]\nj = \"move_down\"\n[normal]\nj = \"nope\"\n") else {
            panic!("expected Err");
        };
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].key.as_deref(), Some("visual"));
    }

    #[test]
    fn non_string_value_error() {
        let found = errors("[normal]\nj = 3\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("found integer"), "{found:?}");
        let found = errors("normal = \"x\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("table"), "{found:?}");
        assert_eq!(found[0].key.as_deref(), Some("normal"));
    }

    #[test]
    fn absent_and_empty_give_preset() {
        let mut out = Vec::new();
        for src in [None, Some(""), Some("# nothing\n")] {
            let keymap = build(KeymapPreset::Vim, src, &mut out);
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "gg"), "exact");
        }
        assert!(out.is_empty());
        let keymap = build(KeymapPreset::Vim, Some("[normal\n"), &mut out);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gg"), "exact");
        assert!(!out.is_empty());
    }

    fn with_preset(preset: &str, keymap: Option<&str>) -> Config {
        let sources = Sources {
            config: Some(format!("keymap_preset = \"{preset}\"\n")),
            keymap: keymap.map(str::to_string),
            ..Sources::default()
        };
        match load_from_sources(&sources) {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        }
    }

    #[test]
    fn preset_vim_equals_default() {
        let config = with_preset("vim", None);
        for (mode, notation, action) in default_bindings() {
            let chords = parse_keys(notation).unwrap();
            assert!(
                matches!(config.keymap.lookup(mode, &chords), Lookup::Exact(a) if *a == action),
                "{notation}"
            );
        }
        assert_eq!(lookup_kind(&config.keymap, Mode::Normal, "<Tab>"), "none");
    }

    #[test]
    fn preset_vim_extras_adds_tab() {
        let config = with_preset("vim-extras", None);
        let tab = parse_keys("<Tab>").unwrap();
        let shift_tab = parse_keys("<S-Tab>").unwrap();
        assert!(matches!(
            config.keymap.lookup(Mode::Normal, &tab),
            Lookup::Exact(Action::FocusNext)
        ));
        assert!(matches!(
            config.keymap.lookup(Mode::Normal, &shift_tab),
            Lookup::Exact(Action::FocusPrev)
        ));
        for (mode, notation, action) in default_bindings() {
            let chords = parse_keys(notation).unwrap();
            assert!(
                matches!(config.keymap.lookup(mode, &chords), Lookup::Exact(a) if *a == action),
                "{notation}"
            );
        }
        // Tab is not bound in the other modes.
        assert_eq!(lookup_kind(&config.keymap, Mode::Insert, "<Tab>"), "none");
    }

    #[test]
    fn vim_extras_tab_moves_focus_rendered() {
        let mut app = app_with_config(with_preset("vim-extras", None), 80, 24);
        let thick_corner_at_28 = |app: &crate::app::App| {
            render_to_string(app, 80, 24)
                .lines()
                .next()
                .and_then(|row| row.chars().nth(28))
                == Some('\u{250f}')
        };
        assert!(!thick_corner_at_28(&app));
        press(&mut app, "<Tab>");
        assert_eq!(app.focus, crate::app::Focus::Messages);
        assert!(thick_corner_at_28(&app));
        press(&mut app, "<S-Tab>");
        assert_eq!(app.focus, crate::app::Focus::Sidebar);
        assert!(!thick_corner_at_28(&app));
    }

    #[test]
    fn cmd_binding_runs_command() {
        let config = match load("[normal]\nQ = \"cmd:quit\"\n") {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        };
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "Q");
        assert!(app.should_quit);
        // `cmd:q` and surrounding spaces are fine too.
        assert!(load("[normal]\nQ = \"cmd: q \"\n").is_ok());
    }

    #[test]
    fn cmd_unknown_command_error() {
        for value in ["cmd:bogus", "cmd:", "cmd:   "] {
            let found = errors(&format!("[normal]\nQ = \"{value}\"\n"));
            assert_eq!(found.len(), 1, "{value}: {found:?}");
            assert_eq!(found[0].file, ConfigFile::Keymap);
            assert_eq!(found[0].pos.map(|(line, _)| line), Some(2));
        }
        let found = errors("[normal]\nQ = \"cmd:bogus\"\n");
        assert!(found[0].message.contains("unknown command 'bogus'"));
    }
}
