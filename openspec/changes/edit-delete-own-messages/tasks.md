# Tasks

Local Docker Rocket.Chat only (`dev/seed.py`, 8.9.1 and 9.0.0). Nothing here points at the company server. Run `cargo test` and `cargo clippy` after each group. Bring the stacks up with `dev/seed.py up` and `seed` (the second one with `RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101`) and take them down when done.

## 1. Measure first

- [x] 1.1 Add `dev/probe_edit.py`, in the style of `dev/probe_send.py` (loopback only, prints statuses, key names and yes/no only, never text). It reports for `chat.update` and `chat.delete` as the app user on its own message: the success status and body keys; the status and closed error class when the server refuses (the settings `Message_AllowEditing`, `Message_AllowDeleting` and the `Block*InMinutes` limits changed through the admin account and restored in a `finally`); the answer for an unknown message id and for another user's message; and whether the own edit and delete come back on the stream. Run it on 8.9.1 and 9.0.0 and record the status and error classes in `design.md` (Decision 7). Verify `PROBE OK` on both and that the settings are back to their original values.

## 2. Plain-text flag

- [x] 2.1 Add a `plain_text` property to `Message`, set in `api::map::message`: false for a message with a file, a files entry or an attachment, for the encrypted placeholder and for a system message; true otherwise. Keep `Debug` free of text. Verify mapping tests for a plain message, a file, an attachment title, an encrypted one and a system one, and that the redaction sweep still passes.

## 3. Requests and effects

- [x] 3.1 Add `Client::edit_message` (`chat.update`) and `Client::delete_message` (`chat.delete`) in `src/api`, with errors mapped to the existing `ApiError` kinds and the refusal classes found in 1.1. Verify tests against the mock server for success, 400, 403, 404, a timeout and a lost connection, and that no body or server text reaches an error value.
- [x] 3.2 Add `Effect::EditMessage { room, id, text }` (text in a type whose `Debug` prints only a length) and `Effect::DeleteMessage { room, id }`, and the Actions `EditDone`, `EditFailed`, `DeleteDone` and `DeleteFailed`, addressed by room and id, unbindable by any keymap row and with a `Debug` that prints no text. Verify effect tests with the mock client and that the redaction sweep and the keymap enumeration tests still pass.

## 4. Own-message rule and keys

- [x] 4.1 Add the one predicate (held as `Sent`, not a tombstone, author equals a known own username, `plain_text`, room not encrypted) and the Actions `EditMessage` and `DeleteMessage` with the names `edit_message` and `delete_message`. Bind `e` and `dd` by default (`dd` moves from `discard_send`, which stays a bindable name that only discards). `DeleteMessage` on a failed or unconfirmed local row discards it as before. Verify tests for every exclusion (someone else's message, pending and failed rows, a tombstone, a message with an attachment, an encrypted room, unknown own username, a username that differs only in case): in each, no state, effect or status line changes. Verify the keymap tests for the new names, the default bindings and the `discard_send` rebinding.

## 5. Editing

- [x] 5.1 Add the editing state: `e` on an editable message stashes the room's draft, loads the message text into the input and enters Insert; the input title says that Enter saves and Esc cancels. Esc cancels and restores the stash; a room switch cancels the same way. Verify tests for start, cancel with and without a prior draft, a room switch, and that `e` elsewhere does nothing.
- [x] 5.2 Make Enter in the editing state save: blank or unchanged text ends the edit with no effect; otherwise record `Effect::EditMessage` and leave the old text on screen. `EditDone` ends the edit, restores the stash and applies the new text through the existing in-place replace; the stream's copy afterwards is a no-op. `EditFailed` keeps the edit open with the text intact and shows one fixed status line per error kind. Verify tests for success from the answer first and from the stream first, a refusal, a timeout, two saves in a row and a message that disappeared meanwhile, and that the send queue and every `SendState` are untouched.

## 6. Deleting

- [x] 6.1 Add the confirmation: `DeleteMessage` on an editable message sets the pending confirm and the status-bar prompt; the check at the top of `Action::Key` consumes the next key (`y` or Enter confirms, anything else cancels with no other effect and no motion). Confirming re-checks the predicate against the current row before recording `Effect::DeleteMessage`. A second `dd` or a count while a prompt is open opens no second one. Verify tests for confirm with `y` and with Enter, cancel with `j` and with Esc, the row changing between prompt and confirm, `3dd`, and that the prompt survives the status-line clearing.
- [x] 6.2 Handle the results: `DeleteDone` applies the tombstone at once (the later event is a no-op); `DeleteFailed` shows one fixed status line and leaves the row as it was. Draw the prompt in the status bar. Verify tests for success, refusal, the later stream event and that no status line contains message text, plus a snapshot of the prompt at 80x24 and at a narrow width.

## 7. Live verification against local Docker

- [x] 7.1 Add a `docker_edit_own` pty scenario: with a message the app user sent into `rt-private`, `e` shows its text in the input, a changed text saves and the row shows the new text and the edited marker in the same position; Esc restores a previous draft; with `Message_AllowEditing` off (set through admin, restored in a `finally`) a save shows the fixed line and keeps the text. Verify it passes on 8.9.1 and 9.0.0.
- [x] 7.2 Add a `docker_delete_own` pty scenario: `dd` then `y` on an own message turns it into the deleted line; `dd` then another key deletes nothing; `e` and `dd` on a message written by `alice` change nothing and send nothing (check the server still has her message afterwards); a refused delete (`Message_AllowDeleting` off) shows the fixed line. Verify it passes on 8.9.1 and 9.0.0.
- [x] 7.3 Document the probe and both scenarios, and any version difference, in `dev/README.md`, then run the whole pty smoke harness and `cargo clippy`. Verify no token and no edited or deleted text appears in any captured output.

## Workflow follow-up

- Update `openspec/specs/` through archive: `message-editing` is new, and `keymap`, `modal-navigation` and `message-sending` gain the deltas.
- Decide the deferred question (a hint on a refused `e` or `dd`) when using it daily; it needs no spec change to add.
