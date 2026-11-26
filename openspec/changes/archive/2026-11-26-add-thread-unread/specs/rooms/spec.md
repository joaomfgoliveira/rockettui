# Spec Delta

## ADDED Requirements

### Requirement: Thread badge position
The thread badge of a room (see `thread-unread`) SHALL come after the unread count and the mention badge in the room's row and SHALL NOT change them: the count, the mention badge and the folder header count keep the rules of "Unread count and mention badge".

#### Scenario: Existing badges unchanged
- **WHEN** a room has 3 unread messages, 1 user mention and no unread threads
- **THEN** the row shows `(3)` and `@1` exactly as before

#### Scenario: Order
- **WHEN** a room has a count, a mention badge and a thread badge
- **THEN** the row shows them in that order
