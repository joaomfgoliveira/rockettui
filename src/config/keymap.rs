//! `keymap.toml`: per-mode tables of `"<keys>" = "action_name"` rows layered on a preset (D-04).
//!
//! Key notation is Vim notation (Phase 1 D-08); action names are the curated vocabulary of
//! [`crate::input::actions`] (D-07). Every problem found is collected with its file, line:col and
//! key path; nothing here panics on user data.
//!
//! Rows are applied in two passes (every `none` removal, then every bind), never in TOML table
//! order: the toml crate iterates keys sorted, not in file order.

use super::diag::{ConfigFile, Problem, Reader, Severity, line_col, parse_table};
use super::settings::KeymapPreset;
use crate::action::Action;
use crate::app::Mode;
use crate::app::command::COMMANDS;
use crate::input::actions::{action_from_name, video_action_from_name};
use crate::input::key::{Chord, format_keys, parse_keys};
use crate::input::keymap::{Keymap, default_bindings, vim_extras_bindings};
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashMap;
use toml::Spanned;
use toml::de::DeValue;

/// The mode tables of the file, in the order their rows are read.
const MODES: [(&str, Mode); 3] = [
    ("normal", Mode::Normal),
    ("insert", Mode::Insert),
    ("command", Mode::Command),
];

/// The table of the player's keys, read apart from the modes: it is not a `Mode`.
const VIDEO_TABLE: &str = "video";

/// Longest piece of user-written text echoed in a problem (T-02-01).
const ECHO_CAP: usize = 32;

/// What a row does to its keys.
enum Binding {
    Bind(Action),
    /// The value `none`: remove the preset binding and leave nothing (D-05).
    Unbind,
}

/// One valid `"<keys>" = "<value>"` row of the user's file.
struct UserRow {
    mode: Mode,
    notation: String,
    chords: Vec<Chord>,
    binding: Binding,
    /// Byte offset of the key in the file: orders rows by position and locates their problems.
    offset: usize,
}

/// The table name of a mode in `keymap.toml`.
fn table_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Insert => "insert",
        Mode::Command => "command",
    }
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
fn key_path(mode: Mode, notation: &str) -> String {
    format!("{}.\"{}\"", table_name(mode), cap(notation))
}

/// The rows of the chosen preset (D-14): `vim` is the Phase 1 defaults, `vim-extras` adds Tab and
/// Shift-Tab pane focus cycling.
fn preset_rows(preset: KeymapPreset) -> Vec<(Mode, &'static str, Action)> {
    match preset {
        KeymapPreset::Vim => default_bindings(),
        KeymapPreset::VimExtras => vim_extras_bindings(),
    }
}

/// The keymap of the preset rows alone. Built-in data: a parse failure is a bug in the preset.
fn preset_keymap(rows: &[(Mode, &str, Action)]) -> Keymap {
    Keymap::from_bindings(rows).expect("built-in keymap parses")
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
    let mut keymap = preset_keymap(&preset_rows(preset));
    let Some(src) = src else {
        return keymap;
    };
    let Some(root) = parse_table(ConfigFile::Keymap, src, out) else {
        return keymap;
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
    let mut video_rows: Vec<VideoRow> = Vec::new();
    if let Some(value) = table.get(VIDEO_TABLE) {
        read_video_table(&mut reader, value, &mut video_rows);
    }
    // D-03: any other top-level key or table is a warning only.
    let mut known: Vec<&str> = MODES.iter().map(|(name, _)| *name).collect();
    known.push(VIDEO_TABLE);
    reader.warn_unknown(table, &known, "");

    // File order, not TOML's sorted iteration order, decides which of two duplicates is "later".
    rows.sort_by_key(|row| row.offset);
    let rows = reject_duplicates(&mut reader, rows);
    let rows = reject_text_eaters(&mut reader, rows);
    warn_dead_chords(&mut reader, &rows);

    // Pass 1: every removal. Pass 2: every bind. Never in table order (RESEARCH Pitfall 2).
    for row in &rows {
        if matches!(row.binding, Binding::Unbind) {
            // An unbound key is a silent no-op (A12).
            keymap.unbind(row.mode, &row.chords);
        }
    }
    for row in &rows {
        if let Binding::Bind(action) = &row.binding {
            keymap.bind(row.mode, &row.chords, action.clone());
        }
    }

    report_clashes(&mut reader, &keymap, &rows);
    check_lockout(&mut reader, &keymap);

    video_rows.sort_by_key(|row| row.offset);
    let video_rows = reject_video_duplicates(&mut reader, video_rows);
    for row in &video_rows {
        if matches!(row.binding, Binding::Unbind) {
            keymap.unbind_video(&row.chords);
        }
    }
    for row in &video_rows {
        if let Binding::Bind(action) = &row.binding {
            keymap.bind_video(&row.chords, action.clone());
        }
    }
    report_video_clashes(&mut reader, &keymap, &video_rows);
    if !keymap
        .video_bindings()
        .iter()
        .any(|(_, action)| **action == Action::VideoStop)
    {
        reader.out.push(Problem {
            file: ConfigFile::Keymap,
            pos: None,
            key: Some(format!("[{VIDEO_TABLE}]")),
            message: "no key stops the video; bind a key to \"video_stop\"".to_string(),
            severity: Severity::Error,
        });
    }
    keymap
}

/// One valid row of the `[video]` table.
struct VideoRow {
    notation: String,
    chords: Vec<Chord>,
    binding: Binding,
    offset: usize,
}

/// The key path of a `[video]` row in problem output: `video."<Space>"`.
fn video_key_path(notation: &str) -> String {
    format!("{VIDEO_TABLE}.\"{}\"", cap(notation))
}

/// Read the `[video]` table into `rows`. A value is `none` or one of the six player actions; any
/// other action name is an error, since the player takes no other action.
fn read_video_table(
    reader: &mut Reader<'_>,
    value: &Spanned<DeValue<'_>>,
    rows: &mut Vec<VideoRow>,
) {
    let DeValue::Table(entries) = value.get_ref() else {
        reader.error(
            value.span().start,
            VIDEO_TABLE,
            "expected a table of key bindings",
        );
        return;
    };
    for (key, entry) in entries.iter() {
        let notation = key.get_ref().to_string();
        let path = video_key_path(&notation);
        let chords = match parse_keys(&notation) {
            Ok(chords) => chords,
            Err(e) => {
                reader.error(key.span().start, &path, cap_message(&e.to_string()));
                continue;
            }
        };
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
        let binding = if text == "none" {
            Binding::Unbind
        } else if let Some(action) = video_action_from_name(text) {
            Binding::Bind(action)
        } else {
            reader.error(
                entry.span().start,
                &path,
                format!(
                    "`{}` is not a video action; the [video] table takes only the video_* actions",
                    cap(text)
                ),
            );
            continue;
        };
        rows.push(VideoRow {
            notation,
            chords,
            binding,
            offset: key.span().start,
        });
    }
}

/// As [`reject_duplicates`], for the `[video]` table.
fn reject_video_duplicates(reader: &mut Reader<'_>, rows: Vec<VideoRow>) -> Vec<VideoRow> {
    let mut first: HashMap<Vec<Chord>, (usize, String)> = HashMap::new();
    let mut kept = Vec::new();
    for row in rows {
        if let Some((earlier_offset, earlier)) = first.get(&row.chords) {
            let line = line_col(reader.src, *earlier_offset).0;
            let message = format!(
                "`{}` and `{}` (line {line}) bind the same keys; keep one",
                cap(&row.notation),
                cap(earlier)
            );
            reader.error(row.offset, &video_key_path(&row.notation), message);
        } else {
            first.insert(row.chords.clone(), (row.offset, row.notation.clone()));
            kept.push(row);
        }
    }
    kept
}

/// As [`report_clashes`], for the `[video]` table.
fn report_video_clashes(reader: &mut Reader<'_>, keymap: &Keymap, rows: &[VideoRow]) {
    let mut pairs: Vec<(String, String, Vec<Chord>, Vec<Chord>)> = keymap
        .video_clashes()
        .into_iter()
        .map(|(short, long)| (format_keys(&short), format_keys(&long), short, long))
        .collect();
    pairs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    for (short_text, long_text, short, long) in pairs {
        let message = format!(
            "`{}` is a prefix of `{}`; unbind one with \"none\"",
            cap(&short_text),
            cap(&long_text)
        );
        let involved = rows
            .iter()
            .filter(|row| {
                matches!(row.binding, Binding::Bind(_))
                    && (row.chords == short || row.chords == long)
            })
            .max_by_key(|row| row.offset);
        match involved {
            Some(row) => {
                reader.error(row.offset, &video_key_path(&row.notation), message);
            }
            None => reader.out.push(Problem {
                file: ConfigFile::Keymap,
                pos: None,
                key: Some(VIDEO_TABLE.to_string()),
                message,
                severity: Severity::Error,
            }),
        }
    }
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
        let path = key_path(mode, &notation);
        let chords = match parse_keys(&notation) {
            Ok(chords) => chords,
            Err(e) => {
                reader.error(key.span().start, &path, cap_message(&e.to_string()));
                continue;
            }
        };
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
        let binding = if text == "none" {
            Binding::Unbind
        } else if video_action_from_name(text).is_some() {
            reader.error(
                entry.span().start,
                &path,
                format!("`{}` belongs in the [video] table", cap(text)),
            );
            continue;
        } else {
            match resolve_value(text) {
                Ok(action) => Binding::Bind(action),
                Err(message) => {
                    reader.error(entry.span().start, &path, message);
                    continue;
                }
            }
        };
        rows.push(UserRow {
            mode,
            notation,
            chords,
            binding,
            offset: key.span().start,
        });
    }
}

/// A parser message with any echoed user text cut to a sane length.
fn cap_message(message: &str) -> String {
    message.chars().take(96).collect()
}

/// D-04 as amended: two spellings of the same keys in one mode table (`none` rows included) are
/// an error naming both; the later row in the file is dropped. Later-wins applies only to user
/// rows over preset rows, which never reaches here.
fn reject_duplicates(reader: &mut Reader<'_>, rows: Vec<UserRow>) -> Vec<UserRow> {
    let mut first: HashMap<(Mode, Vec<Chord>), (usize, String)> = HashMap::new();
    let mut kept = Vec::new();
    for row in rows {
        let id = (row.mode, row.chords.clone());
        if let Some((earlier_offset, earlier)) = first.get(&id) {
            let line = line_col(reader.src, *earlier_offset).0;
            let message = format!(
                "`{}` and `{}` (line {line}) bind the same keys; keep one",
                cap(&row.notation),
                cap(earlier)
            );
            reader.error(row.offset, &key_path(row.mode, &row.notation), message);
        } else {
            first.insert(id, (row.offset, row.notation.clone()));
            kept.push(row);
        }
    }
    kept
}

/// A4: in Insert and Command a sequence that starts with a plain character would make typed text
/// wait for the timeout or get dropped, so it is an error and the row is dropped.
fn reject_text_eaters(reader: &mut Reader<'_>, rows: Vec<UserRow>) -> Vec<UserRow> {
    let mut kept = Vec::new();
    for row in rows {
        let eats = matches!(row.mode, Mode::Insert | Mode::Command)
            && row.chords.len() >= 2
            && matches!(row.chords[0].code, KeyCode::Char(_))
            && !row.chords[0]
                .mods
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if eats {
            let message = format!(
                "a sequence starting with a plain character would eat typed text in {} mode",
                table_name(row.mode)
            );
            reader.error(row.offset, &key_path(row.mode, &row.notation), message);
        } else {
            kept.push(row);
        }
    }
    kept
}

/// Pitfall 6: chords that can never fire as written are warnings; the rows are still applied.
fn warn_dead_chords(reader: &mut Reader<'_>, rows: &[UserRow]) {
    for row in rows {
        let path = key_path(row.mode, &row.notation);
        if row.mode == Mode::Normal
            && row.chords.first().is_some_and(|chord| {
                chord.mods.is_empty() && matches!(chord.code, KeyCode::Char('1'..='9'))
            })
        {
            reader.warning(
                row.offset,
                &path,
                "can never fire: a leading digit starts a count",
            );
        }
        if row.chords.iter().any(|chord| {
            chord.mods == KeyModifiers::CONTROL && matches!(chord.code, KeyCode::Char('i' | 'm'))
        }) {
            reader.warning(
                row.offset,
                &path,
                "arrives as Tab or Enter on most terminals",
            );
        }
    }
}

/// D-06: a binding that is a prefix of another in the same mode is an error naming both in Vim
/// notation, positioned at the user row involved (the later one when both are user rows).
fn report_clashes(reader: &mut Reader<'_>, keymap: &Keymap, rows: &[UserRow]) {
    for (_, mode) in MODES {
        let mut pairs: Vec<(String, String, Vec<Chord>, Vec<Chord>)> = keymap
            .clashes(mode)
            .into_iter()
            .map(|(short, long)| (format_keys(&short), format_keys(&long), short, long))
            .collect();
        // The trie iterates in hash order; sort so the report is stable.
        pairs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        for (short_text, long_text, short, long) in pairs {
            let message = format!(
                "`{}` is a prefix of `{}`; unbind one with \"none\"",
                cap(&short_text),
                cap(&long_text)
            );
            let involved = rows
                .iter()
                .filter(|row| {
                    row.mode == mode
                        && matches!(row.binding, Binding::Bind(_))
                        && (row.chords == short || row.chords == long)
                })
                .max_by_key(|row| row.offset);
            match involved {
                Some(row) => {
                    reader.error(row.offset, &key_path(mode, &row.notation), message);
                }
                None => reader.out.push(Problem {
                    file: ConfigFile::Keymap,
                    pos: None,
                    key: Some(table_name(mode).to_string()),
                    message,
                    severity: Severity::Error,
                }),
            }
        }
    }
}

/// A3: raw mode swallows SIGINT, so a keymap that leaves no way to leave Insert mode, to leave or
/// run the command line, or to quit from Normal mode would trap the user in the terminal.
fn check_lockout(reader: &mut Reader<'_>, keymap: &Keymap) {
    let has = |mode: Mode, wanted: &dyn Fn(&Action) -> bool| {
        keymap
            .bindings(mode)
            .iter()
            .any(|(_, action)| wanted(action))
    };
    let mut fail = |mode: Mode, message: &str| {
        reader.out.push(Problem {
            file: ConfigFile::Keymap,
            pos: None,
            key: Some(format!("[{}]", table_name(mode))),
            message: message.to_string(),
            severity: Severity::Error,
        });
    };
    if !has(Mode::Insert, &|a| *a == Action::Cancel) {
        fail(
            Mode::Insert,
            "no key leaves insert mode; bind a key to \"cancel\"",
        );
    }
    if !has(Mode::Command, &|a| *a == Action::Cancel) {
        fail(
            Mode::Command,
            "no key leaves command mode; bind a key to \"cancel\"",
        );
    }
    if !has(Mode::Command, &|a| *a == Action::ExecuteCommand) {
        fail(
            Mode::Command,
            "no key runs the command line; bind a key to \"execute_command\"",
        );
    }
    if !has(Mode::Normal, &|a| {
        matches!(a, Action::EnterCommand | Action::Quit) || runs_quit(a)
    }) {
        fail(
            Mode::Normal,
            "no key quits or opens the command line; bind a key to \"enter_command\", \"quit\" or \"cmd:quit\"",
        );
    }
}

/// True for a `cmd:` binding whose command is the one that lists the name `quit`.
fn runs_quit(action: &Action) -> bool {
    let Action::RunCommand(line) = action else {
        return false;
    };
    let Some(name) = line.split_whitespace().next() else {
        return false;
    };
    COMMANDS
        .iter()
        .find(|def| def.names.contains(&name))
        .is_some_and(|def| def.names.contains(&"quit"))
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

    fn built(src: &str) -> Keymap {
        build(KeymapPreset::Vim, Some(src), &mut Vec::new())
    }

    fn line_of(problem: &Problem) -> Option<usize> {
        problem.pos.map(|(line, _)| line)
    }

    #[test]
    fn show_reactions_default_rebind_and_unbind() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // Bound to `gr` in every preset, and `gg` still resolves with `g` as their shared prefix.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gr", Action::ShowReactions), "{preset:?}");
            assert!(is(&keymap, "gg", Action::GotoTop), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
        }
        // Rebind: the new key works and the preset key stays (a binding adds, `none` removes).
        let src = "[normal]\n\"R\" = \"show_reactions\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "R", Action::ShowReactions));
        assert!(is(&keymap, "gr", Action::ShowReactions));
        // Unbind with `none`.
        let src = "[normal]\n\"gr\" = \"none\"\n";
        assert!(errors(src).is_empty());
        let keymap = built(src);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gr"), "none");
        assert!(is(&keymap, "gg", Action::GotoTop));
        // The name is part of the vocabulary and round-trips.
        assert_eq!(
            crate::input::actions::name_of(&Action::ShowReactions),
            Some("show_reactions")
        );
    }

    #[test]
    fn open_channel_default_rebind_unbind_and_clash() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // `gc` in every preset, and the other `g` bindings are unchanged.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gc", Action::OpenChannel), "{preset:?}");
            assert!(is(&keymap, "gg", Action::GotoTop), "{preset:?}");
            assert!(is(&keymap, "gr", Action::ShowReactions), "{preset:?}");
            assert!(is(&keymap, "gt", Action::OpenThread), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
        }
        // Rebind: the new key works and `gc` stays (a binding adds, `none` removes).
        let src = "[normal]\n\"C\" = \"open_channel\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "C", Action::OpenChannel));
        assert!(is(&keymap, "gc", Action::OpenChannel));
        // Unbind with `none`; the rest of the `g` keys are untouched.
        let src = "[normal]\n\"gc\" = \"none\"\n";
        assert!(errors(src).is_empty());
        let keymap = built(src);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gc"), "none");
        assert!(is(&keymap, "gg", Action::GotoTop));
        // Binding a key that is a prefix of `gc` is the usual clash error, naming both.
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert!(
            found.iter().any(|p| p.message.contains("`gc`")),
            "{found:?}"
        );
        // Another action on `gc` is a duplicate of the default: the user's row replaces it.
        let keymap = built("[normal]\n\"gc\" = \"quit\"\n");
        assert!(is(&keymap, "gc", Action::Quit));
        // The name is part of the vocabulary and round-trips.
        assert_eq!(
            crate::input::actions::name_of(&Action::OpenChannel),
            Some("open_channel")
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"open_channel"));
    }

    #[test]
    fn show_image_default_rebind_unbind_and_clash() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gp", Action::ShowImage), "{preset:?}");
            assert!(is(&keymap, "gg", Action::GotoTop), "{preset:?}");
            assert!(is(&keymap, "gc", Action::OpenChannel), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
        }
        let src = "[normal]\n\"P\" = \"show_image\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "P", Action::ShowImage));
        assert!(is(&keymap, "gp", Action::ShowImage));
        let keymap = built("[normal]\n\"gp\" = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gp"), "none");
        assert!(is(&keymap, "gg", Action::GotoTop));
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert!(
            found.iter().any(|p| p.message.contains("`gp`")),
            "{found:?}"
        );
        assert_eq!(
            crate::input::actions::name_of(&Action::ShowImage),
            Some("show_image")
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"show_image"));
    }

    #[test]
    fn open_attachment_default_rebind_unbind_and_clash() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "go", Action::OpenAttachment), "{preset:?}");
            assert!(is(&keymap, "gp", Action::ShowImage), "{preset:?}");
            assert!(is(&keymap, "gg", Action::GotoTop), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
        }
        let src = "[normal]\n\"O\" = \"open_attachment\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "O", Action::OpenAttachment));
        assert!(is(&keymap, "go", Action::OpenAttachment));
        let keymap = built("[normal]\n\"go\" = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "go"), "none");
        assert!(is(&keymap, "gp", Action::ShowImage));
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert!(
            found.iter().any(|p| p.message.contains("`go`")),
            "{found:?}"
        );
        assert_eq!(
            crate::input::actions::name_of(&Action::OpenAttachment),
            Some("open_attachment")
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"open_attachment"));
    }

    #[test]
    fn play_video_default_rebind_unbind_and_clash() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gv", Action::PlayVideo), "{preset:?}");
            assert!(is(&keymap, "gV", Action::PlayVideoFullscreen), "{preset:?}");
            for (keys, want) in [
                ("gg", Action::GotoTop),
                ("gp", Action::ShowImage),
                ("go", Action::OpenAttachment),
            ] {
                assert!(is(&keymap, keys, want), "{preset:?} {keys}");
            }
        }
        let src = "[normal]\n\"V\" = \"play_video\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "V", Action::PlayVideo));
        assert!(is(&keymap, "gv", Action::PlayVideo));
        let keymap = built("[normal]\n\"gv\" = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gv"), "none");
        assert!(is(&keymap, "go", Action::OpenAttachment));
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert!(
            found.iter().any(|p| p.message.contains("`gv`")),
            "{found:?}"
        );
        assert_eq!(
            crate::input::actions::name_of(&Action::PlayVideo),
            Some("play_video")
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"play_video"));
    }

    #[test]
    fn play_video_fullscreen_default_rebind_unbind() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            for (keys, want) in [
                ("gV", Action::PlayVideoFullscreen),
                ("gv", Action::PlayVideo),
                ("gp", Action::ShowImage),
                ("go", Action::OpenAttachment),
            ] {
                assert!(is(&keymap, keys, want), "{preset:?} {keys}");
            }
        }
        let src = "[normal]\n\"F\" = \"play_video_fullscreen\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "F", Action::PlayVideoFullscreen));
        assert!(is(&keymap, "gV", Action::PlayVideoFullscreen));
        let keymap = built("[normal]\n\"gV\" = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gV"), "none");
        assert!(is(&keymap, "gv", Action::PlayVideo));
        // A key already bound clashes.
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert!(
            found.iter().any(|p| p.message.contains("`gV`")),
            "{found:?}"
        );
        assert_eq!(
            crate::input::actions::name_of(&Action::PlayVideoFullscreen),
            Some("play_video_fullscreen")
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"play_video_fullscreen"));
    }

    fn video_is(keymap: &Keymap, keys: &str, want: Action) -> bool {
        matches!(
            keymap.lookup_video(&parse_keys(keys).unwrap()),
            Lookup::Exact(a) if *a == want
        )
    }

    #[test]
    fn video_table_defaults_in_every_preset() {
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            for (keys, want) in [
                ("<Space>", Action::VideoPause),
                ("h", Action::VideoSeekBack),
                ("l", Action::VideoSeekForward),
                ("H", Action::VideoSeekBackFar),
                ("L", Action::VideoSeekForwardFar),
                ("<Esc>", Action::VideoStop),
                ("q", Action::VideoStop),
                ("<C-c>", Action::VideoStop),
            ] {
                assert!(video_is(&keymap, keys, want), "{preset:?} {keys}");
            }
            assert_eq!(keymap.video_bindings().len(), 8);
        }
        assert!(errors("").is_empty());
    }

    #[test]
    fn video_table_rebind_and_unbind() {
        let src = "[video]\n\",\" = \"video_seek_back\"\n\".\" = \"video_seek_forward\"\n\"h\" = \"none\"\n\"s\" = \"video_pause\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(video_is(&keymap, ",", Action::VideoSeekBack));
        assert!(video_is(&keymap, ".", Action::VideoSeekForward));
        assert!(video_is(&keymap, "s", Action::VideoPause));
        assert!(
            !video_is(&keymap, "h", Action::VideoSeekBack),
            "h was unbound"
        );
        assert!(
            video_is(&keymap, "l", Action::VideoSeekForward),
            "the others stay"
        );
        // A `none` for a key that is not bound is a silent no-op.
        assert!(errors("[video]\n\"x\" = \"none\"\n").is_empty());
    }

    #[test]
    fn video_table_leaves_normal_mode_unchanged() {
        let keymap = built("[video]\n\"j\" = \"video_pause\"\n\"<Esc>\" = \"none\"\n");
        // `j` and Esc are still the Normal mode's own keys.
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "j"), "exact");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "<Esc>"), "exact");
        assert!(!keymap.bindings(Mode::Normal).iter().any(|(_, a)| {
            crate::input::actions::name_of(a).is_some_and(|n| n.starts_with("video_"))
        }));
        let plain = built("");
        assert_eq!(
            plain.bindings(Mode::Normal).len(),
            keymap.bindings(Mode::Normal).len()
        );
    }

    #[test]
    fn video_actions_belong_only_in_the_video_table() {
        for table in ["normal", "insert", "command"] {
            let src = format!("[{table}]\n\"x\" = \"video_pause\"\n");
            let found = errors(&src);
            assert_eq!(found.len(), 1, "{table}: {found:?}");
            assert!(found[0].message.contains("[video]"), "{found:?}");
            assert_eq!(line_of(&found[0]), Some(2));
        }
        // And the player takes no other action.
        let found = errors("[video]\n\"x\" = \"move_down\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("not a video action"), "{found:?}");
        let found = errors("[video]\n\"x\" = \"cmd:quit\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        let found = errors("[video]\n\"x\" = 3\n");
        assert_eq!(found.len(), 1, "{found:?}");
        let found = errors("video = 3\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key.as_deref(), Some("video"));
        // A bad row does not stop the good ones.
        let keymap = built("[video]\n\"x\" = \"move_down\"\n\"s\" = \"video_pause\"\n");
        assert!(video_is(&keymap, "s", Action::VideoPause));
    }

    #[test]
    fn video_table_needs_a_way_to_stop() {
        let none = "[video]\n\"<Esc>\" = \"none\"\nq = \"none\"\n\"<C-c>\" = \"none\"\n";
        let found = errors(none);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("video_stop"), "{found:?}");
        // Another key to stop satisfies it.
        let again = format!("{none}\"x\" = \"video_stop\"\n");
        assert!(errors(&again).is_empty(), "{:?}", problems(&again));
    }

    #[test]
    fn video_table_reports_duplicates_and_prefix_clashes() {
        let found = errors("[video]\n\"gg\" = \"video_pause\"\n\"g\" = \"video_seek_back\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("`g`") && found[0].message.contains("`gg`"));
        assert_eq!(line_of(&found[0]), Some(3), "the later row");
        // Two spellings of one key.
        let found = errors("[video]\n\"<Space>\" = \"video_pause\"\n\" \" = \"video_seek_back\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("bind the same keys"), "{found:?}");
    }

    #[test]
    fn list_threads_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // `gT` in every preset, beside `gt`, which it must not disturb.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gT", Action::ListThreads), "{preset:?}");
            assert!(is(&keymap, "gt", Action::OpenThread), "{preset:?}");
        }
        // Rebind: the new key works and `gT` stays.
        let src = "[normal]\n\"<C-t>\" = \"list_threads\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "<C-t>", Action::ListThreads));
        assert!(is(&keymap, "gT", Action::ListThreads));
        // Unbind with `none`.
        let src = "[normal]\n\"gT\" = \"none\"\n";
        assert!(errors(src).is_empty());
        let keymap = built(src);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gT"), "none");
        assert!(is(&keymap, "gt", Action::OpenThread));
        // The old guess at the name is a config error that names it.
        for wrong in ["thread_list", "list_thread", "threads_list"] {
            let found = errors(&format!("[normal]\n\"T\" = \"{wrong}\"\n"));
            assert_eq!(found.len(), 1, "{wrong}: {found:?}");
            assert!(found[0].message.contains(wrong), "{found:?}");
        }
        assert_eq!(
            crate::input::actions::name_of(&Action::ListThreads),
            Some("list_threads")
        );
    }

    #[test]
    fn open_thread_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // `gt` in every preset, next to the other `g` bindings it must not disturb.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gt", Action::OpenThread), "{preset:?}");
            assert!(is(&keymap, "gg", Action::GotoTop), "{preset:?}");
            assert!(is(&keymap, "gr", Action::ShowReactions), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
            // `close_thread` has no default key; `Esc` stays `cancel`, which closes the pane.
            assert!(is(&keymap, "<Esc>", Action::Cancel), "{preset:?}");
        }
        // Rebind: the new key works and `gt` stays; `close_thread` can be bound.
        let src = "[normal]\n\"T\" = \"open_thread\"\n\"Q\" = \"close_thread\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "T", Action::OpenThread));
        assert!(is(&keymap, "gt", Action::OpenThread));
        assert!(is(&keymap, "Q", Action::CloseThread));
        // Unbind with `none`.
        let src = "[normal]\n\"gt\" = \"none\"\n";
        assert!(errors(src).is_empty());
        let keymap = built(src);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gt"), "none");
        assert!(is(&keymap, "gg", Action::GotoTop));
        // A misspelled name is a config error that names it.
        for wrong in ["open_threads", "thread_open", "close_thread_pane"] {
            let found = errors(&format!("[normal]\n\"T\" = \"{wrong}\"\n"));
            assert_eq!(found.len(), 1, "{wrong}: {found:?}");
            assert!(found[0].message.contains(wrong), "{found:?}");
        }
        // The names round-trip.
        assert_eq!(
            crate::input::actions::name_of(&Action::OpenThread),
            Some("open_thread")
        );
        assert_eq!(
            crate::input::actions::name_of(&Action::CloseThread),
            Some("close_thread")
        );
    }

    #[test]
    fn thread_reply_actions_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, mode: Mode, keys: &str, want: Action| {
            matches!(
                keymap.lookup(mode, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(
                is(&keymap, Mode::Normal, "gR", Action::ReplyInThread),
                "{preset:?}"
            );
            assert!(
                is(&keymap, Mode::Insert, "<C-b>", Action::ToggleReplyToChannel),
                "{preset:?}"
            );
            assert!(is(&keymap, Mode::Normal, "gr", Action::ShowReactions));
        }
        let src = "[normal]\n\"<C-r>\" = \"reply_in_thread\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, Mode::Normal, "<C-r>", Action::ReplyInThread));
        assert!(is(&keymap, Mode::Normal, "gR", Action::ReplyInThread));
        let src = "[insert]\n\"<C-b>\" = \"none\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        assert_eq!(lookup_kind(&built(src), Mode::Insert, "<C-b>"), "none");
        for wrong in ["reply_thread", "toggle_reply"] {
            let found = errors(&format!("[normal]\n\"<C-r>\" = \"{wrong}\"\n"));
            assert_eq!(found.len(), 1, "{wrong}: {found:?}");
            assert!(found[0].message.contains(wrong), "{found:?}");
        }
    }

    #[test]
    fn toggle_follow_thread_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, keys: &str| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(Action::ToggleFollowThread)
            )
        };
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "gf"), "{preset:?}");
            assert_eq!(lookup_kind(&keymap, Mode::Normal, "gt"), "exact");
        }
        let src = "[normal]\n\"F\" = \"toggle_follow_thread\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "F") && is(&keymap, "gf"));
        let src = "[normal]\n\"gf\" = \"none\"\n";
        assert!(errors(src).is_empty());
        assert_eq!(lookup_kind(&built(src), Mode::Normal, "gf"), "none");
        let found = errors("[normal]\n\"F\" = \"follow_thread\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("follow_thread"), "{found:?}");
    }

    #[test]
    fn react_message_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // `+` in every preset, next to the other bindings it must not disturb.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "+", Action::ReactMessage), "{preset:?}");
            assert!(is(&keymap, "gr", Action::ShowReactions), "{preset:?}");
            assert!(is(&keymap, "?", Action::ShowContext), "{preset:?}");
        }
        // Rebind: the new key works and `+` stays.
        let src = "[normal]\n\"R\" = \"react_message\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "R", Action::ReactMessage));
        assert!(is(&keymap, "+", Action::ReactMessage));
        // Unbind with `none`.
        let src = "[normal]\n\"+\" = \"none\"\n";
        assert!(errors(src).is_empty());
        assert_eq!(lookup_kind(&built(src), Mode::Normal, "+"), "none");
        // A misspelled name is a config error that names it.
        let found = errors("[normal]\n\"R\" = \"react_msg\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("react_msg"), "{found:?}");
        // The name round-trips.
        assert_eq!(
            crate::input::actions::name_of(&Action::ReactMessage),
            Some("react_message")
        );
    }

    #[test]
    fn switch_room_default_rebind_unbind_and_unknown_spelling() {
        let is = |keymap: &Keymap, keys: &str, want: Action| {
            matches!(
                keymap.lookup(Mode::Normal, &parse_keys(keys).unwrap()),
                Lookup::Exact(a) if *a == want
            )
        };
        // `<C-k>` in every preset, next to the bindings it must not disturb.
        for preset in [KeymapPreset::Vim, KeymapPreset::VimExtras] {
            let keymap = build(preset, None, &mut Vec::new());
            assert!(is(&keymap, "<C-k>", Action::SwitchRoom), "{preset:?}");
            assert!(is(&keymap, "+", Action::ReactMessage), "{preset:?}");
            assert!(is(&keymap, "<C-d>", Action::HalfPageDown), "{preset:?}");
        }
        // Rebind: the new key works and `<C-k>` stays.
        let src = "[normal]\n\"<C-p>\" = \"switch_room\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        assert!(is(&keymap, "<C-p>", Action::SwitchRoom));
        assert!(is(&keymap, "<C-k>", Action::SwitchRoom));
        // Unbind with `none`.
        let src = "[normal]\n\"<C-k>\" = \"none\"\n";
        assert!(errors(src).is_empty());
        assert_eq!(lookup_kind(&built(src), Mode::Normal, "<C-k>"), "none");
        // A misspelled name is a config error that names it.
        let found = errors("[normal]\n\"<C-p>\" = \"switch_rooms\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("switch_rooms"), "{found:?}");
        // The name round-trips.
        assert_eq!(
            crate::input::actions::name_of(&Action::SwitchRoom),
            Some("switch_room")
        );
    }

    #[test]
    fn none_unbinds() {
        let keymap = built("[normal]\n\"<C-d>\" = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "<C-d>"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Insert, "<C-d>"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Command, "<C-d>"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "<C-u>"), "exact");
        assert!(errors("[normal]\n\"<C-d>\" = \"none\"\n").is_empty());

        // Unbinding gg leaves gr and gt: g is still a prefix.
        let keymap = built("[normal]\ngg = \"none\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gg"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "prefix");
        // Unbinding all of them leaves nothing: g is no longer a prefix.
        let keymap = built(
            "[normal]\ngg = \"none\"\ngr = \"none\"\ngt = \"none\"\ngR = \"none\"\ngf = \"none\"\ngT = \"none\"\ngc = \"none\"\ngp = \"none\"\ngo = \"none\"\ngv = \"none\"\ngV = \"none\"\n",
        );
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gg"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gr"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gt"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gR"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gf"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gT"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gc"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gp"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "go"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gv"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gV"), "none");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "g"), "none");
    }

    #[test]
    fn none_gg_then_bind_g() {
        // TOML iterates `g` before `gg`; the result must not depend on it.
        let src = "[normal]\ngg = \"none\"\ngr = \"none\"\ngt = \"none\"\ngR = \"none\"\ngf = \"none\"\ngT = \"none\"\ngc = \"none\"\ngp = \"none\"\ngo = \"none\"\ngv = \"none\"\ngV = \"none\"\ng = \"goto_top\"\n";
        assert!(errors(src).is_empty(), "{:?}", problems(src));
        let keymap = built(src);
        let g = parse_keys("g").unwrap();
        assert!(matches!(
            keymap.lookup(Mode::Normal, &g),
            Lookup::Exact(Action::GotoTop)
        ));
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "gg"), "none");
    }

    #[test]
    fn prefix_clash_names_both() {
        // `g` is the prefix of eleven default bindings, `gg`, `gr`, `gc`, `gp`, `go`, `gv`, `gV`, `gt`, `gR`, `gf` and `gT`: one error for each.
        let found = errors("[normal]\ng = \"goto_top\"\n");
        assert_eq!(found.len(), 11, "{found:?}");
        assert_eq!(line_of(&found[0]), Some(2));
        assert!(found.iter().all(|p| p.message.contains("`g`")), "{found:?}");
        assert!(
            found.iter().any(|p| p.message.contains("`gg`")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|p| p.message.contains("`gr`")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|p| p.message.contains("`gt`")),
            "{found:?}"
        );
        // Unbinding one of them is not enough; unbinding all resolves it.
        let half = "[normal]\ng = \"goto_top\"\ngg = \"none\"\n";
        assert_eq!(errors(half).len(), 10);
        let resolved = "[normal]\ng = \"goto_top\"\ngg = \"none\"\ngr = \"none\"\ngt = \"none\"\ngR = \"none\"\ngf = \"none\"\ngT = \"none\"\ngc = \"none\"\ngp = \"none\"\ngo = \"none\"\ngv = \"none\"\ngV = \"none\"\n";
        assert!(errors(resolved).is_empty());
    }

    #[test]
    fn clash_between_user_rows_is_at_the_later_line() {
        let src = "[normal]\nxx = \"move_up\"\nx = \"move_down\"\n";
        let found = errors(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(line_of(&found[0]), Some(3), "later row in the file");
        assert!(found[0].message.contains("`x`") && found[0].message.contains("`xx`"));
        // Written the other way round, the later row is `xx`.
        let src = "[normal]\nx = \"move_down\"\nxx = \"move_up\"\n";
        let found = errors(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(line_of(&found[0]), Some(3));
    }

    #[test]
    fn duplicate_spellings() {
        for src in [
            "[normal]\n\"<C-d>\" = \"half_page_down\"\n\"<C-D>\" = \"half_page_up\"\n",
            "[normal]\n\"<C-D>\" = \"half_page_up\"\n\"<C-d>\" = \"half_page_down\"\n",
        ] {
            let found = errors(src);
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(line_of(&found[0]), Some(3));
            let message = &found[0].message;
            assert!(
                message.contains("`<C-d>`") && message.contains("`<C-D>`"),
                "{message}"
            );
            assert!(message.contains("line 2"), "{message}");
        }
        let found = errors("[normal]\n\"<CR>\" = \"none\"\n\"<Enter>\" = \"select\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(line_of(&found[0]), Some(3));
        // The same notation in different mode tables is not a duplicate.
        assert!(
            errors("[normal]\n\"<C-q>\" = \"quit\"\n[insert]\n\"<C-q>\" = \"cancel\"\n").is_empty()
        );
    }

    #[test]
    fn lockout_guard() {
        let insert = "[insert]\n\"<Esc>\" = \"none\"\n\"<C-c>\" = \"none\"\n";
        let found = errors(insert);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key.as_deref(), Some("[insert]"));
        assert!(found[0].message.contains("cancel"));
        // One of the two keys left is enough.
        assert!(errors("[insert]\n\"<Esc>\" = \"none\"\n").is_empty());

        let found = errors("[command]\n\"<CR>\" = \"none\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key.as_deref(), Some("[command]"));
        assert!(found[0].message.contains("execute_command"));
        let found = errors("[command]\n\"<Esc>\" = \"none\"\n\"<C-c>\" = \"none\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("cancel"));

        let found = errors("[normal]\n\":\" = \"none\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key.as_deref(), Some("[normal]"));
        for rescue in ["cmd:quit", "cmd:q", "quit", "enter_command"] {
            let src = format!("[normal]\n\":\" = \"none\"\nQ = \"{rescue}\"\n");
            assert!(errors(&src).is_empty(), "{rescue}: {:?}", problems(&src));
        }
    }

    #[test]
    fn insert_sequence_rejected() {
        let found = errors("[insert]\njk = \"cancel\"\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("eat typed text"));
        assert_eq!(line_of(&found[0]), Some(2));
        assert_eq!(errors("[command]\nab = \"cancel\"\n").len(), 1);
        assert!(errors("[insert]\n\"<C-x>k\" = \"cancel\"\n").is_empty());
        // The rejected row is dropped, so typed text still works.
        let keymap = built("[insert]\njk = \"cancel\"\n");
        assert_eq!(lookup_kind(&keymap, Mode::Insert, "j"), "none");
        // A sequence is fine in Normal mode.
        assert!(errors("[normal]\nxy = \"cancel\"\n").is_empty());
    }

    #[test]
    fn dead_chords_warn() {
        let src = "[normal]\n\"1\" = \"move_down\"\n\"<C-i>\" = \"select\"\n";
        assert!(load(src).is_ok());
        let warned: Vec<Problem> = problems(src)
            .into_iter()
            .filter(|p| p.severity == Severity::Warning)
            .collect();
        assert_eq!(warned.len(), 2, "{warned:?}");
        // Next to an error they show up as warnings in the report.
        let Err(report) = load(&format!("{src}j = \"nope\"\n")) else {
            panic!("expected Err");
        };
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.warnings.len(), 2);
        // The rows are still applied.
        let keymap = built(src);
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "1"), "exact");
        assert_eq!(lookup_kind(&keymap, Mode::Normal, "<C-i>"), "exact");
        // A digit is only dead as the first key of a Normal row, and `0` is not a count digit.
        assert!(problems("[normal]\n\"0\" = \"goto_top\"\nx1 = \"cancel\"\n").is_empty());
        assert!(problems("[insert]\n\"1\" = \"cancel\"\n").is_empty());
    }

    #[test]
    fn unbind_unbound_is_noop() {
        let src = "[normal]\n\"<C-x>\" = \"none\"\n";
        assert!(problems(src).is_empty());
        assert!(load(src).is_ok());
    }

    #[test]
    fn long_user_text_is_capped() {
        let long = "x".repeat(200);
        let found = errors(&format!("[normal]\nj = \"{long}\"\n"));
        assert_eq!(found.len(), 1);
        assert!(!found[0].message.contains(&long));
        let found = errors(&format!("[normal]\n\"<{long}>\" = \"quit\"\n"));
        assert_eq!(found.len(), 1);
        assert!(found[0].key.as_deref().unwrap().len() < 60);
        assert!(!found[0].message.contains(&long));
    }
}
