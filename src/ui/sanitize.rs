//! The single choke point for untrusted text (T-01-01, T-01-02). Every string from data or the
//! user passes through here before width math or drawing.

use std::borrow::Cow;

/// Width a TAB expands to.
pub const TAB_WIDTH: usize = 4;

/// Invisible or direction-changing characters that can spoof or hide text. U+200C and U+200D
/// are deliberately absent (emoji ZWJ sequences, Indic and Persian scripts), as are the
/// variation selectors and regional indicators.
fn is_spoofing(c: char) -> bool {
    matches!(
        c as u32,
        0x061c                       // Arabic letter mark
        | 0x200b                     // zero-width space
        | 0x200e | 0x200f            // LRM, RLM
        | 0x202a..=0x202e            // embeddings and overrides
        | 0x2060                     // word joiner
        | 0x2066..=0x2069            // isolates
        | 0xfeff                     // BOM / zero-width no-break space
        | 0xe0000..=0xe007f          // Unicode tag block
    )
}

/// True when `sanitize` would change this character.
fn needs_change(c: char) -> bool {
    match c {
        '\n' => false,
        '\r' | '\t' | '\u{2028}' | '\u{2029}' => true,
        _ => c.is_control() || is_spoofing(c),
    }
}

/// Make `s` safe to draw: line breaks become LF, tabs expand to `TAB_WIDTH` spaces, and every
/// other control character (C0 including ESC, BEL and NUL; DEL; C1) plus the spoofing
/// characters above are dropped. Subdivision flags are built from tag characters, so they
/// degrade to the bare black flag: accepted. Returns `Cow::Borrowed` when nothing changes.
pub fn sanitize(s: &str) -> Cow<'_, str> {
    if !s.chars().any(needs_change) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{2028}' | '\u{2029}' => out.push('\n'),
            '\t' => out.extend(std::iter::repeat_n(' ', TAB_WIDTH)),
            '\n' => out.push('\n'),
            _ if c.is_control() || is_spoofing(c) => {}
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// `sanitize`, then line breaks become spaces, for single-row contexts.
pub fn sanitize_line(s: &str) -> Cow<'_, str> {
    match sanitize(s) {
        Cow::Borrowed(b) if !b.contains('\n') => Cow::Borrowed(b),
        other => Cow::Owned(other.replace('\n', " ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_escape_sequences() {
        let out = sanitize("\u{1b}[2J\u{1b}]52;c;ZXZpbA==\u{7}done");
        assert!(!out.contains('\u{1b}'));
        assert!(!out.contains('\u{7}'));
        assert!(out.contains("done"));
    }

    #[test]
    fn strips_c1() {
        let out = sanitize("\u{9b}31m");
        assert!(!out.contains('\u{9b}'));
        assert_eq!(out, "31m");
    }

    #[test]
    fn strips_bidi_and_zero_width() {
        assert_eq!(sanitize("\u{202e}evil"), "evil");
        assert_eq!(sanitize("\u{200b}zw"), "zw");
        assert_eq!(sanitize("a\u{2066}b\u{2069}c"), "abc");
        assert_eq!(sanitize("\u{61c}\u{200e}\u{200f}\u{feff}x"), "x");
        assert_eq!(sanitize("a\u{2060}b"), "ab");
        assert_eq!(sanitize("\u{202a}\u{202b}\u{202c}\u{202d}y"), "y");
    }

    #[test]
    fn keeps_zwj_and_zwnj() {
        assert_eq!(sanitize("a\u{200d}b"), "a\u{200d}b");
        assert_eq!(sanitize("a\u{200c}b"), "a\u{200c}b");
    }

    #[test]
    fn strips_tag_characters() {
        assert_eq!(sanitize("a\u{e0001}\u{e0041}\u{e0042}\u{e007f}b"), "ab");
        let england = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
        assert_eq!(sanitize(england), "\u{1f3f4}");
        assert_eq!(sanitize("\u{e0100}"), "\u{e0100}");
    }

    #[test]
    fn keeps_emoji_sequences() {
        for s in [
            "\u{1f1f5}\u{1f1f9}",
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
            "\u{2764}\u{fe0f}",
        ] {
            assert_eq!(sanitize(s), s);
        }
    }

    #[test]
    fn expands_tab() {
        assert_eq!(sanitize("a\tb"), format!("a{}b", " ".repeat(TAB_WIDTH)));
    }

    #[test]
    fn normalizes_newlines() {
        assert_eq!(sanitize("x\r\ny"), "x\ny");
        assert_eq!(sanitize("x\ry"), "x\ny");
        assert_eq!(sanitize("x\u{2028}y"), "x\ny");
    }

    #[test]
    fn strips_nul_and_del() {
        assert_eq!(sanitize("nul\0byte"), "nulbyte");
        assert_eq!(sanitize("a\u{7f}b"), "ab");
    }

    #[test]
    fn ascii_is_borrowed() {
        assert!(matches!(sanitize("plain ascii 123"), Cow::Borrowed(_)));
    }

    #[test]
    fn sanitize_line_maps_lf_to_space() {
        assert_eq!(sanitize_line("a\nb"), "a b");
    }
}
