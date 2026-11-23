# Proposal

## Why

History is a one-shot REST snapshot, so messages from other people only appear after `:reconnect`. For daily use (core value: live incoming messages) the app needs a realtime connection that survives drops. Replaces GSD Phase 5 (MSG-03, MSG-04).

## What Changes

- A custom minimal DDP client over `tokio-tungstenite` delivers new messages for the open room live.
- On a dropped connection the client reconnects and resumes live updates.
- Own echoed messages are already handled by id (see `message-sending`: an echo of a sent message is ignored, of a pending one confirms it), so only the append path is new.
- Unread state can refresh live instead of only on `:reconnect`.

## Capabilities

### New Capabilities
- `realtime-messages`: live incoming messages over DDP, reconnect and resume behavior.

### Modified Capabilities
- `message-history`: the "one-shot snapshot" limitation goes away; merging live and fetched messages needs a requirement.
- `rooms`: unread flags may refresh without `:reconnect`.

## Impact

New `realtime/` module (DDP over WebSocket), new Action variants, run-loop task. Auth reuses the existing PAT; token must never be logged. Needs design decisions (subscription model, backoff, gap handling after reconnect) before tasks.

## Open Questions

- What happens to messages missed during a disconnect: refetch the open room, or resume from the last timestamp?
- Do all joined rooms stream, or only the open room plus unread changes?
