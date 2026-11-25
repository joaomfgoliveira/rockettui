## ADDED Requirements

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
