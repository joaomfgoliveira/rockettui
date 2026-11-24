# Proposal

## Why

Finding something in a conversation means scrolling with `k` and reading. Vim users expect `/`, `n` and `N`, and rockettui is built around that muscle memory, but `/` is unbound today. The messages are already in memory, so finding in them needs no server and no network.

## What Changes

- Add find in the open room: `/` in Normal mode with the messages pane focused opens a search prompt on the command row. `<CR>` runs it and moves the cursor to the nearest match, `Esc` cancels it.
- Add `n` and `N` to jump to the next and previous match. At the oldest loaded message, `n` loads one more page of older history and keeps looking, and stops with a status line when the room has no more history or nothing matches.
- Matching is smart-case: case-insensitive unless the query has an uppercase letter. The query is plain text, not a regex.
- Highlight the matches in the messages pane. A new search replaces the old one, and `Esc` in Normal mode clears the highlight.
- Add the bindable actions `search_start`, `search_next` and `search_prev`, bound to `/`, `n` and `N` by default in Normal mode. The context popup lists them when a message is selected.
- Like the help overlay and the context popup, the prompt is a state of the app, not a mode: the mode badge keeps showing the real mode and `keymap.toml` gets no new table.

Out of scope: searching on the server (the separate `search-room` change), backward search with `?` (that key opens the context popup), regex or search options, searching the sidebar, and searching across rooms.

## Capabilities

### New Capabilities
- `message-search`: finding text in the messages of the open room: the prompt, the matching rule, jumping between matches, highlighting, and loading older history while looking.

### Modified Capabilities
- `keymap`: new bindable actions `search_start`, `search_next` and `search_prev`, with `/`, `n` and `N` as default keys.
- `theme`: new themeable element for a match in the messages pane.
- `context-help`: a message selected in the messages pane lists the search actions with their live keys.
- `message-history`: older history can also be loaded because a search ran out of loaded messages.

## Impact

- `input/`: three new actions and their default rows.
- `app/`: search state on `App` (query, matches, current match), the prompt handling at the top of the key path, a pure matcher over `Message` bodies, and a request for an older page when `n` runs out.
- `ui/`: highlight spans in the messages pane, the prompt on the command row, a `search.match` theme element.
- No new dependencies and no network call of its own. The query and the matched text are never logged or formatted in `Debug`, like every other message content.
