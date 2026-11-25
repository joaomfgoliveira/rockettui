# Tasks

## 1. Drafts and target

- [x] 1.1 Widen the draft map key to room plus optional thread with no behavior change; verify the existing draft and quit-warning tests still pass.
- [x] 1.2 Add `InputTarget`, set it from the pane and clear it on close and room change; verify with reducer tests, including that text and target survive `Esc`.

## 2. Sending

- [ ] 2.1 Ask the user to send a thread reply and an also-in-channel reply on the local servers and share the non-secret request/response shapes; save redacted fixtures.
- [x] 2.2 Add `tmid` and `tshow` to the send request and the pending message; verify with mock-server tests for a reply, a first reply and a rejection.
- [x] 2.3 Put thread replies in the shared per-room queue and reconcile them in the pane; verify with tests for ordering, the live copy, retry, discard and the unknown outcome.
- [x] 2.4 Bump the parent's reply count on a first reply and correct it from the server; verify with a test.

## 3. Actions and keys

- [x] 3.1 Add `reply_in_thread` (`gR`) and `toggle_reply_to_channel` (`<C-b>`) to the vocabulary and default keymap, checking `<C-b>` is free; verify defaults, rebind, unbind, unknown spelling.
- [x] 3.2 Extend edit, delete and react to messages in the pane; verify with reducer tests for own reply, other's reply and the confirmation prompt.
- [x] 3.3 Refuse `gR` on system and deleted messages and in encrypted rooms; verify with tests.

## 4. Drawing

- [x] 4.1 Show the reply target and the also-to-channel state in the input bar title, and failed rows in the pane; verify with snapshots.
- [x] 4.2 Update the help text and the context popup for the new keys; verify snapshots.

## 5. Wrap-up

- [ ] 5.1 Ask the user to try replying and starting a thread on their local server and report the result.

## Workflow follow-up

- Archive after `add-thread-view`, then update the Obsidian Features list per CLAUDE.md.
