# Design

## Context

After `add-emoji-box-grid` the box is `ReactPicker { room, id, query, selected, typing }`, derived into sections on each draw, and it takes every key while `app.react_picker` is `Some` (checked in `reduce` before the keymap, so it also stops `KeyTimeout`). Insert-mode characters reach the draft as `Action::InsertChar(c)` (from the matcher's `Outcome::Text`) and `Action::Paste`; `edit_input` applies an edit to `app.input`, which is always the live draft, whether room, thread or edit. `thread_bar_is_live()` says whether the thread's own bar is the live one. See proposal.md for the motivation and `add-emoji-box-grid`'s design for the grid.

## Goals / Non-Goals

**Goals:**
- One box, two uses: no second popup, no second search. The target decides what Enter does.
- Typing `:` must never lose or reorder text the user typed, whatever happens next.

**Non-Goals:**
- A setting to turn the trigger off; autocompleting without the box; images in the draft (`add-input-custom-emoji`).

## Decisions

### The picker gets a target
`ReactPicker` becomes `EmojiBox { target: Target, query, selected, typing }` with `Target::React { room, id }` and `Target::Insert`. Everything that needs a message (`held`, `close_if_stale`, the "On this message" section, the own/count marks, `send_selected`) matches on the target; the insert target has none, so `close_if_stale` only checks that the mode is still Insert. `view()` takes the purpose (react or insert) into `sections()` so the insert target skips the message section and keeps emoji the server refuses.
*Alternative:* a separate `InsertPicker` state. Rejected: it would duplicate the grid, search, tone and keys that the first change just built, and the two would drift.

### The trigger lives in the `InsertChar` handler
`update(Action::InsertChar(':'))` in Insert mode, with no box open, inserts the colon as it does now and then asks `react::open_insert(app)`, which checks the rule on the text before the cursor: previous character is not alphanumeric and not `:`, and the number of backticks before the cursor in the whole draft is even. The rule is a pure function on `InputBuffer` (`text_before_cursor`) so the thread bar and edit mode need nothing special. A paste never calls it.
*Alternative:* a keymap action bound to `:` in Insert. Rejected: the keymap forbids multi-key sequences that start with a plain character in Insert (rule A4) and a user could remove the binding by accident; a rule on typed text is not a key binding.

### While open, the draft is not touched until the box ends
The colon stays in the draft, and the typed search lives only in the box. Every ending then has one simple job:
- `Enter` on a result: delete the colon (the character before the cursor, checked) and insert the text, in one `edit_input`.
- fall-through character or paste: close, then `insert_str(query)` and the character, so the draft is exactly what a plain typing would have made.
- `Esc` / `<C-c>`: close, keep the colon.
- `Backspace` on an empty search: close and `backspace` the colon.
The "character before the cursor is the colon" check is defensive; if it ever fails, the text is inserted without deleting anything.

### What is inserted
`Candidate` already holds the glyph (unicode, toned, or an alias's text) and the source. Insert text is `:name:` for `Source::Custom` and the glyph otherwise. The pick is counted under the candidate's name (the crate's first shortcode for an emoji the server refuses), so "Frequently used" can show it again.

### Where the box is drawn
`ui::picker::panel` takes the target. For `Target::React` it is today's panel at the bottom of the column. For `Target::Insert` it ends at the top edge of the live bar (`areas.thread_input` when `thread_bar_is_live()`, else `areas.input`), has that bar's x and width, and the same steady height rule. The hardware cursor goes to the search line as for any box, so it leaves the draft while the box is open.

### No browsing state for the insert target
The box opens searching and `Esc` closes it, so `typing` is always true and the `/`, `i`, `a` keys do not exist there. Letters are the search; the grid is reached with the arrows and `<Tab>`.

## Risks / Trade-offs

- [A colon the user means as text opens the box every time: "see :" ] → only when it could start a shortcode; any non-name character falls through with nothing lost, and `Esc` keeps the colon. A setting is the deferred escape hatch.
- [A pasted or typed fast burst: `:tada:` typed quickly] → each key is processed in order by the reducer, so the fall-through at the second colon yields `:tada:` exactly.
- [Hidden state in the middle of Insert mode: the user may not realise the box owns the keys] → the box shows its search line with the cursor and a hint; `<C-c>` always closes it.
- [The box covers messages above the bar] → intended; it is transient and the draft stays visible.

## Migration Plan

None. The behaviour is on for everyone.

## Open Questions

- A `[input]` setting to turn the colon trigger off. Deferrable: adding it later is an additive config change.
