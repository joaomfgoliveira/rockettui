# Tasks

## 1. Server contract

- [x] 1.1 Write `dev/probe_react_names.py` (standard library, loopback only, output holds only counts and a verdict, never a credential): as the app user, post a throwaway message, then for every shortcode of every emoji the `emojis` crate knows, and then for the other names the server's own emoji data gives it (`dev/emoji_alternates.py`), try `chat.react` add and take back until one is accepted, with the REST rate limiter turned off as admin and restored in a `finally`. Run it against the Docker server (8.9.1) and write the exceptions (emoji whose first shortcode is rejected but another is accepted, and emoji with none) into a generated `src/ui/emoji_names.rs`. Verify: the last probe line says `PROBE OK` with the setting restored, and a unit test shows every table entry names an emoji the crate knows. Measured on 8.9.1: 1451 of 1870 emoji accepted under their first shortcode, 419 renamed (flags are `flag_af`, not `flag_afghanistan`), none left unsendable.
- [x] 1.2 Add `Client::react(id, emoji, add)` for `POST chat.react` with the body `messageId`, `emoji` (name wrapped in colons) and `shouldReact`, reading the answer only as valid JSON and mapping a refusal to `ApiError::Rejected`. Verify: wiremock tests for the exact request body keys, a 200 answer, a 400 answer and an unreachable server (outcome unknown).
- [x] 1.3 Add `Client::custom_emojis()` for `GET emoji-custom.list` with lenient wire types (absent fields and entries without a name skipped, `remove` ignored), after capturing a real answer (with a custom emoji that has an alias) into `src/api/fixtures/docker/` through `dev/capture_reactions.py`, never edited afterwards. Verify: a mapping test against the captured fixture, and tests for an empty list, a missing key and a malformed entry.
- [x] 1.4 Add `Effect::React`, `Effect::LoadCustomEmojis` and the actions `ReactFailed`, `CustomEmojisLoaded` and `CustomEmojisFailed`, with their client-build-failure arms in `effect::run` and redacting `Debug` output. Verify: effect tests for success (no action), failure and client-build failure, and a redaction test that no emoji name or custom emoji name appears in the `Debug` of the new effects and actions.

## 2. Search model

- [x] 2.1 Add `app/react.rs` with the pure candidate builder: the message's own reactions for an empty query, aliases, custom emojis (name and aliases) and unicode emoji by every shortcode, colons stripped, case-insensitive substring, ranked exact then prefix then substring with ties on shorter then alphabetical, one row per emoji under its send name (using the table from 1.1), capped. Verify: unit tests for each search scenario of the spec (shortcode, colons, alias, custom, no match), ranking, the empty-query list with own marks, an emoji that is not offered because it has no accepted name, and hostile custom names.
- [x] 2.2 Add the reactable predicate (a server message that is neither a tombstone nor a system message) and the add-or-take-back decision from the held message and the own username. Verify: unit tests for each kind of message and for add, take back, and a reaction the user did not give.

## 3. Picker state, keys and sending

- [x] 3.1 Add the `react_message` action name, `Action::ReactMessage`, the default `+` in every preset and its help text, and `open` with its checks in order (focus, reactable, detached, own username known) and the two status refusals. Verify: keymap tests for the default, rebind, unbind with `none` and the unknown spelling; reducer tests for opening, each no-op and each refusal, and that mode, focus and cursor are untouched.
- [x] 3.2 Add the picker state and its key handling (`<C-n>`, `<C-p>`, arrows, `Enter`, `Esc`, `<C-c>`, `Backspace` on an empty search, printable characters, paste on one line, selection reset on a changed search) and route keys and pastes to it first in `update::reduce`, ignoring `KeyTimeout`. Verify: reducer tests for each scenario of the "Keys inside the picker" requirement, and that no key reaches the keymap while it is open.
- [x] 3.3 Send on `Enter`: push `Effect::React` with the add or take-back decision, close the picker, and show the fixed failure line on `ReactFailed` through `failure_text`. Add the custom emoji cache (`NotLoaded`, `Loading`, `Loaded`): fetch on the first open, none on later ones, retry after a failure. Verify: reducer tests for add, take back, toggle from the empty list, a server refusal and a network failure (fixed text, no reaction name in it), and for the cache lifecycle.
- [x] 3.4 Keep the picker in step with the room: derive the list from the held message each draw, and close the picker when its message is deleted or no longer held. Verify: tests for a reaction arriving while open, a deletion while open, and the room updating underneath.

## 4. Drawing and context

- [x] 4.1 Add the theme elements `picker.border`, `picker.title`, `picker.input`, `picker.item`, `picker.selected`, `picker.own` and `picker.hint` with defaults that do not rely on colour. Verify: theme tests that each element is settable, that partial overrides keep default modifiers, and that the selected and own defaults are distinct from the plain row.
- [x] 4.2 Add `ui/picker.rs`: the panel over the bottom of the messages and input column, the search row with the hardware cursor after the query, glyph, shortcode and right-hand note per row, a window that scrolls to keep the selection visible, ellipsis for wide rows, the empty-list hint and the key hint only when it fits. Verify: insta snapshots at 80x24 for an empty search on a message with reactions, a typed search and a long list scrolled, a snapshot at the smallest size, and tests for the cursor position and for hostile text.
- [x] 4.3 List the react action in the `?` popup on a reactable message, before the reactions and jump rows, with its live keys. Verify: context tests for a message with and without reactions, a deleted line, other focus, a rebound key and an unbound key.

## 5. Integration

- [x] 5.1 Drive the real binary on a pty against the Docker server (8.9.1): open the picker, type, move, add a standard reaction, take it back, find and add a custom emoji, and see a refused name show its status line, with another user reacting meanwhile. Verify: a throwaway driver reports every check passing, and the throwaway custom emoji and message are removed afterwards.
- [x] 5.2 Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets` and `openspec validate add-reaction-picker --strict`. Verify: all pass with no warnings.
- [x] 5.3 Look at the picker once by eye in kitty, after the two states of group 6 are in: both states, the search row and cursor, the column alignment, the selected row and own marks, and wide glyphs in the list. Verify: a person confirms it looks right or notes any misalignment (a pty screen model cannot show how a real terminal draws wide and modifier sequences). Confirmed by the user after the two-state picker, with flags, skin tones and family emoji under the popup.

## 6. Browsing and searching states

- [x] 6.1 Give the picker two states: a `typing` flag on `ReactPicker` (off on opening) and the key handling of "Keys while browsing" and "Keys while searching" (`j`, `k`, arrows, `<C-n>`, `<C-p>`, `Enter`, `q`, `Esc`, `/`, `i`, `a`, `Backspace`, a paste ignored while browsing, `<C-c>` from both), with `View` reporting the state. Verify: reducer tests for each scenario of the three requirements "Browsing and searching", "Keys while browsing" and "Keys while searching"; the existing tests that typed straight after `+` are rewritten for the new states, and every key still stays out of the keymap.
- [x] 6.2 Draw the state: the search line live with the hardware cursor only while searching and dim without a cursor while browsing, a hint of the current state's keys that still fits the 52-column pane, and the empty-list hints for each state. Verify: insta snapshots for browsing with reactions, searching, and browsing after a search, tests that the cursor is set only while searching and that the longest hint of each state fits, and the hint test for `add` and `remove` updated.
- [x] 6.3 Run the end-to-end driver again with the new keys against the Docker server (open, browse, search, add, take back, refusal, `Esc` and `q`). Verify: every check passes and what the driver created is removed.
- [x] 6.4 Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets` and `openspec validate add-reaction-picker --strict`. Verify: all pass with no warnings.

## Workflow follow-up

- Archive the change with `/openspec-archive-change` after review so the specs are synced.
- A later change can add reacting from the `gr` popup, glyph input in the search, and recently used ranking.
