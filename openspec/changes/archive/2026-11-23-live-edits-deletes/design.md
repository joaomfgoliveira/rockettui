# Design

## Context

See proposal.md for motivation. `realtime-ddp` delivers new messages on `stream-room-messages` `__my_messages__` and keeps the connection alive. Relevant facts: `messages::confirm_held` ignores a held `Sent` id, so an edit (same `_id`) is dropped today. The `ExtraSub` seam sends extra `sub` frames but forgets them when a connection drops. A delete arrives only on `stream-notify-room` `<rid>/deleteMessage` with `{_id}`; the payload does not say who deleted it. Refetch after a reconnect (`history::on_refetched`) only adds rows.

## Goals / Non-Goals

**Goals:**
- Edits and deletes of other people's messages appear live and after a reconnect.
- Keep TEA: the realtime task owns I/O, the reducer owns every state decision.

**Non-Goals:**
- Own edit and delete (`edit-delete-own-messages`), though the deleted line and edited marker are shared with it.
- Reactions, pins, threads, who deleted a message, any record of the deleted text.

## Decisions

**1. An edit is a held `Sent` id whose text or edited state differs.** The stream also redelivers a message for reactions and pins, so "same id" is not an edit. The decision compares body and edited flag only; an equal copy stays ignored. `Message` gains `edited: bool`, read from `editedAt` in `WireMsg`, so live and REST show the marker identically. Alternative: store `editedAt` and compare it. Rejected: a flag is enough and keeps `Message` small.

**2. A delete becomes a tombstone, not a removal.** `MessageKind::Deleted` keeps id, author and time and clears the body. No index shifts, so no cursor or stored-view fix-up is needed, and the text is not retained. The line keeps the author and time columns and shows `message deleted` as the body, styled by a new theme element, and never says who deleted it. Alternative: remove the row. Rejected: the user wants to see that a message was removed, and removal needs index fix-ups. A pending local message (any state other than `Sent`) ignores a delete for its id.

**3. Delete subscriptions cover every joined room, through derived state.** The wanted set is the ids of the joined, non-encrypted rooms (`app.rooms`). After each reducer batch the run loop compares it with the last set and publishes a change on a `tokio::sync::watch` to the realtime task, which subscribes to the whole set after every connect and to added or removed rooms while connected. No new Action or Effect: the set is derived from `App`. Covering all joined rooms means a room's deletes are already live when it is first opened. Alternative: only rooms whose history is loaded. Rejected: it adds a dependency on history state for little saving. Alternative: reuse the `ExtraSub` one-shot seam. Rejected: it forgets subscriptions on a drop and does not know the rooms; the seam stays for other streams.

**4. Reconcile after a reconnect inside the page's time range.** `on_refetched` takes each page message's text and edited state into the held copy, and turns held `Sent` messages whose time falls inside the page's range and whose id is absent into tombstones. Older held rows are not touched, because absent from the page cannot be told from older than the page. Stale rooms use the same path when reopened.

**5. Delete events map like message events.** `events::action_for` gains `stream-notify-room` with an event ending in `/deleteMessage`, giving `Action::RtDeleted { room, id }`. Its `Debug` prints no text (there is none).

## Risks / Trade-offs

- [An edit flips a room back to unread] -> measured on 8.9.1 and 9.0.0 (2026-11-23, `dev/probe_ddp.py` D4f, `PROBE OK` on both): in a public channel, a private group and a DM, the one `subscriptions-changed` update that follows an edit carries neither `alert` nor `unread`, and both the edit and the delete reach the app's own account. `unread_of` already returns `None` for such an update, so no guard was needed.
- [A tombstone vs a refetch disagree after a restart] -> accepted and stated in the spec: tombstones are session-only.
- [A message deleted while offline and older than the refetched page stays visible] -> accepted; fixed by a room reload.
- [A message deleted while offline in the page's oldest or newest second stays visible] -> accepted. Only held server messages strictly between the page's oldest and newest second are judged: one in the oldest second may lie just beyond the page, and one in the newest second may have arrived live after the request left, so judging them could hide a real message. `Message.ts` has whole-second resolution.
- [Many subscriptions on a large account] -> one small frame per joined room at connect and only changes afterwards; revisit if a server limits subscriptions.
- [Row height changes on edit or delete] -> reuse the cursor-and-row anchoring of the older-page and refetch merges.

## Migration Plan

No data migration. Rollback is reverting the change: edits and deletes show only after a reload.
