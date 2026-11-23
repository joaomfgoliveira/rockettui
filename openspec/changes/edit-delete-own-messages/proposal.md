# Proposal

## Why

Sending is covered, but a typo in a message you sent cannot be fixed or removed from the keyboard. Editing and deleting your own messages is part of normal chat use. Replaces the write side of GSD MSG-05.

## What Changes

- A vim-style way to edit the message under the cursor when it is your own: it loads the text into the input for editing, and sending saves it with `chat.update`.
- A way to delete your own message under the cursor (`dd`), with a one-key confirmation step, with `chat.delete`; and never any way to edit or delete someone else's message.
- Failure and in-flight states for both, in the style of the pending/failed send states.
- Keymap action names for the new commands (user-facing vocabulary, so named with care).

## Capabilities

### New Capabilities
- `message-editing`: editing and deleting the user's own messages.

### Modified Capabilities
- `keymap`: new bindable actions.
- `modal-navigation`: an edit mode or a state of the input.
- `message-sending`: how an edit relates to the send queue and the pending states.

## Impact

Depends on `realtime-ddp` and `live-edits-deletes`, because your own edit and delete come back on the same streams and must reconcile with the local optimistic state. New Actions, new input state, new keymap names. Decided in exploration: edit reuses the Insert input through an editing state (`e`; Esc restores the previous draft); delete is `dd`, which opens a one-key status-bar confirmation for an own message and still discards a failed local send; only the user's own plain-text server messages can be edited or deleted, and on anyone else's nothing happens at all; a refused edit or delete shows one fixed status line (an edit stays open so the text is not lost). Not decided here: a hint on a refused key (see design.md, Open Questions).
