# Tasks

Local Docker Rocket.Chat only (`dev/seed.py`, 8.9.1 and 9.0.0). Nothing here points at the company server. Run `cargo test` and `cargo clippy` after each group.

## 1. Measure first

- [x] 1.1 Extend `dev/probe_ddp.py` to report whether the `subscriptions-changed` update that follows an edit carries `alert` or `unread` (key presence only, no values), and whether an edit and a delete reach the app's own account. Run it on 8.9.1 and 9.0.0 and record the answer in `design.md` (Risks). Verify `PROBE OK` on both. If an edit carries the keys, add a guard to `unread_of` in this group, with a test.

## 2. Data and mapping

- [x] 2.1 Add `edited: bool` to `Message` (read from `editedAt` in `WireMsg`) and `MessageKind::Deleted`, keeping the hand-written `Debug` free of text. Verify mapping tests for an edited and an unedited REST message, and that the redaction sweep still passes.
- [x] 2.2 Map `stream-notify-room` `<rid>/deleteMessage` to a new `Action::RtDeleted { room, id }` in `events::action_for`, unbindable by any keymap row, with a `Debug` that prints no text. Verify event tests for a delete, a malformed payload and an unrelated room event, and that the keymap enumeration tests still pass.

## 3. Reducer

- [x] 3.1 Replace a held `Sent` message in place when a live copy differs in text or edited state; keep ignoring equal copies (reactions, pins). Verify tests for an edit in the open room, an edit while scrolled up (cursor message and screen row stay), an unchanged redelivery, an edit of a message not held, and an edit of a pending local row.
- [x] 3.2 Handle `RtDeleted`: turn a held `Sent` message into a tombstone (author and time kept, body cleared, `Deleted` kind); ignore an unknown id and a pending local row. Verify tests for open room, another loaded room, unknown id, pending row, a repeated delete, and that the cursor and stored view do not move.
- [x] 3.3 Reconcile after a refetch in `history::on_refetched`: take text and edited state from the page for held ids, and tombstone held `Sent` rows inside the page's time range that the page lacks; leave older rows alone. Verify tests for edited-offline, deleted-offline, older-than-page, and that the gap-larger-than-a-page reload still behaves as before.

## 4. Rendering and theme

- [x] 4.1 Draw the edited marker and the tombstone line (original author and time, `message deleted`, dim), with a new theme element for each documented where theme names are listed. Verify snapshot tests in Compact and TwoLine layouts, a themed override, and a narrow width without panic.

## 5. Delete subscriptions

- [x] 5.1 Derive the wanted set (joined, non-encrypted rooms) after each reducer batch and publish it on a `tokio::sync::watch` to the realtime task only when it changes. Verify a unit test of the derivation and of publish-on-change.
- [x] 5.2 In the realtime task, subscribe to `stream-notify-room` `<rid>/deleteMessage` for the whole set after every connect and for added or removed rooms while connected (unsub on removal). Verify fake-server tests: set subscribed after login, a room added later, a room removed, and everything resubscribed after a reconnect.

## 6. Live verification against local Docker

- [x] 6.1 Add a `docker_edits` pty scenario: `alice` edits a message in the open room (new text and edited marker show, position unchanged), deletes one (the tombstone shows with `alice` and no text), and deletes one in a room not yet opened (opening it shows no trace). Verify it passes on 8.9.1 and 9.0.0.
- [x] 6.2 Extend the reconnect scenario: through the forwarder, `alice` edits and deletes held messages while the app is cut off; after the reconnect the open room shows the edit and the tombstone. Verify it passes on 8.9.1 and 9.0.0, and document the scenarios and any version difference in `dev/README.md`.
- [x] 6.3 Run the whole pty smoke harness and `cargo clippy`; verify no token and no deleted text appears in any captured output.

## Workflow follow-up

- Update `openspec/specs/` through archive so `realtime-messages` and `message-history` gain the new requirements; also fix the `message-history` line that says loaded rooms are never refetched.
- Next change: `edit-delete-own-messages`, which reuses the edited marker and the tombstone.
