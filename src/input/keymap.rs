//! Keymap data: rows shaped `(Mode, Vim notation, Action)` (D-07, D-08).
//!
//! Phase 2 loads the same row shape from `keymap.toml`; nothing here is hardcoded in the reducer.

use crate::action::Action;
use crate::app::Mode;
use crate::input::key::{Chord, ParseError, parse_keys};
use std::collections::HashMap;

/// Per-mode map from a chord sequence to the Action it fires.
pub struct Keymap {
    maps: HashMap<Mode, HashMap<Vec<Chord>, Action>>,
}

impl Keymap {
    /// Build a keymap from `(mode, notation, action)` rows; a later row replaces an earlier one
    /// with the same mode and keys.
    pub fn from_bindings(rows: &[(Mode, &str, Action)]) -> Result<Keymap, ParseError> {
        let mut maps: HashMap<Mode, HashMap<Vec<Chord>, Action>> = HashMap::new();
        for (mode, notation, action) in rows {
            let chords = parse_keys(notation)?;
            maps.entry(*mode)
                .or_default()
                .insert(chords, action.clone());
        }
        Ok(Keymap { maps })
    }

    /// The Action bound to exactly `seq` in `mode`, if any.
    pub fn lookup(&self, mode: Mode, seq: &[Chord]) -> Option<&Action> {
        self.maps.get(&mode)?.get(seq)
    }
}

/// The built-in default bindings.
pub fn default_bindings() -> Vec<(Mode, &'static str, Action)> {
    vec![
        (Mode::Normal, "j", Action::MoveDown),
        (Mode::Normal, "k", Action::MoveUp),
        (Mode::Normal, ":", Action::EnterCommand),
        (Mode::Normal, "<Esc>", Action::Cancel),
        (Mode::Normal, "<C-c>", Action::Cancel),
        (Mode::Command, "<CR>", Action::ExecuteCommand),
        (Mode::Command, "<Esc>", Action::Cancel),
        (Mode::Command, "<C-c>", Action::Cancel),
        (Mode::Command, "<BS>", Action::DeleteBack),
    ]
}

/// The keymap built from [`default_bindings`].
pub fn default_keymap() -> Keymap {
    Keymap::from_bindings(&default_bindings()).expect("built-in keymap parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keymap_parses() {
        let rows = default_bindings();
        let keymap = Keymap::from_bindings(&rows).expect("default bindings parse");
        for (mode, notation, action) in &rows {
            let chords = parse_keys(notation).unwrap();
            assert!(keymap.lookup(*mode, &chords) == Some(action), "{notation}");
        }
    }
}
