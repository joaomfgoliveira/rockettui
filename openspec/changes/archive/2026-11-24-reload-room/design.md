# Design

## Context

See `proposal.md`. Today a room's first page is requested by `history::on_open` (state `None` or `Failed` becomes `Loading`), a Loaded room that missed messages is refetched by `history::refetch_newest` and merged by id in `on_refetched`, and a detached room (`App.detached`, `app::jump`) holds a window of older history that `jump::reset` can drop. `:reconnect` retries Failed rooms and restarts realtime, but does nothing for a Loaded or detached room.

## Goals / Non-Goals

**Goals:**
- One command that gets the open room to "newest messages shown", whatever state it is in, without losing unsent messages.

**Non-Goals:**
- No new request type, no key binding, no change to `:reconnect`.

## Decisions

**Choose the path by the room's state, reusing existing code** (`history::reload`):
- not found / encrypted → error, nothing changes (the same checks `on_open` makes);
- `Loading` → info "already loading", nothing changes (one request in flight per room);
- detached → `jump::reset` (drops the window, its history state and view), clear the find state (its matches pointed into the old window), then `on_open`: the room is `Loading` and the newest page arrives as a first page;
- `Failed` or no state → remove the state and `on_open` (what reopening does);
- `Loaded` (not detached) → `refetch_newest`: the answer is merged by id (`on_refetched`), which also updates edited messages and tombstones deleted ones inside the page's span, and keeps the cursor and older pages.
Alternative: always drop everything and reload. Rejected: a healthy room would lose its scrolled-back pages and cursor for no reason.

**Unsent messages need no special handling.** A detached room cannot hold unsent messages (the jump refuses while one exists, and writing is refused while detached), and the other paths never remove messages. The first-page merge already keeps held pending messages and confirms a pending copy that the page contains.

**Status.** Info `Reloading the newest messages` is set whenever a request was recorded; errors and "already loading" use fixed text. No room name or message text is ever included. The line clears on the next key, as other info lines do.

**The results list.** Typing a command is allowed while the list is open; the reload runs on the room underneath and leaves the list alone.

## Risks / Trade-offs

- [A refetch of a Loaded room shows nothing visible when nothing changed] → The status line confirms the request; a failed refetch is silent today (`on_failed`), so it keeps the room as it is.
- [`:reload` on a `Loading` room looks ignored] → It says "already loading".
