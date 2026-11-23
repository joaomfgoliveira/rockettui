# Design

## Context

See proposal.md for motivation. Facts that shape the approach:

- Sends go through a per-room queue with `SendState`; `Effect::SendMessage` answers with `MessageSent` or `SendFailed`. An edit or delete is a different kind of request and must not touch that machinery.
- `live-edits-deletes` already replaces a held `Sent` message in place when a live copy differs (`messages::is_edit`) and turns a held message into a tombstone on a delete event or a server-kept `rm` message (`messages::tombstone`). Both are idempotent and keep the cursor and screen row. Your own edit and delete come back on the same streams.
- `kind_and_body` appends ` [attachment: name]` to a body and replaces an encrypted body with a fixed text, so `Message.body` is not always the text the server stores.
- `own_username` is loaded best effort (`LoadMe`) and can be `None`. Author comparison is how "own" is decided.
- `dd` is bound to `discard_send` (local discard of a failed send). Keys reach the reducer through `Action::Key`, which clears the status line and feeds the mode's keymap trie.
- Keymap action names are user-facing vocabulary: a name added now is a contract.

## Goals / Non-Goals

**Goals:**
- Edit and delete your own messages from the keyboard, with a confirmation for delete.
- It must be impossible to edit or delete anyone else's message through the client.

**Non-Goals:**
- Editing or deleting other people's messages (moderation), reactions, threads, editing attachments, an undo for delete, edit history.

## Decisions

**1. Edit reuses the input bar, through an editing state.** `App` gets `editing: Option<{room, message id}>` plus the stashed draft. `e` loads the message text into the input and enters Insert; Enter sends `Effect::EditMessage` instead of queueing a message; Esc cancels and restores the stash. The state ends on save, cancel or a room switch. Alternative: edit in place in the message list. Rejected: it needs a second text editor, while the input already has multi-line editing, wrapping and a draft.

**2. The edit is pessimistic and has no local pending state.** The old text stays until the server accepts. Success applies the new text through the existing `messages::replace_edited` path (from the REST answer or from the stream, whichever comes first; the second is a no-op). Failure shows a fixed status line per error kind and keeps editing open with the text intact. Alternative: show the new text at once with a pending style. Rejected: it needs a rollback state per message and a new `SendState`-like value, for a request that normally takes milliseconds.

**3. "Own message" is one predicate, checked in three places.** A message is editable or deletable when it is held as `Sent`, is not a tombstone, has `author == own_username` (and `own_username` is known), has plain text (no file, no attachment, not the encrypted placeholder) and its room is not encrypted. The predicate runs when `e` or `dd` is pressed, again when `y` confirms (the row may have changed while the prompt was open), and the effect is only built from a row that passed it. Everything else is a silent no-op: no prompt, no request, no local change. The attachment and encrypted exclusions exist because `Message.body` then holds display text, not the stored text. A new `Message` flag or the raw text is not needed: the exclusion is decided where the message is mapped, by a `plain_text` property set by `api::map::message`.

**4. `dd` becomes `delete_message`, acting by row state.** On a `Failed` or `Unconfirmed` local row it discards locally as today (no prompt). On an own `Sent` row it opens the confirmation. On anything else it does nothing. The default binding of `dd` moves from `discard_send` to the new `delete_message`; `discard_send` stays a valid, bindable name that only discards. `e` is bound to `edit_message`. Alternative: a separate key for the server delete. Rejected for now: `dd` already means "remove this row", and one gesture is easier to learn.

**5. The confirmation is a pending state, not a mode.** `App.confirm: Option<Confirm>` is checked at the top of `Action::Key` handling, after the status line is cleared and before the matcher runs: `y` or Enter confirms (re-checking decision 3), any other key cancels and is consumed. The prompt text is set on the status line after the clear. `y` and `n` are therefore not rebindable and no `[confirm]` keymap section exists. A count (`3dd`) opens one prompt: opening is idempotent while one is open. Alternative: a fourth mode with its own trie. Rejected: more vocabulary to keep stable for a one-key yes/no.

**6. Edit and delete are effects of their own.** `Effect::EditMessage { room, id, text }` calls `chat.update` and `Effect::DeleteMessage { room, id }` calls `chat.delete`; each answers with `EditDone`/`EditFailed` and `DeleteDone`/`DeleteFailed`, addressed by room and id. They do not use the send queue or `SendState` (see the `message-sending` delta). The effect's text type prints only a length, like `SendText`. On delete success the reducer applies `messages::tombstone` at once; the later `deleteMessage` event or `rm` update finds nothing to do.

**7. Errors are fixed strings per kind.** Reuse the `ApiError` kinds the send path already maps. A 400 or 403 from an edit or delete (time limit, "allow editing" off, no permission) gets one fixed line. Server error text is never shown.

Measured with `dev/probe_edit.py` on 8.9.1 and 9.0.0 (identical answers): `chat.update` and `chat.delete` take `{roomId, msgId[, text]}`. An own edit answers 200 with keys `message,success` (the message has `editedAt`); an own delete answers 200 with keys `_id,message,success,ts`. Every refusal is HTTP 400, never 403: an unknown message id (no `errorType`), someone else's message, `Message_AllowEditing` or `Message_AllowDeleting` off (`error-action-not-allowed`), and an edit past `Message_AllowEditing_BlockEditInMinutes` (`error-message-editing-blocked`). The refusals are therefore `ApiError::Rejected` (the send path's 4xx class), and the status line says "rejected by the server" for both, with no server text. A delete past `Message_AllowDeleting_BlockDeleteInMinutes` answered 200 in the probe, so no delete time-limit refusal was observed. After a refusal the stored message was unchanged. The refusals above were measured as a plain member of the room: the pty scenario found that a delete with `Message_AllowDeleting` off still succeeds in a room the app user owns (`dev` owns the seeded rooms), so a refusal can only be seen, and tested, in a room where the user is not an owner. Both own results come back on the stream (the edit as one `stream-room-messages` event, the delete as one `stream-notify-room` event), which is why the answer and the stream copy must reconcile.

## Risks / Trade-offs

- [The server's edit or delete window has passed] -> the server refuses, the app shows the fixed line and keeps the text; the client does not know the limit and does not guess it.
- [The user edits a message that another client of the same account changes meanwhile] -> the stream's edit replaces the row; saving then overwrites it. Accepted: last write wins, as on every Rocket.Chat client.
- [Sanitizing] -> `Message.body` is sanitized, so a message with control characters loses them when edited and saved. Accepted and rare.
- [Own-message check relies on the username] -> a username change mid-session makes the app treat earlier own messages as someone else's until reload; the failure is safe (nothing happens).
- [`dd` default binding changes name] -> users who bound `dd` themselves are unaffected; the behavior of the default is a superset of the old one.

## Migration Plan

No data migration. Rollback is reverting the change; `discard_send` keeps working as a bindable name.

## Open Questions

- Whether a short status hint on a refused `e` or `dd` (for example "Only your own messages") is wanted. Currently silent; it can be added without changing the specs' guarantees.
