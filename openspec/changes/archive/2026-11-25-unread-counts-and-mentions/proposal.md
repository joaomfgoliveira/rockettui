## Why

The sidebar shows an unread room as a flag only, so the user cannot tell a room with one new message from one with fifty, or a room where someone mentioned them from one that is just chatty. The server already sends the unread count and the mention counts on every subscription; the client discards them.

## What Changes

- Read `unread`, `userMentions` and `groupMentions` from the subscription list and from live subscription updates, and keep them per room.
- Show the unread count after an unread room's name, and a mention badge when the room has mentions (direct and group mentions added together): `#general (3) @1`.
- A room the server flags (`alert`) with a zero count stays unread and shows no number.
- A live update that omits a key keeps the value held for it; it does not reset it to zero.
- Add a theme element for the mention badge, so it can be styled apart from the unread name.
- The Unread section keeps its current order; mentions do not sort rooms.
- The folder header `(n)` keeps counting unread rooms, not messages.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rooms`: the unread marker is no longer "a flag only, no counts"; it gains the count and the mention badge.
- `realtime-messages`: live subscription updates carry the counts, and a partial update keeps what is held.
- `theme`: a new style element for the mention badge.

## Impact

- `src/api/wire.rs`, `src/api/map.rs`: declare and map the new subscription fields.
- `src/realtime/events.rs`, `src/action.rs`, `src/app/realtime.rs`: the subscription update carries counts instead of a bool.
- `src/data/mod.rs`: `Room.unread: bool` becomes a small struct (count, mentions, flagged) with an `is_unread()` helper; every user of `room.unread` is adapted.
- `src/ui/sidebar.rs`, `src/config/theme.rs`: render the count and badge, and add the theme element.
- No new dependency, no new request: the data is already in the responses.
- Out of scope: sorting by mentions, thread unread counts, a local count of live messages, desktop notifications.
