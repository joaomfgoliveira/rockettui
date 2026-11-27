//! Mentions and channel references in message text: which words are one, and how they are drawn.
//!
//! `tokens` finds `@name`, `@here`, `@all` and `#name` in already-sanitized text. The server's own
//! lists decide (`Refs`: a name must be in `mentions` or `channels`), except `@here` and `@all`,
//! which are matched literally. A message without lists (older history, other servers) is matched
//! by shape: `@` or `#` and a name, and only when the sigil does not directly follow a letter or
//! digit, so `bob@example.com` stays plain. Text between backticks is never a token.
//!
//! In a message the tokens go through wrapping as markers, like the custom emoji cells (see
//! `ui::emoji`): `mark` writes a start marker before and an end marker after each token, the text
//! is wrapped, and `strip` takes the markers out of each row again, giving the clean row and the
//! byte ranges to style, with the open token carried from one row to the next. The markers are
//! zero-width format characters, so they change no measurement, and `sanitize` removes them from
//! server text so a server cannot forge one.
//!
//! Pure: no IO. The text itself is never changed, only what is drawn.

use crate::config::Theme;
use crate::data::Refs;
use ratatui::style::Style;
use std::borrow::Cow;
use std::ops::Range;

/// What a token refers to; each kind has its own theme element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Another user's `@name` (`messages.mention`).
    User,
    /// The user's own `@name` (`messages.mention_self`).
    Own,
    /// `@here` or `@all` (`messages.mention_group`).
    Group,
    /// `#channel` (`messages.channel`).
    Channel,
}

impl Kind {
    /// The style of this kind in `theme`.
    pub fn style(self, theme: &Theme) -> Style {
        match self {
            Kind::User => theme.messages_mention,
            Kind::Own => theme.messages_mention_self,
            Kind::Group => theme.messages_mention_group,
            Kind::Channel => theme.messages_channel,
        }
    }
}

// The markers. All are in the set `sanitize` strips (bidi isolates and the embedding pop), so only
// the client can write one, and all measure zero columns.
const START_USER: char = '\u{2066}';
const START_OWN: char = '\u{2067}';
const START_GROUP: char = '\u{2068}';
const START_CHANNEL: char = '\u{2069}';
pub(crate) const END: char = '\u{202c}';

pub(crate) fn start_marker(kind: Kind) -> char {
    match kind {
        Kind::User => START_USER,
        Kind::Own => START_OWN,
        Kind::Group => START_GROUP,
        Kind::Channel => START_CHANNEL,
    }
}

fn marker_kind(c: char) -> Option<Kind> {
    match c {
        START_USER => Some(Kind::User),
        START_OWN => Some(Kind::Own),
        START_GROUP => Some(Kind::Group),
        START_CHANNEL => Some(Kind::Channel),
        _ => None,
    }
}

/// True for a character a username or room name may hold.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// The tokens of `text` as byte ranges (sigil included), in order and without overlap. `refs` is
/// the server's lists of the message, `None` to match by shape; `own` is the user's own username,
/// compared without regard to case. `in_code` is the backtick state at the start of `text` and is
/// left as it is at the end, so a caller scanning line after line (the draft) carries a fenced
/// block across lines.
pub fn tokens_in(
    text: &str,
    refs: Option<&Refs>,
    own: Option<&str>,
    in_code: &mut bool,
) -> Vec<(Range<usize>, Kind)> {
    if !text.contains(['@', '#']) {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| chars.get(i).map_or(text.len(), |&(b, _)| b);
    let listed = |list: &[String], name: &str| list.iter().any(|n| n.eq_ignore_ascii_case(name));
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '`' {
            *in_code = !*in_code;
        } else if (c == '@' || c == '#')
            && !*in_code
            && (i == 0 || !chars[i - 1].1.is_alphanumeric())
        {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && is_name_char(chars[end].1) {
                end += 1;
            }
            // Punctuation after the name belongs to the sentence: `hi @bob.` and `@bob-`.
            while end > start && matches!(chars[end - 1].1, '.' | '-') {
                end -= 1;
            }
            let name = &text[at(start)..at(end)];
            let kind = if name.is_empty() {
                None
            } else if c == '@' {
                if name.eq_ignore_ascii_case("here") || name.eq_ignore_ascii_case("all") {
                    Some(Kind::Group)
                } else if refs.is_none_or(|r| listed(&r.mentions, name)) {
                    Some(if own.is_some_and(|o| o.eq_ignore_ascii_case(name)) {
                        Kind::Own
                    } else {
                        Kind::User
                    })
                } else {
                    None
                }
            } else if refs.map_or(name.starts_with(char::is_alphabetic), |r| {
                listed(&r.channels, name)
            }) {
                Some(Kind::Channel)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.push((at(i)..at(end), kind));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `tokens_in` for a text scanned on its own.
pub fn tokens(text: &str, refs: Option<&Refs>, own: Option<&str>) -> Vec<(Range<usize>, Kind)> {
    tokens_in(text, refs, own, &mut false)
}

/// `text` with a start marker before and an end marker after each of its tokens, ready to wrap.
/// Text without a token is returned as it is, without allocating.
pub fn mark<'a>(text: &'a str, refs: Option<&Refs>, own: Option<&str>) -> Cow<'a, str> {
    let found = tokens(text, refs, own);
    if found.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + found.len() * 6);
    let mut at = 0;
    for (range, kind) in found {
        out.push_str(&text[at..range.start]);
        out.push(start_marker(kind));
        out.push_str(&text[range.start..range.end]);
        out.push(END);
        at = range.end;
    }
    out.push_str(&text[at..]);
    Cow::Owned(out)
}

/// One wrapped row of marked text without its markers, and the ranges of it (bytes of the clean
/// row) that are tokens. `open` says whether a token was open at the end of the previous row (a
/// token cut by wrapping) and is updated for the next row: a token cut across two rows is a range
/// in both.
pub fn strip(row: &str, open: &mut Option<Kind>) -> (String, Vec<(Range<usize>, Kind)>) {
    let touched = |c: char| c == END || marker_kind(c).is_some();
    if open.is_none() && !row.contains(touched) {
        return (row.to_string(), Vec::new());
    }
    let mut clean = String::with_capacity(row.len());
    let mut ranges = Vec::new();
    // Where the open token starts in `clean`: 0 when it came from the previous row.
    let mut from = 0;
    for c in row.chars() {
        if let Some(kind) = marker_kind(c) {
            *open = Some(kind);
            from = clean.len();
        } else if c == END {
            if let Some(kind) = open.take()
                && from < clean.len()
            {
                ranges.push((from..clean.len(), kind));
            }
        } else {
            clean.push(c);
        }
    }
    if let Some(kind) = *open
        && from < clean.len()
    {
        ranges.push((from..clean.len(), kind));
    }
    (clean, ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(mentions: &[&str], channels: &[&str]) -> Refs {
        Refs {
            mentions: mentions.iter().map(|s| s.to_string()).collect(),
            channels: channels.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// The tokens as `(text, kind)` pairs.
    fn found(text: &str, refs: Option<&Refs>, own: Option<&str>) -> Vec<(String, Kind)> {
        tokens(text, refs, own)
            .into_iter()
            .map(|(r, k)| (text[r].to_string(), k))
            .collect()
    }

    fn one(text: &str, kind: Kind) -> Vec<(String, Kind)> {
        vec![(text.to_string(), kind)]
    }

    #[test]
    fn a_listed_user_is_a_mention() {
        let r = refs(&["bob"], &[]);
        assert_eq!(
            found("thanks @bob", Some(&r), None),
            one("@bob", Kind::User)
        );
    }

    #[test]
    fn the_own_name_is_its_own_kind_whatever_the_case() {
        let r = refs(&["Me"], &[]);
        assert_eq!(
            found("ping @me", Some(&r), Some("ME")),
            one("@me", Kind::Own)
        );
        let r = refs(&["me", "bob"], &[]);
        let all = found("@me and @bob", Some(&r), Some("me"));
        assert_eq!(all[0].1, Kind::Own);
        assert_eq!(all[1].1, Kind::User);
    }

    #[test]
    fn here_and_all_are_matched_literally_even_without_a_list_entry() {
        let r = refs(&[], &[]);
        let all = found("heads up @here and @all", Some(&r), None);
        assert_eq!(
            all,
            vec![
                ("@here".to_string(), Kind::Group),
                ("@all".to_string(), Kind::Group)
            ]
        );
    }

    #[test]
    fn a_listed_channel_is_a_reference() {
        let r = refs(&[], &["dev-chat"]);
        assert_eq!(
            found("see #dev-chat", Some(&r), None),
            one("#dev-chat", Kind::Channel)
        );
    }

    #[test]
    fn a_name_the_server_did_not_list_is_plain_when_it_sent_lists() {
        let r = refs(&["bob"], &["dev-chat"]);
        assert!(found("hi @carol, see #ops", Some(&r), None).is_empty());
        // The email case: `@example.com` is not a mention, nor listed.
        let r = refs(&[], &[]);
        assert!(found("mail bob@example.com", Some(&r), None).is_empty());
    }

    #[test]
    fn without_lists_the_shape_decides() {
        assert_eq!(found("cc @carol", None, None), one("@carol", Kind::User));
        assert_eq!(
            found("see #dev-chat", None, None),
            one("#dev-chat", Kind::Channel)
        );
        assert_eq!(found("cc @me", None, Some("me")), one("@me", Kind::Own));
    }

    #[test]
    fn a_token_glued_to_a_letter_or_digit_is_plain() {
        assert!(found("bob@example.com", None, None).is_empty());
        assert!(found("a1@b", None, None).is_empty());
        assert!(found("issue5#12", None, None).is_empty());
        // Punctuation before it is fine.
        assert_eq!(found("(@bob)", None, None), one("@bob", Kind::User));
    }

    #[test]
    fn a_bare_sigil_or_a_number_sign_digit_is_plain_by_shape() {
        assert!(found("a @ b # c", None, None).is_empty());
        assert!(found("see #1", None, None).is_empty());
        assert!(found("# heading", None, None).is_empty());
    }

    #[test]
    fn names_may_hold_dots_and_dashes_but_the_sentence_end_is_not_part_of_one() {
        let r = refs(&["john.doe"], &["dev-chat"]);
        assert_eq!(
            found("hi @john.doe.", Some(&r), None),
            one("@john.doe", Kind::User)
        );
        assert_eq!(
            found("in #dev-chat, ok", Some(&r), None),
            one("#dev-chat", Kind::Channel)
        );
    }

    #[test]
    fn code_spans_and_fenced_blocks_are_left_alone() {
        assert!(found("run `ping @bob` now", None, None).is_empty());
        assert!(found("```\n@bob #dev\n```", None, None).is_empty());
        assert_eq!(found("`x` @bob `y`", None, None), one("@bob", Kind::User));
        // An unclosed span covers the rest, like the shortcode scan.
        assert!(found("`open @bob", None, None).is_empty());
    }

    #[test]
    fn unicode_names_and_text_keep_their_byte_ranges() {
        let text = "héllo @zoë ok";
        let t = tokens(text, None, None);
        assert_eq!(&text[t[0].0.clone()], "@zoë");
    }

    #[test]
    fn mark_and_strip_round_trip_to_the_same_text_and_ranges() {
        let r = refs(&["bob"], &["dev"]);
        let text = "hi @bob see #dev @here";
        let marked = mark(text, Some(&r), Some("x"));
        assert_ne!(marked, text);
        let (clean, ranges) = strip(&marked, &mut None);
        assert_eq!(clean, text);
        let got: Vec<(&str, Kind)> = ranges
            .iter()
            .map(|(r, k)| (&clean[r.clone()], *k))
            .collect();
        assert_eq!(
            got,
            [
                ("@bob", Kind::User),
                ("#dev", Kind::Channel),
                ("@here", Kind::Group)
            ]
        );
    }

    #[test]
    fn text_without_a_token_is_not_copied() {
        assert!(matches!(mark("nothing here", None, None), Cow::Borrowed(_)));
    }

    #[test]
    fn markers_measure_nothing() {
        use unicode_width::UnicodeWidthStr;
        let marked = mark("hi @bob", None, None);
        assert_eq!(marked.width(), "hi @bob".width());
    }

    #[test]
    fn a_token_cut_across_rows_is_a_range_in_both() {
        let mut open = None;
        let (a, ra) = strip(&format!("x {START_USER}@bo"), &mut open);
        assert_eq!(a, "x @bo");
        assert_eq!(ra, vec![(2..5, Kind::User)]);
        assert_eq!(open, Some(Kind::User));
        let (b, rb) = strip(&format!("b{END} y"), &mut open);
        assert_eq!(b, "b y");
        assert_eq!(rb, vec![(0..1, Kind::User)]);
        assert_eq!(open, None);
    }

    #[test]
    fn server_text_cannot_carry_a_marker() {
        let hostile = format!("a{START_OWN}@bob{END}b{START_CHANNEL}");
        let clean = crate::ui::sanitize::sanitize(&hostile);
        assert_eq!(clean, "a@bobb");
    }
}
