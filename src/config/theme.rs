//! `theme.toml`: per-element styles (D-08 colors, D-09 `[pane.element]` tables).
//!
//! Defaults are Rust values that reproduce the Phase 1 look exactly (RESEARCH Pattern 2); the file
//! only overrides what it names. An element table takes optional `fg`, `bg` and `modifiers`; an
//! unset field inherits the built-in default.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use crate::data::RoomKind;
use crate::ui::palette::DEFAULT_AUTHOR_PALETTE;
use crate::ui::sanitize::sanitize_line;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;
use std::str::FromStr;
use toml::Spanned;
use toml::de::{DeTable, DeValue};
use unicode_width::UnicodeWidthStr;

/// Every styled element a theme.toml may set, as `pane.element`: the complete list (D-09). The
/// pre-App config error screen is the one styled thing without a key (A30). The names are the
/// user-facing vocabulary of theme.toml.
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
    "border.focused",
    "border.unfocused",
    "messages.cursor",
    "messages.cursor_unfocused",
    "messages.time",
    "messages.author",
    "messages.body",
    "messages.notice",
    "messages.pending",
];

/// Widest a marker may be, in columns (A7). The encrypted tag has its own, larger cap.
pub const MAX_MARKER_WIDTH: usize = 4;
pub const MAX_TAG_WIDTH: usize = 12;

/// Most colors an `author_palette` may list (A8).
pub const MAX_PALETTE: usize = 64;

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

/// The keys an element table accepts; `[border.*]` tables also take `type`.
const STYLE_KEYS: &[&str] = &["fg", "bg", "modifiers"];
const BORDER_KEYS: &[&str] = &["fg", "bg", "modifiers", "type"];

/// The keys of the `[markers]` table.
const MARKER_KEYS: &[&str] = &[
    "fold_open",
    "fold_closed",
    "unread",
    "cursor",
    "encrypted",
    "channel",
    "private_group",
    "discussion",
    "team",
    "direct_message",
    "sending",
];

/// Border types a `type` key accepts (A6).
const BORDER_TYPES: &[(&str, BorderType)] = &[
    ("plain", BorderType::Plain),
    ("rounded", BorderType::Rounded),
    ("double", BorderType::Double),
    ("thick", BorderType::Thick),
];

/// The strings drawn in the sidebar: fold, unread and cursor markers, the encrypted tag and the
/// room-kind glyphs (D-11). They are drawn and measured in every row, so each one is checked
/// when it is read.
#[derive(Clone, Debug, PartialEq)]
pub struct Markers {
    pub fold_open: String,
    pub fold_closed: String,
    pub unread: String,
    pub cursor: String,
    pub encrypted: String,
    pub channel: String,
    pub private_group: String,
    pub discussion: String,
    pub team: String,
    pub direct_message: String,
    /// Drawn in place of the time of a message that is `Queued` or `Sending` (D-01).
    pub sending: String,
}

impl Default for Markers {
    fn default() -> Self {
        let glyph = |kind| crate::ui::sidebar::kind_glyph(kind).to_string();
        Markers {
            fold_open: "\u{25be} ".to_string(),
            fold_closed: "\u{25b8} ".to_string(),
            unread: "* ".to_string(),
            cursor: "> ".to_string(),
            encrypted: " [e2e]".to_string(),
            channel: glyph(RoomKind::Channel),
            private_group: glyph(RoomKind::PrivateGroup),
            discussion: glyph(RoomKind::Discussion),
            team: glyph(RoomKind::Team),
            direct_message: glyph(RoomKind::DirectMessage),
            sending: "\u{2026}".to_string(),
        }
    }
}

impl Markers {
    /// The glyph drawn before a room name of this kind.
    pub fn glyph(&self, kind: RoomKind) -> &str {
        match kind {
            RoomKind::Channel => &self.channel,
            RoomKind::PrivateGroup => &self.private_group,
            RoomKind::Discussion => &self.discussion,
            RoomKind::Team => &self.team,
            RoomKind::DirectMessage => &self.direct_message,
        }
    }

    /// The marker field behind a `[markers]` key and its width cap.
    fn slot_mut(&mut self, key: &str) -> Option<(&mut String, usize)> {
        let slot = match key {
            "fold_open" => &mut self.fold_open,
            "fold_closed" => &mut self.fold_closed,
            "unread" => &mut self.unread,
            "cursor" => &mut self.cursor,
            "encrypted" => return Some((&mut self.encrypted, MAX_TAG_WIDTH)),
            "channel" => &mut self.channel,
            "private_group" => &mut self.private_group,
            "discussion" => &mut self.discussion,
            "team" => &mut self.team,
            "direct_message" => &mut self.direct_message,
            "sending" => &mut self.sending,
            _ => return None,
        };
        Some((slot, MAX_MARKER_WIDTH))
    }
}

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
    /// The highlight over the cursor message's rows, focused and not. A foreground set here
    /// overrides the author colors on those rows (RESEARCH Pitfall 13).
    pub messages_cursor: Style,
    pub messages_cursor_unfocused: Style,
    pub messages_time: Style,
    /// The author name. A foreground set here replaces the palette color for every author (A31).
    pub messages_author: Style,
    pub messages_body: Style,
    /// The "No room open" and "No messages yet" lines.
    pub messages_notice: Style,
    /// Author, marker and body of a message the server has not confirmed yet: `Queued` or
    /// `Sending` (D-01).
    pub messages_pending: Style,
    /// Border and title style of the focused and the other panes.
    pub border_focused: Style,
    pub border_unfocused: Style,
    pub border_type_focused: BorderType,
    pub border_type_unfocused: BorderType,
    pub markers: Markers,
    /// The colors authors are hashed onto (D-10). Never empty when it comes from theme.toml;
    /// `author_color` still tolerates an empty slice.
    pub author_palette: Vec<Color>,
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
            messages_cursor: plain.add_modifier(Modifier::REVERSED),
            messages_cursor_unfocused: plain.bg(Color::DarkGray),
            messages_time: plain.add_modifier(Modifier::DIM),
            messages_author: plain.add_modifier(Modifier::BOLD),
            messages_body: plain,
            messages_notice: plain.add_modifier(Modifier::DIM),
            messages_pending: plain.add_modifier(Modifier::DIM),
            border_focused: plain,
            border_unfocused: plain,
            border_type_focused: BorderType::Thick,
            border_type_unfocused: BorderType::Plain,
            markers: Markers::default(),
            author_palette: DEFAULT_AUTHOR_PALETTE.to_vec(),
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
            ("messages", "cursor") => Some(&mut self.messages_cursor),
            ("messages", "cursor_unfocused") => Some(&mut self.messages_cursor_unfocused),
            ("messages", "time") => Some(&mut self.messages_time),
            ("messages", "author") => Some(&mut self.messages_author),
            ("messages", "body") => Some(&mut self.messages_body),
            ("messages", "notice") => Some(&mut self.messages_notice),
            ("messages", "pending") => Some(&mut self.messages_pending),
            ("border", "focused") => Some(&mut self.border_focused),
            ("border", "unfocused") => Some(&mut self.border_unfocused),
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

/// Read `fg`, `bg` and `modifiers` of the element table at `path`; unknown keys warn.
fn read_spec(reader: &mut Reader<'_>, fields: &DeTable<'_>, pane: &str, path: &str) -> StyleSpec {
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
    let known = if pane == "border" {
        BORDER_KEYS
    } else {
        STYLE_KEYS
    };
    reader.warn_unknown(fields, known, &format!("{path}."));
    spec
}

/// The `type` of a `[border.*]` table.
fn parse_border_type(
    reader: &mut Reader<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
) -> Option<BorderType> {
    let text = reader.string(value, key)?;
    let found = BORDER_TYPES.iter().find(|(name, _)| *name == text);
    if found.is_none() {
        reader.error(
            value.span().start,
            key,
            "border type must be one of plain, rounded, double, thick",
        );
    }
    found.map(|(_, border)| *border)
}

/// Read the `[markers]` table over `markers`. Each marker must survive `sanitize_line` unchanged
/// (T-02-14) and fit its width cap (A7); an empty string is allowed.
fn parse_markers(reader: &mut Reader<'_>, value: &Spanned<DeValue<'_>>, markers: &mut Markers) {
    let DeValue::Table(table) = value.get_ref() else {
        reader.error(
            value.span().start,
            "markers",
            format!("expected a table, found {}", value.get_ref().type_str()),
        );
        return;
    };
    let mut fold_at = None;
    for (name_key, entry) in table.iter() {
        let name: &str = name_key.get_ref();
        let key = format!("markers.{}", cap(name));
        // Unknown keys are reported by `warn_unknown` below.
        let Some((slot, max_width)) = markers.slot_mut(name) else {
            continue;
        };
        let Some(text) = reader.string(entry, &key) else {
            continue;
        };
        if sanitize_line(&text) != text.as_str() {
            reader.error(
                entry.span().start,
                &key,
                "must not contain control or bidi characters",
            );
            continue;
        }
        let width = text.width();
        if width > max_width {
            reader.error(
                entry.span().start,
                &key,
                format!("must be at most {max_width} columns wide, got {width}"),
            );
            continue;
        }
        *slot = text;
        if name.starts_with("fold_") {
            fold_at.get_or_insert(entry.span().start);
        }
    }
    if let Some(at) = fold_at
        && markers.fold_open.width() != markers.fold_closed.width()
    {
        reader.warning(
            at,
            "markers.fold_open",
            "fold markers have different widths; folder rows shift when toggled",
        );
    }
    reader.warn_unknown(table, MARKER_KEYS, "markers.");
}

/// Read the top-level `author_palette` array: 1 to `MAX_PALETTE` colors in any D-08 form. Every
/// entry is checked (so each bad one is reported, named by its index); any problem keeps the
/// default palette (D-10, A8, T-02-17). Duplicates are allowed.
fn parse_palette(reader: &mut Reader<'_>, value: &Spanned<DeValue<'_>>) -> Option<Vec<Color>> {
    let DeValue::Array(items) = value.get_ref() else {
        reader.error(
            value.span().start,
            "author_palette",
            format!("expected an array, found {}", value.get_ref().type_str()),
        );
        return None;
    };
    if items.is_empty() {
        reader.error(
            value.span().start,
            "author_palette",
            "must list at least one color",
        );
        return None;
    }
    if items.len() > MAX_PALETTE {
        reader.error(
            value.span().start,
            "author_palette",
            format!("at most {MAX_PALETTE} colors"),
        );
        return None;
    }
    let mut colors = Vec::with_capacity(items.len());
    let mut ok = true;
    for (i, item) in items.iter().enumerate() {
        match parse_color(reader, item, &format!("author_palette[{i}]")) {
            Some(color) => colors.push(color),
            None => ok = false,
        }
    }
    ok.then_some(colors)
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
        if pane == "markers" {
            parse_markers(&mut reader, pane_value, &mut theme.markers);
            continue;
        }
        if pane == "author_palette" {
            if let Some(colors) = parse_palette(&mut reader, pane_value) {
                theme.author_palette = colors;
            }
            continue;
        }
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
            let spec = read_spec(&mut reader, fields, pane, &path);
            if let Some(slot) = theme.style_slot_mut(pane, element) {
                *slot = spec.resolve(*slot);
            }
            if pane == "border"
                && let Some(value) = fields.get("type")
                && let Some(border) = parse_border_type(&mut reader, value, &format!("{path}.type"))
            {
                match element {
                    "focused" => theme.border_type_focused = border,
                    _ => theme.border_type_unfocused = border,
                }
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
    fn every_element_is_themeable() {
        assert_eq!(
            ELEMENT_PATHS.len(),
            23,
            "16 chrome and border paths plus the seven messages elements"
        );
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
        assert_eq!(theme.messages_author.add_modifier, Modifier::BOLD);
        assert_eq!(theme.messages_cursor.add_modifier, Modifier::REVERSED);
    }

    #[test]
    fn messages_defaults_match_phase1() {
        let t = Theme::default();
        assert_eq!(
            t.messages_cursor,
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(
            t.messages_cursor_unfocused,
            Style::default().bg(Color::DarkGray)
        );
        assert_eq!(
            t.messages_time,
            Style::default().add_modifier(Modifier::DIM)
        );
        assert_eq!(
            t.messages_author,
            Style::default().add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            t.messages_notice,
            Style::default().add_modifier(Modifier::DIM)
        );
        assert_eq!(t.messages_body, Style::default());
    }

    #[test]
    fn palette_order_defines_mapping() {
        use crate::ui::palette::author_color;
        let (red_blue, out) = parse_str("author_palette = [\"red\", \"blue\"]\n");
        assert!(out.is_empty(), "{out:?}");
        let (blue_red, out) = parse_str("author_palette = [\"blue\", \"red\"]\n");
        assert!(out.is_empty(), "{out:?}");
        let mut swapped = 0;
        for name in ["alice", "bob", "carol", "dave"] {
            let a = author_color(name, &red_blue.author_palette);
            let b = author_color(name, &blue_red.author_palette);
            assert_ne!(
                a, b,
                "{name}: reversing a two-color palette swaps the color"
            );
            swapped += 1;
        }
        assert_eq!(swapped, 4);
        let (all_red, out) = parse_str("author_palette = [\"red\", \"red\"]\n");
        assert!(out.is_empty(), "{out:?}");
        for name in ["alice", "bob", "carol", "dave"] {
            assert_eq!(author_color(name, &all_red.author_palette), Color::Red);
        }
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
    fn border_types() {
        for (name, want) in [
            ("plain", BorderType::Plain),
            ("rounded", BorderType::Rounded),
            ("double", BorderType::Double),
            ("thick", BorderType::Thick),
        ] {
            let (theme, out) = parse_str(&format!(
                "[border.focused]\ntype = \"{name}\"\n[border.unfocused]\ntype = \"{name}\"\n"
            ));
            assert!(out.is_empty(), "{name}: {out:?}");
            assert_eq!(theme.border_type_focused, want, "{name}");
            assert_eq!(theme.border_type_unfocused, want, "{name}");
        }
        let (theme, out) = parse_str("[border.focused]\ntype = \"dashed\"\n");
        assert_eq!(theme, Theme::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].key.as_deref(), Some("border.focused.type"));
        assert!(out[0].message.contains("rounded"), "{}", out[0].message);
        let (_, out) = parse_str("[border.focused]\ntype = 3\n");
        assert_eq!(out.len(), 1);
        assert!(out[0].message.starts_with("expected a string"));

        // `type` is only a key of [border.*]; elsewhere it is an unknown key.
        let (theme, out) = parse_str("[sidebar.cursor]\ntype = \"rounded\"\n");
        assert_eq!(theme, Theme::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].key.as_deref(), Some("sidebar.cursor.type"));

        // The border style also applies.
        let (theme, out) = parse_str("[border.focused]\nfg = \"cyan\"\ntype = \"rounded\"\n");
        assert!(out.is_empty());
        assert_eq!(theme.border_focused.fg, Some(Color::Cyan));
        assert_eq!(theme.border_type_unfocused, BorderType::Plain);
    }

    fn marker_problems(src: &str) -> Vec<Problem> {
        parse_str(src).1
    }

    #[test]
    fn markers_validation() {
        for bad in ["\\u001b[2J", "a\\u202eb", "a\\nb", "a\\tb", "\\u007f"] {
            let out = marker_problems(&format!("[markers]\nunread = \"{bad}\"\n"));
            assert_eq!(out.len(), 1, "{bad}: {out:?}");
            assert_eq!(out[0].severity, Severity::Error);
            assert_eq!(out[0].key.as_deref(), Some("markers.unread"));
            assert!(out[0].message.contains("control"), "{}", out[0].message);
        }
        // A 4-column marker fits, a 5-column one does not.
        assert!(
            marker_problems("[markers]\nfold_open = \"vvv \"\nfold_closed = \">>> \"\n").is_empty()
        );
        let out = marker_problems("[markers]\nfold_open = \"vvvvv\"\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Error);
        assert!(out[0].message.contains("4 columns"), "{}", out[0].message);
        // Wide characters count by display width: two wide glyphs are four columns.
        assert!(marker_problems("[markers]\nchannel = \"\u{4e2d}\u{6587}\"\n").is_empty());
        let out = marker_problems("[markers]\nchannel = \"\u{4e2d}\u{6587}\u{5b57}\"\n");
        assert_eq!(out.len(), 1, "{out:?}");
        // The encrypted tag may be 12 columns, not 13.
        assert!(marker_problems("[markers]\nencrypted = \"123456789012\"\n").is_empty());
        let out = marker_problems("[markers]\nencrypted = \"1234567890123\"\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].message.contains("12 columns"), "{}", out[0].message);
        // An empty marker is a legitimate choice.
        let (theme, out) = parse_str("[markers]\nunread = \"\"\nchannel = \"\"\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.markers.unread, "");
        assert_eq!(theme.markers.glyph(RoomKind::Channel), "");
        // A non-string value and a non-table `markers`.
        let out = marker_problems("[markers]\nunread = 1\n");
        assert_eq!(out.len(), 1);
        assert!(out[0].message.starts_with("expected a string"));
        let out = marker_problems("markers = 1\n");
        assert_eq!(out.len(), 1);
        assert!(out[0].message.starts_with("expected a table"));
        // Unequal fold widths only warn.
        let out = marker_problems("[markers]\nfold_open = \"v \"\nfold_closed = \">>> \"\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Warning);
        assert!(out[0].message.contains("different widths"));
        // Unknown marker keys warn once.
        let out = marker_problems("[markers]\nbogus = \"x\"\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].key.as_deref(), Some("markers.bogus"));
        // A rejected marker keeps its default.
        let (theme, _) = parse_str("[markers]\nunread = \"\\u202e\"\n");
        assert_eq!(theme.markers.unread, "* ");
    }

    #[test]
    fn marker_defaults_match_phase1() {
        let m = Markers::default();
        for kind in [
            RoomKind::Channel,
            RoomKind::PrivateGroup,
            RoomKind::Discussion,
            RoomKind::Team,
            RoomKind::DirectMessage,
        ] {
            assert_eq!(m.glyph(kind), crate::ui::sidebar::kind_glyph(kind));
        }
        assert_eq!(m.fold_open, "\u{25be} ");
        assert_eq!(m.fold_closed, "\u{25b8} ");
        assert_eq!(m.unread, "* ");
        assert_eq!(m.cursor, "> ");
        assert_eq!(m.encrypted, " [e2e]");
        assert_eq!(Theme::default().border_type_focused, BorderType::Thick);
        assert_eq!(Theme::default().border_type_unfocused, BorderType::Plain);
    }

    #[test]
    fn send_marker_defaults() {
        assert_eq!(Markers::default().sending, "\u{2026}");
        assert_eq!(
            Theme::default().messages_pending,
            Style::default().add_modifier(Modifier::DIM)
        );
    }

    #[test]
    fn send_vocabulary_parses() {
        let (theme, out) =
            parse_str("[markers]\nsending = \"~\"\n[messages.pending]\nfg = \"blue\"\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.markers.sending, "~");
        assert_eq!(theme.messages_pending.fg, Some(Color::Blue));
        // The default dim survives when only fg is set.
        assert_eq!(theme.messages_pending.add_modifier, Modifier::DIM);
        for bad in ["12345", "\\u001b[2J", "a\\u202eb"] {
            let out = marker_problems(&format!("[markers]\nsending = \"{bad}\"\n"));
            assert_eq!(out.len(), 1, "{bad}: {out:?}");
            assert_eq!(out[0].severity, Severity::Error);
            assert_eq!(out[0].key.as_deref(), Some("markers.sending"));
        }
    }

    #[test]
    fn equal_values_allowed() {
        // Two elements with the same color, and fg equal to bg.
        let (theme, out) = parse_str(
            "[sidebar.cursor]\nfg = 4\nbg = 4\n[status.mode]\nfg = 4\n[status.error]\nfg = \"red\"\nbg = \"red\"\n",
        );
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.sidebar_cursor.fg, theme.sidebar_cursor.bg);
        assert_eq!(theme.status_mode.fg, theme.sidebar_cursor.fg);

        // Every value spelled out at its default equals leaving it unset.
        let mut defaults = Theme::default();
        let mut toml = String::new();
        for path in ELEMENT_PATHS {
            let (pane, element) = path.split_once('.').expect("pane.element");
            let style = *defaults.style_slot_mut(pane, element).expect("slot");
            toml.push_str(&format!("[{path}]\n"));
            if let Some(fg) = style.fg {
                toml.push_str(&format!("fg = \"{fg}\"\n"));
            }
            if let Some(bg) = style.bg {
                toml.push_str(&format!("bg = \"{bg}\"\n"));
            }
            let names: Vec<String> = MODIFIER_NAMES
                .iter()
                .filter(|(_, m)| style.add_modifier.contains(*m))
                .map(|(n, _)| format!("\"{n}\""))
                .collect();
            toml.push_str(&format!("modifiers = [{}]\n", names.join(", ")));
            match *path {
                "border.focused" => toml.push_str("type = \"thick\"\n"),
                "border.unfocused" => toml.push_str("type = \"plain\"\n"),
                _ => {}
            }
        }
        let m = Markers::default();
        toml.push_str(&format!(
            "[markers]\nfold_open = {:?}\nfold_closed = {:?}\nunread = {:?}\ncursor = {:?}\nencrypted = {:?}\nchannel = {:?}\nprivate_group = {:?}\ndiscussion = {:?}\nteam = {:?}\ndirect_message = {:?}\nsending = {:?}\n",
            m.fold_open, m.fold_closed, m.unread, m.cursor, m.encrypted, m.channel, m.private_group, m.discussion, m.team, m.direct_message, m.sending
        ));
        let (theme, out) = parse_str(&toml);
        assert!(out.is_empty(), "{out:?}\n{toml}");
        assert_eq!(theme, Theme::default());
    }

    #[test]
    fn empty_tables_inherit() {
        for src in [
            "",
            "# nothing\n",
            "[sidebar.cursor]\n",
            "[sidebar]\n",
            "[border.focused]\n[markers]\n",
        ] {
            let (theme, out) = parse_str(src);
            assert!(out.is_empty(), "{src:?}: {out:?}");
            assert_eq!(theme, Theme::default(), "{src:?}");
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

    #[test]
    fn tracer_author_palette_from_dir() {
        let dir = temp_config_dir(
            "theme-palette",
            &[("theme.toml", "author_palette = [\"red\"]\n")],
        );
        let config = match load_from_dir(&dir) {
            Ok(config) => config,
            Err(report) => panic!("expected Ok, got {report}"),
        };
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = app_with_config(config, 80, 25);
        app.open_room(crate::data::RoomId("general".to_string()));
        let buffer = render_buffer(&app, 80, 25);
        // Compact rows inside the pane border: "HH:MM " is 6 columns, so the ':' is at offset 3
        // and the author name starts at offset 7 from the pane's left edge.
        let pane = app.areas().messages;
        let mut names = 0;
        for y in pane.y + 1..pane.y + pane.height - 1 {
            if buffer[(pane.x + 3, y)].symbol() == ":" {
                let cell = &buffer[(pane.x + 7, y)];
                assert_eq!(cell.fg, Color::Red, "row {y}");
                names += 1;
            }
        }
        assert!(names >= 4, "only {names} author rows found");
    }

    #[test]
    fn palette_validation() {
        let (theme, out) = parse_str("author_palette = [\"cyan\", \"#ff8800\", 208]\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(
            theme.author_palette,
            [Color::Cyan, Color::Rgb(255, 136, 0), Color::Indexed(208)]
        );

        let default = Theme::default().author_palette;
        let (theme, out) = parse_str("author_palette = []\n");
        assert_eq!(theme.author_palette, default);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].key.as_deref(), Some("author_palette"));
        assert!(
            out[0].message.contains("at least one"),
            "{}",
            out[0].message
        );

        let many = vec!["\"red\""; MAX_PALETTE + 1].join(", ");
        let (theme, out) = parse_str(&format!("author_palette = [{many}]\n"));
        assert_eq!(theme.author_palette, default);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].message.contains("at most 64"), "{}", out[0].message);
        let max = vec!["\"red\""; MAX_PALETTE].join(", ");
        let (theme, out) = parse_str(&format!("author_palette = [{max}]\n"));
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.author_palette.len(), MAX_PALETTE);

        let (theme, out) = parse_str("author_palette = [\"cyan\", \"nope\"]\n");
        assert_eq!(theme.author_palette, default);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].key.as_deref(), Some("author_palette[1]"));
        assert!(!out[0].message.contains("nope"), "{}", out[0].message);

        let (theme, out) = parse_str("author_palette = \"red\"\n");
        assert_eq!(theme.author_palette, default);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Severity::Error);
        assert!(out[0].message.starts_with("expected an array"));

        let (theme, out) = parse_str("author_palette = [\"red\", \"red\"]\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(theme.author_palette, [Color::Red, Color::Red]);
    }

    #[test]
    fn default_palette_is_phase1() {
        assert_eq!(Theme::default().author_palette, DEFAULT_AUTHOR_PALETTE);
    }
}
