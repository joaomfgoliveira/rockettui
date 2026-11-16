//! `config.toml`: the scalar settings of D-12 and their D-13 validation.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use crate::ui::wrap::MessageLayout;
use chrono::format::{Item, StrftimeItems};
use chrono::{TimeZone, Utc};
use std::fmt::Write as _;
use unicode_width::UnicodeWidthStr;

pub const DEFAULT_HISTORY_SIZE: u16 = 50;
pub const DEFAULT_KEY_TIMEOUT_MS: u64 = 1000;
/// Inclusive bounds of the integer settings (D-13; the key timeout is A1).
pub const SIDEBAR_WIDTH_RANGE: (i64, i64) = (10, 60);
pub const HISTORY_SIZE_RANGE: (i64, i64) = (1, 1000);
pub const KEY_TIMEOUT_MS_RANGE: (i64, i64) = (100, 5000);
/// Widest a rendered timestamp may be, in columns (A7).
pub const MAX_TIMESTAMP_WIDTH: usize = 40;
/// 2026-01-31 23:59:59 UTC: the instant a format is test-rendered with (month, day and hour all
/// wide, so a format that overflows with real data overflows here too).
const SAMPLE_TS: i64 = 1_769_903_999;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeymapPreset {
    #[default]
    Vim,
    VimExtras,
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
    if let Some(value) = table.get("key_timeout_ms") {
        let (lo, hi) = KEY_TIMEOUT_MS_RANGE;
        if let Some(n) = reader.int_in(value, "key_timeout_ms", lo, hi) {
            settings.key_timeout_ms = u64::try_from(n).unwrap_or(settings.key_timeout_ms);
        }
    }
    // D-03: unknown keys and tables only warn. `server_url` is one of them until Phase 3 (D-12).
    reader.warn_unknown(table, KNOWN_KEYS, "");
    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diag::Severity;

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
        let cases: [Case; 3] = [
            ("sidebar_width", 10, 60, &[10, 60], &[9, 61]),
            ("history_size", 1, 1000, &[1, 1000], &[0, 1001]),
            ("key_timeout_ms", 100, 5000, &[100, 5000], &[99, 5001]),
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
