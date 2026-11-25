# Spec Delta

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Opening does not mark read
**Reason**: The app is now a daily client, and an Unread section that never empties by reading is the problem this change solves.
**Migration**: See "Mark read after a delay" and "Live messages in the open room". The scenarios "Live unread flag" and "Server clears unread" keep their meaning as part of the sidebar sections behaviour and are re-stated by the new requirements.
