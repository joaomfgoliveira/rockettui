# Tasks

## 1. Carry mentions and channels on messages

- [x] 1.1 Read `mentions` and `channels` in `api/wire.rs` and map them into new `Message` fields in `api/map.rs` and `data/mod.rs`; verify a mapping test with a fixture message holding both, and one without the arrays
- [x] 1.2 Keep `Message`'s `Debug` redacting for the new fields (counts only); verify a test that a sentinel username and channel name never appear in the output
- [x] 1.3 Add fixture messages with mentions, an own mention, `@here`, `@all` and channels to the docker fixtures used by the mapping tests; verify the fixtures parse

## 2. Highlight in message text

- [x] 2.1 Add the tokenizer in `ui/` that finds `@name`, `@here`, `@all` and `#name` in sanitized text, skipping code spans and tokens glued to a letter or digit; verify unit tests for each scenario in `specs/message-mentions/spec.md` (list-driven and shape fallback)
- [x] 2.2 Add the four theme styles and their `theme.toml` elements (`messages.mention`, `messages.mention_self`, `messages.mention_group`, `messages.channel`) with defaults; verify config tests for a styled element, the defaults and an unknown key error
- [x] 2.3 Write marker cells before wrapping and resolve them to styles after wrapping in `ui/wrap.rs`, carrying the open token across rows; strip the markers from server text in `sanitize`; verify snapshot/unit tests for a plain mention, own mention, a wrapped mention and a code span
- [x] 2.4 Apply the same pass in the thread pane, search results and the draft in `ui/input.rs` (shape fallback); verify a test per surface, and that the sent draft text is unchanged
- [x] 2.5 Keep a mention readable on the cursor row; verify a test that the cursor highlight does not erase the mention style

## 3. Open a referenced channel

- [x] 3.1 Add `Action::OpenChannel`, the `open_channel` vocabulary entry and the `gc` default in every preset in `input/keymap.rs` and `config/keymap.rs`; verify keymap tests for the default, rebind, unbind and the clash check
- [x] 3.2 Implement the reducer arm: one channel opens via `open_room`, no channel is a no-op, a room not in the subscriptions sets a status notice without any effect; verify reducer tests for all three
- [x] 3.3 Add the channel picker for a message with several channels, reusing the switcher/picker widget pattern; verify tests for choose, cancel and a picker that lists only that message's channels
- [x] 3.4 List the action in the context popup for a message with a channel reference, using the live key; verify tests for listed, not listed, rebound and unbound

## 4. Integration

- [x] 4.1 Run `cargo fmt --check`, `cargo clippy` and `cargo test`; verify they pass
- [ ] 4.2 Ask the user to check the highlights and `gc` by eye against their local Docker server (kitty terminal, mentions in a channel and a thread); the checks are theirs to run
- [ ] 4.3 Update the Features list in the Obsidian vault (tick or add the line, refresh `Progress` and `Last updated`) when the change is archived

## Workflow follow-up

- Archive the change with `/openspec-archive-change` once implemented and reviewed.
