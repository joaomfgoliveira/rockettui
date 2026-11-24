# Design

## Context

See `proposal.md` and the specs. Facts measured on the local 8.9.1 server (read-only script against `rt-public`, 100 messages in the page):

- `channels.history?latest=T&inclusive=true&count=N` returns the result and the N-1 messages before it: contiguous, newest first.
- `channels.history?oldest=T&count=N` returns the **newest** N messages after T, not the N next to T. Adding a far `latest` changes nothing. So the newer neighbours are reachable only when fewer than the page maximum (100) messages exist after T; then the answer is all of them and the nearest are its oldest end.
- The `latest` value must be the raw server timestamp, sent verbatim. A result in the list holds only whole seconds (`Message.ts`), which is not enough: a result in the same second as a neighbour would be mis-cut.

The search page already goes through `map::search_page`; the results list holds `Vec<Message>`.

## Goals / Non-Goals

**Goals:**
- A context window that needs no merging with the room's messages, so no gap logic.
- Same key handling shape as the results list and help overlay: a state checked before the keymap.

**Non-Goals:**
- Paging inside the window, and any change to the room's own history state.

## Decisions

**A separate `App.context_view: Option<ContextView>`, above `App.results`.** It holds the room, the search id of the list it came from, the window messages (oldest first), selection, `top_row` and the flag "newer not shown". Keys go to it first, then to the results list. `q` sets it to `None`, which shows the list again. Alternative: one overlay enum `Results | Context`. Rejected: the list must keep its selection, scroll and paging state intact underneath, which two options do for free.

**Keep each result's raw timestamp.** `Client::search_room` returns the messages plus the raw `ts` string of each; `Action::SearchPage` and `ResultsState` carry them in a parallel vector. Alternative: an extra `chat.getMessage` call on open to read the raw time. Rejected: one more request per jump for data the search answer already had.

**One effect, two concurrent requests.** `Effect::LoadContext { room, kind, private, id, raw_ts, before: 25, after: 25, ctx_id }` runs the older request (`latest=raw_ts`, `inclusive=true`, `count=before+1`) and the newer request (`oldest=raw_ts`, `count=100`) together with `tokio::join!`. The older answer must contain the result's id, else the result is gone (`NotFound`). If the newer answer has fewer than 100 messages, its oldest `after` messages are the neighbours; if it has exactly 100, the neighbours are unreachable and `newer_missing` is set. A failure of either request fails the whole load. Answer: `Action::ContextLoaded { room, ctx_id, messages, selected, newer_missing }` or `ContextFailed`.

**Ids for stale answers.** `App.context_seq` and `App.pending_context: Option<u64>` as for searches. An answer is accepted only if `pending_context` equals its id and the results list is still open for that room. Closing the list, opening another room or opening another result clears or replaces the pending id.

**Reuse list code.** The motions use `messages::step` as the results list does. The drawing is a `render_context` function in `ui/results.rs` next to the list's, using the messages cursor style for the selected row and the same row context as the list (so day dividers show, see `day-dividers`). Title: ` Context (q to return) `, styled with the existing `results.*` elements; no new theme element.

**Window size.** 25 before and 25 after are constants in `app/results.rs`, not settings. The newer request always asks for 100 (the server maximum), because only a short answer proves the neighbours are all there.

**Jump to the real chat: replace and detach.** The `select` action in the context view takes the window and does the following for its room:
- replaces `app.messages[room]` with the window (oldest first), sets `HistoryState::Loaded { oldest_ts: <raw ts of the window's oldest message>, exhausted: false, loading_older: false }`, and records the room in `App.detached` (a `HashSet<RoomId>`);
- closes the view and the list, focuses the messages pane and puts the messages cursor on the result.
The window's raw oldest timestamp comes with `ContextLoaded` (`oldest_raw`), so `maybe_load_older` keeps working: older pages merge by id as usual.

While a room is detached: `realtime::on_message` does not append to it (the message is dropped from the list; the sidebar unread marker still updates through the subscription path); send, edit, delete and retry are refused with a status message; `GotoBottom` at the last loaded message, or opening another room, **returns**: the room's messages, history state and detached flag are dropped and, for `G`, the room is loaded again as on first open (`history::on_open`), cursor on the newest. Alternative considered: append live messages anyway. Rejected: a message would appear after a gap and the "newest" cursor would lie. Alternative: allow sending while detached. Rejected: a sent message would land in the middle of an old conversation.

**Reached by `Enter` twice.** The first `Enter` (in the list) only reads; the second (in the window) is the explicit step into the real chat. `o` was considered and dropped: it is already bound to `EnterInsert`, so a new action needs no new key and no keymap change.

## Risks / Trade-offs

- [Detached state touches live merge, send and history] → One flag, checked in four places (`on_message`, send/edit/delete/retry, `G`, room switch); tests for each, and the invariant that a non-detached room behaves exactly as before.
- [Detached room may end before the present when more than 100 newer messages exist] → The status line says so on the jump; `G` goes to the real newest.

- [Result with more than 100 newer messages has no forward context] → Stated in the spec and on the status line. A later change could page forward with repeated `latest`-bounded requests from the live end, or use another endpoint.
- [Two requests per jump] → Run concurrently; both are small.
- [Window can be stale] → It is a snapshot at open time; live messages are not added (spec).
- [A new overlay state can swallow keys] → One close path; a test that every non-motion action is a no-op while open, as for the list.
- [9.0.0 not measured] → The history probe is run against 9.0.0 while implementing; a difference stops the work and updates this design.

## Migration Plan

No data or config migration. Rollback is reverting the change.
