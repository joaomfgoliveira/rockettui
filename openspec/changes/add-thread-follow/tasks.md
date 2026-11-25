# Tasks

## 1. Data and API

- [ ] 1.1 Ask the user to run follow and unfollow against the local server (8.9.1 and 9.0.0) and share the non-secret response shapes and the `replies` field before and after; save redacted fixtures and verify they hold no token or real names.
- [ ] 1.2 Add `followers` to the parent `Message` mapping and a `followed(own_id)` helper; verify with mapper tests (followed, unfollowed, field absent).
- [ ] 1.3 Add `Client::follow_thread` and `unfollow_thread`; verify with mock-server tests for success, rejection and a timeout, and that errors carry no message text.

## 2. Reducer

- [ ] 2.1 Add `ToggleFollowThread`, the effect, the pending flag and the result actions; verify with reducer tests for follow, unfollow, rejection, double press and no thread.
- [ ] 2.2 Apply the parent's live update to `followers`; verify with a realtime reducer test for a change from another client.

## 3. Input and drawing

- [ ] 3.1 Add `toggle_follow_thread` (`gf`) to the vocabulary and default keymap; verify defaults, rebind, unbind and the unknown-spelling config error.
- [ ] 3.2 Draw `following` in the marker row and the pane header; verify with snapshots for followed and unfollowed.
- [ ] 3.3 Add the action to the context popup and the help text; verify the snapshots.

## 4. Wrap-up

- [ ] 4.1 Ask the user to try it on their local server and report the result.

## Workflow follow-up

- Archive after `add-thread-view`, then update the Obsidian Features list per CLAUDE.md.
