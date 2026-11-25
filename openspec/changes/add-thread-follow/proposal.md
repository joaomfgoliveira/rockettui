# Proposal

## Why

Rocket.Chat lets a user follow a thread: followed threads notify the user of new replies and appear in their followed list, and an unfollowed thread goes quiet. The server follows threads automatically (for the author of the parent and for anyone who replies), so without a way to toggle it from rockettui the user cannot silence a noisy thread or pick up one they only read. This is the second thread change and depends on `add-thread-view`.

## What Changes

- The thread marker row and the thread pane header show whether the user follows the thread.
- A new Normal-mode action `toggle_follow_thread` (`gf` by default) follows the thread of the message under the cursor, or unfollows it when it is already followed. It works in the message pane (on a parent or a channel-visible reply) and in the thread pane.
- The follow state is kept in step with the server, including changes made from another client.
- A failed request leaves the state as it was and says so in the status bar, without message text.

## Capabilities

### New Capabilities
- `thread-follow`: the follow state of a thread, how it is shown, how it is toggled, and how it stays in step with the server.

### Modified Capabilities
- `keymap`: adds the `toggle_follow_thread` action and its default key (ADDED requirement).

## Impact

- `src/data`: the parent's follower list or a derived `followed: bool` on `Message`.
- `src/api`: `chat.followMessage` and `chat.unfollowMessage`, with the user's id (already known to the connection) used to read the follower list.
- `src/app`: an `Action` and an `Effect`, reusing the pending-and-revert pattern of reactions.
- `src/ui`: the follow indicator in the marker row and the pane header.
- Depends on the thread fields and the pane from `add-thread-view`.
- No new dependencies.
