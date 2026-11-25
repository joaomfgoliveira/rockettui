# Tasks

## 1. API and data

- [x] 1.1 Check `subscriptions.get` on the local Docker 8.9.1 and 9.0.0 servers for the `ls` field and record what it looks like (type, presence); verify by adding it to the subscription fixture and a parsing test
- [x] 1.2 Add `last_seen: Option<i64>` to `Room`, filled from `ls` in `src/api/map.rs`; verify a map test covers present and absent `ls`
- [x] 1.3 Add `Client::mark_read` (`POST subscriptions.read` with `rid`) in `src/api/mod.rs`; verify with a wiremock test that checks the path, body and the auth headers, and that an error status is an `ApiError`
- [x] 1.4 Add `mark_read_delay_ms` to `src/config/settings.rs` (0-60000, default 3000, known key); verify tests for default, valid, out-of-range and wrong type, and update the `configuration` docs if the repo documents config keys

## 2. Mark read

- [x] 2.1 Add `Effect::MarkRead`, `Action::MarkReadDue` and `Action::MarkReadDone` with `Debug`/`Display` that print no message text, and run the effect in `lib.rs`; verify an effect test that a failure sends no action
- [x] 2.2 Add `pending_read` state to `App`; start it in `open_room` when the room is unread, clear it when the room changes, and handle `MarkReadDue` (stale generation, other room, already read are no-ops); verify reducer tests for stay, leave early, already read and delay 0
- [x] 2.3 Restart the delay in `src/app/realtime.rs` when another user's live message arrives in the open room, not for own messages or other rooms; verify reducer tests
- [x] 2.4 Arm the timer in the `lib.rs` select loop from `pending_read`, like `key_deadline`; verify with a paused-time test that the action fires after the delay and not before
- [x] 2.5 Clear `Room.unread` on `MarkReadDone` through the path `on_subscription` uses, keeping the sidebar cursor; replace the test `open_room_stays_flagged_and_opening_does_not_mark_read` and update the module docs in `history.rs` and `realtime.rs` that say opening never marks read; verify `cargo test`

## 3. Unread divider

- [x] 3.1 Snapshot `last_seen` when a room is opened and drop it when the room is left; keep the id of the message the divider sits above (first from another user newer than the snapshot, or the first live one from another user), set once and never moved; verify reducer tests for own-only messages, unknown `ls`, later messages and reopening
- [x] 3.2 Add the `messages.unread_divider` theme element, red by default, to `src/config/theme.rs` and the vocabulary list; verify theme tests for default and override
- [x] 3.3 Draw the `unread` divider in `src/ui/messages.rs` below the date divider when both fall above the same message, and make the cursor skip it; verify a UI snapshot for compact and two-line layouts, a test that the cursor passes over it, and that existing snapshots do not change
- [x] 3.4 Update the help or context text if it describes unread behaviour; verify with `cargo test` and `cargo clippy`

## 4. Check in the real app

- [x] 4.1 Run against the local Docker server: open an unread room and stay 3 seconds (it leaves Unread, the divider stays), open another room before 3 seconds (stays unread), receive a live message in the open room (divider above it, then read after the delay); verify by eye in kitty

## Workflow follow-up

- Archive the change with `/openspec-archive-change` once implemented.
