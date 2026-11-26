# Proposal

## Why

Thread replies are hidden from the main flow, so a thread the user did not see being created, or one that scrolled far up, is hard to find. Rocket.Chat has a per-room thread list (all, followed, unread), shown in a side column next to the messages. A keyboard version of it, in the slot of the thread pane, is the natural way to find and open threads. Third thread change; depends on `add-thread-view` (the pane) and benefits from `add-thread-follow` (the followed filter).

## What Changes

- A new Normal-mode action `list_threads` (`gT` by default) opens a side column listing the threads of the open room, most recently active first. The column is a focusable pane in the place of the thread pane: beside the messages on a wide terminal, in place of them on a narrow one.
- Each row shows the parent's author, a one-line shortened start of its text, the reply count and the time of the last reply, and `following` when followed.
- The list can be filtered between all threads, followed threads and unread threads, and searched by text.
- The pane takes vim-style keys (`j`, `k`, `gg`, `G`, `/` to search, `h` and `l` for the filter, `Enter` to open, `Esc` or `q` to close).
- Choosing a thread replaces the list with the thread in the same column, whether or not the parent is loaded in the main flow. Closing that thread returns to the list with its filter, search and selection kept.
- The list loads pages from the server as the user moves down it.

## Capabilities

### New Capabilities
- `thread-list`: the per-room thread list pane: rows, filters, search, paging and choosing.

### Modified Capabilities
- `keymap`: adds the `list_threads` action and its default key (ADDED requirement).
- `modal-navigation`: the thread list is a pane like the thread pane, not a mode or an overlay (ADDED requirement).
- `message-threads`: closing a thread opened from the list returns to the list (MODIFIED requirement `Pane focus and closing`).

## Impact

- `src/api`: `chat.getThreadsList` (room id, type filter, text, offset, count) and its mapping.
- `src/app`: a `Focus::ThreadList`, the list state, its actions and effects, and the hand-over between the list and the thread pane.
- `src/ui`: the column drawing in the thread pane's slot (layout shared with the thread pane).
- Needs `get_message` for a parent that is not in the main flow.
- No new dependencies.
