# Spec Delta

## ADDED Requirements

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
