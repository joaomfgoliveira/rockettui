# realtime-messages Specification

## Purpose

Defines how the app receives new messages and unread changes from the server as they happen, and how it keeps that live connection healthy and recovers after it drops.

## Requirements

### Requirement: Live connection
After the rooms load, the app SHALL open a realtime connection to the configured server, authenticated with the user's own Personal Access Token, and keep it open for the whole run. The token SHALL NOT be logged or shown. A failure to connect SHALL NOT stop the app from reading cached history or sending by REST.

#### Scenario: Connected
- **WHEN** the rooms have loaded and the server accepts the token
- **THEN** the connection is live and incoming messages begin to arrive

#### Scenario: Rejected token
- **WHEN** the server rejects the realtime login
- **THEN** the status shows that live updates are unavailable, the rest of the app keeps working, and the token is not shown

### Requirement: Incoming messages of loaded rooms
A new message in any joined room SHALL be appended to that room, in order, when the room's history is loaded, except a thread reply that is not also shown in the channel: that reply SHALL NOT be appended to the room's main flow and SHALL be handled as a live thread reply (see `message-threads`). A message already held by id SHALL NOT be duplicated. When the user is reading the newest messages of the open room the view SHALL follow per the existing follow rules.

#### Scenario: Message in the open room
- **WHEN** another user posts in the open room
- **THEN** the message appears at the bottom without any command

#### Scenario: Message in another loaded room
- **WHEN** another user posts in a loaded room that is not open
- **THEN** the message is held and shown when that room is opened

#### Scenario: Duplicate delivery
- **WHEN** the same message id arrives twice
- **THEN** the room holds it once

#### Scenario: Thread reply in the open room
- **WHEN** another user posts a thread reply that is not shown in the channel
- **THEN** the main flow does not change and the parent's reply count goes up

#### Scenario: Thread reply shown in the channel
- **WHEN** another user posts a thread reply that is also shown in the channel
- **THEN** it appears in the main flow and in its thread

### Requirement: Rooms not yet loaded
A live message for a room whose history has not loaded SHALL NOT be stored as that room's history. The first open of such a room SHALL still fetch history as if no live message had arrived.

#### Scenario: Live message before first open
- **WHEN** another user posts in a joined room the user has never opened
- **THEN** opening it later fetches its history and shows that message once

### Requirement: Heartbeat
The app SHALL answer every server heartbeat. A connection that the server closes, or that stays silent for longer than twice the heartbeat interval, SHALL be treated as dropped.

#### Scenario: Server ping
- **WHEN** the server sends a heartbeat ping
- **THEN** the app answers at once and the connection stays open

#### Scenario: Silent connection
- **WHEN** nothing arrives for longer than twice the heartbeat interval
- **THEN** the connection is treated as dropped and reconnect begins

### Requirement: Reconnect with backoff
After a drop the app SHALL reconnect on its own, waiting longer between failed attempts up to a fixed ceiling and resetting the wait after a connection stays up. The status line SHALL show that the app is reconnecting and SHALL clear it when live updates resume. `:reconnect` SHALL also restart the realtime connection.

#### Scenario: Server restarts
- **WHEN** the connection drops and the server is unreachable for a while
- **THEN** the app keeps retrying with increasing waits and the status shows it is reconnecting

#### Scenario: Back online
- **WHEN** a retry succeeds
- **THEN** live updates resume and the reconnecting status clears

### Requirement: Gap recovery after reconnect
After a reconnect the app SHALL refetch the newest page of the open room and merge it by message id. Every other loaded room SHALL be marked stale and refetched the next time it is opened. When the refetched page overlaps no held message, the gap is larger than one page and the app SHALL drop that room's cache and reload it.

#### Scenario: Missed messages in the open room
- **WHEN** messages were posted while the connection was down and it reconnects
- **THEN** the open room shows them without duplicates and without losing the cursor message

#### Scenario: Other loaded room
- **WHEN** the user opens a loaded room that went stale during the outage
- **THEN** its newest page is fetched and merged

#### Scenario: Gap larger than a page
- **WHEN** the refetched page of the open room overlaps nothing held
- **THEN** the room is reloaded from its newest page rather than leaving a hole

### Requirement: Own messages through the stream
The echo of a message the user sent SHALL be handled by id as in `message-sending`: ignored when already confirmed, confirming it when still pending. It SHALL NOT appear twice.

#### Scenario: Echo of a pending send
- **WHEN** the realtime stream delivers a message whose id matches a pending sent message
- **THEN** that message is confirmed in place and no second row appears

### Requirement: Live edits
When another user edits a message held in a loaded room, the held message SHALL be replaced in place by the edited copy and marked edited. An update to a held message that changes only its reactions SHALL replace the held message's reactions in place, without marking it edited. An update that changes none of its text, its edited state or its reactions (a pin) SHALL leave it untouched.

#### Scenario: Message edited in the open room
- **WHEN** another user edits a message that is held in the open room
- **THEN** the row shows the new text and the edited marker without any command, in the same position

#### Scenario: Reaction on a held message
- **WHEN** the server delivers a held message again with only a reaction added
- **THEN** the row shows the new reaction without any command, the message is not marked edited and the cursor stays on the same message at the same screen row

#### Scenario: Reaction removed from a held message
- **WHEN** the server delivers a held message again with a reaction removed
- **THEN** the row no longer shows it, and a message left with no reactions loses its reaction row

#### Scenario: Update that changes nothing shown
- **WHEN** the server delivers a held message again with the same text, edited state and reactions (a pin)
- **THEN** the row does not change

#### Scenario: Edit of a message not held
- **WHEN** an edit arrives for a message the room does not hold
- **THEN** it is handled like any other live message for that room

#### Scenario: Reaction on a message not held
- **WHEN** a reaction update arrives for a message the room does not hold
- **THEN** it is handled like any other live message for that room

### Requirement: Live deletes
When a message held in a loaded room is deleted, whoever deleted it, the row SHALL stay in place as a deleted line and its text SHALL be dropped from memory. The deleted line SHALL NOT say who deleted the message. A delete for a message not held, or still pending locally, SHALL be ignored.

#### Scenario: Message deleted in the open room
- **WHEN** a message held in the open room is deleted
- **THEN** its row becomes a dim "message deleted" line that keeps the original author and time, in the same position, and the cursor and scroll position do not move

#### Scenario: Delete of an unknown message
- **WHEN** a delete arrives for an id the room does not hold
- **THEN** nothing changes

### Requirement: Delete notifications cover joined rooms
The app SHALL receive delete notifications for every joined room that can show messages (not encrypted rooms) and SHALL resubscribe after every reconnect. A room joined or left while connected SHALL be added or dropped.

#### Scenario: Delete in a room not yet opened
- **WHEN** a message is deleted in a joined room the user has not opened yet, and the user opens it later
- **THEN** the room's history does not contain the message

#### Scenario: After a reconnect
- **WHEN** the connection drops and comes back
- **THEN** deletes in every joined room are received again

### Requirement: Realtime never logs content
No log, error text or status message SHALL contain the token or message content, and the realtime frames SHALL NOT be written to any log.

#### Scenario: Error while reading frames
- **WHEN** a frame cannot be parsed
- **THEN** the app skips it and any message shown names the failure class only

### Requirement: Live unread counts
A live update to a subscription SHALL carry the room's unread count and mention counts to the sidebar. A field the update does not contain SHALL keep the value the app holds for it; an update that carries none of the unread fields SHALL change nothing. A room whose update leaves it with no flag and a zero count SHALL show no count and no badge. The counts SHALL be the server's, never computed from the messages the app receives.

#### Scenario: Message in another room
- **WHEN** another user posts in a room that is not open and the server reports its unread count as 4 and one user mention
- **THEN** the sidebar shows `(4)` and `@1` for that room without any command

#### Scenario: Partial update
- **WHEN** a second update for the same room arrives with neither an unread count nor mention counts
- **THEN** the count and badge shown stay as they were

#### Scenario: Read elsewhere
- **WHEN** another client marks the room read and the server reports a zero count and no alert
- **THEN** the room leaves Unread and its count and badge disappear
