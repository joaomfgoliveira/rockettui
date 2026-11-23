# Proposal

## Why

Once messages arrive live (`realtime-ddp`), other people's edits and deletes must show up live too, otherwise the screen drifts from the truth: an edited message keeps its old text and a deleted one stays visible. Replaces the receive side of GSD MSG-05.

## What Changes

- An edit arrives on the existing `__my_messages__` stream as the same `_id` with `editedAt` and `editedBy`. The held message is replaced in place and shows an edited marker. Today `merge_incoming` ignores a held `Sent` id, so this needs a new path.
- A delete arrives on `stream-notify-room` `<rid>/deleteMessage` with only `{_id}` and is NOT carried by `__my_messages__`. The client subscribes per joined room over the same connection and turns the held row into a tombstone (a dim "message deleted" line that keeps the author and time and drops the text), so no index shifts. A delete for a message still pending locally is ignored.
- Edits and deletes also apply to history fetched after a reconnect (a refetched page shows the edited text; a deleted id simply is not returned, so stale rows must be reconciled).

## Capabilities

### New Capabilities

### Modified Capabilities
- `realtime-messages`: live edits and deletes; per-room delete subscriptions.
- `message-history`: an edited marker in rendering; reconciling rows deleted while offline.

## Impact

Depends on `realtime-ddp` (transport and the way to add `sub` frames after connect). Reducer and renderer changes (edited marker, tombstone line, a new theme element). Delete behaviour was measured by the realtime spike on 8.9.1 and 9.0.0: payload is `{_id}` only. Decided: a deleted message stays as a tombstone, and the line never says who deleted it. Open: whether an edit in a read room flips it back to unread (measured in the first task).
