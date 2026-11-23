# Spec Delta

## ADDED Requirements

### Requirement: Live edits
When another user edits a message held in a loaded room, the held message SHALL be replaced in place by the edited copy and marked edited. An update to a held message that changes neither its text nor its edited state (a reaction, a pin) SHALL leave it untouched.

#### Scenario: Message edited in the open room
- **WHEN** another user edits a message that is held in the open room
- **THEN** the row shows the new text and the edited marker without any command, in the same position

#### Scenario: Reaction on a held message
- **WHEN** the server delivers a held message again with only a reaction added
- **THEN** the row does not change

#### Scenario: Edit of a message not held
- **WHEN** an edit arrives for a message the room does not hold
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
