# Design

## Context

See proposal.md for motivation and the spike findings (`dev/probe_ddp.py`). Today every server interaction is a one-shot REST effect: `effect::spawn` runs a task, which sends `Action`s back through the one channel to the one reducer. The reducer never reads a clock. `messages::merge_incoming` already implements the held-id rule (ignore a held `Sent`, confirm a pending copy in place, otherwise append), so the append path for live messages exists.

The realtime connection is the first long-lived task. It does I/O, timing and backoff, so it lives outside the reducer and talks to it only through `Action`s.

```
 run loop
   |  spawns once, after rooms load (and again on :reconnect)
   v
 realtime task (owns the WebSocket)
   connect -> login{resume: PAT} -> subscribe x3 -> read loop
   answers ping / detects silence / backs off / reconnects
   |
   |  Action::RtConnected / RtDisconnected / RtMessage / RtSubscription
   v
 one channel -> update() -> merge_incoming / sidebar unread / Effect::LoadHistory
```

## Goals / Non-Goals

**Goals:**
- Live new messages and live unread flags for every joined room, surviving drops.
- Keep the TEA split: the task owns time and I/O, the reducer owns state decisions.
- Leave a seam for a per-room stream (deletes) without building it.

**Non-Goals:**
- Edits and deletes (`live-edits-deletes`, `edit-delete-own-messages`), typing indicators, presence, thread streams.
- Resuming from a timestamp (`chat.syncMessages`).
- Marking rooms read.

## Decisions

**1. Transport: a minimal DDP client on `tokio-tungstenite`, in a `realtime/` module.** The author chose a custom client (CLAUDE.md). DDP is small: `connect`, `method`, `sub`, `ping`/`pong`, `changed`, `ready`. Alternative: a generic Meteor DDP crate, rejected as unmaintained and as a learning shortcut. The TLS feature must match `reqwest`'s so one trust store serves both.

**2. Auth: `method login [{resume: <PAT>}]`.** The spike showed the PAT works as the resume token on 8.9.1 and 9.0.0. No password and no REST login. The token is held only by the realtime task and the existing `api::Client`, never by `App`, and frames are never logged.

**3. Subscriptions: three, fixed.**
- `stream-room-messages ["__my_messages__", false]` carries new messages (and, later, edits) for all joined rooms, so no per-room bookkeeping.
- `stream-notify-user "<uid>/subscriptions-changed"` carries `alert`/`unread`.
- `stream-notify-user "<uid>/rooms-changed"` carries room updates (activity ordering).
Alternative: one `stream-room-messages` per room, rejected as N subscriptions to track across joins, leaves and reconnects. Deletes need `stream-notify-room "<rid>/deleteMessage"` per room; that is the seam for `live-edits-deletes`, and this change's task only needs a way to send extra `sub` frames after connect.

**4. Heartbeat and liveness in the task.** Answer each DDP `ping` with `pong` at once. Declare the connection dead when nothing arrives for 2 x the observed ping interval (about 30 s, so a fixed 75 s timeout is the simplest start) or when the socket closes or errors. The spike confirmed that an unanswered ping gets the connection closed.

**5. Reconnect and backoff live in the task.** Exponential backoff with jitter, ceiling about 30 s, reset after a connection has been up for a while. The task emits `RtConnected` and `RtDisconnected`. Keeping the clock out of the reducer preserves the existing rule.

**6. Gap recovery is decided in the reducer.** On every `RtConnected` (not only a later one: a message can also land between a room's first fetch and the first connection; at startup nothing is loaded yet, so it costs nothing), the reducer pushes `Effect::LoadHistory` for the open room and marks other loaded rooms stale (a `HashSet<RoomId>` on `App`). When that refetch lands it merges by id; if the page overlaps nothing held, the room's cache is dropped and reloaded (a hole larger than one page cannot be repaired by merging). Alternative: `chat.syncMessages` from the last timestamp, rejected for now as more machinery than a personal client needs.

**7. Live messages for rooms never loaded are dropped from the message cache.** `merge_incoming` creates the room's vector with `entry().or_default()`, which could make an unopened room look loaded and skip its first history fetch. The reducer checks the room's loaded state before calling it; for an unloaded room only the unread state is updated. First-open verification of how "loaded" is tracked is the first implementation task.

**8. Unread from `subscriptions-changed`.** The reducer applies `alert`/`unread` to the room's sidebar state, which moves rooms between Unread and the rest as the existing sidebar logic already does. Opening a room still does not mark it read.

**9. Parsing reuses `api::map`.** Stream messages have the same shape as REST history messages (same keys in the spike), so they go through the same mapping, including the encrypted-message and sanitizing rules. A frame that fails to parse is skipped and counted by class, never logged.

**10. Status.** `ConnState` or an adjacent field shows `Reconnecting` while the socket is down and clears when live. A rejected login shows "live updates unavailable" and stops retrying until `:reconnect` (retrying a rejected token would only hammer the server).

## Risks / Trade-offs

- [Live message for an unloaded room is mistaken for loaded history] -> Decision 7, plus a regression test.
- [Gap larger than a page leaves a hole] -> overlap check and reload (Decision 6), tested with a gap longer than `history_size`.
- [Missed pings, e.g. a laptop sleep] -> silence timeout plus reconnect; a reconnect always runs gap recovery.
- [TLS feature mismatch with `reqwest`, private CA later] -> pick the same rustls/native-tls choice as `reqwest`; revisit in `company-server`.
- [Token leakage through a frame or error] -> the login frame is built and sent in one place, no `Debug` on it, and the existing redaction tests cover the new module.
- [A stale socket after `:reconnect`] -> `:reconnect` cancels the old task before spawning a new one.
- [Behaviour differs on the company server] -> verified against local Docker only; `company-server` re-runs the probe there once policy is confirmed.
- [Message arrives on the stream before a sent message's REST answer] -> already handled by the held-id rule and `awaiting_answer`.

## Migration Plan

No data migration. Rollout is by build. Rollback is reverting the change: the app falls back to the REST snapshot and `:reconnect`.

## Resolved scope call

`rooms-changed` is NOT subscribed in this change: live unread flags come from `subscriptions-changed`, and sidebar activity ordering keeps refreshing on connect and `:reconnect`. Live reordering can be a later pass. Decision 3 therefore lists two subscriptions to build (`__my_messages__` and `subscriptions-changed`); the `rooms-changed` bullet is kept only as the known option.
