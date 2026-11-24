# Spec Delta

## ADDED Requirements

### Requirement: Newer pages while detached
Moving onto the last loaded message of a detached room SHALL load the next page of newer messages and append it after the loaded ones, one request at a time. The cursor and the scroll position SHALL NOT move when a page arrives. A page that fails SHALL show an error and leave the room detached; moving onto the last message again SHALL ask again.

#### Scenario: Newer page on reaching the end
- **WHEN** the cursor moves onto the last loaded message of a detached room that has newer messages
- **THEN** the next page of newer messages is requested and appended after it, and the cursor and scroll position do not move

#### Scenario: One request at a time
- **WHEN** a newer page is in flight and the cursor is on the last loaded message
- **THEN** no second request is sent

#### Scenario: Failed newer page
- **WHEN** a newer page fails
- **THEN** an error is shown, the room stays detached, and moving onto the last message again asks once more

### Requirement: Reaching the newest message
When a newer page contains the newest message of the room, the room SHALL stop being detached: live messages are added again, writing works, and the status line says the newest messages were reached. Messages that arrived while the room was detached SHALL be present afterwards.

#### Scenario: Leaving the detached state by reading on
- **WHEN** a newer page contains the newest message of the room
- **THEN** the room is no longer detached, live messages are added again, writing works, and the status line says the newest messages were reached

#### Scenario: Messages that arrived meanwhile
- **WHEN** a message arrived for the room while it was detached and the user then reads on to the newest
- **THEN** that message is in the room's list
