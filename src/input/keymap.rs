//! Keymap data: rows shaped `(Mode, Vim notation, Action)` (D-07, D-08).
//!
//! Phase 2 loads the same row shape from `keymap.toml`; nothing here is hardcoded in the reducer.

use crate::action::Action;
use crate::app::Mode;
use crate::input::key::{Chord, ParseError, parse_keys};
use crate::input::trie::{Lookup, Trie};

/// One key-sequence trie per mode. All three modes always exist, possibly empty.
pub struct Keymap {
    normal: Trie<Action>,
    insert: Trie<Action>,
    command: Trie<Action>,
}

impl Keymap {
    /// Build a keymap from `(mode, notation, action)` rows; a later row replaces an earlier one
    /// with the same mode and keys.
    pub fn from_bindings(rows: &[(Mode, &str, Action)]) -> Result<Keymap, ParseError> {
        let mut keymap = Keymap {
            normal: Trie::new(),
            insert: Trie::new(),
            command: Trie::new(),
        };
        for (mode, notation, action) in rows {
            let chords = parse_keys(notation)?;
            keymap.trie_mut(*mode).insert(&chords, action.clone());
        }
        Ok(keymap)
    }

    fn trie_mut(&mut self, mode: Mode) -> &mut Trie<Action> {
        match mode {
            Mode::Normal => &mut self.normal,
            Mode::Insert => &mut self.insert,
            Mode::Command => &mut self.command,
        }
    }

    /// The whole trie of `mode`, as the matcher consumes it.
    pub fn trie(&self, mode: Mode) -> &Trie<Action> {
        match mode {
            Mode::Normal => &self.normal,
            Mode::Insert => &self.insert,
            Mode::Command => &self.command,
        }
    }

    /// What `seq` resolves to in `mode`.
    pub fn lookup(&self, mode: Mode, seq: &[Chord]) -> Lookup<'_, Action> {
        self.trie(mode).lookup(seq)
    }
}

/// The built-in default bindings. `<C-i>` and `<C-m>` are never bound: legacy terminals send them
/// as Tab and Enter.
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
            assert!(
                matches!(keymap.lookup(*mode, &chords), Lookup::Exact(bound) if bound == action),
                "{notation}"
            );
        }
    }
}
