# Proposal

## Why

Rocket.Chat threads are invisible in rockettui. A parent message shows no sign that it has replies, a live reply lands in the main flow of the room as if it were an ordinary message (the realtime stream delivers it with a `tmid`), and there is no way to read a thread. This is the first of five thread changes and the foundation for the rest: nothing else (follow, list, reply, unread) works until a thread can be seen and opened.

## What Changes

- A message that has replies shows a thread marker row under its body (reply count and time of the last reply), in the same style as the reaction row.
- Thread replies are kept out of the main message flow, unless the server marks the reply as also shown in the channel (`tshow`). This applies to fetched history, search context and live messages.
- A new Normal-mode action `open_thread` (`gt` by default) opens the thread of the message under the cursor, or of the reply's parent when the cursor is on a reply, in a right-side thread pane.
- The thread pane shows the parent message followed by its replies, loaded from the server, with its own cursor and scrolling. Live replies to the open thread appear in it without any command.
- The pane joins the `<Tab>` cycle while open and closes with `Esc` or a `close_thread` action. On a narrow terminal it replaces the messages pane instead of splitting it.
- Read-only: following, listing, replying and unread tracking are separate changes (`add-thread-follow`, `add-thread-list`, `add-thread-reply`, `add-thread-unread`).

## Capabilities

### New Capabilities
- `message-threads`: how a thread parent is marked, how replies are kept out of the main flow, and how a thread is opened, read and kept live in a pane.

### Modified Capabilities
- `realtime-messages`: a live message that is a thread reply is no longer appended to the room's main flow; it goes to the thread instead.
- `keymap`: adds the `open_thread` and `close_thread` actions and their default keys (ADDED requirement, not a rewrite of the action vocabulary).
- `context-help`: the popup on a message lists the thread action.
- `theme`: adds the element for the thread marker row and the pane.

## Impact

- `src/data`: `Message` gains thread fields (`tmid`, `tshow`, `tcount`, last-reply time).
- `src/api`: `chat.getThreadMessages` and the mapping of thread fields on every message path (history, search, context, single message, realtime).
- `src/app`: a `ThreadPane` state on `App`, new `Action`s, `Focus::Thread`, entry in `CYCLE` (`update.rs`).
- `src/realtime`: routing of replies, and the parent's `tcount` update.
- `src/ui`: marker row, thread pane, layout split.
- Needs measuring against the local Docker servers (8.9.1 and 9.0.0) before the specs are final: which fields the history and stream messages really carry.
- No new dependencies, no new permissions, nothing logged.
