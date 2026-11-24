# Design

## Context

See `proposal.md`. Measured on 8.9.1 (read-only probe on `rt-public`, 132 messages, 121 after the target): `channels.history?oldest=T&count=N&offset=K` skips the K newest messages after `T` (the server sorts newest first and applies `offset` before `count`), so `offset = M - N` returns the N messages next to `T`, where `M` is how many messages follow `T`; `offset=M, count=1` returns nothing. `oldest` is exclusive of `T`. Without `offset`, `oldest` returns the newest N after `T`, which is why the context window used to need "at most 100 newer".

## Goals / Non-Goals

**Goals:**
- Read forward from any point to the present, one page at a time, with the same one-request-in-flight discipline as older pages.

**Non-Goals:**
- No change for rooms that are not detached; no new key.

## Decisions

**One primitive: the nearest N messages after a raw timestamp.** `Client::load_newer(room, kind, private, raw_ts, n) -> NewerPage { messages, newest_raw, reached_newest }`:
1. Ask `oldest=T&count=100`. A short answer (fewer than 100) holds every message after `T`: take its oldest `n`; `reached_newest` is true when it held at most `n`.
2. Otherwise at least 100 follow. Find `M`, the number after `T`, by probing `count=1` at an offset: double the offset from 100 until the answer is empty, then bisect. About `2*log2(M)` small requests, none larger than one message.
3. Ask `offset=max(0, M-n), count=min(n, M)`: the nearest `n` after `T`. `reached_newest` is `M <= n`.
Messages are mapped by the existing history mapper (oldest first, with the raw timestamp of the newest). Any failed request fails the page. A bound on the probing (at most 24 probe requests, which covers more messages than a room can hold in practice) turns a runaway into `BadResponse`.
Alternative: walk back from the present with `latest`, paging until reaching `T`. Rejected: its cost grows with the distance to the present (`M/100` full pages), where this one grows with its logarithm and fetches only `n` messages.

**The context window uses the same primitive** for its newer side (`n = 25`), replacing the "at most 100" rule: `ContextWindow` loses `newer_missing`, and the "newer messages are not shown" status and the "ends before the newest" jump status go. The two requests (older and newer) still run together.

**Detached state becomes a record.** `App.detached` changes from a set of rooms to a map `RoomId -> Detached { newest_raw: Option<String>, loading_newer: bool }`. A jump sets `newest_raw` from the window's newest message (the context load returns it with the window). `is_detached` and `reset` keep their meaning.

**Forward page trigger.** After a messages-pane motion, like `maybe_load_older`: if the room is detached, the cursor is on the last loaded message and no forward page is in flight, record `Effect::LoadNewer { room, kind, private, raw_ts, n: 50, newer_id }` and set `loading_newer`. The answer is `Action::NewerLoaded` or `Action::NewerFailed`, dropped unless it matches the room, the detached record and `newer_id`.

**Applying a page.** Messages the room already holds (by id) are skipped, the rest are appended in order; the cursor and top row stay (an append shifts no index). `newest_raw` becomes the page's newest raw timestamp. When `reached_newest` is true the detached record is removed, the status line says the newest messages were reached, and one `refetch_newest` is recorded: live messages that arrived while the room was detached were dropped, and the merge by id closes any gap between the page and the present. A failed page clears `loading_newer` and shows the error.

**`G` and `:reload`** keep returning to the newest at once (they drop the window). `G` from the middle still moves to the last loaded message, which also triggers a forward page; a second `G` returns.

## Risks / Trade-offs

- [Probing adds small requests] → Only for the forward page and the context load, only when at least 100 messages follow; each is one message. Mitigation: the bound above, and a page that fails shows an error and is retried by moving.
- [A message arriving between the probe and the page request shifts the offset by one] → The page then holds one message too few or too many at its edge; ids are merged, and the final `refetch_newest` repairs the end. Documented, not worked around.
- [9.0.0 behaves differently] → The probe is run on 9.0.0 before the code; a difference stops the work and updates this design.
