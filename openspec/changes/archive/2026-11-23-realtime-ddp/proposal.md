# Proposal

## Why

History is a one-shot REST snapshot, so messages from other people only appear after `:reconnect`. For daily use (core value: live incoming messages) the app needs a realtime connection that survives drops. Replaces GSD Phase 5 (MSG-03, MSG-04).

## What Changes

- A custom minimal DDP client over `tokio-tungstenite` delivers new messages for every joined room live, through one subscription.
- The client answers the server's heartbeat; on a dropped or silent connection it reconnects with backoff and resumes.
- After a reconnect the open room is refetched and merged, and every other loaded room is marked stale and refetched on its next open.
- Own echoed messages are already handled by id (see `message-sending`: an echo of a sent message is ignored, of a pending one confirms it), so only the append path for other people's messages is new.
- Unread flags refresh live from the user's subscription updates instead of only on connect and `:reconnect`.
- Out of scope here, planned as follow-ups: `live-edits-deletes` (receiving others' edits and deletes) and `edit-delete-own-messages`. The transport is built so a per-room stream can be added later.

## Capabilities

### New Capabilities
- `realtime-messages`: live incoming messages over DDP, heartbeat, reconnect and gap recovery.

### Modified Capabilities
- `message-history`: the "reopen never refetches" rule gains the stale-after-reconnect exception; merging live and fetched messages needs a requirement.
- `rooms`: unread flags refresh live instead of only on connect and `:reconnect`.

## Impact

New `realtime/` module (DDP over WebSocket), new Action variants, a long-lived run-loop task. Auth reuses the existing PAT; the token is sent once in the DDP login frame and must never be logged. New dependency `tokio-tungstenite` with a TLS feature matching `reqwest` (this matters again in `company-server` if a private CA is needed).

## Findings (spike, `dev/probe_ddp.py`, local Docker 8.9.1 and 9.0.0, identical on both)

- `login {resume: <PAT>}` works over DDP; no REST login needed.
- `sub stream-room-messages ["__my_messages__", false]` delivers messages of all joined rooms (channel, group, DM), each with a `rid`.
- `stream-notify-user` `<uid>/subscriptions-changed` fires per message; DM and group subscription docs carry `alert`/`unread`.
- An edit arrives on the same stream, same `_id`, with `editedAt`/`editedBy`. A delete arrives on `stream-notify-room` `<rid>/deleteMessage` with only `{_id}` and is not carried by `__my_messages__`.
- The server sends a DDP `ping` about every 30 s (first at ~17 s); an unanswered ping gets the connection closed.

## Resolved Questions

- Messages missed during a disconnect: refetch the newest page of the open room and merge by id; other loaded rooms are marked stale and refetch on next open; a page with no overlap means a gap larger than a page, so that room's cache is dropped and reloaded.
- Which rooms stream: all joined rooms, via the single `__my_messages__` subscription.
