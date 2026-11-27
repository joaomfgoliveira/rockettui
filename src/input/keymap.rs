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

    /// Bind `seq` in `mode`, replacing any earlier binding of the same keys.
    pub fn bind(&mut self, mode: Mode, seq: &[Chord], action: Action) {
        self.trie_mut(mode).insert(seq, action);
    }

    /// Remove the binding of `seq` in `mode`, pruning empty branches. False when it was not bound.
    pub fn unbind(&mut self, mode: Mode, seq: &[Chord]) -> bool {
        self.trie_mut(mode).remove(seq)
    }

    /// Every `(short, long)` pair in `mode` where a binding is a strict prefix of another.
    pub fn clashes(&self, mode: Mode) -> Vec<(Vec<Chord>, Vec<Chord>)> {
        self.trie(mode).clashes()
    }

    /// Every binding of `mode`, in no particular order.
    pub fn bindings(&self, mode: Mode) -> Vec<(Vec<Chord>, &Action)> {
        self.trie(mode).entries()
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
        (Mode::Normal, "h", Action::MoveLeft),
        (Mode::Normal, "l", Action::MoveRight),
        (Mode::Normal, "gg", Action::GotoTop),
        (Mode::Normal, "G", Action::GotoBottom),
        (Mode::Normal, "<C-d>", Action::HalfPageDown),
        (Mode::Normal, "<C-u>", Action::HalfPageUp),
        (Mode::Normal, "<C-w>h", Action::FocusLeft),
        (Mode::Normal, "<C-w>j", Action::FocusDown),
        (Mode::Normal, "<C-w>l", Action::FocusRight),
        (Mode::Normal, "<CR>", Action::Select),
        (Mode::Normal, "za", Action::ToggleFold),
        (Mode::Normal, ":", Action::EnterCommand),
        (Mode::Normal, "<Esc>", Action::Cancel),
        (Mode::Normal, "<C-c>", Action::Cancel),
        // i, a and o are identical in Phase 1: there is one input bar and no text to insert around.
        (Mode::Normal, "i", Action::EnterInsert),
        (Mode::Normal, "a", Action::EnterInsert),
        (Mode::Normal, "o", Action::EnterInsert),
        // Recovery of a failed or unconfirmed send (D-06): `r` retries the message under the
        // messages-pane cursor. `dd` (`delete_message`) discards a failed or unconfirmed one
        // locally, and on one of the user's own server messages asks to delete it. `d` is unbound
        // alone, so `dd` is a pure prefix like `gg`. `e` (`edit_message`) edits one of the user's
        // own server messages. All are silent no-ops on any other message or pane.
        (Mode::Normal, "r", Action::RetrySend),
        (Mode::Normal, "e", Action::EditMessage),
        (Mode::Normal, "dd", Action::DeleteMessage),
        // `gr` (`show_reactions`) opens the popup of who reacted to the message under the cursor.
        // `g` is already the prefix of `gg`, so this adds a second continuation, not a new prefix.
        (Mode::Normal, "gr", Action::ShowReactions),
        // `gc` (`open_channel`) opens the room a `#channel` in the message under the cursor names,
        // or a picker when it names several. Another continuation of the `g` prefix.
        (Mode::Normal, "gc", Action::OpenChannel),
        // `gp` (`show_image`) opens the image of the message under the cursor at full size.
        (Mode::Normal, "gp", Action::ShowImage),
        // `gt` (`open_thread`) opens the thread of the message under the cursor in the thread pane.
        // It is a third continuation of the `g` prefix. `close_thread` has no default key: `Esc`
        // closes the pane while it is focused.
        (Mode::Normal, "gt", Action::OpenThread),
        // `gT` (`list_threads`) opens the thread list of the open room, a pane in the thread column. A sixth
        // continuation of the `g` prefix.
        (Mode::Normal, "gT", Action::ListThreads),
        // `gf` (`toggle_follow_thread`) follows the thread of the message under the cursor, or
        // unfollows it when it is already followed. A fifth continuation of the `g` prefix.
        (Mode::Normal, "gf", Action::ToggleFollowThread),
        // `gR` (`reply_in_thread`) starts a reply to the message under the cursor, opening its
        // thread even when it has no replies yet. `<C-b>` (`toggle_reply_to_channel`) in Insert
        // mode marks the reply to be shown in the channel too.
        (Mode::Normal, "gR", Action::ReplyInThread),
        // `+` (`react_message`) opens the reaction picker for the message under the cursor.
        (Mode::Normal, "+", Action::ReactMessage),
        // `<C-k>` (`switch_room`) opens the room switcher from any pane.
        (Mode::Normal, "<C-k>", Action::SwitchRoom),
        // `?` opens the context popup; `?` again opens the help overlay, generated from this very
        // keymap. `show_help` has no default key.
        (Mode::Normal, "?", Action::ShowContext),
        // Find in the open room: `/` opens the prompt, `n` goes to the next match toward older
        // messages (the way a chat is read) and `N` back toward newer ones.
        (Mode::Normal, "/", Action::SearchStart),
        (Mode::Normal, "n", Action::SearchNext),
        (Mode::Normal, "N", Action::SearchPrev),
        // Insert rows stay single-key so typing never waits for the sequence timeout. Shift-Enter
        // is indistinguishable from Enter in legacy terminals, so a newline is Alt-Enter or Ctrl-j.
        (Mode::Insert, "<Esc>", Action::Cancel),
        (Mode::Insert, "<C-c>", Action::Cancel),
        // Enter sends the draft to the open room; Alt-Enter and Ctrl-j insert a newline instead
        // (InsertNewline, rebindable in keymap.toml, D-09).
        (Mode::Insert, "<CR>", Action::SubmitInput),
        (Mode::Insert, "<A-CR>", Action::InsertNewline),
        (Mode::Insert, "<C-j>", Action::InsertNewline),
        (Mode::Insert, "<C-b>", Action::ToggleReplyToChannel),
        (Mode::Insert, "<BS>", Action::DeleteBack),
        (Mode::Insert, "<C-w>", Action::DeleteWordBack),
        (Mode::Insert, "<C-u>", Action::DeleteToLineStart),
        (Mode::Insert, "<Left>", Action::CursorLeft),
        (Mode::Insert, "<Right>", Action::CursorRight),
        (Mode::Insert, "<Up>", Action::CursorUp),
        (Mode::Insert, "<Down>", Action::CursorDown),
        (Mode::Insert, "<Home>", Action::CursorHome),
        (Mode::Insert, "<End>", Action::CursorEnd),
        (Mode::Command, "<CR>", Action::ExecuteCommand),
        (Mode::Command, "<Esc>", Action::Cancel),
        (Mode::Command, "<C-c>", Action::Cancel),
        (Mode::Command, "<BS>", Action::DeleteBack),
    ]
}

/// The `vim-extras` preset: [`default_bindings`] plus Tab and Shift-Tab pane focus cycling in
/// Normal mode (D-14). Nothing else is added.
pub fn vim_extras_bindings() -> Vec<(Mode, &'static str, Action)> {
    let mut rows = default_bindings();
    rows.push((Mode::Normal, "<Tab>", Action::FocusNext));
    rows.push((Mode::Normal, "<S-Tab>", Action::FocusPrev));
    rows
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

    #[test]
    fn vim_extras_parses() {
        let rows = vim_extras_bindings();
        let keymap = Keymap::from_bindings(&rows).expect("vim-extras bindings parse");
        for (mode, notation, action) in &rows {
            let chords = parse_keys(notation).unwrap();
            assert!(
                matches!(keymap.lookup(*mode, &chords), Lookup::Exact(bound) if bound == action),
                "{notation}"
            );
        }
    }

    #[test]
    fn vim_extras_adds_only_tab_rows() {
        let base = default_bindings();
        let extras = vim_extras_bindings();
        assert_eq!(extras.len(), base.len() + 2);
        assert!(extras[..base.len()] == base[..]);
        assert!(matches!(
            extras[base.len()],
            (Mode::Normal, "<Tab>", Action::FocusNext)
        ));
        assert!(matches!(
            extras[base.len() + 1],
            (Mode::Normal, "<S-Tab>", Action::FocusPrev)
        ));
    }

    #[test]
    fn keymap_d_is_prefix() {
        let keymap = default_keymap();
        let d = parse_keys("d").unwrap();
        let dd = parse_keys("dd").unwrap();
        let r = parse_keys("r").unwrap();
        assert!(matches!(keymap.lookup(Mode::Normal, &d), Lookup::Prefix));
        assert!(matches!(
            keymap.lookup(Mode::Normal, &dd),
            Lookup::Exact(Action::DeleteMessage)
        ));
        assert!(matches!(
            keymap.lookup(Mode::Normal, &r),
            Lookup::Exact(Action::RetrySend)
        ));
        let e = parse_keys("e").unwrap();
        assert!(matches!(
            keymap.lookup(Mode::Normal, &e),
            Lookup::Exact(Action::EditMessage)
        ));
        // The newline keys of the Insert mode stay as documented (D-09).
        for notation in ["<A-CR>", "<C-j>"] {
            let chords = parse_keys(notation).unwrap();
            assert!(matches!(
                keymap.lookup(Mode::Insert, &chords),
                Lookup::Exact(Action::InsertNewline)
            ));
        }
    }

    #[test]
    fn question_mark_is_show_context() {
        let keymap = default_keymap();
        let chords = parse_keys("?").unwrap();
        assert!(matches!(
            keymap.lookup(Mode::Normal, &chords),
            Lookup::Exact(Action::ShowContext)
        ));
        assert!(keymap.clashes(Mode::Normal).is_empty());
        assert!(
            !keymap
                .bindings(Mode::Normal)
                .iter()
                .any(|(_, a)| matches!(a, Action::ShowHelp)),
            "show_help has no default key"
        );
    }

    #[test]
    fn search_keys_are_bound_by_default() {
        let keymap = default_keymap();
        for (keys, want) in [
            ("/", Action::SearchStart),
            ("n", Action::SearchNext),
            ("N", Action::SearchPrev),
        ] {
            let chords = parse_keys(keys).unwrap();
            match keymap.lookup(Mode::Normal, &chords) {
                Lookup::Exact(got) => assert!(*got == want, "{keys}"),
                _ => panic!("{keys} is not bound"),
            }
        }
        assert!(keymap.clashes(Mode::Normal).is_empty());
    }

    #[test]
    fn search_keys_can_be_rebound_and_removed() {
        let config = crate::test_util::config_from_keymap(
            "[normal]\n\"<C-f>\" = \"search_start\"\n\"n\" = \"none\"\n",
        );
        let ctrl_f = parse_keys("<C-f>").unwrap();
        assert!(matches!(
            config.keymap.lookup(Mode::Normal, &ctrl_f),
            Lookup::Exact(Action::SearchStart)
        ));
        let n = parse_keys("n").unwrap();
        assert!(!matches!(
            config.keymap.lookup(Mode::Normal, &n),
            Lookup::Exact(_)
        ));
    }

    #[test]
    fn question_mark_can_be_rebound_to_show_help() {
        let config = crate::test_util::config_from_keymap("[normal]\n\"?\" = \"show_help\"\n");
        let chords = parse_keys("?").unwrap();
        assert!(matches!(
            config.keymap.lookup(Mode::Normal, &chords),
            Lookup::Exact(Action::ShowHelp)
        ));
    }

    #[test]
    fn keymap_g_is_prefix() {
        let keymap = default_keymap();
        let g = parse_keys("g").unwrap();
        let gg = parse_keys("gg").unwrap();
        assert!(matches!(keymap.lookup(Mode::Normal, &g), Lookup::Prefix));
        assert!(matches!(
            keymap.lookup(Mode::Normal, &gg),
            Lookup::Exact(Action::GotoTop)
        ));
        // Neither a bare <C-w> nor <C-w>k is bound (D-01 lists h/j/l only).
        let ctrl_w = parse_keys("<C-w>").unwrap();
        let ctrl_w_k = parse_keys("<C-w>k").unwrap();
        assert!(matches!(
            keymap.lookup(Mode::Normal, &ctrl_w),
            Lookup::Prefix
        ));
        assert!(matches!(
            keymap.lookup(Mode::Normal, &ctrl_w_k),
            Lookup::None
        ));
    }
}
