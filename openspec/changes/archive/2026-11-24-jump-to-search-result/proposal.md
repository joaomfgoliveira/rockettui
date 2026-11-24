# Proposal

## Why

`:search` finds old messages but only lists them: a result cannot be opened, so the user sees a line without the conversation around it. `search-room` left this out because the messages in memory are only the newest page, so putting an old result "in place" would leave a gap between it and them.

## What Changes

- `Enter` (the existing `select` action) on a result opens a **context view**: a read-only window of the room's history around that result, drawn over the messages pane, with the result selected.
- The window is fetched on demand: the result and up to 25 messages before it, and up to 25 after it **when the server lets the client reach the ones next to it**. Measured on 8.9.1 and 9.0.0: the history `oldest` parameter returns the newest messages after a time, not the adjacent ones, so the newer side is reachable only when at most 100 newer messages exist; otherwise the window ends at the result and the status line says newer messages are not shown.
- The window is separate from the room's loaded messages, so there is no gap to manage and live messages are not mixed into it.
- `q`, `Esc` or `Ctrl-c` closes the context view and returns to the results list with the same result selected; a second `q` closes the list, as today.
- The view scrolls with the usual motions, is read-only, and is a state of the app, not a mode.
- `Enter` (the `select` action) again, in the context view, **jumps to the real chat**: the window's messages become the room's loaded messages, the list and the view close, the messages pane has focus and its cursor is on the result. The room is then **detached from the newest messages**: live messages are not appended (they would sit after a gap), sending, editing and deleting are refused with a status message, older history loads as usual with `gg`/`k`, and `G` at the last loaded message, or opening another room, returns to the newest messages by reloading the room.

Out of scope: loading more history from inside the window, jumping from `/` matches, paging forward from a detached room, a `:reload` command, and searching across rooms.

## Capabilities

### New Capabilities
- `search-result-context`: the read-only window around a search result: what it holds, how it is fetched, how it is left, and its error states.

### Modified Capabilities
- `room-search-results`: a result can be opened with the select action.
- `context-help`: `?` lists the keys of the results list and of the context view.
- `message-history`: a room can be detached from the newest messages after a jump.

## Impact

- `api/`: history requests with `latest` and `oldest` for the window (the raw server timestamp is needed, so the search page also keeps each result's raw `ts`); the existing message mapper is reused.
- `app/`: a context-view state on `App`, an effect and action pair, key routing above the results list.
- `ui/`: the window drawn over the messages pane with the message cursor style.
- Builds on `day-dividers`: the window shows day dividers the same way as the results list, because it can span days. Apply that change first.
- Query text, message text and timestamps are never logged or formatted in `Debug`.
- Server behaviour was measured on 8.9.1 with a read-only probe; 9.0.0 is checked while implementing.
