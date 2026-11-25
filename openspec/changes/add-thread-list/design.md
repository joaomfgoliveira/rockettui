# Design

## Context

See `proposal.md`. The room switcher (`src/app/switcher.rs`, `src/ui/switcher.rs`) is the model: an overlay with its own search line and selection that takes keys before the keymap. `add-thread-view` provides the pane and the `OpenThread` flow; `add-thread-follow` provides the `following` state.

## Goals / Non-Goals

**Goals:**
- Find and open any thread of the open room from the keyboard.
- Reuse the switcher's overlay mechanics and the pane's open flow.

**Non-Goals:**
- A list across all rooms (one room at a time, like message search).
- Showing replies in the list rows, or acting on threads from the list (follow toggle in the list).

## Decisions

**1. Server-side listing and search.** `GET chat.getThreadsList` with `rid`, `type` (`all`, `following`, `unread`), `text`, `offset` and `count`. Filter and search are server parameters, so paging and filtering stay correct for rooms with many threads. *Alternative:* load everything and filter locally like the switcher. Rejected: the switcher filters a bounded list of rooms; threads are unbounded. The text match is the server's (regex-like, verify on 8.9.1 and 9.0.0 what an unescaped special character does and escape the text before sending).

**2. Overlay state mirrors the switcher.** A `ThreadList { filter, query, rows, selected, load, has_more, generation }` on `App`. A `generation` counter is bumped on every filter or text change so a late response for an older query is dropped. The search request is debounced by a short timer effect rather than sent per key.

**3. Choosing reuses `OpenThread`.** The row carries the parent id; opening goes through the same action as `gt`. If the parent is not in the main flow, the pane's parent comes from `get_message` (already used for jump targets); a 404 is "thread is gone".

**4. Keys.** `list_threads` on `gT`. Inside the overlay: `Tab`/`S-Tab` for the filter, arrows and `C-n`/`C-p` for the selection, as the switcher does, so typing stays free for the search text.

**5. Rows are not messages.** The row mapping keeps only author, a bounded first line of text, counts, last-reply time and followed state, so the list holds little text. Text is sanitized with the existing `ui/sanitize.rs`.

## Risks / Trade-offs

- [Search text with regex characters breaks the request or matches too much] -> Escape before sending, test with `(`, `[` and `.`.
- [Stale answers overwrite a newer list] -> `generation` check.
- [Parent deleted but thread listed] -> The "gone" path keeps the overlay open.
- [The unread filter depends on thread-unread data from the server] -> It is the server's answer; `add-thread-unread` only adds marks elsewhere.

## Open Questions

- None that change the specs.
