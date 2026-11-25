# Design

## Context

`Room.unread` is a `bool` set from `sub.unread > 0` (REST, `src/api/map.rs`) and from `alert || unread > 0` (stream, `src/realtime/events.rs`). `Action::RtSubscription` carries that bool, `app::realtime::on_subscription` stores it, and the sidebar, the section grouping (`app::sidebar`) and mark-read (`app::read`) all read it. The wire structs do not declare `userMentions` or `groupMentions`. Stream updates are partial: a message is followed by a second update with neither `alert` nor `unread`. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Carry count and mention total from the server to the sidebar with one data type.
- Keep every existing "is this room unread" decision unchanged.

**Non-Goals:**
- Counting messages locally, sorting by mentions, thread counts, notifications.

## Decisions

**1. `Room.unread` becomes `Unread { flagged: bool, count: u32, mentions: u32 }` with `is_unread()`.**
`is_unread()` is `flagged || count > 0`, the same rule the stream already uses. Grouping, mark-read and the divider call `is_unread()`, so their behavior does not change. Alternative: keep the bool and add two fields on `Room`; rejected, because the three values change together and a struct makes the "update" rule one function.

**2. Mentions are one number: `userMentions + groupMentions`.**
Decided with the user. Stored summed; the two are never shown apart.

**3. Partial updates merge, they do not replace.**
The stream event becomes `Option<u32>` per field (`alert`, `unread`, `userMentions`, `groupMentions`). `Unread::merge` applies only the fields present, and an update with none is dropped as it is today. Mentions are tied to the count: when the update carries `unread: 0` and no mention keys, mentions are cleared too, since a read room has no mentions. Alternative: replace on every update; rejected, because the second update of each message would wipe the count.

**4. The action carries the merge input, not the result.**
`Action::RtSubscription { room, update: UnreadUpdate }` where `UnreadUpdate` holds the optional fields. The reducer merges into the held `Unread`. This keeps the reducer the only place state changes (TEA) and keeps the stream mapper pure.

**5. Mark-read response clears all three.**
When the server confirms mark-read, the room's `Unread` is reset to default, as the bool was cleared before.

**6. Rendering.**
`name (count)` after the name; `@N` after that in `sidebar.mention` (default red, bold). Over 99 shows `99+`. `flagged` with `count == 0` shows neither. The count is part of the row text, so long names truncate the name first and keep the count and badge visible. Alternative: right-align the numbers in a column; deferred, because widths need more layout code for little gain.

**7. Folder header unchanged.** Its `(n)` counts unread rooms, as the spec already says.

## Risks / Trade-offs

- [The server's count depends on the user's notification preferences (all messages vs mentions only)] → Show the server's number as is; note it in the Features file.
- [A missed update leaves a stale count] → Reconnect already refetches subscriptions' state through gap recovery; confirm in tasks that the room list reload resets counts.
- [`Room` construction sites in tests and fake data are many] → `Unread::default()` plus a `Unread::flag()` test helper keep the diff mechanical.

## Open Questions

None blocking. Whether to also show the count for the open room (kept flagged until the server answers) can be settled when implementing; the default is to show what the server last said.
