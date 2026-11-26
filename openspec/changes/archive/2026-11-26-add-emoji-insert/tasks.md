# Tasks

Depends on `add-emoji-box-grid` being implemented first.

## 1. The picker target

- [x] 1.1 Rename the picker state to the emoji box with a `Target` (`React { room, id }`, `Insert`) and make `held`, `close_if_stale`, `view` and `send_selected` match on it; verify the whole existing reaction-picker test suite passes unchanged
- [x] 1.2 Let `sections()` take the purpose: the insert target has no "On this message", keeps the emoji the server refuses and has no own/count marks; verify with unit tests for both purposes

## 2. The trigger

- [x] 2.1 Add `InputBuffer::text_before_cursor` and the pure trigger rule (previous character not alphanumeric and not `:`, even number of backticks before the cursor); verify with tests for the scenarios: start of draft, after a space, `10:30`, `std::fs`, `(:`, inside inline code and inside a fenced block
- [x] 2.2 In the `InsertChar(':')` handler open the box with `Target::Insert`, searching, after inserting the colon; verify with a reducer test in the room bar, in a thread bar and while editing, and that a `Paste` with a colon does not open it

## 3. Keys while open from a colon

- [x] 3.1 Route name characters to the search, `Backspace` (including on an empty search), `Esc`, `<C-c>`, arrows, `<Tab>`, `<S-Tab>` and `<C-t>`; verify with key-driven tests for each scenario, including that `j` is text
- [x] 3.2 Implement the fall-through for any other character and for a paste with such characters (insert the typed search, then the character); verify with tests for `:)`, `:tada ` and `:tada:`
- [x] 3.3 Implement `Enter`: replace the colon with the glyph, the alias text or `:name:` for a custom emoji, at the cursor and in the middle of a draft; with no match close and change nothing; verify with tests for unicode, skin tone, alias, custom, middle-of-draft and no match
- [x] 3.4 Count the pick for "Frequently used" on `Enter` and push the save effect from `add-emoji-box-grid`; verify with a reducer test, and that `Esc` counts nothing

## 4. Drawing

- [x] 4.1 Anchor the panel above the live bar for the insert target (room bar, thread bar), with a hint that says Enter inserts; verify with insta snapshots for the room bar, the thread bar and 40x8
- [x] 4.2 Update the Insert-mode help text to mention the colon; verify with the updated help snapshot

## 5. Integration

- [x] 5.1 Run `cargo test`, `cargo clippy` and `cargo fmt --check`; verify all pass
- [x] 5.2 Ask the user to try it against the local Docker server: `:` then a search, the fall-through cases, a thread reply and an edit; verify by the user's confirmation

## Workflow follow-up

- Update `/home/joliveira/Documents/Obsidian Vault/Personal/Projects/Rockettui/Features.md` when the change is archived.
- Archive after `add-emoji-box-grid`, so its `reaction-picker` and `emoji-recents` specs exist first.
