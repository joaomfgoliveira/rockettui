# Tasks

## 1. Resolving names

- [x] 1.1 Add the server-to-crate direction to `ui::emoji_names` and a `lookup` in `ui::emoji` that reports an unknown name (`resolve` becomes `lookup` or the raw `:name:`), resolving names the server uses (alias, crate and legacy names, server names, tone suffix). Verify: unit tests for every layer and its order, and a test that for every renamed emoji of the table the server's name draws the same glyph as the crate's. A check against the server's own emoji data (`emojibase-data` in the dev container) also found four table entries that would have sent a different emoji (the alternates builder now never offers a name the crate uses for another emoji, and the table was regenerated); all 419 renamed names and every as-is name now agree with the server's data.
- [x] 1.2 Check that reactions use it: the row, the `gr` popup and the picker draw a reaction named `flag_af` as the flag. Verify: a test in each of the three that a `flag_af` reaction shows the glyph and not the raw name.

## 2. Drawing shortcodes in message text

- [x] 2.1 Add `draw_shortcodes(text, aliases)` to `ui::emoji`: the boundary rule, the allowed name characters and the length limit, code spans and fences left alone, an unresolved name left as written, no allocation without a colon. Verify: unit tests for each scenario of "Shortcodes in message text are drawn as emoji", "Where a shortcode counts" and "Code is left alone", including adjacent shortcodes, `a:tada:`, `:tada:b`, `10:100:20`, an unbalanced backtick and a fenced block.
- [x] 2.2 Draw message bodies through it in `wrap::body_rows` for ordinary messages only, so wrapping and the row model use the drawn text, and a pending local row looks like the confirmed one. Verify: tests that a body draws with the glyph in both layouts, that a system message and a deleted line are untouched, that no row is wider than the pane with shortcodes that change width, that the row count equals the row model's, and an insta snapshot at 52x20.
- [x] 2.3 Keep the stored text: editing a message loads what was typed. Verify: a reducer test that `e` on an own message sent as `nice :tada:` puts `nice :tada:` in the input bar, and that the held message body is unchanged after drawing.

## 3. Integration

- [x] 3.1 Drive the real binary on a pty against the Docker server (8.9.1): another user posts messages with shortcodes, a server-only name, code spans and a time, and reacts with a flag; check what is drawn, and that editing an own message shows the shortcode. Verify: a throwaway driver reports every check passing and removes what it posted.
- [x] 3.2 Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets` and `openspec validate render-emoji-in-messages --strict`. Verify: all pass with no warnings.
- [x] 3.3 Look at it once by eye in kitty: shortcodes in messages, wide and skin-tone glyphs inside wrapped lines, and the reaction row with a flag. Verify: a person confirms it looks right or notes any misalignment. Confirmed by the user.

## Workflow follow-up

- Archive the change with `/openspec-archive-change` after review so the specs are synced. There is no spec dependency on `add-reaction-picker` (the code builds on its name table, the specs do not overlap), so the two can be archived in either order.
- A later change can make search match the drawn text.
