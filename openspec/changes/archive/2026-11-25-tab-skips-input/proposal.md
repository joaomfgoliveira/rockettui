# Proposal

## Why

`<Tab>` and `<S-Tab>` cycle focus through the sidebar, the messages pane and the input bar. The input bar is a poor stop in that cycle: it is reached deliberately (`Ctrl-w j`, or `i` to type), and landing on it by accident while moving between the room list and the conversation is noise. Threads are planned, and the cycle should grow to include the open thread rather than the input bar.

## What Changes

- `<Tab>` and `<S-Tab>` cycle only between the panes you navigate by reading: the sidebar and the messages pane. They no longer stop on the input bar.
- When focus is already on the input bar (reached with `Ctrl-w j`), `<Tab>` moves to the sidebar and `<S-Tab>` moves to the messages pane.
- `Ctrl-w j` still focuses the input bar.
- The cycle is built from an ordered list of cyclable panes, so a future open-thread pane can be appended without reworking the cycle.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `keymap`: the `vim-extras` `Tab` / `Shift-Tab` focus cycling skips the input bar.

## Impact

- `src/app/update.rs`: the `FocusNext` and `FocusPrev` handlers and the `focus_cycle` test.
- `src/action.rs`: doc comments on `FocusNext` and `FocusPrev`.
- Help overlay text is unchanged ("focus the next pane").
