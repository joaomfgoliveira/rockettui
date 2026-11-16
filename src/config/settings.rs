//! `config.toml`: the scalar settings of D-12.

use super::diag::{ConfigFile, Problem, Reader, parse_table};
use crate::ui::wrap::MessageLayout;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeymapPreset {
    #[default]
    Vim,
    VimExtras,
}

/// A strftime pattern for message times. Task 2 adds the validating constructor.
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
            history_size: 50,
            timestamp_format: TimeFormat::default(),
            message_layout: MessageLayout::Compact,
            key_timeout_ms: 1000,
        }
    }
}

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
    if let Some(value) = root.get_ref().get("message_layout")
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
}
