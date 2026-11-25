# Design

## Context

See proposal.md for motivation. Today opening a room is all GETs (`history.rs` documents it as D-11), `Room.unread` is a bool filled from `subscriptions.get`'s `unread` count, and `RtSubscription` events from the realtime stream set and clear it. The reducer is pure: IO is an `Effect` drained in `lib.rs`, and the only timer is the key timeout, armed in the `lib.rs` select loop from state on `App`.

## Goals / Non-Goals

**Goals:**
- Mark an open room read after a configurable delay, cancelled by leaving the room.
- Show where the unread messages start, without it disappearing mid-read.

**Non-Goals:**
- Unread counts, mention/ping highlighting, per-message unread marks, marking read from `:reload` or search context views.
- Retrying a failed mark-read in the background.

## Decisions

**Delay as app state plus a loop timer, not a spawned sleeping task.** The reducer records `pending_read: Option<(RoomId, generation)>` and `lib.rs` arms a `sleep_until` for it exactly like `key_deadline`, firing `Action::MarkReadDue { room, generation }`. The reducer then checks the room is still the open one and still unread before pushing `Effect::MarkRead`. Leaving a room clears `pending_read`, and a restart bumps the generation, so a stale timer firing is a no-op. Alternative: a spawned `tokio::sleep` task per open. It cannot be cancelled by the pure reducer and would need an abort handle in state.

**Trigger only when something is unread.** The timer starts on open if `Room.unread` is true, and on a live message from another user in the open room. A read room opened and left never costs a request. Alternative: always POST on open; rejected as needless writes against the company server.

**Optimistic clear on success.** `Action::MarkReadDone { room }` clears `Room.unread` through the same path `on_subscription` uses, so the sidebar updates even if the realtime stream is down. The server's own `RtSubscription` then finds nothing to change. A failure is dropped silently (no `Failed` action carries any text).

**The divider comes from the subscription's last-seen time.** `subscriptions.get` returns `ls` per subscription. It is read into the room list as `last_seen`, snapshotted into per-room state when the room is opened, and the divider goes above the first message from another user newer than the snapshot. If nothing qualifies and another user's message arrives live, the divider goes above that message; it is placed once and never moves. The snapshot is not advanced when the room is marked read, so the divider stays until the room is left (state dropped in `open_room` when switching away). Alternative: count the `unread` newest messages. Rejected: the count is not reliably in the live stream (updates may omit it) and ignores own messages in between.

**A divider, not per-message marks.** Unread messages are always the newest ones, so one line says where they start. It takes no column, so no alignment gutter and no layout shift, which per-message dots needed. It reuses the date-divider path in `ui/messages.rs`, including the cursor skipping it. Alternatives considered: a red dot per message (needs a column, either always reserved or a reflow when the first one appears), and a coloured author name (subtle, and clashes with the author palette).

**Config key in milliseconds.** `mark_read_delay_ms`, range 0-60000, default 3000, matching `key_timeout_ms`. With 0 the timer is skipped and the effect is pushed immediately.

## Risks / Trade-offs

- [`ls` may be absent or shaped differently on 8.9.1 or 9.0.0, and the fixtures do not include it] -> Verify against both local Docker servers in the first task before building on it; the spec already says fetched messages get no divider when it is unknown.
- [Marking read while the user is in a different terminal window] -> Accepted: the app has no focus signal, and the delay is configurable. Setting a very long delay is the escape hatch.
- [Scrolling history: older pages load above the divider and the divider's anchor is a message id, so it must not shift with the list] -> Anchor the divider to the message id, not to an index.
