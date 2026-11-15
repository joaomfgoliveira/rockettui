//! The Vim key matcher: counts, multi-key sequences, a timeout and the text fallback (D-05, D-06).
//!
//! The matcher is pure state. It never reads a clock: the run loop turns a quiet second into
//! `Action::KeyTimeout`, which the reducer passes to [`Matcher::timeout`].

use crate::action::Action;
use crate::app::Mode;
use crate::input::key::Chord;
use crate::input::trie::Trie;
use std::fmt;

/// Largest count the matcher will ever report (`99999999999j` is `9999j`).
pub const MAX_COUNT: u32 = 9999;

/// What feeding one key (or a timeout) to the matcher produced.
#[derive(PartialEq)]
pub enum Outcome {
    /// The key is part of an unfinished count or sequence.
    Pending,
    /// A binding completed.
    Fire { action: Action, count: Option<u32> },
    /// An unbound plain character outside Normal mode: typed text.
    Text(char),
    /// Pending state was discarded without firing anything.
    Cancelled,
    /// The key matched nothing and is dropped.
    NoMatch,
}

/// Redacting `Debug` (T-01-03): `Text` never prints its character.
impl fmt::Debug for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Pending => write!(f, "Pending"),
            Outcome::Fire { action, count } => {
                write!(f, "Fire {{ action: {action:?}, count: {count:?} }}")
            }
            Outcome::Text(_) => write!(f, "Text(..)"),
            Outcome::Cancelled => write!(f, "Cancelled"),
            Outcome::NoMatch => write!(f, "NoMatch"),
        }
    }
}

/// Pending chords and count between keys.
#[derive(Default)]
pub struct Matcher {
    pending: Vec<Chord>,
    count: Option<u32>,
}

/// Redacting `Debug` (T-01-03): pending chords and count digits may be typed text, so only their
/// presence is printed.
impl fmt::Debug for Matcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Matcher {{ pending: {} chords, count set: {} }}",
            self.pending.len(),
            self.count.is_some()
        )
    }
}

impl Matcher {
    /// Feed one normalized chord.
    pub fn feed(&mut self, _trie: &Trie<Action>, _mode: Mode, _chord: Chord) -> Outcome {
        Outcome::NoMatch
    }

    /// The key timeout elapsed: fire an `ExactAndPrefix` binding or drop what is pending.
    pub fn timeout(&mut self, _trie: &Trie<Action>) -> Outcome {
        Outcome::NoMatch
    }

    /// True while chords or a count are waiting for more keys.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.count.is_some()
    }

    /// The pending count digits followed by the pending chords in Vim notation.
    pub fn display(&self) -> String {
        String::new()
    }

    /// Drop all pending state.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.count = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::key::parse_keys;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn trie(rows: &[(&str, Action)]) -> Trie<Action> {
        let mut t = Trie::new();
        for (notation, action) in rows {
            t.insert(&parse_keys(notation).expect("notation parses"), action.clone());
        }
        t
    }

    fn chord(s: &str) -> Chord {
        parse_keys(s).expect("notation parses")[0]
    }

    fn feed(m: &mut Matcher, t: &Trie<Action>, mode: Mode, keys: &str) -> Outcome {
        let mut last = Outcome::NoMatch;
        for c in parse_keys(keys).expect("notation parses") {
            last = m.feed(t, mode, c);
        }
        last
    }

    fn fire(action: Action, count: Option<u32>) -> Outcome {
        Outcome::Fire { action, count }
    }

    #[test]
    fn counts_wrap_exact() {
        let t = trie(&[("j", Action::MoveDown)]);
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &t, Mode::Normal, "5"), Outcome::Pending);
        assert_eq!(m.display(), "5");
        assert!(m.is_pending());
        assert_eq!(
            feed(&mut m, &t, Mode::Normal, "j"),
            fire(Action::MoveDown, Some(5))
        );
        assert!(!m.is_pending());
        assert_eq!(m.display(), "");
    }

    #[test]
    fn count_extends_with_zero() {
        let t = trie(&[("j", Action::MoveDown), ("0", Action::EnterCommand)]);
        let mut m = Matcher::default();
        assert_eq!(
            feed(&mut m, &t, Mode::Normal, "10j"),
            fire(Action::MoveDown, Some(10))
        );
        // A leading 0 is an ordinary key.
        assert_eq!(feed(&mut m, &t, Mode::Normal, "0"), fire(Action::EnterCommand, None));
        let unbound = trie(&[("j", Action::MoveDown)]);
        assert_eq!(feed(&mut m, &unbound, Mode::Normal, "0"), Outcome::NoMatch);
        assert!(!m.is_pending());
    }

    #[test]
    fn count_saturates() {
        let t = trie(&[("j", Action::MoveDown)]);
        let mut m = Matcher::default();
        let keys = format!("{}j", "9".repeat(11));
        assert_eq!(
            feed(&mut m, &t, Mode::Normal, &keys),
            fire(Action::MoveDown, Some(MAX_COUNT))
        );
        assert_eq!(MAX_COUNT, 9999);
    }

    #[test]
    fn multi_key_prefix() {
        let t = trie(&[("<C-w>h", Action::MoveDown)]);
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &t, Mode::Normal, "<C-w>"), Outcome::Pending);
        assert_eq!(m.display(), "<C-w>");
        assert_eq!(
            feed(&mut m, &t, Mode::Normal, "h"),
            fire(Action::MoveDown, None)
        );
        assert!(!m.is_pending());
        assert_eq!(feed(&mut m, &t, Mode::Normal, "<C-w>"), Outcome::Pending);
        assert_eq!(feed(&mut m, &t, Mode::Normal, "x"), Outcome::NoMatch);
        assert!(!m.is_pending());
    }

    #[test]
    fn exact_and_prefix_timeout_fires() {
        let t = trie(&[("g", Action::MoveDown), ("gg", Action::MoveUp)]);
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &t, Mode::Normal, "g"), Outcome::Pending);
        assert_eq!(m.timeout(&t), fire(Action::MoveDown, None));
        assert!(!m.is_pending());
        // The second g completes the longer binding instead.
        assert_eq!(
            feed(&mut m, &t, Mode::Normal, "gg"),
            fire(Action::MoveUp, None)
        );
    }

    #[test]
    fn prefix_only_timeout_cancels() {
        let t = trie(&[("gg", Action::MoveUp)]);
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &t, Mode::Normal, "g"), Outcome::Pending);
        assert_eq!(m.timeout(&t), Outcome::Cancelled);
        assert!(!m.is_pending());
        assert_eq!(feed(&mut m, &t, Mode::Normal, "5"), Outcome::Pending);
        assert_eq!(m.timeout(&t), Outcome::Cancelled);
        assert!(!m.is_pending());
        assert_eq!(m.timeout(&t), Outcome::NoMatch);
    }

    #[test]
    fn esc_and_ctrl_c_cancel_first() {
        let t = trie(&[
            ("<C-w>h", Action::MoveDown),
            ("<Esc>", Action::Cancel),
            ("<C-c>", Action::Cancel),
        ]);
        for cancel in ["<Esc>", "<C-c>"] {
            let mut m = Matcher::default();
            feed(&mut m, &t, Mode::Normal, "<C-w>");
            assert!(m.is_pending());
            assert_eq!(
                feed(&mut m, &t, Mode::Normal, cancel),
                fire(Action::Cancel, None),
                "{cancel}"
            );
            assert!(!m.is_pending(), "{cancel}");
            feed(&mut m, &t, Mode::Normal, "5");
            assert_eq!(
                feed(&mut m, &t, Mode::Normal, cancel),
                fire(Action::Cancel, None),
                "{cancel}"
            );
            assert!(!m.is_pending(), "{cancel}");
        }
        // Unbound cancel chords still clear pending state.
        let bare = trie(&[("gg", Action::MoveUp)]);
        let mut m = Matcher::default();
        feed(&mut m, &bare, Mode::Normal, "g");
        assert_eq!(feed(&mut m, &bare, Mode::Normal, "<Esc>"), Outcome::Cancelled);
        assert!(!m.is_pending());
    }

    #[test]
    fn text_fallback() {
        let empty = trie(&[]);
        for mode in [Mode::Insert, Mode::Command] {
            let mut m = Matcher::default();
            for c in ['a', '5', 'A', 'z'] {
                let ch = Chord {
                    code: KeyCode::Char(c),
                    mods: KeyModifiers::NONE,
                };
                assert_eq!(m.feed(&empty, mode, ch), Outcome::Text(c));
                assert!(!m.is_pending(), "counts never start outside Normal");
            }
            assert_eq!(m.feed(&empty, mode, chord("<C-a>")), Outcome::NoMatch);
            assert_eq!(m.feed(&empty, mode, chord("<A-a>")), Outcome::NoMatch);
            assert_eq!(m.feed(&empty, mode, chord("<CR>")), Outcome::NoMatch);
        }
        // Normal mode never types text.
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &empty, Mode::Normal, "x"), Outcome::NoMatch);
    }

    #[test]
    fn debug_redacts_typed_chars() {
        let empty = trie(&[]);
        let mut m = Matcher::default();
        let out = feed(&mut m, &empty, Mode::Insert, "Ж");
        assert_eq!(out, Outcome::Text('Ж'));
        assert!(!format!("{out:?}").contains('Ж'));

        let t = trie(&[("Жx", Action::MoveDown)]);
        let mut m = Matcher::default();
        assert_eq!(feed(&mut m, &t, Mode::Normal, "Ж"), Outcome::Pending);
        assert!(!format!("{m:?}").contains('Ж'));
        assert!(!format!("{:?}", feed(&mut m, &t, Mode::Normal, "7")).contains('7'));
        let mut counted = Matcher::default();
        feed(&mut counted, &t, Mode::Normal, "7");
        assert!(!format!("{counted:?}").contains('7'));

        let fired = Outcome::Fire {
            action: Action::InsertChar('Q'),
            count: None,
        };
        let shown = format!("{fired:?}");
        assert!(!shown.contains('Q'));
        assert!(shown.contains("InsertChar(..)"));
    }
}
