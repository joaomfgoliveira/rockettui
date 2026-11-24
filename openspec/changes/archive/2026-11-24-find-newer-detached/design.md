# Design

## Context

See `proposal.md`. A search waits for an older page with `Search.seeking` and `history::request_older`; `search::on_page` looks again when the page is merged and `search::on_failed` stops the wait. A detached room (`app::jump`) loads newer pages through `maybe_load_newer` (cursor on the last loaded message) and applies them in `on_newer_loaded`, which ends the detached state when a page reaches the newest.

## Goals / Non-Goals

**Goals:**
- `N` behaves toward newer messages in a detached room exactly as `n` does toward older ones, reusing the same wait, the same status texts and the same abandon rules.

**Non-Goals:**
- No change for rooms that are not detached, and none to `n`.

## Decisions

**Remember the direction.** `Search` gains `seeking_newer: bool`, meaningful while `seeking` is true. `seek_older` sets it false; the new `seek_newer` sets it true. The older page handler (`on_page`) returns early when the wait is toward newer, and the newer handler returns early when it is toward older, so a page of the other kind (for example one the cursor asked for) never resolves the wrong wait.

**`prev` starts a newer wait.** When `N` finds no match among the loaded messages and the room is detached, `seek_newer` sets the wait and asks for a forward page through a function shared with the cursor trigger (`jump::request_newer`, which does nothing while a page is already in flight, so the wait then simply uses that page). A room that is not detached says "No newer match" at once, as today.

**Resolving a page.** After `on_newer_loaded` applies a page it calls `search::on_newer_page(room)`: if the search waits toward newer for this room, look for a match after the cursor and jump on one (stopping the wait); with none, ask for the next page if the room is still detached, or stop the wait and say "No newer match" once the newest was reached. The cursor never moves without a match. A failed page calls the existing `search::on_failed`, which stops any wait.

**Abandoning.** Opening another room or clearing the search already drops the search (`search::clear`), so a later page finds no search and requests nothing; the page itself is still appended, since it belongs to the room.

## Risks / Trade-offs

- [A long distance to the newest costs several requests] → One page of 50 at a time, each searched before the next is asked, and the search stops at the first match; the same trade-off as older pages.
- [Reaching the newest ends the detached state as a side effect] → Intended and stated in the spec; the room is then live and complete.
