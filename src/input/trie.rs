//! A generic key-sequence trie: the lookup structure behind the per-mode keymaps (D-06).

use crate::input::key::Chord;
use std::collections::HashMap;

struct Node<A> {
    value: Option<A>,
    children: HashMap<Chord, Node<A>>,
}

impl<A> Node<A> {
    fn new() -> Self {
        Node {
            value: None,
            children: HashMap::new(),
        }
    }
}

/// What a chord sequence resolves to in a [`Trie`].
pub enum Lookup<'a, A> {
    /// No binding starts with this sequence.
    None,
    /// The sequence is the start of at least one longer binding and is not bound itself.
    Prefix,
    /// The sequence is bound and nothing longer starts with it.
    Exact(&'a A),
    /// The sequence is bound and also the start of a longer binding (`g` next to `gg`).
    ExactAndPrefix(&'a A),
}

/// Maps chord sequences to values of type `A`.
pub struct Trie<A> {
    root: Node<A>,
}

impl<A> Default for Trie<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Trie<A> {
    pub fn new() -> Self {
        Trie { root: Node::new() }
    }

    /// Bind `seq` to `value`. An empty sequence is ignored; re-inserting a sequence replaces its
    /// value, which is what Phase 2 keymap overrides rely on.
    pub fn insert(&mut self, _seq: &[Chord], _value: A) {}

    /// Resolve `seq`; an empty sequence resolves to [`Lookup::None`].
    pub fn lookup(&self, _seq: &[Chord]) -> Lookup<'_, A> {
        let _ = &self.root;
        Lookup::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::key::parse_keys;

    fn seq(s: &str) -> Vec<Chord> {
        parse_keys(s).expect("test notation parses")
    }

    #[test]
    fn trie_lookup_kinds() {
        let mut t = Trie::new();
        t.insert(&seq("gg"), 1);
        assert!(matches!(t.lookup(&seq("g")), Lookup::Prefix));
        assert!(matches!(t.lookup(&seq("gg")), Lookup::Exact(&1)));
        t.insert(&seq("g"), 2);
        assert!(matches!(t.lookup(&seq("g")), Lookup::ExactAndPrefix(&2)));
        assert!(matches!(t.lookup(&seq("gg")), Lookup::Exact(&1)));
        assert!(matches!(t.lookup(&seq("x")), Lookup::None));
        assert!(matches!(t.lookup(&seq("ggg")), Lookup::None));
    }

    #[test]
    fn trie_reinsert_replaces() {
        let mut t = Trie::new();
        t.insert(&seq("j"), 1);
        t.insert(&seq("j"), 2);
        assert!(matches!(t.lookup(&seq("j")), Lookup::Exact(&2)));
        t.insert(&[], 3);
        assert!(matches!(t.lookup(&[]), Lookup::None));
        assert!(matches!(t.lookup(&seq("k")), Lookup::None));
    }
}
