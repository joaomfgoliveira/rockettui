# Proposal

## Why

After a jump from a search result the room is detached and ends where the window ends, so a user reading on toward the present hits a wall: with more than 100 messages after the result the window ends at the result (nothing newer is shown at all) and the only way on is `G` or `:reload`, which throw the position away. The history endpoint was assumed unable to return the messages next to a point in time, but a probe on 8.9.1 shows that `oldest` combined with `offset` can: the messages after a time are reachable at any distance. See `message-history` and `search-result-context`.

## What Changes

- Moving onto the last loaded message of a **detached** room loads the next page of newer messages (up to 50) and appends it, so the user can scroll on toward the present.
- When a page reaches the newest message, the room stops being detached: live messages resume and writing works again; the status line says so.
- The **context window** no longer ends at the result when many newer messages exist: it always shows up to 25 messages after the result, wherever they are. The "newer messages are not shown" status goes away.
- A failed forward page shows an error and is retried by moving onto the last message again, as for older pages.
- `G` and `:reload` still return to the newest messages at once.

Out of scope: paging forward in a room that is not detached (it already has the newest), a visible "newer messages exist" row, jumping from `/` find matches.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `message-history`: a detached room loads newer pages and ends its detached state at the newest message.
- `search-result-context`: the window always includes the messages after the result; the jump no longer has a "newest not reached" case.

## Impact

- `api/`: one request primitive, the nearest N messages after a raw timestamp (`oldest` + `offset`), used by both the context load and the forward page. Probe added; checked on 8.9.1 and 9.0.0.
- `app/`: per-room detached state grows from a flag to a small record (newest raw timestamp, in-flight flag); one effect/action pair for the forward page; `app/jump.rs` and the motion hook in `app/update.rs`.
- No new key, config key or theme element. Message text and timestamps are never logged or shown in `Debug`.
