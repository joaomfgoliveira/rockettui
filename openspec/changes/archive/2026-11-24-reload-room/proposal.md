# Proposal

## Why

After jumping into a search result the room is **detached** from the newest messages (`jump-to-search-result`), and the only way back is `G` on the last loaded message. That is easy to miss, and nothing lets the user refresh a room that looks stale or failed. A `:reload` command gives one obvious way to get the open room's newest messages, from any state.

## What Changes

- `:reload` reloads the open room's newest messages from the server.
- In a **detached** room it ends the detached state and shows the newest messages with the cursor on the newest, like `G` does.
- In a room that is **Loaded**, it fetches the newest page again and merges it by id (new messages appear, edited and deleted ones are updated), keeping the cursor and the older pages already loaded.
- In a room whose first load **Failed**, it retries the load.
- While the room is already **Loading**, it does nothing and says so.
- It never touches the room's unsent messages (queued, sending, failed, unconfirmed): they stay and keep their state.
- It says on the status line that it is reloading; the line clears on the next key like other info messages.
- No room open, or an encrypted room, gives a one-line error and changes nothing.

Out of scope: a key binding, a config key, reloading other rooms or all rooms (`:reconnect` does the rooms), paging forward from a detached room.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `modal-navigation`: the command table gains `:reload`.
- `message-history`: a requirement describes what reloading does in each room state.

## Impact

- `app/command.rs`: one table row and its function (it also shows in the help overlay, which is generated from the table).
- `app/history.rs`: a `reload` function that picks the path by history state; the existing first-page, refetch and detach code is reused unchanged.
- No new network call, `Effect`, `Action`, config key or theme element: it uses `Effect::LoadHistory` as opening a room does.
