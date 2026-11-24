# Proposal

## Why

`n` finds the next match toward older messages and, when none is loaded, fetches older pages until it finds one. `N` (toward newer) only looks at the messages already loaded. In a room detached after a jump from a search result, the newer messages are fetched page by page as the cursor reaches the end (`page-forward-detached`), so `N` misses every match that lies further toward the present than the loaded window: it says "No newer match" although one exists. See `message-search`.

## What Changes

- In a **detached** room, `N` with no newer match among the loaded messages fetches the next page of newer messages, searches it, and repeats until a match is found, the newest message is reached, or the search is abandoned, like `n` does toward older messages.
- Finding a match stops the fetching and puts the cursor on it.
- Reaching the newest message without a match says so on the status line ("No newer match") and leaves the cursor where it was; the room then is no longer detached, as in any forward page that reaches the newest.
- A failed page stops the search and shows the error.
- In a room that is not detached nothing changes: `N` still only searches the loaded messages, which already end at the newest.

Out of scope: `/` starting toward newer, wrapping around, a change to `n`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `message-search`: a new requirement for loading newer history while searching in a detached room.

## Impact

- `app/search.rs`: the search remembers which direction it is waiting in; `prev` can start a newer wait; a forward page answer is checked like an older one.
- `app/jump.rs`: the forward request is shared between the cursor trigger and the search; the page handler tells the search.
- No new request, `Effect`, `Action`, key or config. Query and message text are never logged.
