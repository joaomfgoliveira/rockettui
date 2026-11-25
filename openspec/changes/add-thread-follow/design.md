# Design

## Context

See `proposal.md`. `add-thread-view` gives `Message` the thread fields, a `ThreadPane` on `App` and the marker row. Reactions (`src/app/react.rs`) already have a send-then-confirm flow with an effect and a failure status; the follow toggle has the same shape.

## Goals / Non-Goals

**Goals:**
- Show and toggle the follow state, in step with the server.

**Non-Goals:**
- Listing followed threads (`add-thread-list`).
- Unread or notification display (`add-thread-unread`).
- Any local notification: the app only mirrors the server's state.

## Decisions

**1. Follow state is derived, not stored twice.** The server's parent message carries a `replies` list of follower user ids. Keep that list on the parent (`followers: Vec<String>`, private, never drawn or logged) and compute `followed = followers.contains(own_user_id)`. The user's id comes from the connection that already holds it for the `X-User-Id` header. *Alternative:* a separate `followed: bool` set locally. Rejected: it would drift when the server follows automatically or another client toggles.

**2. Confirm on answer, no optimistic flip.** Send `chat.followMessage` or `chat.unfollowMessage` with `mid`, and change the state when the server answers success, or when the parent's update arrives, whichever comes first. This is a rare action, so a short wait is fine, and it makes the failure case trivial (nothing to revert). A `follow_pending` flag on the thread blocks a second request. *Alternative:* optimistic with revert like reactions. Rejected: more state for no felt gain.

**3. Live updates.** The parent's stream update carries the new `replies` list; the reducer replaces `followers` on it. Needs checking on the local servers that a follow change from another client produces an event at all. If it does not, the state is corrected on the next fetch and the spec scenario "Changed elsewhere" is narrowed to that.

**4. Key.** `toggle_follow_thread` on `gf` in Normal mode (`g` prefix, next to `gt`, `gr`, `gg`). The action resolves its target thread from the focused pane.

## Risks / Trade-offs

- [`replies` may be capped or absent on some message payloads] -> Treat absent as "unknown": show no `following` text and keep the last known state; confirm on the local servers.
- [Endpoint names or parameters differ between 8.9.1 and 9.0.0] -> Measure first, record fixtures; the client calls one method per verb.
- [Following state of a thread the user has not opened is stale until a fetch] -> Acceptable; the marker row shows what the last message payload said.

## Open Questions

- None that change the specs.
