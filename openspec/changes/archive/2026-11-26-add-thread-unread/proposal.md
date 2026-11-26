# Proposal

## Why

Thread replies are hidden from the main flow and do not count in a room's unread number, so a busy thread can go unnoticed: the sidebar shows nothing, the parent shows nothing, and the user only finds out by opening the thread. The server tracks per-thread unread state in the user's subscription. This change surfaces it and marks a thread read when the user reads it. Fifth and last thread change; depends on `add-thread-view`, and complements `add-thread-list` (its unread filter) and `add-thread-follow`.

## What Changes

- A thread with replies the user has not read shows a `new` mark in its marker row.
- A room with unread threads shows a thread badge in the sidebar (`T` and the number of unread threads), after the unread count and the mention badge, and a thread where the user was mentioned makes the badge stand out like the mention badge does.
- A room that has unread threads but no unread messages is listed in the Unread section.
- Reading a thread (it is open in the pane, and it stays so for a short delay) marks it read on the server, best effort. The `new` mark and the badge go down when the thread is read, here or in another client.
- Live subscription updates carry the thread unread state; a partial update keeps what is held.

## Capabilities

### New Capabilities
- `thread-unread`: the unread state of threads, how it is shown on parents, how a thread is marked read, and how it stays in step with the server.

### Modified Capabilities
- `rooms`: adds the thread badge to the sidebar row and the Unread section rule (ADDED requirement; the existing count and mention badge rules are unchanged).
- `theme`: adds the elements for the thread badge and the `new` mark (ADDED requirement).

## Impact

- `src/api`: the thread unread fields of a subscription (`tunread`, `tunreadUser`, `tunreadGroup`) and the mark-thread-read call.
- `src/realtime`: carries the thread fields of subscription updates; may need the DDP client to call a method (see design).
- `src/app`: room state, the mark-read timer for the pane (extends the existing room one).
- `src/ui`: sidebar badge, marker row mark.
- No new dependencies.
