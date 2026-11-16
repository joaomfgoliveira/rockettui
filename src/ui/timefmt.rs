//! Message time formatting: a validated strftime pattern applied to a Unix timestamp in an
//! injected zone. The reducer never reads a clock or a time zone; the zone lives on `App`, so
//! tests and snapshots pick UTC and the binary picks the system local zone.

use crate::config::settings::TimeFormat;
use crate::ui::sanitize::sanitize_line;
use chrono::format::StrftimeItems;
use chrono::{Local, TimeZone, Utc};
use std::fmt::Write as _;

/// What an unrepresentable timestamp or a failed render shows (A25).
pub const OUT_OF_RANGE: &str = "?";

/// The time zone message times are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zone {
    /// UTC; the default of `App::new` and `App::with_config`, so tests do not depend on the machine.
    #[default]
    Utc,
    /// The system local zone; the running binary sets this once in `run`.
    Local,
}

/// `ts` (seconds since the Unix epoch) formatted with `format` in `zone`.
///
/// Never panics: a timestamp chrono cannot represent, or a formatter error, gives `?`. The result
/// is sanitized, so the string that is measured is the string that is drawn (T-02-08).
pub fn format_ts(format: &TimeFormat, ts: i64, zone: Zone) -> String {
    let Some(utc) = Utc.timestamp_opt(ts, 0).single() else {
        return OUT_OF_RANGE.to_string();
    };
    let items = StrftimeItems::new(format.as_str());
    let mut text = String::new();
    // `write!` returns the formatter error; `to_string` on a bad format would panic.
    let written = match zone {
        Zone::Utc => write!(text, "{}", utc.format_with_items(items)),
        Zone::Local => write!(
            text,
            "{}",
            utc.with_timezone(&Local).format_with_items(items)
        ),
    };
    if written.is_err() {
        return OUT_OF_RANGE.to_string();
    }
    sanitize_line(&text).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(pattern: &str) -> TimeFormat {
        TimeFormat::parse(pattern).expect("valid test pattern")
    }

    #[test]
    fn default_matches_phase1() {
        let f = TimeFormat::default();
        assert_eq!(format_ts(&f, 0, Zone::Utc), "00:00");
        assert_eq!(format_ts(&f, 13 * 3600 + 5 * 60, Zone::Utc), "13:05");
        assert_eq!(format_ts(&f, -60, Zone::Utc), "23:59");
    }

    #[test]
    fn custom_format() {
        assert_eq!(
            format_ts(&fmt("%Y-%m-%d %H:%M"), 1_794_614_400, Zone::Utc),
            "2026-11-14 00:00"
        );
    }

    #[test]
    fn out_of_range_is_placeholder() {
        let f = TimeFormat::default();
        assert_eq!(format_ts(&f, i64::MAX, Zone::Utc), "?");
        assert_eq!(format_ts(&f, i64::MIN, Zone::Utc), "?");
        assert_eq!(format_ts(&f, i64::MAX, Zone::Local), "?");
    }

    #[test]
    fn local_zone_formats() {
        let out = format_ts(&TimeFormat::default(), 1_794_614_400, Zone::Local);
        let chars: Vec<char> = out.chars().collect();
        assert_eq!(chars.len(), 5, "{out:?}");
        assert!(chars[0].is_ascii_digit() && chars[1].is_ascii_digit());
        assert_eq!(chars[2], ':');
        assert!(chars[3].is_ascii_digit() && chars[4].is_ascii_digit());
    }
}
