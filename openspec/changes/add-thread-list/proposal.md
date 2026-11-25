# Proposal

## Why

Thread replies are hidden from the main flow, so a thread the user did not see being created, or one that scrolled far up, is hard to find. Rocket.Chat has a per-room thread list (all, followed, unread). A keyboard picker for it, in the style of the room switcher, is the natural way to find and open threads. Third thread change; depends on `add-thread-view` (the pane) and benefits from `add-thread-follow` (the followed filter).

## What Changes

- A new Normal-mode action `list_threads` (`gT` by default) opens an overlay listing the threads of the open room, most recently active first.
- Each row shows the parent's author, a one-line shortened start of its text, the reply count and the time of the last reply, and `following` when followed.
- The list can be filtered between all threads, followed threads and unread threads, and searched by text.
- Choosing a thread opens it in the thread pane, whether or not the parent is loaded in the main flow.
- The list loads pages from the server as the user moves down it.

## Capabilities

### New Capabilities
- `thread-list`: the per-room thread picker: rows, filters, search, paging and choosing.

### Modified Capabilities
- `keymap`: adds the `list_threads` action and its default key (ADDED requirement).
- `modal-navigation`: the thread list is an overlay like the room switcher, not a mode (ADDED requirement).

## Impact

- `src/api`: `chat.getThreadsList` (room id, type filter, text, offset, count) and its mapping.
- `src/app`: overlay state, its actions and effects, reusing the switcher's input handling.
- `src/ui`: the overlay drawing.
- Needs `get_message` for a parent that is not in the main flow.
- No new dependencies.
