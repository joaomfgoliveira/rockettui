# Tasks

## 1. API

- [ ] 1.1 Ask the user to run `chat.getThreadsList` on the local servers (all, following, unread, with text, with offset) and share the non-secret shapes; save redacted fixtures and verify they hold no token or real names.
- [ ] 1.2 Add `Client::list_threads` and the row mapping (bounded first line, counts, last reply, followed); verify with mapper and mock-server tests, including a rejected request and an empty list.

## 2. Overlay state

- [ ] 2.1 Add the `ThreadList` state, its actions, filter cycling, selection, generation check and debounced search; verify with reducer tests, including a stale response dropped.
- [ ] 2.2 Add paging and the failed-page path; verify with a reducer test that loaded rows stay after a failure.
- [ ] 2.3 Choose a thread through `OpenThread`, fetching the parent when it is not loaded, with the "gone" path; verify with tests for loaded parent, unloaded parent and a 404.

## 3. Input and drawing

- [ ] 3.1 Add `list_threads` (`gT`) and make the overlay take keys before the keymap; verify default, rebind, unbind, unknown spelling and that the mode badge does not change.
- [ ] 3.2 Draw the overlay (rows, title with filter, loading, empty and error lines); verify with snapshots at 80 and 160 columns.
- [ ] 3.3 Add the action to the help text and the context popup; verify the snapshots.

## 4. Wrap-up

- [ ] 4.1 Ask the user to try it on their local server and report the result.

## Workflow follow-up

- Archive after `add-thread-view`, then update the Obsidian Features list per CLAUDE.md.
