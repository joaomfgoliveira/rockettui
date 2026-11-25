# Tasks

## 1. Recent rooms and ranking

- [x] 1.1 Add a recently visited list to `App`, updated in `open_room` when the room changes; verify with a unit test that opening A then B gives `[B, A]` and reopening A gives `[A, B]`
- [x] 1.2 Add `src/app/switcher.rs` with the subsequence matcher and its four tiers; verify unit tests cover prefix, word start, contiguous, scattered, case folding and no match
- [x] 1.3 Add the room list builder (empty-query order, query ranking, collapsed-folder rooms included, closed rooms excluded); verify unit tests for the order scenarios in the `room-switcher` spec

## 2. State, keys and action

- [x] 2.1 Add `SwitchRoom` to `Action` and the bindable action names, with `<C-k>` bound in Normal mode in every preset and the stale "deferred" comment removed; verify keymap tests for default, rebind, unbind and the unknown-spelling config error
- [x] 2.2 Add the `switcher` field on `App`, `open`, `key`, `paste` and the stale rebuild, and route keys to it before the keymap in `update.rs`; verify reducer tests for typing, `Up`/`Down`/`Ctrl-n`/`Ctrl-p`, `Backspace`, `Ctrl-u`, selection reset, `Esc`/`Ctrl-c` and that the mode and focus are unchanged
- [x] 2.3 Implement choosing: `Enter` calls `open_room` and closes the overlay, the already-open room only closes it; verify tests for opening another room (focus on messages, draft kept) and for choosing the open room
- [x] 2.4 Verify a room removed while the switcher is open is never opened, with a reducer test

## 3. Rendering and help

- [x] 3.1 Draw the popup (search line, scrolling list, selection, kind marker, unread count and mention badge via the sidebar's helper, sanitized names); verify snapshot tests for the empty query, a query, no match and a long list
- [x] 3.2 List `Ctrl-k` in the help overlay; verify a test checks the entry appears (the `?` context popup is left as it is: its rows are specified per object kind in `context-help`, outside this change)

## 4. Integration

- [x] 4.1 Run `cargo test`, `cargo clippy` and `openspec validate add-room-switcher --strict`; verify all pass
- [x] 4.2 Check by hand in the user's kitty terminal against the local Docker server: open, type, switch, cancel, and the `Ctrl-k` then `Enter` back-to-previous flow (the user runs this)

## Workflow follow-up

- Update `Features.md` in the Obsidian vault when the change is archived, as the project instructions require.
- Archive the change with `/openspec-archive-change`.
