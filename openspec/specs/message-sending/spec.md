# message-sending Specification

## Purpose

Defines how users compose and send messages to the open room, how delivery states are shown and recovered, and how undelivered messages affect quitting.

## Requirements

### Requirement: Optimistic send
Pressing `Enter` in Insert mode SHALL clear the input, stay in Insert mode and append the message immediately as a pending row marked "sending". Empty or whitespace-only input SHALL NOT be sent. The newline key keeps its separate, rebindable binding.

#### Scenario: Send
- **WHEN** the user types text and presses `Enter`
- **THEN** a pending row appears authored by the user's own username and the input is empty

### Requirement: Confirmation in place
On success the pending row SHALL be replaced by the server message in the same position, without refetching history. The client SHALL supply the message id so that a later realtime echo of the same message is not duplicated.

#### Scenario: Sent
- **WHEN** the server accepts the message
- **THEN** the "sending" marker disappears and the row stays in place

### Requirement: Per-room ordered queue
Sends SHALL be serialized per room with at most one in flight, preserving order. A failed or unconfirmed message SHALL NOT block later queued messages.

#### Scenario: Slow network
- **WHEN** the user sends three messages quickly
- **THEN** they are delivered in order

### Requirement: Failed and unconfirmed
A rejected send (4xx, unreachable, TLS) SHALL mark the row "failed". A timeout, lost connection, 5xx answer or unreadable or oversized answer SHALL mark it "unconfirmed", since the server may have stored it. Either outcome SHALL show a fixed status-bar error, never server text or message body; the unconfirmed text says the message may not have been sent.

#### Scenario: Server error
- **WHEN** the server answers 500
- **THEN** the row is marked unconfirmed

### Requirement: Retry and discard
In Normal mode with the messages pane focused, `r` on the failed or unconfirmed row under the cursor SHALL retry it and `dd` SHALL discard it (both rebindable). A retry SHALL first check whether the server already has the message and SHALL NOT resend if it does, and if the resend errors SHALL check once more before marking it failed or unconfirmed.

#### Scenario: Already delivered
- **WHEN** the user retries an unconfirmed message the server already stored
- **THEN** it is marked sent and not posted again

### Requirement: Reconciliation with incoming messages
A message arriving with an id already known SHALL be ignored when that message is sent, and SHALL replace and confirm the row when it is still pending.

#### Scenario: Echo of own message
- **WHEN** the same id arrives for an already sent message
- **THEN** nothing changes

### Requirement: Send guards
Sending in an encrypted room SHALL be refused with an "encrypted, not supported" status message. Sending with no room open SHALL show a "No room open" error. The app SHALL NOT pre-check permissions, and rejections SHALL surface as a failed row.

#### Scenario: Encrypted room
- **WHEN** the user sends in an encrypted room
- **THEN** nothing is sent and the status bar shows the refusal

### Requirement: Drafts
Each room SHALL keep its own draft in memory (not on disk), restored when switching back to the room.

#### Scenario: Switch rooms
- **WHEN** the user leaves a half-typed message and returns
- **THEN** the draft is restored

### Requirement: Quit warning
If any message in any room is queued, sending, failed or unconfirmed, `:q` SHALL show a one-line warning with the count of undelivered messages (never their text) and not quit. A repeated `:q`, or `:q!`, SHALL quit. Any other command SHALL disarm the warning; an empty command line leaves it armed. `:reconnect` SHALL NOT affect sends.

#### Scenario: Undelivered quit
- **WHEN** a message is failed and the user runs `:q`
- **THEN** a warning is shown and the app stays open, and a second `:q` quits

### Requirement: Own username
The logged-in username SHALL be fetched once per connection and on `:reconnect`, kept in memory only, and used as author of pending rows. Until it is known, pending rows use a placeholder author that is renamed when it arrives; a failed fetch leaves it unset.

#### Scenario: Reconnect
- **WHEN** the user runs `:reconnect`
- **THEN** the username is fetched again
