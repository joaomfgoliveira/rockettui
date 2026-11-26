# Design

## Context

See `proposal.md`. The room switcher (`src/app/switcher.rs`) and the search results list (`src/app/results.rs`) are the models for the state and the keys; the thread pane (`src/app/thread.rs`, `src/ui/thread.rs`) is the model for the column. `add-thread-view` provides the pane and the `OpenThread` flow; `add-thread-follow` provides the `following` state.

## Goals / Non-Goals

**Goals:**
- Find and open any thread of the open room from the keyboard.
- Reuse the pane's column and open flow, and keep the user's place when going from the list to a thread and back.

**Non-Goals:**
- A list across all rooms (one room at a time, like message search).
- Showing replies in the list rows, or acting on threads from the list (follow toggle in the list).

## Decisions

**1. Server-side listing and search.** `GET chat.getThreadsList` with `rid`, `type` (`following` or `unread`; omitted means all, and `type=all` is a 400 on 8.9.1 and 9.0.0), `text`, `offset` and `count`. Filter and search are server parameters, so paging and filtering stay correct for rooms with many threads. *Alternative:* load everything and filter locally like the switcher. Rejected: the switcher filters a bounded list of rooms; threads are unbounded. The text match is the server's. Measured on 8.9.1 and 9.0.0: `text=(` answers 200 and matches the parenthesis literally, so the client sends the text as typed and does not escape it (escaping would double-escape).

**2. A pane in the thread column.** A `ThreadList { filter, query, rows, selected, load, has_more, generation }` on `App`, as built for the first version of this change, with a `Focus::ThreadList` for the pane. The column is the thread pane's slot (`ui/layout`), so beside the messages on a wide terminal and in place of them on a narrow one, and one column holds either the list or a thread. A `generation` counter is bumped on every filter or text change so a late response for an older query is dropped. The search request is debounced by a short timer effect rather than sent per key. *Alternative:* an overlay like the room switcher (the first version). Rejected after use: it hides the messages and cannot be left without closing it.

**3. Choosing hands the column over, and closing gives it back.** The row carries the parent id; opening goes through the same open flow as `gt`, in the same column. While a thread opened from the list is shown, the `ThreadList` state is kept (its rows, filter, query, selection, and which rows are loaded). Closing the thread returns to the list as it was, without a new request when the rows are held. Closing the list returns focus to the pane it was opened from. If the parent is not in the main flow, it comes from `get_message` before the swap (already used for jump targets); a 404 is "thread is gone" and the list stays. `gT` while a thread is open shows the list in its place; the thread's unsent replies are kept like on any close.

**4. Keys.** `list_threads` on `gT`. In the focused pane the motions (`j`, `k`, `gg`, `G`, arrows, half pages), `select` and `search_start` (`/`) resolve through the keymap, so rebinding applies. `move_left` and `move_right` (`h` and `l`) change the filter, so `Tab` stays the pane cycle; `Esc` and `q` are the pane's only fixed keys and close it. The focus keys (`Ctrl-w h`) leave it for the messages. `/` opens a search line inside the pane (a state, not a mode: the badge stays NORMAL); typed text edits the query, `Enter` keeps it and returns to the list keys, `Esc` clears it and returns to the list keys.

**5. Rows are not messages.** A row has no `following` field: `replies` holds the ids of the thread's followers, so the thread is followed when the user's own id is in it; `tcount` is the reply count and `tlm` the last reply time. The row mapping keeps only author, a bounded first line of text, counts, last-reply time and followed state, so the list holds little text. Text is sanitized with the existing `ui/sanitize.rs`.

## Risks / Trade-offs

- [Search text with special characters breaks the request] -> The server matches them literally (measured); test that `(`, `[` and `.` are sent unchanged.
- [Stale answers overwrite a newer list] -> `generation` check.
- [Parent deleted but thread listed] -> The "gone" path keeps the list shown.
- [`h` and `l` change the filter instead of moving left and right] -> There is nothing to move sideways in a list; the focus keys (`Ctrl-w h`) still leave the pane, and `Tab` cycles through it.
- [`gT` over an open thread drops the thread] -> Its unsent replies go to the outbox as on any close; the thread can be reopened from the list.
- [Two things share one column] -> One state each, one visible at a time, checked by tests for list to thread and back, and for room change closing both.
- [The unread filter depends on thread-unread data from the server] -> It is the server's answer; `add-thread-unread` only adds marks elsewhere.

## Open Questions

- None that change the specs.
