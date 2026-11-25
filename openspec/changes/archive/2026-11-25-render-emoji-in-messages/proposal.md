# Proposal

## Why

People write emoji in messages as shortcodes (`:tada:`, `:+1:`): the server stores the text exactly as typed and only the web client turns it into a picture. The terminal client shows those messages with the raw `:tada:` text, so a busy room is full of colons and words where everyone else sees emoji. Reactions already draw as glyphs, so messages look inconsistent next to them.

A related gap: the resolver only knows the `emojis` crate's names, but the server names many emoji differently (flags are `flag_af`, not `afghanistan`). A reaction added under such a name, which the picker does, or written in a message by another client, is shown raw.

## What Changes

- A shortcode in the body of a message is drawn as its emoji when it resolves, using the same resolution as reactions: the user's `[emoji_aliases]` first, then the unicode table, then the names the server uses, with a `_toneN` suffix applied as a skin tone. One that does not resolve (a custom emoji with no alias, a typo) stays as written.
- A shortcode counts only where the server's own parser counts it: delimited by non-alphanumeric characters or the edges of the text, so `10:100:20` stays text, and never inside inline code or a fenced code block.
- Only the drawing changes. The stored text is untouched: editing a message still loads what was typed, and lines wrap by the width of what is drawn.
- Reactions, in the row, the `gr` popup and the picker, resolve the names the server uses too, so `:flag_af:` draws as the flag.
- The `[emoji_aliases]` setting now also applies to message text.
- Out of scope: search (it keeps matching the stored text, so a search for `🎉` does not find `:tada:`, and a match inside a shortcode is found but not highlighted), custom emoji images, and any markdown beyond emoji.

## Capabilities

### New Capabilities
- `message-emoji`: drawing emoji shortcodes in message text, where a shortcode counts, code left alone, the stored text kept, and server-named emoji in reactions.

### Modified Capabilities
- `configuration`: `[emoji_aliases]` applies to message text as well as reactions.

## Impact

- Code: `src/ui/emoji.rs` (a lookup that reports "unknown", server names, and the scanner), `src/ui/emoji_names.rs` (the server-to-crate direction), `src/ui/wrap.rs` (the body goes through the scanner before it is wrapped).
- Dependency: none new.
- Cost: the scanner runs for each visible message each time rows are measured; text without a colon is returned untouched without allocating.
- Privacy: nothing is logged; the scanner only produces the text that is drawn.
