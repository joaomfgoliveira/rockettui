# rooms Specification

## Purpose

Defines how the sidebar lists the user's rooms and how rooms are opened, including encrypted rooms that the client cannot read.

## Requirements

### Requirement: Joined rooms only
The sidebar SHALL list the user's joined channels, private groups, discussions, teams and DMs. DMs SHALL be named by the other user's username. Closed subscriptions, omnichannel rooms and subscriptions without a room record SHALL be skipped. There SHALL be no directory browsing or joining.

#### Scenario: Closed DM
- **WHEN** a subscription has `open: false`
- **THEN** it does not appear

### Requirement: Sidebar sections
The sidebar SHALL group rooms into Unread, Folders and the rest, with the rest ordered by most recent activity. Folders SHALL be Favorites (favorited rooms without a category) followed by the user's custom categories. Empty folders SHALL be omitted, and Folders SHALL fold and unfold. A room that is unread appears in Unread rather than in its folder.

#### Scenario: Unread room
- **WHEN** a subscription has unread messages
- **THEN** the room is shown in Unread with an unread marker (a flag only, no counts)

### Requirement: Encrypted rooms
Encrypted rooms SHALL be listed with the encrypted marker. Opening one SHALL show an "encrypted, not supported" placeholder and SHALL NOT request history or send messages.

#### Scenario: Open encrypted room
- **WHEN** the user opens an encrypted room
- **THEN** the placeholder is shown and no history request is made

### Requirement: Mark read after a delay
A room that is open and unread SHALL be marked read on the server once it has stayed the open room for the configured delay (`mark_read_delay_ms`, default 3000 ms). Opening another room, or leaving the room, before the delay ends SHALL cancel it and send nothing. A room that is not unread SHALL NOT be marked read.

#### Scenario: Stay in the room
- **WHEN** the user opens an unread room and keeps it open for the delay
- **THEN** the room is marked read on the server and leaves Unread

#### Scenario: Leave before the delay
- **WHEN** the user opens an unread room and opens another one before the delay ends
- **THEN** nothing is sent and the first room stays unread

#### Scenario: Already read
- **WHEN** the user opens a room that is not unread
- **THEN** no mark-read request is sent

### Requirement: Live messages in the open room
A message from another user arriving live in the open room SHALL restart the delay, so a room the user is looking at is marked read after the delay following its newest message. Own messages SHALL NOT start the delay.

#### Scenario: Message arrives while reading
- **WHEN** another user posts in the open room
- **THEN** the room is marked read once the delay has passed since that message

### Requirement: Mark read is best effort
A failed mark-read request SHALL show nothing and SHALL NOT change the room's unread state. The next open of the room, or the next live message in it while it is open, SHALL try again. The request SHALL be sent with the user's own token and SHALL NOT log message contents.

#### Scenario: Request fails
- **WHEN** the mark-read request fails
- **THEN** no error is shown and the room stays unread
