# Tasks

## 1. Data path: wire, domain, mapping

- [x] 1.1 Capture real `reactions` payloads (history and `stream-room-messages`, with a skin-tone, a custom and an unknown shortcode) from the Docker server 8.9.1 and 9.0.0 into `src/api/fixtures/`, never edited afterwards, and evaluate the unicode shortcode crate against the captured names (license, size, coverage); record the choice in design.md. Verify: fixtures exist and a throwaway lookup test shows the coverage.
- [x] 1.2 Add `reactions` to `WireMsg` with a map-visitor `Deserialize` that keeps server order and tolerates absent, `null` and malformed values. Verify: unit tests for order, empty, `null`, a bad entry and a missing `usernames`.
- [x] 1.3 Add `Reaction` and `Message.reactions`, map them in `api::map::message`, and make the redacting `Debug` print only the reaction count. Verify: mapping tests against the captured fixtures and a `Debug` test that no name or username appears.

## 2. Resolution, config and theme

- [x] 2.1 Add the shortcode resolver (`alias` then unicode then raw, `_toneN` handling) behind one function. Verify: unit tests for a known name, each tone, a base without tones, a custom name, an alias that overrides the table, and an alias for a `_toneN` name.
- [x] 2.2 Parse and validate `[emoji_aliases]` in `config.toml` (no colons, width <= 8, sanitizing must not change it, entry named in the error); the repo has no config reference document, so the spec delta is the documentation. Verify: config tests for each scenario of the spec, including the error-screen message.
- [x] 2.3 Add the theme elements `messages.reactions`, `messages.reactions_own` and `reactions.{border,title,name,users,own}` with defaults (the spec delta is the documentation; there is no theme reference document). Verify: theme tests that each element is settable and that defaults keep the own style distinct.

## 3. Reaction row

- [x] 3.1 Extend `RowCtx` with the alias table and own username, and add the reaction rows to `wrap::block_rows` and `render_rows` (whole-item wrapping, ellipsis for an oversized item, both layouts, cursor highlight, sanitizing). Verify: unit tests for height with and without reactions at several widths, and insta snapshots for compact and two-line at 52x20.
- [x] 3.2 Test that a taller message above the cursor keeps the cursor on the same message and screen row, like the existing edit-above-the-cursor test. Verify: the new test passes.

## 4. Live and history updates

- [x] 4.1 Make the held-message change check consider reactions (rename `is_edit` accordingly) and replace in place through the existing anchored path without setting `edited`. Update `redelivery_without_a_change_is_ignored` so only a pin is a no-op. Verify: tests for reaction added, removed to none, pin untouched, reaction on a message not held, and cursor stability.
- [x] 4.2 Copy `reactions` in the history merge and in the reconnect reconcile for messages the refetched page covers; leave older messages alone. Verify: tests for reacted-while-offline, removed-while-offline and older-than-the-page, plus a merge test of an overlapping page.
- [x] 4.3 Add a realtime test that feeds the live reaction frame captured in 1.1 through the stream parser and the reducer. Verify: the test passes against the unedited fixture.

## 5. Popup and keymap

- [x] 5.1 Add the `show_reactions` action name, its `Action` variant and the default `gr` binding in every preset, with the help text entry. Verify: keymap tests for the default, rebind, unbind with `none`, and that `gg` still resolves.
- [x] 5.2 Add the popup state (room id and message id), the pure content builder (grouping, `you` first, own highlight, `N more` cut) and the no-op cases (no reactions, other focus). Verify: unit tests per scenario in the spec, including a live update while open and closing when the last reaction is removed.
- [x] 5.3 Draw the popup in `ui/` with the `reactions.*` styles, sized to content with a cap, usable at the smallest size, and swallow the closing key in `update.rs` like the `?` popup. Verify: insta snapshots at 80x24 and the smallest size, and a test that any key closes without moving the cursor.
- [x] 5.4 List the reactions action in the `?` popup for messages with reactions, using the live keys. Verify: context tests for with and without reactions and for a rebound key.

## 6. Integration

- [x] 6.1 Run the real binary on a pty against the Docker server (8.9.1) while other users react through the API with each kind of shortcode (standard, skin-tone, aliased custom, plain custom), and check the row, the `gr` popup, the live update with the popup open, the key that closes it, and the popup closing itself when the last reaction is removed. Verify: 17 of 17 checks of a throwaway driver pass (the screen model of `scripts/smoke_pty.py` cannot draw skin-tone modifier sequences as one glyph, so that glyph was checked in the bytes the app wrote).
- [x] 6.3 Look at it once by eye in kitty: the skin-tone glyph, the aliased and raw custom emoji, and the column alignment of the row and the popup. Verify: a person confirms it looks right, or notes any width misalignment found (a pty screen model cannot show how a real terminal draws wide and modifier sequences). Confirmed by the user, who saw the reactions drawn in kitty and reported no misalignment.
- [x] 6.2 Run `cargo test`, `cargo clippy` and `openspec validate show-message-reactions --strict`. Verify: all pass.

## Workflow follow-up

- Archive the change with `/openspec-archive-change` after review so the specs are synced.
- A later change can add sending reactions (`chat.react`) and a spike on custom emoji images through the kitty graphics protocol.
