//! The shortcode to send for a unicode emoji.
//!
//! The picker finds unicode emoji with the `emojis` crate (GitHub's shortcodes), but the server's
//! standard set is an older, different one, and it answers 400 to a name it does not know. A probe
//! (`dev/probe_react_names.py`) asked a real server which shortcode of each emoji it accepts and
//! generated `data`: only the exceptions are stored. An emoji is identified by its first shortcode.
//!
//! - an emoji not in `SEND_AS` is sent under its first shortcode;
//! - one in `SEND_AS` is sent under the other name the server accepted (one of the crate's other
//!   shortcodes, or a name from the server's own emoji data, such as `flag_af`);
//! - one in `UNSENDABLE` has no accepted shortcode, so the picker does not offer it.
//!
//! The table is a measurement of the server version the probe ran against.

mod data;
mod tone_data;

use data::{SEND_AS, UNSENDABLE};
use std::collections::HashMap;
use std::sync::LazyLock;
use tone_data::TONE_REFUSED;

/// The other direction: a name the server uses for an emoji, to the first shortcode the crate
/// knows it by. Built once from `SEND_AS`.
static FROM_SERVER: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| SEND_AS.iter().map(|(from, to)| (*to, *from)).collect());

/// The crate's first shortcode for the emoji the server calls `server_name`, when that name is one
/// the server uses in place of the crate's (a flag is `flag_af` to the server and `afghanistan` to
/// the crate). `None` for any other name.
pub fn crate_name_of(server_name: &str) -> Option<&'static str> {
    FROM_SERVER.get(server_name).copied()
}

/// Every `(crate name, server name)` pair of the table, for tests.
#[cfg(test)]
pub fn renamed() -> impl Iterator<Item = (&'static str, &'static str)> {
    SEND_AS.iter().copied()
}

/// The shortcode to send for the emoji whose first shortcode is `first`, or `None` when the server
/// accepts none of its shortcodes.
pub fn send_name(first: &'static str) -> Option<&'static str> {
    if UNSENDABLE.contains(&first) {
        return None;
    }
    Some(
        SEND_AS
            .iter()
            .find(|(from, _)| *from == first)
            .map_or(first, |(_, to)| *to),
    )
}

/// True when the server accepts `<name>_tone1..5` for the emoji whose first shortcode is `first`.
/// `tone_data` (a second probe, `--tones`) lists the exceptions; an emoji without tones is not one.
pub fn tone_sendable(first: &str) -> bool {
    !TONE_REFUSED.contains(&first)
}

/// The name to send for the emoji whose first shortcode is `first` in skin tone `tone` (1 to 5):
/// the name it is sent under plus `_tone<N>`. `None` when the emoji is not offered at all, when the
/// tone is not 1 to 5 or when the server refuses its toned names.
pub fn tone_name(first: &'static str, tone: u8) -> Option<String> {
    tone_name_unless(first, tone, TONE_REFUSED)
}

/// `tone_name` against a given list of refused emoji, so a test can name one.
fn tone_name_unless(first: &'static str, tone: u8, refused: &[&str]) -> Option<String> {
    if !(1..=5).contains(&tone) || refused.contains(&first) {
        return None;
    }
    send_name(first).map(|name| format!("{name}_tone{tone}"))
}

/// `name` with a `_tone<N>` ending cut off, when `name` has one: what a toned name is a
/// variant of. Returns `name` itself otherwise.
pub fn strip_tone(name: &str) -> &str {
    match name.rsplit_once("_tone") {
        Some((base, n)) if !base.is_empty() && matches!(n, "1" | "2" | "3" | "4" | "5") => base,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emoji_not_in_the_table_is_sent_under_its_first_shortcode() {
        assert_eq!(send_name("tada"), Some("tada"));
    }

    #[test]
    fn every_table_entry_names_an_emoji_the_crate_knows() {
        for (from, to) in SEND_AS {
            let emoji = emojis::get_by_shortcode(from)
                .unwrap_or_else(|| panic!("SEND_AS names an unknown emoji: {from}"));
            assert_eq!(
                emoji.shortcodes().next(),
                Some(*from),
                "{from} is not a first shortcode"
            );
            // The name to send may come from the server's own emoji data, so it need not be one of
            // the crate's shortcodes, but it must be a plain name and a different one.
            assert!(
                !to.is_empty()
                    && to
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+')),
                "{from} -> {to:?} is not a plain shortcode"
            );
            assert_ne!(from, to);
        }
        for first in UNSENDABLE {
            let emoji = emojis::get_by_shortcode(first)
                .unwrap_or_else(|| panic!("UNSENDABLE names an unknown emoji: {first}"));
            assert_eq!(emoji.shortcodes().next(), Some(*first));
        }
        // Nothing is both renamed and dropped, and no emoji is listed twice.
        for (from, _) in SEND_AS {
            assert!(!UNSENDABLE.contains(from), "{from}");
        }
        let mut seen = std::collections::HashSet::new();
        assert!(SEND_AS.iter().all(|(from, _)| seen.insert(*from)));
    }

    #[test]
    fn a_renamed_emoji_is_sent_under_the_name_the_server_accepted() {
        assert_eq!(send_name("afghanistan"), Some("flag_af"));
        assert_eq!(send_name("hand"), Some("raised_hand"));
    }

    #[test]
    fn a_toned_name_is_the_sent_name_plus_the_tone() {
        assert_eq!(tone_name("+1", 2).as_deref(), Some("+1_tone2"));
        assert_eq!(tone_name("+1", 0), None);
        assert_eq!(tone_name("+1", 6), None);
    }

    #[test]
    fn a_toned_name_follows_the_name_the_server_wants() {
        // `hand` is sent as `raised_hand`, so its toned name is built from that.
        assert_eq!(tone_name("hand", 3).as_deref(), Some("raised_hand_tone3"));
    }

    #[test]
    fn an_emoji_whose_toned_name_the_server_refuses_has_no_toned_name() {
        assert_eq!(tone_name_unless("+1", 2, &["+1"]), None);
        assert_eq!(
            tone_name_unless("+1", 2, &["wave"]).as_deref(),
            Some("+1_tone2")
        );
        // Nothing in the generated list is an unknown emoji or one listed twice.
        let mut seen = std::collections::HashSet::new();
        for first in TONE_REFUSED {
            let emoji = emojis::get_by_shortcode(first)
                .unwrap_or_else(|| panic!("TONE_REFUSED names an unknown emoji: {first}"));
            assert!(
                emoji.with_skin_tone(emojis::SkinTone::Light).is_some(),
                "{first}"
            );
            assert!(seen.insert(*first), "{first} twice");
            assert!(!tone_sendable(first));
        }
    }

    #[test]
    fn a_tone_ending_is_cut_off_only_when_it_is_one() {
        assert_eq!(strip_tone("+1_tone2"), "+1");
        assert_eq!(strip_tone("raised_hand_tone5"), "raised_hand");
        assert_eq!(strip_tone("+1_tone6"), "+1_tone6");
        assert_eq!(strip_tone("_tone2"), "_tone2");
        assert_eq!(strip_tone("tada"), "tada");
    }

    /// Not a test: writes `first<TAB>sent name` for every emoji with skin tones to the file named by
    /// `EMOJI_TONES`, for `dev/probe_react_names.py --tones`. Run with
    /// `EMOJI_TONES=/tmp/tones.tsv cargo test --lib dump_toned_names -- --ignored`.
    #[test]
    #[ignore]
    fn dump_toned_names() {
        let path = std::env::var("EMOJI_TONES").expect("EMOJI_TONES names the output file");
        let mut out = String::new();
        for emoji in emojis::iter() {
            let Some(first) = emoji.shortcodes().next() else {
                continue;
            };
            let Some(sent) = send_name(first) else {
                continue;
            };
            if emoji.with_skin_tone(emojis::SkinTone::Light).is_some() {
                out.push_str(&format!("{first}\t{sent}\n"));
            }
        }
        std::fs::write(path, out).expect("write the dump");
    }

    /// Not a test: writes `glyph<TAB>first<TAB>second<TAB>..` for every emoji with a shortcode to the
    /// file named by `EMOJI_DUMP`, for `dev/probe_react_names.py`. Run with
    /// `EMOJI_DUMP=/tmp/emoji.tsv cargo test --lib dump_crate_shortcodes -- --ignored`.
    #[test]
    #[ignore]
    fn dump_crate_shortcodes() {
        let path = std::env::var("EMOJI_DUMP").expect("EMOJI_DUMP names the output file");
        let mut out = String::new();
        for emoji in emojis::iter() {
            let codes: Vec<&str> = emoji.shortcodes().collect();
            if !codes.is_empty() {
                out.push_str(emoji.as_str());
                out.push('\t');
                out.push_str(&codes.join("\t"));
                out.push('\n');
            }
        }
        std::fs::write(path, out).expect("write the dump");
    }
}
