# Design

## Context

See `proposal.md` for the motivation and `specs/message-emoji/spec.md` for the behavior. Today:

- The server stores `msg` as typed. A check on the Docker server (8.9.1) showed `ship :tada: it :+1: `:smile:` now 10:100:20 🎉` coming back unchanged, and the server's own parsed form (`md`) marking `:tada:` and `:+1:` as emoji, `:smile:` inside backticks as inline code, `10:100:20` as plain text, and the unicode character as an emoji too. So the rules a client should follow are the parser's, and a plain scan can reproduce them.
- `ui::emoji::resolve` turns a name into the text to draw for reactions: alias, then the `emojis` crate (with a small map of legacy names and `_toneN` handling), then the raw `:name:`. It does not know the names the server uses where they differ from the crate's.
- `ui::emoji_names` holds, for each crate emoji whose first shortcode the server rejects, the name the server accepts (`SEND_AS`, crate name to server name). That is the data needed to go the other way.
- A message body is drawn by `wrap::render_rows`, which calls `body_rows`, which wraps `msg.body`. The reducer's row model measures through the same function, so what is drawn there is what scrolling counts.
- Editing loads `msg.body`; search matches `msg.body`.

## Goals / Non-Goals

**Goals:**
- Shortcodes in message text look like emoji, by the same resolution as reactions, following the server's rules for what a shortcode is.
- Nothing stored changes, and the pane and the row model cannot disagree about a message's height.
- Server-named emoji draw correctly everywhere a name is resolved.

**Non-Goals:**
- Changing search (it matches the stored text), custom emoji images, and any other markdown.

## Decisions

### Scan the text, do not read the server's parsed form
A function `draw_shortcodes(text, aliases)` walks the body once and replaces each shortcode that counts and resolves. It returns the text untouched, without allocating, when it holds no colon.

*Alternative:* use the `md` field the server sends, which has the emoji already tagged. Rejected: older servers do not send it, it is large (it was already dropped from the fixtures to keep them small), a message still being sent has none, and the rules are simple enough to follow exactly. The scan keeps working for all of them.

### The rules, from the server's behavior
- A shortcode is `:` name `:` with a name of letters, digits, `_`, `+` and `-`, up to 64 characters.
- The opening colon is not preceded by a letter or digit, and the closing colon is not followed by one (`10:100:20` stays text; `(:tada:)` and `:tada::+1:` work).
- Backticks toggle a code state: inside it nothing is replaced. Three backticks toggle three times, so a fenced block is code from its opening fence to the closing one. An unbalanced backtick leaves the rest of the message as written, the safe direction.
- A name that does not resolve is left as written, and the scan goes on from the next character, so `:a:tada:` still finds `:tada:`.

### One lookup, three layers, used by both reactions and messages
`ui::emoji` gains `lookup(name, aliases) -> Option<String>` (the existing layers, plus a new one) and `resolve` becomes `lookup` or the raw `:name:`. The new layer maps a name the server uses back to the crate's emoji through a map built once from `SEND_AS` (server name to crate name), so a reaction, a picker row and a message all resolve `flag_af` to the flag. Order stays alias, crate and legacy names, server names, tone suffix, raw. This also fixes a gap in the picker change: a reaction added under a renamed name used to draw raw.

### Drawn in `body_rows`, for ordinary messages only
`body_rows` runs the scan over `msg.body` before wrapping, only for messages of kind `Normal`: a system message is fixed text the client wrote itself and a deleted line shows no body. Because the wrap works on the drawn text, widths are measured on what is on screen and the row model, which goes through the same function, stays in agreement. Pending and failed local rows go through the same path, so a message looks the same before and after the server confirms it.

*Alternative:* substitute when the message is mapped from the wire. Rejected: it would store the substituted text, so edit would load glyphs instead of what was typed, and a changed alias would need a reload.

### Search is left alone
Search matches `msg.body`, and highlighting happens on the drawn rows. So a search for `tada` finds `:tada:` but highlights nothing (the drawn row has the glyph), and a search for `🎉` does not find it. This is the same kind of mismatch the code already documents for a match that crosses a wrap boundary. Fixing it means search matching the drawn text, which touches the jump, the count and the results list, and is a change of its own.

## Risks / Trade-offs

- [A false positive turns text into an emoji] -> The server's own boundary rule is followed, so `10:100:20` stays text; names must also resolve, so most colon pairs do nothing.
- [Wide and modifier glyphs can disagree between terminals] -> Widths are measured on the drawn text with `unicode-width`, as for reactions; kitty draws them as one glyph, and the pty harness cannot show it, so a person checks.
- [Search finds a message it cannot highlight, or misses a glyph search] -> Known and documented; see above.
- [The scan runs on every measure of every message] -> Text without a colon returns at once, and a colon costs one pass with a few hash lookups.
- [An unbalanced backtick leaves the rest of a message as written] -> The safe direction: nothing is changed that should not be.

## Open Questions

- None that change the specs or the tasks.
