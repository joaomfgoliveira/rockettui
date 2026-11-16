//! Deterministic per-author colors for the messages pane.
//!
//! Callers pass `sanitize_line` output (threats T-01-01/T-01-16), the same string that is drawn
//! and measured. The hash is FNV-1a 64: it is unseeded and fully specified, so a username maps to
//! the same color across runs, rooms and platforms (the std hasher is randomly seeded per process
//! and unspecified, so it cannot give a stable per-user color). Untrusted input only selects an
//! index into a fixed table of named ANSI colors, so no user data ever reaches an escape sequence.

use ratatui::style::Color;

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The author colors. These are named ANSI colors, which the terminal theme remaps, so they
/// adapt to dark and light themes. The order is part of the stable user-to-color mapping:
/// reordering or editing it changes every user's color.
///
/// Excluded colors and why:
/// - Reset, Black, White and Gray: Reset is indistinguishable from the default fg and the
///   uncolored focus border; Black, White and Gray each disappear on one theme family.
/// - DarkGray: the unfocused cursor-row background.
/// - Red and LightRed: status-line errors use Red.
/// - Blue: lowest contrast on dark backgrounds and on the DarkGray cursor row (LightBlue
///   replaces it).
/// - LightGreen, LightCyan and LightYellow: wash out on light backgrounds.
const AUTHOR_PALETTE: [Color; 6] = [
    Color::Cyan,
    Color::Magenta,
    Color::LightMagenta,
    Color::Green,
    Color::Yellow,
    Color::LightBlue,
];

/// 64-bit FNV-1a over `bytes`.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The color for an author. `sanitized` must already be `sanitize_line` output; its bytes are
/// hashed exactly as given, with no case folding (the server sends the canonical username).
pub fn author_color(sanitized: &str) -> Color {
    let len = AUTHOR_PALETTE.len() as u64;
    let index = usize::try_from(fnv1a_64(sanitized.as_bytes()) % len).unwrap_or(0);
    AUTHOR_PALETTE[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_published_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn golden_author_colors() {
        assert_eq!(author_color("alice"), Color::LightBlue);
        assert_eq!(author_color("bob"), Color::Cyan);
        assert_eq!(author_color("carol"), Color::Yellow);
        assert_eq!(author_color("dave"), Color::Green);
    }

    #[test]
    fn palette_avoids_clashes() {
        for (i, a) in AUTHOR_PALETTE.iter().enumerate() {
            for b in &AUTHOR_PALETTE[i + 1..] {
                assert_ne!(a, b, "duplicate palette entry");
            }
        }
        let banned = [
            Color::Reset,
            Color::Black,
            Color::White,
            Color::Gray,
            Color::DarkGray,
            Color::Red,
            Color::LightRed,
            Color::Blue,
            Color::LightGreen,
            Color::LightCyan,
            Color::LightYellow,
        ];
        for color in AUTHOR_PALETTE {
            assert!(!banned.contains(&color), "{color:?} is excluded");
            assert!(!matches!(color, Color::Rgb(..) | Color::Indexed(_)));
        }
        for i in 0..1000 {
            let color = author_color(&format!("user{i}"));
            assert!(AUTHOR_PALETTE.contains(&color), "{color:?}");
        }
    }

    #[test]
    fn empty_author_is_a_palette_entry() {
        assert!(AUTHOR_PALETTE.contains(&author_color("")));
    }
}
