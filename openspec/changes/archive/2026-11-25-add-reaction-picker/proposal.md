# Proposal

## Why

The client can now show reactions and who gave them, but not give one: the user has to leave the terminal to answer a message with a "+1" or a "done". Reacting is the cheapest way to reply in a busy room, so the keyboard-only core value is incomplete without it. A bare shortcode prompt is awkward on a company server, where the useful emojis are custom ones whose exact names nobody remembers, so reacting needs a search, not just a text field.

## What Changes

- A new bindable action `react_message`, bound to `+` in Normal mode, opens the **reaction picker** for the message under the cursor: a popup with a search line, in the family of the `gr` popup.
- The picker has two states, as a vim buffer has. It opens **browsing**: it lists the message's current reactions, the user's own marked, to toggle them quickly. `/` or `i` starts **searching**, where the list follows what is typed: unicode emoji by name and by every shortcode, the user's `[emoji_aliases]`, and the server's custom emojis. The text typed is matched as a substring, and results are ranked exact, then prefix, then substring.
- While browsing, `j`, `k`, `<C-n>`, `<C-p>` and the arrow keys move through the list, `Enter` sends the selected reaction, and `q` or `Esc` closes. `/` starts a new search and `i` or `a` continue the last one. While searching, letters are text (so `j` and `k` are typed), `<C-n>`, `<C-p>` and the arrows move, `Enter` sends, `Esc` goes back to browsing with the results kept, and `Backspace` on an empty search does too. `<C-c>` closes from either state.
- Sending uses `chat.react` with an explicit add or remove: removing when the user already gave that reaction on the message, adding otherwise. The row and the `gr` popup update from the message the server streams back, so there is no optimistic edit.
- The server's custom emojis are fetched once with `emoji-custom.list` the first time the picker opens and kept for the session. If the fetch fails, the picker works with unicode emoji and aliases only.
- Refusals: a detached room, an unknown own username, and a pending, deleted or system message do nothing or say why, like edit and delete. A server refusal shows one fixed status line and never the server's text.
- Out of scope: reacting from the `gr` popup, glyph input (typing 🎉 to find `tada`), recently used or frequently used ranking, reaction notifications, and drawing custom emoji as images.

## Capabilities

### New Capabilities
- `reaction-picker`: opening the picker on a message, searching unicode emoji, aliases and custom emojis, choosing with the keyboard, and sending or taking back a reaction.

### Modified Capabilities
- `keymap`: the action vocabulary gains `react_message`, bound to `+` by default.
- `context-help`: the context popup lists the react action on a message the user can react to.
- `theme`: new themeable elements for the picker.
- `message-history`: "No writing while detached" also covers reacting.

## Impact

- Code: `src/api/mod.rs` (`chat.react` and `emoji-custom.list`) and `src/api/wire.rs`, `src/effect.rs` (two effects and their client-error arms), `src/action.rs` and `src/input/{actions,keymap}.rs`, `src/app/` (a picker state and module, the custom-emoji cache, `update.rs` key routing, `context.rs`, `help.rs`), `src/ui/` (a picker drawer), `src/config/theme.rs`.
- Dependency: none new. The `emojis` crate already in the tree supplies names and shortcodes to search.
- Server contract: the accepted shortcode set is the server's older emojione-style one, not the crate's GitHub one. A probe against the Docker server settles which offered names are accepted and feeds a small translation table, so a rejection is the exception.
- Privacy: nothing is logged. Search text, shortcodes and custom emoji names are never written to a log, an error or a status line, and the wire types do not print.
