# Proposal

## Why

Mentions (`@user`, `@here`, `@all`) and channel references (`#channel`) are plain text in rockettui today. Only the sidebar's `@N` badge hints that someone mentioned you; inside a room you must read every line to find out who or what a message points at. In the web client they stand out, and a `#channel` can be followed. A keyboard client should offer both: see them at a glance, and jump to a referenced room without leaving the keys.

## What Changes

- Highlight mentions and channel references in message text, in four themable styles: another user's `@name`, your own `@name`, `@here` / `@all`, and `#channel`.
- Read the server's `mentions` and `channels` arrays on each message, so only real references light up (no `@` in an email address or a code span). `@here` and `@all` are matched literally. Without the arrays (older history, other servers) fall back to matching the text.
- Draw the highlights wherever message text is drawn: the main flow, the thread pane, search results, and the draft in the input bar.
- Add a Normal-mode action `open_channel`, bound to `gc` by default, that opens the room a `#channel` on the cursor message points at. One channel opens directly; several open a small picker. A room the user is not subscribed to shows a notice and changes nothing: the client never joins a room on its own.
- Show the action in the context popup for a message that references a channel.

Out of scope: autocomplete on `@` / `#`, opening a DM from an `@user`, a "next mention" motion, joining rooms.

## Capabilities

### New Capabilities
- `message-mentions`: highlighting of user, group and channel references in message text, and opening a referenced channel from the cursor message.

### Modified Capabilities
- `keymap`: new bindable action `open_channel`, default `gc` in Normal mode.
- `context-help`: a message with a channel reference lists the open-channel action with its live key.
- `theme`: new elements for the four mention and channel styles.

## Impact

- `src/api/wire.rs`, `src/api/map.rs`, `src/data/mod.rs`: parse and carry `mentions` / `channels` on `Message` (never printed by `Debug`).
- `src/ui/` (`wrap.rs`, `input.rs`, `results.rs`, `thread.rs`), next to `emoji::expand`: the styling pass.
- `src/config/theme.rs`, `src/config/keymap.rs`, `src/input/keymap.rs`, `src/action.rs`, `src/app/`: theme keys, the action, the reducer arm, the picker.
- `help` is generated from the live keymap, so it needs no spec change.
- New theme element names and an action name are user-facing vocabulary and become stable once shipped.
- Features list in the Obsidian vault is updated on archive.
