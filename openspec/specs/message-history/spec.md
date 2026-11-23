# message-history Specification

## Purpose

Defines how the history of the open room is fetched, paged, cached and rendered as text.

## Requirements

### Requirement: First-open history
On first open of a room the app SHALL fetch the newest `history_size` messages, cache them per room and position the cursor at the newest message. Reopening SHALL NOT refetch. There SHALL be no startup prefetch of all rooms.

#### Scenario: Reopen
- **WHEN** the user reopens a room that already loaded
- **THEN** no new history request is made

### Requirement: Loading and failure states
While history loads, the messages pane SHALL show a loading placeholder. On failure it SHALL show "Could not load history" plus the status-bar error, and reopening the room SHALL retry. `:reconnect` SHALL clear the failed state of all rooms and refetch the open room if it failed, other failed rooms retrying on next open. Loaded rooms SHALL NOT be refetched.

#### Scenario: Retry after failure
- **WHEN** the open room's history failed and the user runs `:reconnect`
- **THEN** that room is fetched again and loaded rooms are not

### Requirement: Older pages
Reaching the oldest loaded message SHALL load the next older page of `history_size` messages, prepended with the cursor staying on the same message. A page that adds no new message, or an empty first page, SHALL mark the end of history and show a "Beginning of history" status message.

#### Scenario: End of history
- **WHEN** an older page adds no messages
- **THEN** no further pages are requested for that room

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
