# message-history Specification

## Purpose

Defines how the history of the open room is fetched, paged, cached and rendered as text.

## Requirements

### Requirement: First-open history
On first open of a room the app SHALL fetch the newest `history_size` messages, cache them per room and position the cursor at the newest message. Reopening SHALL NOT refetch, except that a room marked stale by a realtime reconnect SHALL refetch its newest page on its next open. There SHALL be no startup prefetch of all rooms.

#### Scenario: Reopen
- **WHEN** the user reopens a room that already loaded and is not stale
- **THEN** no new history request is made

#### Scenario: Reopen a stale room
- **WHEN** the user reopens a loaded room that was marked stale by a reconnect
- **THEN** its newest page is fetched and merged by message id, and the room is no longer stale

### Requirement: Loading and failure states
While history loads, the messages pane SHALL show a loading placeholder. On failure it SHALL show "Could not load history" plus the status-bar error, and reopening the room SHALL retry. `:reconnect` SHALL clear the failed state of all rooms and refetch the open room if it failed, other failed rooms retrying on next open. `:reconnect` itself SHALL NOT refetch loaded rooms; refetching after a lost realtime connection is gap recovery (see `realtime-messages`).

#### Scenario: Retry after failure
- **WHEN** the open room's history failed and the user runs `:reconnect`
- **THEN** that room is fetched again and loaded rooms are not

### Requirement: Older pages
Reaching the oldest loaded message, or a search running out of loaded messages older than the cursor (see `message-search`), SHALL load the next older page of `history_size` messages, prepended with the cursor staying on the same message unless the search then moves it to a match. A page that adds no new message, or an empty first page, SHALL mark the end of history and show a "Beginning of history" status message. One older page SHALL be in flight per room, whoever asked for it.

#### Scenario: End of history
- **WHEN** an older page adds no messages
- **THEN** no further pages are requested for that room

#### Scenario: Search asks for a page
- **WHEN** a search finds no match older than the cursor and the room has more history
- **THEN** one older page is requested, prepended, and the cursor stays on its message unless the page holds a match

#### Scenario: Page already in flight
- **WHEN** an older page is loading because the cursor reached the oldest message, and a search then needs older history
- **THEN** no second request is sent and the search continues with the page that is loading

### Requirement: Text-only rendering
Messages SHALL render as text, sanitized for terminal output. System messages SHALL render as a dim one-liner and attachments as `[attachment: name]`. Messages SHALL wrap with a hanging indent in the compact layout or use a header line in the two-line layout, with bodies aligned at a shared column.

#### Scenario: Attachment
- **WHEN** a message has a file attachment
- **THEN** the row shows the attachment tag with its name

#### Scenario: Control characters
- **WHEN** a message contains terminal control characters
- **THEN** they are not passed to the terminal

### Requirement: Leftover encrypted messages
A message from an encrypted source SHALL render as `[encrypted message]` and its content SHALL NOT be mapped.

#### Scenario: Encrypted leftover
- **WHEN** a plain room contains an encrypted message
- **THEN** the row shows the fixed text

### Requirement: Live and fetched messages merge by id
Messages that arrive live and messages from fetched pages SHALL be merged by message id within a room: a held message is never duplicated, a pending sent message is confirmed in place, and order follows message time. Live messages SHALL NOT disturb the cursor message, and older pages SHALL still prepend with the cursor staying on the same message.

#### Scenario: Live message while reading older messages
- **WHEN** the cursor is on an older message and a live message arrives
- **THEN** the cursor stays on the same message and the new message is held at the bottom

#### Scenario: Page overlaps live messages
- **WHEN** a refetched page contains messages that already arrived live
- **THEN** each message appears once

### Requirement: Edited marker
A message the server reports as edited SHALL be drawn with a visible edited marker, whether it arrived live or in a fetched page. The marker SHALL be themeable like the other message elements.

#### Scenario: Edited message in history
- **WHEN** a fetched page contains a message with an edit time
- **THEN** its row shows the edited marker

### Requirement: Deleted lines
A deleted message SHALL be drawn as one dim line that keeps its place and the original author and time, says "message deleted" and never shows its text or who deleted it. Deleted lines exist only for the running session: history never returns deleted messages, so they are absent after a restart or a room reload.

#### Scenario: Deleted message the server keeps
- **WHEN** the server keeps a deleted message as a deleted-status message (setting "show deleted status"), live or in a fetched page
- **THEN** it is drawn as a deleted line with the original author, without its text and without an edited marker, and never as a system message

#### Scenario: Reload after a delete
- **WHEN** the app restarts and loads a room that had a deleted message
- **THEN** the deleted message is simply not shown

### Requirement: Reconcile edits and deletes after a reconnect
When a refetched page after a reconnect covers a held message, the held copy SHALL take the page's text and edited state. A held server message that falls inside the page's time range and is absent from it SHALL become a deleted line. Held messages older than the page SHALL be left as they are.

#### Scenario: Edited while offline
- **WHEN** a message was edited during an outage and the refetched page contains its new text
- **THEN** the row shows the new text and the edited marker

#### Scenario: Deleted while offline
- **WHEN** a held message inside the refetched page's time range is absent from it
- **THEN** it becomes a deleted line

#### Scenario: Older than the page
- **WHEN** a held message is older than everything in the refetched page
- **THEN** it is unchanged, even if it was deleted or edited meanwhile

### Requirement: Detached room
After a jump from a search result (see `search-result-context`), a room SHALL be detached from the newest messages: its loaded messages are a window of older history. While a room is detached, a message that arrives live for it SHALL NOT be added to its list. Older pages SHALL load as usual.

#### Scenario: Live message while detached
- **WHEN** a message arrives for a detached room
- **THEN** the room's list does not change

#### Scenario: Older pages still load
- **WHEN** the cursor reaches the oldest loaded message of a detached room
- **THEN** the next older page is requested and merged by id

### Requirement: No writing while detached
Sending, editing and deleting in a detached room SHALL be refused with a status message saying the room is showing older messages.

#### Scenario: Send while detached
- **WHEN** the user sends a message in a detached room
- **THEN** nothing is sent and the status line says the room is showing older messages

### Requirement: Leaving the detached state
Pressing the go-to-bottom action (`G`) with the cursor on the last loaded message of a detached room, or opening another room, SHALL end the detached state by dropping the room's loaded messages. For go-to-bottom the newest messages SHALL be loaded again with the cursor on the newest.

#### Scenario: Return with go-to-bottom
- **WHEN** the cursor is on the last loaded message of a detached room and the user presses `G`
- **THEN** the room is no longer detached and shows its newest messages with the cursor on the newest

#### Scenario: Leave the room
- **WHEN** the user opens another room and later returns
- **THEN** the room shows its newest messages and is not detached

### Requirement: Reloading a room
`:reload` SHALL reload the newest messages of the open room from the server. In a detached room it SHALL end the detached state and show the newest messages with the cursor on the newest. In a loaded room it SHALL fetch the newest messages again and merge them by id, keeping the cursor, the older pages already loaded and the room's unsent messages. In a room whose first load failed it SHALL retry the load. It SHALL say on the status line that it is reloading.

#### Scenario: Leave a detached room
- **WHEN** the room is detached after a jump from a search result and the user runs `:reload`
- **THEN** the room is no longer detached and shows its newest messages with the cursor on the newest

#### Scenario: Refresh a loaded room
- **WHEN** the room is loaded and the user runs `:reload`
- **THEN** the newest messages are requested and any message the room lacks is added, with the cursor and the older pages kept

#### Scenario: Retry a failed room
- **WHEN** the room's first load failed and the user runs `:reload`
- **THEN** the load is requested again

#### Scenario: Unsent messages are kept
- **WHEN** the room holds a failed or queued message and the user runs `:reload`
- **THEN** that message is still in the room, in the same state

#### Scenario: Status line
- **WHEN** the user runs `:reload` and a request is sent
- **THEN** the status line says the newest messages are being reloaded and the line clears on the next key

### Requirement: Reload refusals
`:reload` SHALL change nothing and show a one-line message when no room is open, when the open room is encrypted, or when the room is already loading its messages.

#### Scenario: No room open
- **WHEN** no room is open and the user runs `:reload`
- **THEN** the status line asks to open a room and nothing is requested

#### Scenario: Encrypted room
- **WHEN** the open room is encrypted and the user runs `:reload`
- **THEN** the status line says encrypted rooms are not supported and nothing is requested

#### Scenario: Already loading
- **WHEN** the room is still loading its first page and the user runs `:reload`
- **THEN** the status line says it is already loading and no second request is sent
