# Spec Delta

## MODIFIED Requirements

### Requirement: No writing while detached
Sending, editing, deleting and reacting in a detached room SHALL be refused with a status message saying the room is showing older messages.

#### Scenario: Send while detached
- **WHEN** the user sends a message in a detached room
- **THEN** nothing is sent and the status line says the room is showing older messages

#### Scenario: React while detached
- **WHEN** the user presses the react key in a detached room
- **THEN** the picker does not open and the status line says the room is showing older messages
