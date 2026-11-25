# Proposal

## Why

Reaching a room today means moving through the sidebar with `j`/`k` and folds, which gets slow with many rooms. Every chat client has a quick switcher (Ctrl-K style) that jumps to a room by typing a few letters. The keymap already reserved Ctrl-K for it and left it deferred.

## What Changes

- Add a room switcher: a popup with a search line and a ranked list of rooms, opened with `Ctrl-K` in Normal mode.
- With an empty query, rooms are listed most recently visited first, with unread rooms boosted. With a query, rooms are matched by a subsequence match on the room name and ranked by match quality.
- `Enter` opens the chosen room (through the same path as the sidebar), `Esc` or `Ctrl-c` closes the popup without changing anything.
- Keep a small in-memory list of recently visited rooms, so `Ctrl-K` then `Enter` returns to the previous room.
- The switcher lists every joined room the sidebar can show, including rooms inside collapsed folders.
- Add a bindable action `switch_room`, bound to `<C-k>` in Normal mode by default in every preset.

Out of scope: opening the switcher from Insert mode, rooms the user has not joined (no directory browsing), and persisting the recent list to disk.

## Capabilities

### New Capabilities
- `room-switcher`: the switcher popup, its ordering, matching, navigation and how it opens a room.

### Modified Capabilities
- `keymap`: adds the `switch_room` action and its default `<C-k>` binding in Normal mode.
- `modal-navigation`: states that the switcher is an overlay that takes keys while open without changing the mode.

## Impact

- New `src/app/switcher.rs` (state, ranking, key handling) and a popup renderer in `src/ui/`.
- `src/app/mod.rs`: a `switcher` field on `App` and a recent-rooms list updated in `open_room`.
- `src/app/update.rs`: the switcher takes keys before the keymap, like the reaction picker.
- `src/action.rs`, `src/input/actions.rs`, `src/input/keymap.rs`: the `SwitchRoom` action and its default binding.
- Help overlay and context help list the new key.
- No new dependencies.
