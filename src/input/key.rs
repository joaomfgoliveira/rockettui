//! Key chords, normalization of live key events and the Vim-notation parser (D-08).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fmt;

/// One key press as the keymap sees it: a key code plus the CONTROL / ALT / SHIFT modifiers.
///
/// It has no derived `Debug`: a `Char` code is typed text.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

/// Redacting `Debug` (T-01-03): a `Char` code is typed text and prints as `Char(..)`; every other
/// code (Enter, Esc, Tab, arrows and the other named keys) is not typed text and prints as is.
impl fmt::Debug for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            KeyCode::Char(_) => write!(f, "Chord(Char(..), {:?})", self.mods),
            other => write!(f, "Chord({:?}, {:?})", other, self.mods),
        }
    }
}

/// Turn a live key event into the canonical chord used for keymap lookups.
///
/// Keeps only CONTROL, ALT and SHIFT; drops SHIFT on `Char` codes (the case is in the char);
/// maps `BackTab` to `Tab` + SHIFT; lowercases the char under CONTROL.
pub fn normalize(ev: KeyEvent) -> Chord {
    let mut mods = ev.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
    let code = match ev.code {
        KeyCode::BackTab => {
            mods |= KeyModifiers::SHIFT;
            KeyCode::Tab
        }
        KeyCode::Char(c) => {
            mods.remove(KeyModifiers::SHIFT);
            if mods.contains(KeyModifiers::CONTROL) {
                KeyCode::Char(c.to_ascii_lowercase())
            } else {
                KeyCode::Char(c)
            }
        }
        other => other,
    };
    Chord { code, mods }
}

/// Why a Vim-notation string failed to parse. It carries keymap notation, never typed text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The notation string was empty.
    Empty,
    /// An angle token had no closing bracket; the value is the byte offset of its `<`.
    UnclosedAngle(usize),
    /// A key name inside an angle token is not known.
    UnknownKey(String),
    /// A modifier prefix inside an angle token is not known.
    UnknownModifier(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty key notation"),
            ParseError::UnclosedAngle(at) => {
                write!(f, "unclosed '<' at byte {at} in key notation")
            }
            ParseError::UnknownKey(name) => write!(f, "unknown key name '{name}'"),
            ParseError::UnknownModifier(m) => write!(f, "unknown modifier '{m}-'"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse Vim key notation (`gg`, `<C-w>h`, `<Esc>`, `<CR>`) into chords.
///
/// Every plain char is one chord. Angle tokens are case-insensitive.
pub fn parse_keys(s: &str) -> Result<Vec<Chord>, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut chords = Vec::new();
    let mut at = 0;
    while let Some(c) = s[at..].chars().next() {
        if c == '<' {
            let rest = &s[at + 1..];
            let Some(close) = rest.find('>') else {
                return Err(ParseError::UnclosedAngle(at));
            };
            chords.push(parse_angle(&rest[..close])?);
            // Skip the opening bracket, the token body and the closing bracket.
            at += close + 2;
        } else {
            chords.push(Chord {
                code: KeyCode::Char(c),
                mods: KeyModifiers::NONE,
            });
            at += c.len_utf8();
        }
    }
    Ok(chords)
}

/// Parse the inside of an angle token: optional `C-`, `A-`, `M-`, `S-` modifiers then a key name.
///
/// The result is already in the canonical form `normalize` produces for the same live key:
/// SHIFT is folded into the char of a `Char` key, and CONTROL lowercases it.
fn parse_angle(token: &str) -> Result<Chord, ParseError> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = token;
    // A modifier prefix is one ASCII letter, a dash and a non-empty remainder (`<C-->` is Ctrl and `-`).
    while rest.len() > 2 && rest.as_bytes()[1] == b'-' {
        match rest[..1].to_ascii_lowercase().as_str() {
            "c" => mods |= KeyModifiers::CONTROL,
            "a" | "m" => mods |= KeyModifiers::ALT,
            "s" => mods |= KeyModifiers::SHIFT,
            _ => return Err(ParseError::UnknownModifier(rest[..1].to_string())),
        }
        rest = &rest[2..];
    }
    let mut code = match rest.to_ascii_lowercase().as_str() {
        "esc" => KeyCode::Esc,
        "cr" | "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "bs" => KeyCode::Backspace,
        "del" => KeyCode::Delete,
        "space" => KeyCode::Char(' '),
        "lt" => KeyCode::Char('<'),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return Err(ParseError::UnknownKey(rest.to_string())),
            }
        }
    };
    if let KeyCode::Char(c) = code {
        if mods.contains(KeyModifiers::CONTROL) {
            code = KeyCode::Char(c.to_ascii_lowercase());
        } else if mods.contains(KeyModifiers::SHIFT) {
            code = KeyCode::Char(c.to_ascii_uppercase());
        }
        mods.remove(KeyModifiers::SHIFT);
    }
    Ok(Chord { code, mods })
}

/// Print chords in Vim notation: the canonical inverse of [`parse_keys`].
pub fn format_keys(chords: &[Chord]) -> String {
    chords.iter().map(format_chord).collect()
}

fn format_chord(chord: &Chord) -> String {
    let named = match chord.code {
        KeyCode::Char(' ') => Some("Space"),
        KeyCode::Char('<') => Some("lt"),
        KeyCode::Enter => Some("CR"),
        KeyCode::Esc => Some("Esc"),
        KeyCode::Tab => Some("Tab"),
        KeyCode::Backspace => Some("BS"),
        KeyCode::Delete => Some("Del"),
        KeyCode::Up => Some("Up"),
        KeyCode::Down => Some("Down"),
        KeyCode::Left => Some("Left"),
        KeyCode::Right => Some("Right"),
        KeyCode::Home => Some("Home"),
        KeyCode::End => Some("End"),
        KeyCode::PageUp => Some("PageUp"),
        KeyCode::PageDown => Some("PageDown"),
        _ => None,
    };
    let mut prefix = String::new();
    if chord.mods.contains(KeyModifiers::CONTROL) {
        prefix.push_str("C-");
    }
    if chord.mods.contains(KeyModifiers::ALT) {
        prefix.push_str("A-");
    }
    if chord.mods.contains(KeyModifiers::SHIFT) {
        prefix.push_str("S-");
    }
    match (named, chord.code) {
        (Some(name), _) => format!("<{prefix}{name}>"),
        (None, KeyCode::Char(c)) if prefix.is_empty() => c.to_string(),
        (None, KeyCode::Char(c)) => format!("<{prefix}{c}>"),
        // Keys the notation has no name for (function keys, media keys) never come from a keymap row.
        (None, other) => format!("<{prefix}{other:?}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(code: KeyCode, mods: KeyModifiers) -> Chord {
        Chord { code, mods }
    }

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn normalize_cases() {
        assert!(
            normalize(ev(KeyCode::Char('G'), KeyModifiers::SHIFT))
                == chord(KeyCode::Char('G'), KeyModifiers::NONE)
        );
        assert!(
            normalize(ev(KeyCode::Char('D'), KeyModifiers::CONTROL))
                == chord(KeyCode::Char('d'), KeyModifiers::CONTROL)
        );
        assert!(
            normalize(ev(KeyCode::BackTab, KeyModifiers::NONE))
                == chord(KeyCode::Tab, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn parse_keys_notation() {
        assert_eq!(parse_keys("gg").unwrap().len(), 2);
        assert!(
            parse_keys("<C-w>h").unwrap()
                == vec![
                    chord(KeyCode::Char('w'), KeyModifiers::CONTROL),
                    chord(KeyCode::Char('h'), KeyModifiers::NONE),
                ]
        );
        assert!(parse_keys("<lt>").unwrap() == vec![chord(KeyCode::Char('<'), KeyModifiers::NONE)]);
        assert!(parse_keys("<S-Tab>").unwrap() == vec![chord(KeyCode::Tab, KeyModifiers::SHIFT)]);
        assert!(parse_keys("<A-CR>").unwrap() == vec![chord(KeyCode::Enter, KeyModifiers::ALT)]);
        assert!(parse_keys("<M-x>").unwrap() == vec![chord(KeyCode::Char('x'), KeyModifiers::ALT)]);
        for one in ["<Space>", "<Up>", "<PageDown>"] {
            assert_eq!(parse_keys(one).unwrap().len(), 1, "{one}");
        }
        assert!(parse_keys("<esc>").unwrap() == parse_keys("<Esc>").unwrap());
    }

    #[test]
    fn debug_redacts_typed_char() {
        let plain = chord(KeyCode::Char('Ж'), KeyModifiers::NONE);
        let ctrl = chord(KeyCode::Char('Ж'), KeyModifiers::CONTROL);
        assert!(!format!("{plain:?}").contains('Ж'));
        assert!(!format!("{ctrl:?}").contains('Ж'));
        assert!(format!("{ctrl:?}").contains("CONTROL"));
        let parsed = parse_keys("Ж").unwrap();
        assert!(!format!("{parsed:?}").contains('Ж'));
        let enter = chord(KeyCode::Enter, KeyModifiers::NONE);
        assert!(format!("{enter:?}").contains("Enter"));
    }

    #[test]
    fn format_round_trip() {
        use crate::input::keymap::default_bindings;
        for (_, row, _) in default_bindings() {
            assert_eq!(format_keys(&parse_keys(row).unwrap()), row, "row {row}");
        }
        for row in [
            "<lt>",
            "<Space>",
            "<S-Tab>",
            "<A-CR>",
            "<C-w>h",
            "gg",
            "G",
            "<C-A-x>",
            "<C-Space>",
            "<S-Up>",
            "<Esc>",
            "<BS>",
            "<Del>",
            "<Home>",
            "<End>",
            "<PageUp>",
            "<PageDown>",
            "<Left><Right><Down>",
            "5<Tab>",
        ] {
            assert_eq!(format_keys(&parse_keys(row).unwrap()), row, "row {row}");
        }
        assert_eq!(format_keys(&[]), "");
    }

    #[test]
    fn parse_keys_errors() {
        assert_eq!(parse_keys("<C-d").err(), Some(ParseError::UnclosedAngle(0)));
        assert!(matches!(
            parse_keys("<Bogus>"),
            Err(ParseError::UnknownKey(_))
        ));
        assert!(matches!(
            parse_keys("<Q-x>"),
            Err(ParseError::UnknownModifier(_))
        ));
        assert_eq!(parse_keys("").err(), Some(ParseError::Empty));
    }
}
