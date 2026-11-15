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

/// Parse the inside of an angle token: optional `C-` modifier then a key name.
fn parse_angle(token: &str) -> Result<Chord, ParseError> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = token;
    while rest.len() > 2 && rest.as_bytes()[1] == b'-' {
        match rest[..1].to_ascii_lowercase().as_str() {
            "c" => mods |= KeyModifiers::CONTROL,
            _ => return Err(ParseError::UnknownModifier(rest[..1].to_string())),
        }
        rest = &rest[2..];
    }
    let code = match rest.to_ascii_lowercase().as_str() {
        "esc" => KeyCode::Esc,
        "cr" => KeyCode::Enter,
        "bs" => KeyCode::Backspace,
        _ if rest.chars().count() == 1 => {
            let c = rest.chars().next().unwrap_or(' ');
            if mods.contains(KeyModifiers::CONTROL) {
                KeyCode::Char(c.to_ascii_lowercase())
            } else {
                KeyCode::Char(c)
            }
        }
        _ => return Err(ParseError::UnknownKey(rest.to_string())),
    };
    Ok(Chord { code, mods })
}
