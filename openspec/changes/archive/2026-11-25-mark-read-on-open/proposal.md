# Proposal

## Why

Reading a room in rockettui never clears its unread state: the room stays in the Unread section until another client reads it. That was a deliberate read-only safeguard from the early phases (see the `rooms` spec), but the app is now the author's daily client, and an Unread section that never empties is noise. There is also no way to tell, inside a room, where the new messages start.

## What Changes

- After a room has been open for a configurable delay (default 3 seconds), the app marks it read on the server with `POST /api/v1/subscriptions.read`. Switching to another room before the delay ends cancels it, so skimming past a room never marks it read.
- A live message arriving in the open room restarts the same delay, so a room you are looking at does not stay flagged unread.
- New `config.toml` key `mark_read_delay_ms` sets the delay (0-60000, default 3000; 0 marks the room read at once).
- An `unread` divider line is drawn above the first message from another user that was unread when the room was opened, or that arrives live in an open room with nothing unread. It stays until the room is left, so marking the room read does not make it vanish mid-read. Like the date divider it takes no column, so message rows keep their alignment.
- The divider has a new theme element, `messages.unread_divider`.
- **BREAKING (behaviour)**: the requirement "Opening does not mark read" is removed and replaced.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rooms`: replaces "Opening does not mark read" with marking a room read after the delay, including cancel on switch, the restart on live messages and silent failure.
- `configuration`: adds `mark_read_delay_ms` to the display settings.
- `message-history`: adds the `unread` divider above the first unread message.
- `theme`: adds the `messages.unread_divider` element.

## Impact

- `src/api`: new `Client::mark_read` (`subscriptions.read`), and the subscription's last-seen time is read from `subscriptions.get` (`ls`).
- `src/effect.rs`, `src/action.rs`, `src/app/realtime.rs`, `src/app/mod.rs`: a `MarkRead` effect, a timer action, per-room pending-read state.
- `src/lib.rs`: arms the timer like the key timeout.
- `src/config/settings.rs`, `src/config/theme.rs`: the new key and theme element.
- `src/ui/messages.rs`, `src/ui/wrap.rs`: drawing the divider, next to the date divider.
- First state-changing call besides messages and reactions; it uses the user's own token like every other call.
