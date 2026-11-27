//! `config.toml`: the scalar settings of D-12 and their D-13 validation.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use crate::ui::sanitize::sanitize_line;
use crate::ui::wrap::MessageLayout;
use chrono::format::{Item, StrftimeItems};
use chrono::{TimeZone, Utc};
use std::collections::HashMap;
use std::fmt::Write as _;
use toml::Spanned;
use toml::de::DeValue;
use unicode_width::UnicodeWidthStr;

pub const DEFAULT_HISTORY_SIZE: u16 = 50;
pub const DEFAULT_KEY_TIMEOUT_MS: u64 = 1000;
pub const DEFAULT_MARK_READ_DELAY_MS: u64 = 3000;
pub const DEFAULT_MESSAGE_GROUPING_SECONDS: u32 = 180;
pub const DEFAULT_IMAGE_MAX_ROWS: u16 = 12;
/// Inclusive bounds of the integer settings (D-13; the key timeout is A1).
pub const SIDEBAR_WIDTH_RANGE: (i64, i64) = (10, 60);
pub const HISTORY_SIZE_RANGE: (i64, i64) = (1, 1000);
pub const KEY_TIMEOUT_MS_RANGE: (i64, i64) = (100, 5000);
pub const MARK_READ_DELAY_MS_RANGE: (i64, i64) = (0, 60_000);
pub const MESSAGE_GROUPING_SECONDS_RANGE: (i64, i64) = (0, 3600);
pub const IMAGE_MAX_ROWS_RANGE: (i64, i64) = (3, 40);
/// Widest a rendered timestamp may be, in columns (A7).
pub const MAX_TIMESTAMP_WIDTH: usize = 40;
/// Widest an `[emoji_aliases]` value may be, in columns.
pub const MAX_ALIAS_WIDTH: usize = 8;
/// 2026-01-31 23:59:59 UTC: the instant a format is test-rendered with (month, day and hour all
/// wide, so a format that overflows with real data overflows here too).
const SAMPLE_TS: i64 = 1_769_903_999;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeymapPreset {
    #[default]
    Vim,
    VimExtras,
}

/// What the messages pane does after the user sends a message (config.toml `follow_on_send`, D-05).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FollowOnSend {
    /// Jump to the newest message after every send (`always`, the default).
    #[default]
    Always,
    /// Follow only when the newest row was already visible before the send (`if_at_bottom`), so a
    /// user reading older messages is not pulled away from them.
    IfAtBottom,
}

/// A strftime pattern for message times. The field is private: the only way to build one from
/// user input is `TimeFormat::parse`, which proves the pattern is valid and renders on one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeFormat(String);

impl Default for TimeFormat {
    fn default() -> Self {
        TimeFormat("%H:%M".to_string())
    }
}

impl TimeFormat {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate a user strftime pattern (T-02-05): non-empty, no invalid specifier, and a test
    /// render that is a single line of at most `MAX_TIMESTAMP_WIDTH` columns. The render goes
    /// through `write!`, because `to_string` on a bad format panics.
    pub fn parse(pattern: &str) -> Result<TimeFormat, String> {
        if pattern.is_empty() {
            return Err("must not be empty".to_string());
        }
        if StrftimeItems::new(pattern).any(|item| matches!(item, Item::Error)) {
            return Err("invalid strftime format".to_string());
        }
        let Some(sample) = Utc.timestamp_opt(SAMPLE_TS, 0).single() else {
            return Err("invalid strftime format".to_string());
        };
        let mut rendered = String::new();
        if write!(
            rendered,
            "{}",
            sample.format_with_items(StrftimeItems::new(pattern))
        )
        .is_err()
        {
            return Err("invalid strftime format".to_string());
        }
        if rendered.chars().any(char::is_control) {
            return Err("must render on one line".to_string());
        }
        if rendered.width() > MAX_TIMESTAMP_WIDTH {
            return Err(format!("renders wider than {MAX_TIMESTAMP_WIDTH} columns"));
        }
        Ok(TimeFormat(pattern.to_string()))
    }
}

/// The values of `config.toml`; `Default` reproduces the Phase 1 behaviour exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub keymap_preset: KeymapPreset,
    pub sidebar_width: u16,
    pub history_size: u16,
    pub timestamp_format: TimeFormat,
    pub message_layout: MessageLayout,
    pub key_timeout_ms: u64,
    /// config.toml key `mark_read_delay_ms`: how long a room stays open before it is marked read on
    /// the server. 0 marks it at once.
    pub mark_read_delay_ms: u64,
    /// config.toml key `message_grouping_seconds`: the longest pause between two messages of one
    /// author that still share a header in the two-line layout. 0 turns grouping off.
    pub message_grouping_seconds: u32,
    /// config.toml key `follow_on_send` (D-05): `"always"` (default) jumps to the newest message
    /// after every send; `"if_at_bottom"` follows only when the newest row was already visible.
    pub follow_on_send: FollowOnSend,
    /// config.toml key `inline_images`: draw image attachments inline (and fetch them) where the
    /// terminal can show images. On by default.
    pub inline_images: bool,
    /// config.toml key `animate_images`: play animated gif and webp attachments. On by default;
    /// off shows their first frame.
    pub animate_images: bool,
    /// config.toml key `image_max_rows`: the tallest an inline image may be, in rows.
    pub image_max_rows: u16,
    /// config.toml table `[emoji_aliases]`: the text drawn for a reaction by shortcode name
    /// (without colons), so a custom server emoji can show as a lookalike. Empty by default. An
    /// alias wins over the unicode table and over the server's custom emojis, which are drawn as
    /// their `:name:` in the theme's `[messages.custom_emoji]` style (their image where the
    /// terminal can show it).
    pub emoji_aliases: HashMap<String, String>,
    /// config.toml table `[open]`: the program that opens a downloaded attachment, per kind.
    pub open: OpenSettings,
}

/// The programs `[open]` names: each an argument list (the program, then its leading arguments),
/// never a shell string. The file is added as the last argument when one runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenSettings {
    pub image: Option<Vec<String>>,
    pub video: Option<Vec<String>>,
    pub audio: Option<Vec<String>>,
    pub default: Option<Vec<String>>,
}

impl OpenSettings {
    /// The command (program and leading arguments) for an attachment of `kind`: its own key, else
    /// `default`, else the platform's opener. `None` for a kind that is not opened.
    pub fn command(&self, kind: crate::data::AttachmentKind) -> Option<Vec<String>> {
        use crate::data::AttachmentKind;
        let own = match kind {
            AttachmentKind::Image => &self.image,
            AttachmentKind::Video => &self.video,
            AttachmentKind::Audio => &self.audio,
            AttachmentKind::Other => return None,
        };
        Some(
            own.clone()
                .or_else(|| self.default.clone())
                .unwrap_or_else(|| vec![platform_opener().to_string()]),
        )
    }
}

/// The opener used when `[open]` names none: `open` on macOS, `xdg-open` elsewhere.
pub const fn platform_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            keymap_preset: KeymapPreset::Vim,
            sidebar_width: crate::ui::layout::SIDEBAR_WIDTH,
            history_size: DEFAULT_HISTORY_SIZE,
            timestamp_format: TimeFormat::default(),
            message_layout: MessageLayout::Compact,
            key_timeout_ms: DEFAULT_KEY_TIMEOUT_MS,
            mark_read_delay_ms: DEFAULT_MARK_READ_DELAY_MS,
            message_grouping_seconds: DEFAULT_MESSAGE_GROUPING_SECONDS,
            follow_on_send: FollowOnSend::Always,
            inline_images: true,
            animate_images: true,
            image_max_rows: DEFAULT_IMAGE_MAX_ROWS,
            emoji_aliases: HashMap::new(),
            open: OpenSettings::default(),
        }
    }
}

const KNOWN_KEYS: &[&str] = &[
    "keymap_preset",
    "sidebar_width",
    "history_size",
    "timestamp_format",
    "message_layout",
    "key_timeout_ms",
    "mark_read_delay_ms",
    "message_grouping_seconds",
    "follow_on_send",
    "inline_images",
    "animate_images",
    "image_max_rows",
    "emoji_aliases",
    "open",
    // Read by `connection::parse_file`, not here; known so they are not reported as unknown (D-01).
    "server_url",
    "user_id",
    "token",
];

/// Read `config.toml` (`None` = absent). Each invalid key keeps its default and records one
/// problem.
pub fn parse(src: Option<&str>, out: &mut Vec<Problem>) -> Settings {
    let mut settings = Settings::default();
    let Some(src) = src else {
        return settings;
    };
    let Some(root) = parse_table(ConfigFile::Config, src, out) else {
        return settings;
    };
    let mut reader = Reader {
        file: ConfigFile::Config,
        src,
        out,
    };
    let table = root.get_ref();
    if let Some(value) = table.get("keymap_preset")
        && let Some(preset) = reader.one_of(
            value,
            "keymap_preset",
            &[
                ("vim", KeymapPreset::Vim),
                ("vim-extras", KeymapPreset::VimExtras),
            ],
        )
    {
        settings.keymap_preset = preset;
    }
    if let Some(value) = table.get("sidebar_width") {
        let (lo, hi) = SIDEBAR_WIDTH_RANGE;
        if let Some(n) = reader.int_in(value, "sidebar_width", lo, hi) {
            settings.sidebar_width = u16::try_from(n).unwrap_or(settings.sidebar_width);
        }
    }
    if let Some(value) = table.get("history_size") {
        let (lo, hi) = HISTORY_SIZE_RANGE;
        if let Some(n) = reader.int_in(value, "history_size", lo, hi) {
            settings.history_size = u16::try_from(n).unwrap_or(settings.history_size);
        }
    }
    if let Some(value) = table.get("timestamp_format")
        && let Some(text) = reader.string(value, "timestamp_format")
    {
        match TimeFormat::parse(&text) {
            Ok(format) => settings.timestamp_format = format,
            Err(message) => reader.error(value.span().start, "timestamp_format", message),
        }
    }
    if let Some(value) = table.get("message_layout")
        && let Some(layout) = reader.one_of(
            value,
            "message_layout",
            &[
                ("compact", MessageLayout::Compact),
                ("two_line", MessageLayout::TwoLine),
            ],
        )
    {
        settings.message_layout = layout;
    }
    if let Some(value) = table.get("follow_on_send")
        && let Some(follow) = reader.one_of(
            value,
            "follow_on_send",
            &[
                ("always", FollowOnSend::Always),
                ("if_at_bottom", FollowOnSend::IfAtBottom),
            ],
        )
    {
        settings.follow_on_send = follow;
    }
    if let Some(value) = table.get("key_timeout_ms") {
        let (lo, hi) = KEY_TIMEOUT_MS_RANGE;
        if let Some(n) = reader.int_in(value, "key_timeout_ms", lo, hi) {
            settings.key_timeout_ms = u64::try_from(n).unwrap_or(settings.key_timeout_ms);
        }
    }
    if let Some(value) = table.get("mark_read_delay_ms") {
        let (lo, hi) = MARK_READ_DELAY_MS_RANGE;
        if let Some(n) = reader.int_in(value, "mark_read_delay_ms", lo, hi) {
            settings.mark_read_delay_ms = u64::try_from(n).unwrap_or(settings.mark_read_delay_ms);
        }
    }
    if let Some(value) = table.get("message_grouping_seconds") {
        let (lo, hi) = MESSAGE_GROUPING_SECONDS_RANGE;
        if let Some(n) = reader.int_in(value, "message_grouping_seconds", lo, hi) {
            settings.message_grouping_seconds =
                u32::try_from(n).unwrap_or(settings.message_grouping_seconds);
        }
    }
    if let Some(value) = table.get("inline_images")
        && let Some(on) = reader.boolean(value, "inline_images")
    {
        settings.inline_images = on;
    }
    if let Some(value) = table.get("animate_images")
        && let Some(on) = reader.boolean(value, "animate_images")
    {
        settings.animate_images = on;
    }
    if let Some(value) = table.get("image_max_rows") {
        let (lo, hi) = IMAGE_MAX_ROWS_RANGE;
        if let Some(n) = reader.int_in(value, "image_max_rows", lo, hi) {
            settings.image_max_rows = u16::try_from(n).unwrap_or(settings.image_max_rows);
        }
    }
    if let Some(value) = table.get("emoji_aliases") {
        settings.emoji_aliases = parse_emoji_aliases(&mut reader, value);
    }
    if let Some(value) = table.get("open") {
        settings.open = parse_open(&mut reader, value);
    }
    // D-03: unknown keys and tables only warn. The connection keys are known (Phase 3, D-01).
    reader.warn_unknown(table, KNOWN_KEYS, "");
    settings
}

/// Read the `[open]` table. Each key is a non-empty array of strings whose first entry, the
/// program, is not empty; no entry may hold a NUL character. An invalid key is reported and left
/// unset, the others are kept.
fn parse_open(reader: &mut Reader<'_>, value: &Spanned<DeValue<'_>>) -> OpenSettings {
    let mut open = OpenSettings::default();
    let DeValue::Table(table) = value.get_ref() else {
        reader.error(
            value.span().start,
            "open",
            format!("expected a table, found {}", value.get_ref().type_str()),
        );
        return open;
    };
    for (name, slot) in [
        ("image", &mut open.image),
        ("video", &mut open.video),
        ("audio", &mut open.audio),
        ("default", &mut open.default),
    ] {
        if let Some(entry) = table.get(name) {
            *slot = open_command(reader, entry, &format!("open.{name}"));
        }
    }
    reader.warn_unknown(table, &["image", "video", "audio", "default"], "open.");
    open
}

/// One program and its leading arguments from an array of strings.
fn open_command(
    reader: &mut Reader<'_>,
    value: &Spanned<DeValue<'_>>,
    key: &str,
) -> Option<Vec<String>> {
    let DeValue::Array(items) = value.get_ref() else {
        reader.error(
            value.span().start,
            key,
            format!(
                "expected an array of strings, found {}",
                value.get_ref().type_str()
            ),
        );
        return None;
    };
    if items.is_empty() {
        reader.error(
            value.span().start,
            key,
            "a program is required: an array with the program first, then its arguments",
        );
        return None;
    }
    let mut command = Vec::new();
    for item in items.iter() {
        let DeValue::String(text) = item.get_ref() else {
            reader.error(
                item.span().start,
                key,
                format!(
                    "every entry must be a string, found {}",
                    item.get_ref().type_str()
                ),
            );
            return None;
        };
        if text.contains('\0') {
            reader.error(item.span().start, key, "must not contain a NUL character");
            return None;
        }
        command.push(text.to_string());
    }
    if command[0].is_empty() {
        reader.error(
            items[0].span().start,
            key,
            "the program name must not be empty",
        );
        return None;
    }
    Some(command)
}

/// At most this many characters of a user-written alias name are echoed in an error.
const ALIAS_NAME_ECHO: usize = 32;

/// Read the `[emoji_aliases]` table. A name is written without colons and holds no whitespace or
/// control character; a value must survive `sanitize_line` unchanged and fit `MAX_ALIAS_WIDTH`
/// columns. An invalid entry is reported and skipped, the others are kept.
fn parse_emoji_aliases(
    reader: &mut Reader<'_>,
    value: &Spanned<DeValue<'_>>,
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let DeValue::Table(table) = value.get_ref() else {
        reader.error(
            value.span().start,
            "emoji_aliases",
            format!("expected a table, found {}", value.get_ref().type_str()),
        );
        return aliases;
    };
    for (name_key, entry) in table.iter() {
        let name: &str = name_key.get_ref();
        let shown: String = name.chars().take(ALIAS_NAME_ECHO).collect();
        let key = format!("emoji_aliases.{shown}");
        if name.is_empty() {
            reader.error(name_key.span().start, &key, "the name must not be empty");
            continue;
        }
        if name.contains(':') {
            reader.error(
                name_key.span().start,
                &key,
                "write the name without colons (partyparrot, not :partyparrot:)",
            );
            continue;
        }
        if name.chars().any(|c| c.is_whitespace() || c.is_control()) {
            reader.error(
                name_key.span().start,
                &key,
                "the name must not contain spaces or control characters",
            );
            continue;
        }
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
        if width > MAX_ALIAS_WIDTH {
            reader.error(
                entry.span().start,
                &key,
                format!("must be at most {MAX_ALIAS_WIDTH} columns wide, got {width}"),
            );
            continue;
        }
        aliases.insert(name.to_string(), text);
    }
    aliases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diag::Severity;

    #[test]
    fn emoji_aliases_default_to_none() {
        let (s, out) = errors_of("");
        assert!(s.emoji_aliases.is_empty());
        assert!(out.is_empty());
        let (s, out) = errors_of("[emoji_aliases]\n");
        assert!(s.emoji_aliases.is_empty());
        assert!(out.is_empty(), "known table, no warning: {out:?}");
    }

    #[test]
    fn emoji_aliases_are_read() {
        let (s, out) = errors_of("[emoji_aliases]\npartyparrot = \"🦜\"\nship_it = \"🚢\"\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(s.emoji_aliases.len(), 2);
        assert_eq!(s.emoji_aliases["partyparrot"], "🦜");
        assert_eq!(s.emoji_aliases["ship_it"], "🚢");
    }

    #[test]
    fn emoji_alias_value_width_is_capped_at_eight() {
        let (s, out) = errors_of("[emoji_aliases]\nok = \"12345678\"\nwide = \"123456789\"\n");
        assert_eq!(s.emoji_aliases.len(), 1, "the valid one is kept");
        assert_eq!(s.emoji_aliases["ok"], "12345678");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].key.as_deref(), Some("emoji_aliases.wide"));
        assert_eq!(out[0].message, "must be at most 8 columns wide, got 9");
        assert_eq!(out[0].pos.map(|(line, _)| line), Some(3));
    }

    #[test]
    fn emoji_alias_value_with_control_characters_is_an_error() {
        let (s, out) = errors_of("[emoji_aliases]\nbad = \"a\\u001bb\"\n");
        assert!(s.emoji_aliases.is_empty());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("emoji_aliases.bad"));
        assert!(out[0].message.contains("control"), "{}", out[0].message);
    }

    #[test]
    fn emoji_alias_name_with_colons_is_an_error() {
        let (s, out) = errors_of("[emoji_aliases]\n\":partyparrot:\" = \"🦜\"\n");
        assert!(s.emoji_aliases.is_empty());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Error);
        assert_eq!(out[0].key.as_deref(), Some("emoji_aliases.:partyparrot:"));
        assert!(
            out[0].message.contains("without colons"),
            "{}",
            out[0].message
        );
    }

    #[test]
    fn emoji_alias_bad_names_and_types_are_errors() {
        let (s, out) = errors_of("[emoji_aliases]\n\"\" = \"x\"\n\"a b\" = \"x\"\nnum = 3\n");
        assert!(s.emoji_aliases.is_empty());
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(out[0].message.contains("empty"));
        assert!(out[1].message.contains("spaces"));
        assert!(out[2].message.contains("expected a string"));

        let (s, out) = errors_of("emoji_aliases = \"x\"\n");
        assert!(s.emoji_aliases.is_empty());
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("expected a table"));
    }

    #[test]
    fn emoji_alias_long_names_are_truncated_in_the_error() {
        let long = "n".repeat(100);
        let (_, out) = errors_of(&format!("[emoji_aliases]\n\"{long}:\" = \"x\"\n"));
        assert_eq!(out.len(), 1);
        let key = out[0].key.as_deref().unwrap();
        assert_eq!(key, format!("emoji_aliases.{}", "n".repeat(32)));
    }

    #[test]
    fn message_layout_values() {
        let mut out = Vec::new();
        let s = parse(Some("message_layout = \"compact\"\n"), &mut out);
        assert_eq!(s.message_layout, MessageLayout::Compact);
        let s = parse(Some("message_layout = \"two_line\"\n"), &mut out);
        assert_eq!(s.message_layout, MessageLayout::TwoLine);
        assert!(out.is_empty());

        let s = parse(Some("message_layout = \"wide\"\n"), &mut out);
        assert_eq!(s.message_layout, MessageLayout::Compact);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, ConfigFile::Config);
        assert_eq!(out[0].pos.map(|(line, _)| line), Some(1));
        assert_eq!(out[0].key.as_deref(), Some("message_layout"));
        assert_eq!(out[0].severity, Severity::Error);

        out.clear();
        parse(Some("message_layout = 3\n"), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("expected a string"));
    }

    #[test]
    fn follow_on_send_values() {
        let (s, out) = errors_of("");
        assert_eq!(s.follow_on_send, FollowOnSend::Always);
        assert!(out.is_empty());
        let (s, out) = errors_of("follow_on_send = \"always\"\n");
        assert_eq!(s.follow_on_send, FollowOnSend::Always);
        assert!(out.is_empty());
        let (s, out) = errors_of("follow_on_send = \"if_at_bottom\"\n");
        assert_eq!(s.follow_on_send, FollowOnSend::IfAtBottom);
        assert!(out.is_empty(), "known key, no warning: {out:?}");

        let (s, out) = errors_of("follow_on_send = \"sometimes\"\n");
        assert_eq!(s.follow_on_send, FollowOnSend::Always);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, ConfigFile::Config);
        assert_eq!(out[0].pos.map(|(line, _)| line), Some(1));
        assert_eq!(out[0].key.as_deref(), Some("follow_on_send"));
        assert_eq!(out[0].severity, Severity::Error);
        assert!(out[0].message.contains("\"always\""), "{}", out[0].message);
        assert!(
            out[0].message.contains("\"if_at_bottom\""),
            "{}",
            out[0].message
        );

        let (s, out) = errors_of("follow_on_send = 1\n");
        assert_eq!(s.follow_on_send, FollowOnSend::Always);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("expected a string"));
    }

    fn errors_of(src: &str) -> (Settings, Vec<Problem>) {
        let mut out = Vec::new();
        let settings = parse(Some(src), &mut out);
        (settings, out)
    }

    #[test]
    fn defaults_match_phase1() {
        let defaults = Settings::default();
        assert_eq!(defaults.keymap_preset, KeymapPreset::Vim);
        assert_eq!(defaults.sidebar_width, 28);
        assert_eq!(defaults.history_size, 50);
        assert_eq!(defaults.timestamp_format.as_str(), "%H:%M");
        assert_eq!(defaults.message_layout, MessageLayout::Compact);
        assert_eq!(defaults.key_timeout_ms, 1000);
        assert_eq!(defaults.mark_read_delay_ms, 3000);
        assert_eq!(defaults.message_grouping_seconds, 180);
        assert_eq!(defaults.follow_on_send, FollowOnSend::Always);
        assert!(defaults.inline_images);
        assert!(defaults.animate_images);
        assert_eq!(defaults.image_max_rows, 12);
        for src in ["", "# only a comment\n"] {
            let (settings, out) = errors_of(src);
            assert_eq!(settings, defaults);
            assert!(out.is_empty());
        }
    }

    #[test]
    fn bounds_inclusive() {
        // (key, lo, hi, accepted values, rejected values)
        type Case = (&'static str, i64, i64, &'static [i64], &'static [i64]);
        let cases: [Case; 6] = [
            ("sidebar_width", 10, 60, &[10, 60], &[9, 61]),
            ("history_size", 1, 1000, &[1, 1000], &[0, 1001]),
            ("key_timeout_ms", 100, 5000, &[100, 5000], &[99, 5001]),
            ("mark_read_delay_ms", 0, 60_000, &[0, 60_000], &[-1, 60_001]),
            ("message_grouping_seconds", 0, 3600, &[0, 3600], &[-1, 3601]),
            ("image_max_rows", 3, 40, &[3, 40], &[2, 41, 100]),
        ];
        for (key, lo, hi, good, bad) in cases {
            for n in good {
                let (_, out) = errors_of(&format!("{key} = {n}\n"));
                assert!(out.is_empty(), "{key} = {n}: {out:?}");
            }
            for n in bad {
                let (settings, out) = errors_of(&format!("{key} = {n}\n"));
                assert_eq!(settings, Settings::default(), "{key} = {n}");
                assert_eq!(out.len(), 1, "{key} = {n}");
                assert_eq!(
                    out[0].message,
                    format!("must be between {lo} and {hi}, got {n}")
                );
                assert_eq!(out[0].key.as_deref(), Some(key));
                assert_eq!(out[0].pos, Some((1, key.len() + 4)));
            }
        }
    }

    #[test]
    fn mark_read_delay_value_and_type() {
        assert_eq!(
            errors_of("mark_read_delay_ms = 5000\n")
                .0
                .mark_read_delay_ms,
            5000
        );
        assert_eq!(
            errors_of("mark_read_delay_ms = 0\n").0.mark_read_delay_ms,
            0
        );
        let (settings, out) = errors_of("mark_read_delay_ms = \"3s\"\n");
        assert_eq!(settings.mark_read_delay_ms, 3000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("mark_read_delay_ms"));
    }

    #[test]
    fn message_grouping_value_and_type() {
        let grouping = |src: &str| errors_of(src).0.message_grouping_seconds;
        assert_eq!(grouping("message_grouping_seconds = 120\n"), 120);
        assert_eq!(grouping("message_grouping_seconds = 0\n"), 0);
        let (settings, out) = errors_of("message_grouping_seconds = \"5m\"\n");
        assert_eq!(settings.message_grouping_seconds, 180);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("message_grouping_seconds"));
        let (_, out) = errors_of("message_grouping_seconds = 100000\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("message_grouping_seconds"));
    }

    #[test]
    fn animate_images_setting() {
        let (settings, out) = errors_of("");
        assert!(out.is_empty() && settings.animate_images);
        let (settings, out) = errors_of("animate_images = false\n");
        assert!(out.is_empty(), "{out:?}");
        assert!(!settings.animate_images);
        let (settings, out) = errors_of("animate_images = \"no\"\n");
        assert!(
            settings.animate_images,
            "an invalid value keeps the default"
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("animate_images"));
    }

    #[test]
    fn open_defaults_to_the_platform_opener() {
        use crate::data::AttachmentKind::*;
        let (settings, out) = errors_of("");
        assert!(out.is_empty());
        assert_eq!(settings.open, OpenSettings::default());
        for kind in [Image, Video, Audio] {
            assert_eq!(
                settings.open.command(kind),
                Some(vec![platform_opener().to_string()])
            );
        }
        assert_eq!(
            settings.open.command(Other),
            None,
            "other files are not opened"
        );
        let (settings, out) = errors_of("[open]\n");
        assert!(out.is_empty(), "known table, no warning: {out:?}");
        assert_eq!(settings.open, OpenSettings::default());
    }

    #[test]
    fn open_reads_each_key_and_falls_back_to_default() {
        use crate::data::AttachmentKind::*;
        let (settings, out) = errors_of(
            "[open]\nvideo = [\"mpv\", \"--force-window\"]\nimage = [\"feh\"]\ndefault = [\"xdg-open\"]\n",
        );
        assert!(out.is_empty(), "{out:?}");
        let cmd = |k| settings.open.command(k).unwrap();
        assert_eq!(cmd(Video), ["mpv", "--force-window"]);
        assert_eq!(cmd(Image), ["feh"]);
        assert_eq!(cmd(Audio), ["xdg-open"], "falls back to default");
        let (settings, out) = errors_of("[open]\naudio = [\"vlc\", \"--intf\", \"dummy\"]\n");
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(
            settings.open.command(Audio).unwrap(),
            ["vlc", "--intf", "dummy"]
        );
        assert_eq!(
            settings.open.command(Video).unwrap(),
            [platform_opener()],
            "no default set: the platform opener"
        );
    }

    #[test]
    fn open_rejects_an_empty_list_a_non_array_and_bad_entries() {
        use crate::data::AttachmentKind::*;
        let (settings, out) = errors_of("[open]\nvideo = []\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].key.as_deref(), Some("open.video"));
        assert!(out[0].message.contains("program is required"));
        assert_eq!(settings.open.video, None, "an invalid key stays unset");
        let (_, out) = errors_of("[open]\nvideo = \"mpv\"\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("open.video"));
        assert_eq!(out[0].message, "expected an array of strings, found string");
        let (_, out) = errors_of("[open]\nvideo = [\"mpv\", 3]\n");
        assert_eq!(out.len(), 1);
        assert!(
            out[0].message.contains("must be a string"),
            "{}",
            out[0].message
        );
        let (_, out) = errors_of("[open]\nvideo = [\"\", \"x\"]\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "the program name must not be empty");
        let (_, out) = errors_of("[open]\nvideo = [\"mpv\", \"a\\u0000b\"]\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "must not contain a NUL character");
        // The other keys are still read when one is wrong.
        let (settings, out) = errors_of("[open]\nvideo = []\naudio = [\"vlc\"]\n");
        assert_eq!(out.len(), 1);
        assert_eq!(settings.open.command(Audio).unwrap(), ["vlc"]);
        let (_, out) = errors_of("open = 3\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("open"));
        let (_, out) = errors_of("[open]\nmovie = [\"x\"]\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].key.as_deref(), Some("open.movie"));
        let _ = (Image, Other);
    }

    #[test]
    fn inline_image_settings() {
        let (settings, out) = errors_of("");
        assert!(out.is_empty() && settings.inline_images && settings.image_max_rows == 12);
        let (settings, out) = errors_of("inline_images = false\nimage_max_rows = 20\n");
        assert!(out.is_empty(), "{out:?}");
        assert!(!settings.inline_images);
        assert_eq!(settings.image_max_rows, 20);
        let (settings, out) = errors_of("inline_images = \"no\"\n");
        assert!(settings.inline_images, "an invalid value keeps the default");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("inline_images"));
        let (_, out) = errors_of("image_max_rows = 100\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "must be between 3 and 40, got 100");
    }

    #[test]
    fn keymap_preset_values() {
        assert_eq!(
            errors_of("keymap_preset = \"vim\"\n").0.keymap_preset,
            KeymapPreset::Vim
        );
        assert_eq!(
            errors_of("keymap_preset = \"vim-extras\"\n")
                .0
                .keymap_preset,
            KeymapPreset::VimExtras
        );
        let (_, out) = errors_of("keymap_preset = \"emacs\"\n");
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("\"vim\""));
        assert!(out[0].message.contains("\"vim-extras\""));
    }

    #[test]
    fn timestamp_format_validation() {
        for good in ["%H:%M", "%Y-%m-%d %H:%M"] {
            assert!(TimeFormat::parse(good).is_ok(), "{good}");
        }
        let too_wide = "%Y".repeat(21);
        for bad in ["%Q", "%", "", "%H%n%M", "%H%t%M", too_wide.as_str()] {
            assert!(TimeFormat::parse(bad).is_err(), "{bad:?}");
        }
        let (settings, out) = errors_of("timestamp_format = \"%Q\"\n");
        assert_eq!(settings.timestamp_format, TimeFormat::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("timestamp_format"));
        let (settings, out) = errors_of("timestamp_format = \"%H:%M:%S\"\n");
        assert!(out.is_empty());
        assert_eq!(settings.timestamp_format.as_str(), "%H:%M:%S");
    }

    #[test]
    fn type_errors() {
        let (_, out) = errors_of("sidebar_width = \"40\"\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "expected an integer, found string");
        let (_, out) = errors_of("sidebar_width = 40.0\n");
        assert_eq!(out.len(), 1);
        assert!(out[0].message.starts_with("expected an integer"));
        let (settings, out) = errors_of("history_size = 200\n");
        assert!(out.is_empty());
        assert_eq!(settings.history_size, 200);
    }
}
