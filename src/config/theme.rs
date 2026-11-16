//! `theme.toml`: per-element styles (D-08 colors, D-09 `[pane.element]` tables).
//!
//! Defaults are Rust values that reproduce the Phase 1 look exactly (RESEARCH Pattern 2); the file
//! only overrides what it names. An element table takes optional `fg`, `bg` and `modifiers`; an
//! unset field inherits the built-in default.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use ratatui::style::{Color, Modifier, Style};
use std::str::FromStr;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

/// Every styled element a theme.toml may set, as `pane.element`. Grows with each task of plan
/// 02-04 and with plan 02-05; the names are the user-facing vocabulary of theme.toml.
pub const ELEMENT_PATHS: &[&str] = &[
    "sidebar.header",
    "sidebar.folder",
    "sidebar.room",
    "sidebar.unread",
    "sidebar.cursor",
    "sidebar.cursor_unfocused",
    "status.mode",
    "status.room",
    "status.pending",
    "status.info",
    "status.error",
    "cmdline.text",
    "input.text",
    "input.hint",
];

/// Modifier names accepted in `modifiers = [..]` (D-09).
pub const MODIFIER_NAMES: &[(&str, Modifier)] = &[
    ("bold", Modifier::BOLD),
    ("dim", Modifier::DIM),
    ("italic", Modifier::ITALIC),
    ("underlined", Modifier::UNDERLINED),
    ("slow_blink", Modifier::SLOW_BLINK),
    ("rapid_blink", Modifier::RAPID_BLINK),
    ("reversed", Modifier::REVERSED),
    ("hidden", Modifier::HIDDEN),
    ("crossed_out", Modifier::CROSSED_OUT),
];

/// The keys an element table accepts.
const STYLE_KEYS: &[&str] = &["fg", "bg", "modifiers"];

/// Longest user-written name echoed back in a problem (T-02-01).
const MAX_ECHO: usize = 32;

/// The resolved look of the UI. `Default` is the Phase 1 look.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub sidebar_header: Style,
    pub sidebar_folder: Style,
    pub sidebar_room: Style,
    pub sidebar_unread: Style,
    pub sidebar_cursor: Style,
    pub sidebar_cursor_unfocused: Style,
    pub status_mode: Style,
    pub status_room: Style,
    pub status_pending: Style,
    pub status_info: Style,
    pub status_error: Style,
    pub cmdline_text: Style,
    pub input_text: Style,
    pub input_hint: Style,
}

impl Default for Theme {
    fn default() -> Self {
        let plain = Style::default();
        Theme {
            sidebar_header: plain.add_modifier(Modifier::BOLD | Modifier::DIM),
            sidebar_folder: plain,
            sidebar_room: plain,
            sidebar_unread: plain.add_modifier(Modifier::BOLD),
            sidebar_cursor: plain.add_modifier(Modifier::REVERSED),
            sidebar_cursor_unfocused: plain.add_modifier(Modifier::BOLD),
            status_mode: plain.add_modifier(Modifier::REVERSED | Modifier::BOLD),
            status_room: plain,
            status_pending: plain,
            status_info: plain,
            status_error: plain.fg(Color::Red),
            cmdline_text: plain,
            input_text: plain,
            input_hint: plain.add_modifier(Modifier::DIM),
        }
    }
}

impl Theme {
    /// The style field behind `pane.element`, or `None` for a path that is not an element.
    fn style_slot_mut(&mut self, pane: &str, element: &str) -> Option<&mut Style> {
        match (pane, element) {
            ("sidebar", "header") => Some(&mut self.sidebar_header),
            ("sidebar", "folder") => Some(&mut self.sidebar_folder),
            ("sidebar", "room") => Some(&mut self.sidebar_room),
            ("sidebar", "unread") => Some(&mut self.sidebar_unread),
            ("sidebar", "cursor") => Some(&mut self.sidebar_cursor),
            ("sidebar", "cursor_unfocused") => Some(&mut self.sidebar_cursor_unfocused),
            ("status", "mode") => Some(&mut self.status_mode),
            ("status", "room") => Some(&mut self.status_room),
            ("status", "pending") => Some(&mut self.status_pending),
            ("status", "info") => Some(&mut self.status_info),
            ("status", "error") => Some(&mut self.status_error),
            ("cmdline", "text") => Some(&mut self.cmdline_text),
            ("input", "text") => Some(&mut self.input_text),
            ("input", "hint") => Some(&mut self.input_hint),
            _ => None,
        }
    }
}

/// The optional fields of one element table.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StyleSpec {
    fg: Option<Color>,
    bg: Option<Color>,
    modifiers: Option<Modifier>,
}

impl StyleSpec {
    /// `default` with every set field replaced (RESEARCH Pattern 3). A set `modifiers` replaces the
    /// default modifiers (an empty list clears them); an absent one keeps them. Built from the
    /// Style fields, never with `Style::patch`, which can only add modifiers.
    fn resolve(self, default: Style) -> Style {
        let mut style = default;
        if let Some(fg) = self.fg {
            style.fg = Some(fg);
        }
        if let Some(bg) = self.bg {
            style.bg = Some(bg);
        }
        if let Some(modifiers) = self.modifiers {
            style.add_modifier = modifiers;
            style.sub_modifier = Modifier::empty();
        }
        style
    }
}

fn cap(name: &str) -> String {
    name.chars().take(MAX_ECHO).collect()
}

/// A color: a name (`cyan`, `light-blue`), a 256-color index as an integer or a string, or
/// `#rrggbb` (D-08). The offending value is never echoed.
fn parse_color(reader: &mut Reader<'_>, value: &Spanned<DeValue<'_>>, key: &str) -> Option<Color> {
    let parsed = match value.get_ref() {
        DeValue::String(text) => Color::from_str(text).ok(),
        DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix())
            .ok()
            .and_then(|n| u8::try_from(n).ok())
            .map(Color::Indexed),
        _ => None,
    };
    if parsed.is_none() {
        reader.error(
            value.span().start,
            key,
            "invalid color (use a name like \"light-blue\", an index 0-255, or \"#rrggbb\")",
        );
    }
    parsed
}

/// A list of modifier names, OR-ed together; order and repetition do not matter.
fn parse_modifiers(
    reader: &mut Reader<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
) -> Option<Modifier> {
    let DeValue::Array(items) = value.get_ref() else {
        reader.error(
            value.span().start,
            key,
            format!("expected an array, found {}", value.get_ref().type_str()),
        );
        return None;
    };
    let mut all = Modifier::empty();
    let mut ok = true;
    for item in items.iter() {
        let DeValue::String(name) = item.get_ref() else {
            reader.error(
                item.span().start,
                key,
                format!("expected a string, found {}", item.get_ref().type_str()),
            );
            ok = false;
            continue;
        };
        match MODIFIER_NAMES.iter().find(|(known, _)| *known == &**name) {
            Some((_, modifier)) => all |= *modifier,
            None => {
                let valid: Vec<&str> = MODIFIER_NAMES.iter().map(|(n, _)| *n).collect();
                reader.error(
                    item.span().start,
                    key,
                    format!(
                        "unknown modifier \"{}\"; valid names: {}",
                        cap(name),
                        valid.join(", ")
                    ),
                );
                ok = false;
            }
        }
    }
    ok.then_some(all)
}

/// Read `fg`, `bg` and `modifiers` of the element table at `path`.
fn read_spec(reader: &mut Reader<'_>, fields: &DeTable<'_>, path: &str) -> StyleSpec {
    let mut spec = StyleSpec::default();
    if let Some(value) = fields.get("fg") {
        spec.fg = parse_color(reader, value, &format!("{path}.fg"));
    }
    if let Some(value) = fields.get("bg") {
        spec.bg = parse_color(reader, value, &format!("{path}.bg"));
    }
    if let Some(value) = fields.get("modifiers") {
        spec.modifiers = parse_modifiers(reader, value, &format!("{path}.modifiers"));
    }
    reader.warn_unknown(fields, STYLE_KEYS, &format!("{path}."));
    spec
}

fn is_pane(name: &str) -> bool {
    ELEMENT_PATHS
        .iter()
        .any(|path| path.split_once('.').is_some_and(|(pane, _)| pane == name))
}

/// Read `theme.toml` (`None` = absent). An invalid value keeps its default and records one
/// problem; unknown panes, elements and keys only warn (D-03).
pub fn parse(src: Option<&str>, out: &mut Vec<Problem>) -> Theme {
    let mut theme = Theme::default();
    let Some(src) = src else {
        return theme;
    };
    let Some(root) = parse_table(ConfigFile::Theme, src, out) else {
        return theme;
    };
    let mut reader = Reader {
        file: ConfigFile::Theme,
        src,
        out,
    };
    for (pane_key, pane_value) in root.get_ref().iter() {
        let pane: &str = pane_key.get_ref();
        if !is_pane(pane) {
            reader.warning(pane_key.span().start, &cap(pane), "unknown key, ignored");
            continue;
        }
        let DeValue::Table(elements) = pane_value.get_ref() else {
            reader.error(
                pane_value.span().start,
                pane,
                format!(
                    "expected a table, found {}",
                    pane_value.get_ref().type_str()
                ),
            );
            continue;
        };
        for (element_key, element_value) in elements.iter() {
            let element: &str = element_key.get_ref();
            let path = format!("{pane}.{element}");
            if !ELEMENT_PATHS.contains(&path.as_str()) {
                reader.warning(
                    element_key.span().start,
                    &format!("{pane}.{}", cap(element)),
                    "unknown key, ignored",
                );
                continue;
            }
            let DeValue::Table(fields) = element_value.get_ref() else {
                reader.error(
                    element_value.span().start,
                    &path,
                    format!(
                        "expected a table, found {}",
                        element_value.get_ref().type_str()
                    ),
                );
                continue;
            };
            let spec = read_spec(&mut reader, fields, &path);
            if let Some(slot) = theme.style_slot_mut(pane, element) {
                *slot = spec.resolve(*slot);
            }
        }
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diag::Severity;
    use crate::config::{Sources, load_from_dir, load_from_sources};
    use crate::test_util::{app_with_config, render_buffer, temp_config_dir};

    fn parse_str(src: &str) -> (Theme, Vec<Problem>) {
        let mut out = Vec::new();
        let theme = parse(Some(src), &mut out);
        (theme, out)
    }

    /// `status.mode` fg as parsed from `fg = <value>` (a TOML literal).
    fn mode_fg(value: &str) -> (Option<Color>, Vec<Problem>) {
        let (theme, out) = parse_str(&format!("[status.mode]\nfg = {value}\n"));
        (theme.status_mode.fg, out)
    }

    #[test]
    fn tracer_status_mode_from_dir() {
        let dir = temp_config_dir(
            "theme-tracer",
            &[(
                "theme.toml",
                "[status.mode]\nfg = 208\nbg = \"#102030\"\nmodifiers = [\"italic\"]\n",
            )],
        );
        let config = match load_from_dir(&dir) {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        };
        let app = app_with_config(config, 80, 24);
        let buffer = render_buffer(&app, 80, 24);
        let cell = &buffer[(1, 22)];
        assert_eq!(cell.symbol(), "N", "the mode badge");
        assert_eq!(cell.fg, Color::Indexed(208));
        assert_eq!(cell.bg, Color::Rgb(16, 32, 48));
        assert_eq!(cell.modifier, Modifier::ITALIC);
        let _ = std::fs::remove_dir_all(&dir);

        // Positive control: with no theme the badge is the Phase 1 reversed bold one.
        let app = app_with_config(crate::config::Config::default(), 80, 24);
        let cell = &render_buffer(&app, 80, 24)[(1, 22)];
        assert_eq!(cell.modifier, Modifier::REVERSED | Modifier::BOLD);
        assert_eq!(cell.fg, Color::Reset);
    }

    #[test]
    fn color_forms() {
        for (value, want) in [
            ("\"light-blue\"", Color::LightBlue),
            ("\"cyan\"", Color::Cyan),
            ("\"#ff8800\"", Color::Rgb(255, 136, 0)),
            ("\"208\"", Color::Indexed(208)),
            ("208", Color::Indexed(208)),
            ("0", Color::Indexed(0)),
            ("255", Color::Indexed(255)),
        ] {
            let (fg, out) = mode_fg(value);
            assert!(out.is_empty(), "{value}: {out:?}");
            assert_eq!(fg, Some(want), "{value}");
        }
        for bad in ["300", "-1", "\"nope\"", "true", "1.5", "256", "\"#12345\""] {
            let (fg, out) = mode_fg(bad);
            assert_eq!(fg, None, "{bad}");
            assert_eq!(out.len(), 1, "{bad}: {out:?}");
            assert_eq!(out[0].file, ConfigFile::Theme);
            assert_eq!(out[0].severity, Severity::Error);
            assert_eq!(out[0].key.as_deref(), Some("status.mode.fg"), "{bad}");
            assert_eq!(out[0].pos, Some((2, 6)), "{bad}");
        }
    }

    #[test]
    fn modifiers_clear_and_inherit() {
        let (theme, out) = parse_str("[status.mode]\nmodifiers = []\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.status_mode.add_modifier, Modifier::empty());
        assert_eq!(theme.status_mode.sub_modifier, Modifier::empty());

        let (theme, out) = parse_str("[status.mode]\nfg = 1\n");
        assert!(out.is_empty());
        assert_eq!(
            theme.status_mode.add_modifier,
            Modifier::REVERSED | Modifier::BOLD
        );

        let (a, _) = parse_str("[status.mode]\nmodifiers = [\"bold\", \"dim\", \"bold\"]\n");
        let (b, _) = parse_str("[status.mode]\nmodifiers = [\"dim\", \"bold\"]\n");
        assert_eq!(a, b);
        assert_eq!(a.status_mode.add_modifier, Modifier::BOLD | Modifier::DIM);

        let (theme, out) = parse_str("[status.mode]\nmodifiers = [\"blinky\"]\n");
        assert_eq!(theme, Theme::default());
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("blinky"), "{}", out[0].message);
        assert!(out[0].message.contains("crossed_out"), "{}", out[0].message);
        assert_eq!(out[0].key.as_deref(), Some("status.mode.modifiers"));

        for bad in ["\"bold\"", "[1]", "3"] {
            let (_, out) = parse_str(&format!("[status.mode]\nmodifiers = {bad}\n"));
            assert_eq!(out.len(), 1, "{bad}: {out:?}");
            assert!(out[0].message.starts_with("expected"), "{}", out[0].message);
        }
        for name in [
            "slow_blink",
            "rapid_blink",
            "reversed",
            "hidden",
            "crossed_out",
        ] {
            let (_, out) = parse_str(&format!("[status.mode]\nmodifiers = [\"{name}\"]\n"));
            assert!(out.is_empty(), "{name}: {out:?}");
        }
    }

    #[test]
    fn default_theme_matches_phase1() {
        let theme = Theme::default();
        assert_eq!(
            theme.status_mode,
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(theme.status_mode.fg, None);
        assert_eq!(theme.status_mode.bg, None);
        let (parsed, out) = parse_str("");
        assert!(out.is_empty());
        assert_eq!(parsed, theme);
    }

    #[test]
    fn unknown_theme_keys_warn() {
        for alone in [
            "[status.mode]\nunderline = true\n",
            "[nope]\nx = 1\n",
            "[status.nope]\nx = 1\n",
            "top = 1\n",
        ] {
            let sources = Sources {
                theme: Some(alone.to_string()),
                ..Sources::default()
            };
            assert!(load_from_sources(&sources).is_ok(), "{alone}");
        }
        let sources = Sources {
            theme: Some(
                "[status.mode]\nunderline = true\nfg = 300\n[nope]\nx = 1\n[status.nope]\nx = 1\n"
                    .to_string(),
            ),
            ..Sources::default()
        };
        let Err(report) = load_from_sources(&sources) else {
            panic!("expected Err");
        };
        assert_eq!(report.errors.len(), 1);
        let keys: Vec<_> = report
            .warnings
            .iter()
            .map(|p| p.key.clone().unwrap_or_default())
            .collect();
        assert_eq!(keys, ["status.mode.underline", "nope", "status.nope"]);
        assert!(report.warnings.iter().all(|p| p.file == ConfigFile::Theme));
    }

    #[test]
    fn chrome_elements_parse() {
        assert_eq!(ELEMENT_PATHS.len(), 14);
        let mut toml = String::new();
        for (i, path) in ELEMENT_PATHS.iter().enumerate() {
            let (pane, element) = path.split_once('.').expect("pane.element");
            toml.push_str(&format!("[{pane}.{element}]\nfg = {i}\n"));
        }
        let (mut theme, out) = parse_str(&toml);
        assert!(out.is_empty(), "{out:?}");
        for (i, path) in ELEMENT_PATHS.iter().enumerate() {
            let (pane, element) = path.split_once('.').expect("pane.element");
            let style = theme.style_slot_mut(pane, element).expect("slot");
            assert_eq!(style.fg, Some(Color::Indexed(i as u8)), "{path}");
        }
        // The default-valued elements keep their modifiers when only fg is set.
        assert_eq!(
            theme.sidebar_header.add_modifier,
            Modifier::BOLD | Modifier::DIM
        );
        assert_eq!(theme.status_error.fg, Some(Color::Indexed(10)));
    }

    #[test]
    fn defaults_match_phase1_chrome() {
        let t = Theme::default();
        assert_eq!(
            t.sidebar_header,
            Style::default().add_modifier(Modifier::BOLD | Modifier::DIM)
        );
        assert_eq!(
            t.sidebar_unread,
            Style::default().add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            t.sidebar_cursor,
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(
            t.sidebar_cursor_unfocused,
            Style::default().add_modifier(Modifier::BOLD)
        );
        assert_eq!(t.status_error, Style::default().fg(Color::Red));
        assert_eq!(t.input_hint, Style::default().add_modifier(Modifier::DIM));
        for plain in [
            t.sidebar_folder,
            t.sidebar_room,
            t.status_room,
            t.status_pending,
            t.status_info,
            t.cmdline_text,
            t.input_text,
        ] {
            assert_eq!(plain, Style::default());
        }
    }

    #[test]
    fn wrong_shapes_are_errors() {
        let (theme, out) = parse_str("status = 1\n");
        assert_eq!(theme, Theme::default());
        assert_eq!(out.len(), 1);
        assert!(out[0].message.starts_with("expected a table"));
        let (_, out) = parse_str("[status]\nmode = 1\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("status.mode"));
        let (_, out) = parse_str("[status.mode\n");
        assert!(!out.is_empty());
    }
}
