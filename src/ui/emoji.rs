//! Shortcode to glyph: how a reaction's name is drawn.
//!
//! Rocket.Chat sends a reaction as a shortcode (`:+1:`, `:thumbsup_tone2:`, `:partyparrot:`).
//! `resolve` turns the name (without the colons) into the text to draw, trying in this order:
//!
//! 1. the user's `[emoji_aliases]` entry for the full name;
//! 2. the unicode emoji for the name (the `emojis` crate, GitHub shortcodes), then a small map of
//!    Rocket.Chat's older emojione-only names, then the names the server uses where they differ
//!    from the crate's (`flag_af`, measured by `dev/probe_react_names.py`);
//! 3. the same lookup for the name without a `_tone1`..`_tone5` suffix, with that skin tone
//!    applied (the plain glyph when the emoji has no skin-tone variants);
//! 4. the raw `:name:`, which is also how a custom server emoji appears: those are images and a
//!    terminal cannot draw them.
//!
//! `lookup` is those steps without the last one, for a caller that must tell a known name from an
//! unknown one (the text of a message); `resolve` adds the raw fallback (a reaction).
//!
//! Pure: no IO. The result is always safe to draw, because the raw fallback is sanitized and a
//! glyph or an alias (validated when the config loads) holds no control characters.

use crate::ui::emoji_names::crate_name_of;
use crate::ui::sanitize::sanitize_line;
use emojis::SkinTone;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

/// No aliases: for a caller (a test, a context without config) that has none to give.
pub static NO_ALIASES: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

/// Rocket.Chat's emojione-era names that GitHub's set spells differently.
const LEGACY: &[(&str, &str)] = &[
    ("slight_smile", "slightly_smiling_face"),
    ("simple_smile", "slightly_smiling_face"),
    ("slight_frown", "slightly_frowning_face"),
    ("upside_down", "upside_down_face"),
    ("white_frowning_face", "frowning_face"),
];

/// The emoji a name stands for in the unicode table, the legacy map or the server's own names.
fn find(name: &str) -> Option<&'static emojis::Emoji> {
    emojis::get_by_shortcode(name)
        .or_else(|| {
            LEGACY
                .iter()
                .find(|(old, _)| *old == name)
                .and_then(|(_, new)| emojis::get_by_shortcode(new))
        })
        .or_else(|| crate_name_of(name).and_then(emojis::get_by_shortcode))
}

/// Split `base_toneN` (N in 1..=5) into the base name and the skin tone.
fn split_tone(name: &str) -> Option<(&str, SkinTone)> {
    let (base, digit) = name.rsplit_once("_tone")?;
    let tone = match digit {
        "1" => SkinTone::Light,
        "2" => SkinTone::MediumLight,
        "3" => SkinTone::Medium,
        "4" => SkinTone::MediumDark,
        "5" => SkinTone::Dark,
        _ => return None,
    };
    (!base.is_empty()).then_some((base, tone))
}

/// The text to draw for `name` (no colons), or `None` when nothing knows it. See the module docs
/// for the order.
pub fn lookup(name: &str, aliases: &HashMap<String, String>) -> Option<String> {
    if let Some(alias) = aliases.get(name) {
        return Some(alias.clone());
    }
    if let Some(emoji) = find(name) {
        return Some(emoji.as_str().to_string());
    }
    if let Some((base, tone)) = split_tone(name)
        && let Some(emoji) = find(base)
    {
        return Some(
            emoji
                .with_skin_tone(tone)
                .unwrap_or(emoji)
                .as_str()
                .to_string(),
        );
    }
    None
}

/// The text to draw for the reaction `name` (no colons): what `lookup` finds, else the raw
/// `:name:`, sanitized.
pub fn resolve(name: &str, aliases: &HashMap<String, String>) -> String {
    lookup(name, aliases).unwrap_or_else(|| format!(":{}:", sanitize_line(name)))
}

/// Longest name between the colons of a shortcode in message text. Real names are far shorter;
/// the cap only keeps the scan from looking far ahead on a stray colon.
const MAX_SHORTCODE: usize = 64;

/// True for a character a shortcode name may hold.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-')
}

/// The text of a message with its emoji shortcodes drawn as emoji.
///
/// The rules are the server's own (checked against the parsed form it sends): a shortcode is
/// `:name:` with a name of letters, digits, `_`, `+` and `-`; the colon that opens it is not
/// preceded by a letter or digit and the one that closes it is not followed by one, so `10:100:20`
/// stays text while `(:tada:)` and `:tada::+1:` work; and nothing between backticks is touched,
/// which covers inline code and fenced blocks alike (three backticks toggle three times). A name
/// that `lookup` does not know is left as written and the scan goes on from the next character.
///
/// Text without a colon is returned as it is, without allocating. Pure: the stored text of the
/// message is never changed, only what is drawn.
pub fn draw_shortcodes<'a>(text: &'a str, aliases: &HashMap<String, String>) -> Cow<'a, str> {
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            in_code = !in_code;
        } else if c == ':' && !in_code && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && end - start <= MAX_SHORTCODE && is_name_char(chars[end]) {
                end += 1;
            }
            let closed = end < chars.len() && chars[end] == ':' && end > start;
            let free = end + 1 >= chars.len() || !chars[end + 1].is_alphanumeric();
            if closed && free {
                let name: String = chars[start..end].iter().collect();
                if let Some(glyph) = lookup(&name, aliases) {
                    out.push_str(&glyph);
                    changed = true;
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> HashMap<String, String> {
        HashMap::new()
    }

    fn aliases(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn known_names_become_glyphs() {
        assert_eq!(resolve("+1", &none()), "👍");
        assert_eq!(resolve("thumbsup", &none()), "👍");
        assert_eq!(resolve("tada", &none()), "🎉");
        assert_eq!(resolve("white_check_mark", &none()), "✅");
    }

    #[test]
    fn every_skin_tone_is_applied() {
        let tones: Vec<String> = (1..=5)
            .map(|n| resolve(&format!("thumbsup_tone{n}"), &none()))
            .collect();
        assert_eq!(tones, ["👍🏻", "👍🏼", "👍🏽", "👍🏾", "👍🏿"]);
        // The base name does not have to be the first spelling: `+1` takes a tone too.
        assert_eq!(resolve("+1_tone5", &none()), "👍🏿");
    }

    #[test]
    fn a_base_without_skin_tones_gives_the_plain_glyph() {
        assert_eq!(resolve("tada_tone2", &none()), "🎉");
    }

    #[test]
    fn legacy_emojione_names_resolve() {
        assert_eq!(resolve("slight_smile", &none()), "🙂");
        assert_eq!(resolve("simple_smile", &none()), "🙂");
        assert_eq!(resolve("slight_frown", &none()), "🙁");
        assert_eq!(resolve("upside_down", &none()), "🙃");
        assert_eq!(resolve("white_frowning_face", &none()), "☹\u{fe0f}");
    }

    #[test]
    fn unknown_and_custom_names_stay_raw() {
        assert_eq!(resolve("partyparrot", &none()), ":partyparrot:");
        assert_eq!(
            resolve("rt_no_such_emoji_zz", &none()),
            ":rt_no_such_emoji_zz:"
        );
        // An unknown base with a tone suffix stays whole, and a bad tone number is not a tone.
        assert_eq!(resolve("nothing_tone2", &none()), ":nothing_tone2:");
        assert_eq!(resolve("thumbsup_tone9", &none()), ":thumbsup_tone9:");
        assert_eq!(resolve("_tone2", &none()), ":_tone2:");
    }

    #[test]
    fn an_alias_covers_a_custom_name() {
        let a = aliases(&[("partyparrot", "🦜")]);
        assert_eq!(resolve("partyparrot", &a), "🦜");
        // Other names are not affected.
        assert_eq!(resolve("tada", &a), "🎉");
    }

    #[test]
    fn an_alias_overrides_the_unicode_table() {
        let a = aliases(&[("+1", "yes")]);
        assert_eq!(resolve("+1", &a), "yes");
        // Only the exact name: `thumbsup` is another name for the same emoji.
        assert_eq!(resolve("thumbsup", &a), "👍");
    }

    #[test]
    fn an_alias_for_a_tone_name_wins_over_the_tone() {
        let a = aliases(&[("thumbsup_tone2", "T")]);
        assert_eq!(resolve("thumbsup_tone2", &a), "T");
        assert_eq!(resolve("thumbsup_tone3", &a), "👍🏽");
    }

    #[test]
    fn the_raw_fallback_is_sanitized() {
        let shown = resolve("bad\u{1b}[31mname\u{7}", &none());
        assert!(!shown.chars().any(|c| c.is_control()), "{shown:?}");
        assert!(shown.starts_with(':') && shown.ends_with(':'), "{shown:?}");
    }

    // ---- lookup and the server's names

    #[test]
    fn lookup_reports_an_unknown_name_instead_of_a_raw_one() {
        assert_eq!(lookup("tada", &none()).as_deref(), Some("🎉"));
        assert_eq!(lookup("partyparrot", &none()), None);
        assert_eq!(lookup("thumbsup_tone9", &none()), None);
        assert_eq!(lookup("nothing_tone2", &none()), None);
        // `resolve` is `lookup` with the raw name as the fallback.
        assert_eq!(resolve("partyparrot", &none()), ":partyparrot:");
        assert_eq!(resolve("tada", &none()), "🎉");
    }

    #[test]
    fn the_names_the_server_uses_resolve_like_the_crates() {
        assert_eq!(lookup("flag_af", &none()).as_deref(), Some("🇦🇫"));
        assert_eq!(resolve("flag_af", &none()), "🇦🇫");
        assert_eq!(
            lookup("man_running", &none()).as_deref(),
            lookup("running_man", &none()).as_deref()
        );
        assert!(lookup("man_running", &none()).is_some());
    }

    #[test]
    fn every_name_the_server_uses_draws_the_same_glyph_as_the_crates_name() {
        let mut checked = 0;
        for (crate_name, server_name) in crate::ui::emoji_names::renamed() {
            let by_crate = lookup(crate_name, &none());
            let by_server = lookup(server_name, &none());
            assert!(by_crate.is_some(), "{crate_name}");
            assert_eq!(by_server, by_crate, "{server_name} against {crate_name}");
            checked += 1;
        }
        assert!(checked > 0, "the table is not empty");
    }

    #[test]
    fn the_layers_keep_their_order() {
        // An alias beats the crate, the server's names and a tone suffix.
        let a = aliases(&[("flag_af", "AF"), ("tada", "T"), ("tada_tone2", "T2")]);
        assert_eq!(lookup("flag_af", &a).as_deref(), Some("AF"));
        assert_eq!(lookup("tada", &a).as_deref(), Some("T"));
        assert_eq!(lookup("tada_tone2", &a).as_deref(), Some("T2"));
        // A crate name beats the legacy map and the server's names (none of the three collide
        // today, so check the order through a name only the table knows).
        assert_eq!(lookup("afghanistan", &none()).as_deref(), Some("🇦🇫"));
    }

    // ---- shortcodes in message text

    fn draw(text: &str) -> String {
        draw_shortcodes(text, &none()).into_owned()
    }

    #[test]
    fn a_known_shortcode_becomes_the_emoji() {
        assert_eq!(draw("ship it :tada:"), "ship it 🎉");
        assert_eq!(draw(":+1: from me"), "👍 from me");
        assert_eq!(draw("both :tada: and :rocket: here"), "both 🎉 and 🚀 here");
        assert_eq!(draw(":thumbsup_tone2:"), "👍🏼");
        assert_eq!(draw(":flag_af:"), "🇦🇫");
        assert_eq!(draw(":slight_smile:"), "🙂");
    }

    #[test]
    fn an_alias_applies_in_message_text() {
        let a = aliases(&[("partyparrot", "🦜")]);
        assert_eq!(draw_shortcodes("go :partyparrot:", &a), "go 🦜");
    }

    #[test]
    fn an_unknown_shortcode_and_a_character_emoji_stay_as_written() {
        assert_eq!(draw(":rt_not_an_emoji: ok"), ":rt_not_an_emoji: ok");
        assert_eq!(draw("party 🎉 now"), "party 🎉 now");
        assert_eq!(draw("no colon at all"), "no colon at all");
        assert_eq!(draw(""), "");
        assert_eq!(draw(":"), ":");
        assert_eq!(draw("::"), "::");
        assert_eq!(draw(":::"), ":::");
    }

    #[test]
    fn without_a_colon_nothing_is_allocated() {
        assert!(matches!(
            draw_shortcodes("plain words", &none()),
            Cow::Borrowed(_)
        ));
        // And when a colon changes nothing, the original is returned too.
        assert!(matches!(
            draw_shortcodes("note: see below", &none()),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            draw_shortcodes(":nothing_here:", &none()),
            Cow::Borrowed(_)
        ));
        assert!(matches!(draw_shortcodes(":tada:", &none()), Cow::Owned(_)));
    }

    #[test]
    fn a_shortcode_needs_a_boundary_on_both_sides() {
        // The server's own parser leaves these as text.
        assert_eq!(draw("at 10:100:20"), "at 10:100:20");
        assert_eq!(draw("a:tada:"), "a:tada:");
        assert_eq!(draw(":tada:b"), ":tada:b");
        assert_eq!(draw("x:tada:y"), "x:tada:y");
        assert_eq!(draw("9:30:00"), "9:30:00");
        // Punctuation and the edges of the text are boundaries.
        assert_eq!(draw("(:tada:)"), "(🎉)");
        assert_eq!(draw(":tada:,"), "🎉,");
        assert_eq!(draw("\":tada:\""), "\"🎉\"");
        assert_eq!(draw("wow!:tada:"), "wow!🎉");
        assert_eq!(draw(":tada:"), "🎉");
    }

    #[test]
    fn adjacent_shortcodes_all_become_emoji() {
        assert_eq!(draw(":tada::+1:"), "🎉👍");
        assert_eq!(draw(":tada: :tada:"), "🎉 🎉");
        assert_eq!(draw(":tada::tada::tada:"), "🎉🎉🎉");
    }

    #[test]
    fn the_name_is_letters_digits_and_a_few_symbols_only() {
        assert_eq!(draw(":not a code:"), ":not a code:");
        assert_eq!(draw(":ta da:"), ":ta da:");
        assert_eq!(draw(":ta.da:"), ":ta.da:");
        assert_eq!(draw(":100:"), "💯");
        assert_eq!(draw(":-1:"), "👎");
        // A failed candidate does not hide the one that follows it.
        assert_eq!(draw(":a:tada:"), ":a:tada:");
        assert_eq!(draw(":a :tada:"), ":a 🎉");
        assert_eq!(draw("::tada:"), ":🎉");
        // Far longer than any name: left alone, and fast.
        let long = format!(":{}:", "a".repeat(500));
        assert_eq!(draw(&long), long);
    }

    #[test]
    fn code_is_left_alone() {
        assert_eq!(draw("use `:smile:` here :tada:"), "use `:smile:` here 🎉");
        assert_eq!(draw("`:tada:`"), "`:tada:`");
        assert_eq!(draw(":tada: `x` :tada:"), "🎉 `x` 🎉");
        // A fenced block, with its fences, is code from the opening fence to the closing one.
        let fenced = "before :tada:\n```\nlet a = :tada:;\n```\nafter :tada:";
        assert_eq!(
            draw(fenced),
            "before 🎉\n```\nlet a = :tada:;\n```\nafter 🎉"
        );
        // An unbalanced backtick leaves the rest as written: the safe direction.
        assert_eq!(draw("odd ` :tada:"), "odd ` :tada:");
    }

    #[test]
    fn hostile_text_around_a_shortcode_passes_through_for_the_wrapper_to_sanitize() {
        // The scanner changes nothing but shortcodes; control characters are the wrapper's job.
        let drawn = draw("\u{1b}[31m :tada: \u{7}");
        assert!(drawn.contains('🎉'));
        assert!(drawn.contains('\u{1b}') && drawn.contains('\u{7}'));
    }
}
