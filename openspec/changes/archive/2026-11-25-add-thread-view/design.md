# Design

## Context

See `proposal.md`. Today `Message` has no thread fields, `Focus` is `Sidebar | Messages | Input`, and `update.rs` keeps a `CYCLE` constant with a comment that an open thread pane will be appended to it. History is fetched per room and merged by id; live messages arrive through `src/realtime` and the reducer appends them to the room. Everything goes through one `Action` enum and one reducer (Elm/TEA), and effects (`src/effect.rs`) do the I/O.

## Goals / Non-Goals

**Goals:**
- Read-only threads: marker, hiding replies from the main flow, a pane, live updates.
- Reuse the message drawing, the message cursor and the paging logic instead of copying them.
- Leave seams for the four following changes (follow state, thread list entry, reply input, unread).

**Non-Goals:**
- Following, listing, replying, unread tracking.
- Threads in search results (a result that is a reply opens its room as today).
- A thread pane per room remembered across room switches.

## Decisions

**1. Thread fields on `Message`, one mapping point.** Add `tmid: Option<String>`, `tshow: bool`, `tcount: u32` and `tlm: Option<i64>` to `Message`, filled in `api/map.rs` for every path that builds a `Message` (history, search, context, `get_message`, realtime). *Alternative:* a side table keyed by id. Rejected: the fields belong to the message and the merge-by-id logic already carries them.

**2. Filter at the room's message store, not at each source.** A helper `in_main_flow(&Message) -> bool` (`tmid.is_none() || tshow`) decides what the main flow draws, and the realtime reducer uses it to decide between "append to room" and "route to thread". Refinement found while implementing (same behavior): the room's store is indexed by the cursor, the scroll, the divider and the follow rule, so a hidden entry in it would need a skip in every one of them. Instead a hidden reply never enters the room's list at all (`on_message` returns early, `history::on_loaded` filters), and the only place that holds a reply is the open thread pane (decision 3). Edits, deletes and reactions on a reply are applied in the pane (task 3.3). *Alternative:* drop replies at the API boundary. Rejected: the thread pane and live edits of replies need them, and gap recovery would then have to refetch.

**3. Thread state is one `Option<ThreadPane>` on `App`.** `ThreadPane { room, parent_id, replies: Vec<Message>, cursor, scroll, load: LoadState, paging }`. Opening sets it and emits `Effect::LoadThread`. A reply is held in the thread pane's list only while that thread is open; a reply for a thread that is not open only updates the parent's `tcount` and `tlm`. *Alternative:* hold every thread of every room in a map. Rejected: unbounded memory for something the user views one at a time. The cost is a refetch each time a thread is opened, which is one request.

**4. REST for reading.** `GET chat.getThreadMessages` with `tmid`, `count` and `offset`, mapped like history. The parent is taken from the main flow (or `get_message` when it is not loaded, as for a jump target). Paging uses `offset` because this endpoint is offset-based, unlike history's `latest`. Measure on the local 8.9.1 and 9.0.0 whether the replies come oldest-first or newest-first and normalize in the mapper.

**5. Live routing.** The stream delivers a reply as a normal message with `tmid`. The reducer: (a) stores it, (b) if its thread is open, appends to the pane (merge by id), (c) bumps the parent's `tcount` and `tlm` when the parent is held. The parent's own update event from the server also carries `tcount`; the server value wins when it arrives, so the local bump is only an optimistic step. Refinement found while implementing: the captured streams (8.9.1 and 9.0.0) send the parent's update with the new `tcount` and `tlm` just *before* each reply, so a local bump on the reply would count it twice. The reducer therefore does no local bump and takes the count from the parent's own update (`is_update` now also compares `tcount` and `tlm`). A parent update for a parent the room does not hold is ignored instead of being appended as a new message. Edits, deletes and reactions already look a message up by id, so they are extended to search the pane's list too.

**6. Focus and layout.** Add `Focus::Thread`. `CYCLE` becomes dynamic: `[Sidebar, Messages]` plus `Thread` while `app.thread.is_some()`. Layout splits the area right of the sidebar into messages and thread (about 60/40). Below a minimum total width (starting value 100 columns, tuned in the implementation) the thread pane takes the message pane's rect and the message pane is not drawn. `Ctrl-w l` from Messages focuses the thread, `Ctrl-w h` from it goes back.

**7. Keys.** `open_thread` on `gt` (the `g` prefix already holds `gg` and `gr`). Closing is `Esc` in Normal mode with the pane focused, plus an unbound-by-default `close_thread` action. Vocabulary names are user-facing, so they are fixed here: `open_thread`, `close_thread`, theme elements `messages.thread`, `messages.thread_reply_hint`, `thread_pane.border`.

**8. Detached rooms.** A room in the detached state (jumped away from the newest messages) can still open a thread: the pane is independent of the main flow's window. Live replies still reach the pane.

## Risks / Trade-offs

- [The stream or history may not carry `tcount`/`tmid` the way I expect on 8.9.1 or 9.0.0] -> Measure with the local Docker servers first, record the real fields in a fixture under `src/api/fixtures`, and write the specs' scenarios against those.
- [A reply that is hidden from the flow but changes `tcount` could desync from the server] -> Server value wins on the parent update; gap recovery after a reconnect already refetches and reapplies by id.
- [Offset paging while replies arrive can duplicate or skip a reply] -> Merge by id, so a duplicate is harmless; a skip heals on the next open.
- [The 100-column threshold is a guess] -> It is one constant; adjust after trying it in kitty.
- [`gt` shadows nothing today, but a user's own `g`-prefixed bindings could collide] -> The matcher already reports prefix conflicts for overrides.

## Open Questions

- Whether the threshold for the takeover layout should be a `config.toml` setting. Deferred: a constant is enough until it annoys the author.
