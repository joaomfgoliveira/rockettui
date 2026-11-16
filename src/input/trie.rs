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
    pub fn insert(&mut self, seq: &[Chord], value: A) {
        if seq.is_empty() {
            return;
        }
        let mut node = &mut self.root;
        for chord in seq {
            node = node.children.entry(*chord).or_insert_with(Node::new);
        }
        node.value = Some(value);
    }

    /// Resolve `seq`; an empty sequence resolves to [`Lookup::None`].
    pub fn lookup(&self, seq: &[Chord]) -> Lookup<'_, A> {
        if seq.is_empty() {
            return Lookup::None;
        }
        let mut node = &self.root;
        for chord in seq {
            match node.children.get(chord) {
                Some(next) => node = next,
                None => return Lookup::None,
            }
        }
        match (&node.value, node.children.is_empty()) {
            (Some(value), true) => Lookup::Exact(value),
            (Some(value), false) => Lookup::ExactAndPrefix(value),
            (None, false) => Lookup::Prefix,
            (None, true) => Lookup::None,
        }
    }

    /// Unbind `seq` and prune every branch it leaves with no value and no children, so removing
    /// `gg` leaves no `g` prefix behind (D-05). Returns whether `seq` was bound. An empty or
    /// unbound sequence changes nothing.
    pub fn remove(&mut self, seq: &[Chord]) -> bool {
        if seq.is_empty() {
            return false;
        }
        remove_from(&mut self.root, seq)
    }

    /// Every bound sequence with its value, in no particular order.
    pub fn entries(&self) -> Vec<(Vec<Chord>, &A)> {
        let mut out = Vec::new();
        collect(&self.root, &mut Vec::new(), &mut out);
        out
    }

    /// Every pair `(short, long)` where bound `short` is a strict prefix of bound `long`: such a
    /// pair makes `short` wait for the sequence timeout instead of firing at once (D-06).
    pub fn clashes(&self) -> Vec<(Vec<Chord>, Vec<Chord>)> {
        let entries = self.entries();
        let mut out = Vec::new();
        for (short, _) in &entries {
            for (long, _) in &entries {
                if long.len() > short.len() && long.starts_with(short) {
                    out.push((short.clone(), long.clone()));
                }
            }
        }
        out
    }
}

fn remove_from<A>(node: &mut Node<A>, seq: &[Chord]) -> bool {
    let Some((first, rest)) = seq.split_first() else {
        return node.value.take().is_some();
    };
    let Some(child) = node.children.get_mut(first) else {
        return false;
    };
    let removed = remove_from(child, rest);
    if child.value.is_none() && child.children.is_empty() {
        node.children.remove(first);
    }
    removed
}

fn collect<'a, A>(node: &'a Node<A>, path: &mut Vec<Chord>, out: &mut Vec<(Vec<Chord>, &'a A)>) {
    if let Some(value) = &node.value {
        out.push((path.clone(), value));
    }
    for (chord, child) in &node.children {
        path.push(*chord);
        collect(child, path, out);
        path.pop();
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

    #[test]
    fn remove_prunes() {
        let mut t = Trie::new();
        t.insert(&seq("gg"), 1);
        assert!(t.remove(&seq("gg")));
        assert!(matches!(t.lookup(&seq("g")), Lookup::None));
        assert!(matches!(t.lookup(&seq("gg")), Lookup::None));
        assert!(t.entries().is_empty());

        assert!(!t.remove(&seq("gg")), "already removed");
        assert!(!t.remove(&seq("x")), "never bound");
        assert!(!t.remove(&[]), "empty sequence");

        // Removing a prefix keeps the longer binding; removing only a prefix node is a no-op.
        t.insert(&seq("g"), 2);
        t.insert(&seq("gg"), 1);
        assert!(!t.remove(&seq("ggg")));
        assert!(t.remove(&seq("g")));
        assert!(matches!(t.lookup(&seq("g")), Lookup::Prefix));
        assert!(matches!(t.lookup(&seq("gg")), Lookup::Exact(&1)));
        assert!(!t.remove(&seq("g")), "g is only a prefix now");
        assert!(matches!(t.lookup(&seq("gg")), Lookup::Exact(&1)));
    }

    #[test]
    fn clashes_lists_pairs() {
        let mut t = Trie::new();
        t.insert(&seq("gg"), 1);
        assert!(t.clashes().is_empty());
        t.insert(&seq("g"), 2);
        let clashes = t.clashes();
        assert_eq!(clashes.len(), 1);
        assert!(clashes[0].0 == seq("g") && clashes[0].1 == seq("gg"));
        t.insert(&seq("x"), 3);
        assert_eq!(t.clashes().len(), 1);
        t.insert(&seq("ggg"), 4);
        // g/gg, g/ggg and gg/ggg.
        assert_eq!(t.clashes().len(), 3);
    }

    #[test]
    fn entries_lists_every_binding() {
        let mut t = Trie::new();
        t.insert(&seq("gg"), 1);
        t.insert(&seq("g"), 2);
        t.insert(&seq("<C-w>h"), 3);
        let mut found: Vec<(usize, i32)> =
            t.entries().iter().map(|(s, v)| (s.len(), **v)).collect();
        found.sort();
        assert_eq!(found, [(1, 2), (2, 1), (2, 3)]);
    }
}
