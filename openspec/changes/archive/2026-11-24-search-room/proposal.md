# Proposal

## Why

`find-in-room` only sees the messages already loaded. A message from last month, or from before the first page, can never be found that way. Rocket.Chat searches a room on the server, and a spike against the local 8.9.1 server (`dev/probe_search.py`) showed that `chat.search` answers with the same message shape as history, so it fits the existing client with little new code.

## What Changes

- Add the command `:search <text>`: it asks the server to search the open room and shows the results in a list over the messages pane, newest first. `/` stays the in-memory find of `find-in-room`.
- The results list is read-only and is a state of the app, not a mode, like the help overlay. It scrolls with the usual motions (`j`, `k`, `gg`, `G`, `Ctrl-d`, `Ctrl-u`), loads the next page when the cursor reaches the end, and `q` or `Esc` closes it.
- The text is sent as typed, so the server's own query syntax (`from:user`, `has:star`, `is:pinned`, `on:date`, `/regex/`) works without client code. Server search is always case-insensitive, unlike the smart-case `/`; the spec says so.
- An empty query, no open room, or an encrypted room is refused with a status line and nothing is sent. No results, a failed request and a room without access each get a clear status line.
- Results can be read but not jumped to. Showing a result in its place in the conversation would leave a gap between an old result and the messages in memory, so that is a later change.

Out of scope: jumping to a result in context, searching across rooms, a query builder or filters UI, and searching encrypted rooms (the server only holds ciphertext).

## Capabilities

### New Capabilities
- `room-search-results`: searching the open room on the server: the `:search` command, the request and paging, the results list, and its error states.

### Modified Capabilities
- `modal-navigation`: the command table gains `:search`.
- `theme`: new themeable elements for the results list.

## Impact

- `api/`: a `chat.search` call returning the existing message type, mapped by the existing mapper, with the request carrying a query text and an offset.
- `app/`: a results state on `App`, a new effect and action pair for the request and its answer, and a `:search` row in the command table.
- `ui/`: the results list over the messages pane, themed through `theme.toml`.
- Depends on `find-in-room` being archived first, to reuse its status wording and highlighting.
- The query is the user's own text and goes only to their own server. It is never logged, formatted in `Debug`, or stored, and neither are the results.
- Server behaviour was verified on 8.9.1 only; 9.0.0 is checked while implementing.
