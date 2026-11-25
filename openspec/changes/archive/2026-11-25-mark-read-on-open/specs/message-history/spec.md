# Spec Delta

## ADDED Requirements

### Requirement: Unread divider
When a room is opened, a divider line reading `unread` SHALL be drawn above the first message from another user that is newer than the room's last-seen time on the server. When the open room has no such message and another user's message arrives live, the divider SHALL be drawn above that message.

#### Scenario: Unread messages on open
- **WHEN** the user opens a room that has messages from others newer than its last-seen time
- **THEN** an `unread` divider is drawn above the oldest of them

#### Scenario: Nothing unread on open
- **WHEN** the user opens a room with nothing newer than its last-seen time
- **THEN** no divider is drawn

#### Scenario: Live message in a read room
- **WHEN** another user posts in an open room that has no divider
- **THEN** an `unread` divider is drawn above the new message

#### Scenario: Own messages only
- **WHEN** the only messages newer than the last-seen time are the user's own
- **THEN** no divider is drawn

#### Scenario: Last-seen time unknown
- **WHEN** the server gives no last-seen time for the room
- **THEN** fetched messages get no divider and only a live message from another user draws one

### Requirement: Unread divider drawing
There SHALL be at most one divider, and it SHALL NOT move when later messages arrive. It SHALL take no column from message rows, SHALL be drawn below the date divider when both fall above the same message, and the cursor SHALL skip it like a date divider.

#### Scenario: Later messages
- **WHEN** more messages arrive while the divider is shown
- **THEN** the divider stays above the first unread message

#### Scenario: Cursor skips the divider
- **WHEN** the user moves the cursor over the divider
- **THEN** it passes to the next message

#### Scenario: With a date divider
- **WHEN** the first unread message is also the first of its day
- **THEN** the date divider is drawn above the `unread` divider

### Requirement: Unread divider lifetime
The divider SHALL stay when the room is marked read and SHALL be dropped when the room is left, so reopening a room that has since been read shows no divider.

#### Scenario: Marking read keeps the divider
- **WHEN** the room is marked read while open
- **THEN** the divider stays until the user leaves the room

#### Scenario: Reopening
- **WHEN** the user leaves the room and opens it again after it was marked read
- **THEN** no divider is drawn
