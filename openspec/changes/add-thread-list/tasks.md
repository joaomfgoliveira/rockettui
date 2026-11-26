# Tasks

## 1. API

- [x] 1.1 Ask the user to run `chat.getThreadsList` on the local servers (all, following, unread, with text, with offset) and share the non-secret shapes; save redacted fixtures and verify they hold no token or real names.
- [x] 1.2 Add `Client::list_threads` and the row mapping (bounded first line, counts, last reply, followed); verify with mapper and mock-server tests, including a rejected request and an empty list.

## 2. Overlay state

- [x] 2.1 Add the `ThreadList` state, its actions, filter cycling, selection, generation check and debounced search; verify with reducer tests, including a stale response dropped.
- [x] 2.2 Add paging and the failed-page path; verify with a reducer test that loaded rows stay after a failure.
- [x] 2.3 Choose a thread through the open-thread flow, fetching the parent first when it is not loaded, with the "gone" path (the list stays shown); verify with tests for loaded parent, unloaded parent and a 404. The fetch and the "gone" path exist; the hand-over is reworked in 5.3.

## 3. Input and drawing

- [x] 3.1 Add `list_threads` (`gT`); verify default, rebind, unbind, unknown spelling and that the mode badge does not change. The action and its default are done; the key handling is reworked in 5.1.
- [x] 3.2 Draw the list (rows, title with filter, loading, empty and error lines); verify with snapshots at 80 and 160 columns. The row and line drawing exist; the column is reworked in 5.2.
- [x] 3.3 Add the action to the help text and the context popup; verify the snapshots. The action rows exist; the pane's own rows are added in 5.4.

## 4. Wrap-up

- [x] 4.1 Ask the user to try it on their local server and report the result.

## 5. Rework as a side column

- [x] 5.1 Add `Focus::ThreadList` and its keys: motions, `select` and `search_start` through the keymap, `move_left`/`move_right` (filter) and fixed `Esc`/`q` (close), the in-pane search line (`Enter` keeps, `Esc` clears); remove the overlay's key interception; verify with reducer tests for each key, typing as text, the badge staying NORMAL, and `Ctrl-w h` leaving the pane.
- [x] 5.2 Draw the list in the thread column, beside the messages and in place of them on a narrow terminal, with the focused-pane border; remove the overlay panel; verify with snapshots at 80 and 160 columns and a test that closing restores the message pane.
- [x] 5.3 Hand the column over: `Enter` shows the thread in the column keeping the list state, `Esc` in it returns to the list unchanged, closing the list restores the previous focus, `gT` over an open thread shows the list (unsent replies kept), a room change closes both; verify with tests for each, including the gone parent leaving the list shown.
- [x] 5.4 Add the pane to the `<Tab>` cycle and to `Ctrl-w l`, and its rows to the context popup and help; verify the snapshots.

## Workflow follow-up

- Archive after `add-thread-view`, then update the Obsidian Features list per CLAUDE.md.
