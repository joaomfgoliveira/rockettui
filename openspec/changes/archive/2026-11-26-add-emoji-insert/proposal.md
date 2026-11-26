# Proposal

## Why

Writing an emoji in a draft today means knowing its shortcode by heart or pasting a glyph from elsewhere, while the reaction box already knows every emoji. Typing `:` in Insert mode is how people ask for an emoji in every chat client, so it should open the box, ready to search, and put the chosen emoji in the draft.

## What Changes

- Typing `:` in Insert mode, where a shortcode could start, opens the emoji box (from `add-emoji-box-grid`) above the input bar, already searching. What the user types next goes to the box's search line.
- `Enter` puts the selected emoji in the draft in place of the `:`: the character itself for a unicode emoji or an alias, `:name:` for a custom emoji. The box closes and the user keeps typing.
- A character that cannot be part of a shortcode (a space, `)`, punctuation) closes the box and goes into the draft after what was typed, so `:)`, `10 :` and `: ` stay plain text. `Esc` and `<C-c>` close the box and leave the `:` in the draft; `Backspace` on an empty search takes the `:` back.
- A colon that cannot start a shortcode (after a letter or digit, after another colon, or inside backticks) does not open the box, so `10:30`, `std::fs` and code are not interrupted.
- It works in the room's input bar, the thread's input bar and when editing a message.
- Emoji the server refuses as reactions are still offered here, because a character in a message needs no server name. The picks count towards "Frequently used".

Out of scope: drawing custom emoji images in the draft (`add-input-custom-emoji`), a setting to turn the colon trigger off (see design), autocompleting an emoji as the user types a whole `:name` without the box.

## Capabilities

### New Capabilities
- `emoji-insert`: opening the emoji box from a colon in Insert mode, what each key does there, what is put in the draft, and which colons do not open it.

### Modified Capabilities

None. The box's own rules stay in `reaction-picker`; this capability only adds a second way to open it and a second thing it does with the choice.

## Impact

- `src/app/react.rs` (the picker gets a target: react to a message, or insert into the draft), `src/app/update.rs` (`InsertChar` and `Paste` hand a colon to it), `src/ui/picker.rs` (the box is anchored above the live input bar for the insert target), `src/app/input_buf.rs` (a helper for the text before the cursor).
- Depends on `add-emoji-box-grid`; it must be implemented after it.
- Help text for Insert mode is updated.
