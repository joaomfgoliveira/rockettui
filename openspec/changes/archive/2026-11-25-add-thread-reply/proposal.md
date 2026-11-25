# Proposal

## Why

With `add-thread-view` the user can read a thread but not answer it: the input bar only sends to the main flow. A thread reader that cannot reply forces a switch to another client for every answer, and a message that has no replies yet cannot start a thread at all. Fourth thread change; depends on `add-thread-view`.

## What Changes

- With the thread pane focused, `i` starts typing a reply; the input bar then targets the thread and says so in its title. `Enter` sends the text as a reply to the open thread.
- A new action `reply_in_thread` (`gR` by default) on any message in the message pane opens that message's thread pane (empty if it has no replies yet) and starts typing a reply, which creates the thread when sent.
- An Insert-mode action `toggle_reply_to_channel` (`<C-b>` by default) marks the reply to also be shown in the channel; the state is visible in the input bar.
- A reply uses the existing optimistic send, ordered queue, retry and discard, and appears in the pane at once.
- Edit, delete and react work on the messages of the thread pane as they do in the main flow.
- Drafts are kept per thread, not mixed with the room's draft.

## Capabilities

### New Capabilities
- `thread-reply`: starting and sending a thread reply, the also-send-to-channel option, per-thread drafts and message actions inside the pane.

### Modified Capabilities
- `message-sending`: a thread reply is a send and follows the existing send rules (ADDED requirement).
- `keymap`: adds `reply_in_thread` and `toggle_reply_to_channel` (ADDED requirement).

## Impact

- `src/api`: `send_message` gains an optional thread id and the also-in-channel flag.
- `src/app`: reply target on the input state, drafts keyed by room and thread, the send queue and reconciliation extended to carry the target.
- `src/ui`: input bar title and the also-in-channel indicator.
- Edit, delete and react reducers look messages up in the pane as well as the main flow.
- No new dependencies.
