# Spec Delta

## MODIFIED Requirements

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
