# Spec Delta

## RENAMED Requirements
- FROM: `### Requirement: Newer messages only when reachable`
- TO: `### Requirement: Newer messages in the window`

## MODIFIED Requirements

### Requirement: Context window contents
The context view SHALL show the selected result, up to 25 messages before it and up to 25 messages after it, oldest first, in the same row format as the results list. It SHALL fetch them from the server when opened and SHALL NOT change the room's loaded messages, history state or cursor.

#### Scenario: Result in the middle
- **WHEN** a result has more than 25 messages before it and more than 25 after it
- **THEN** the view shows 25 before, the result and 25 after, in time order

#### Scenario: Result near the start of the room
- **WHEN** only 3 messages exist before the result
- **THEN** the view shows those 3 and the result and does not ask for more

#### Scenario: Room's messages untouched
- **WHEN** the user opens and closes a context view
- **THEN** the room's loaded messages and its message cursor are as before

### Requirement: Newer messages in the window
The view SHALL include the up to 25 messages written immediately after the result, however many newer messages the room holds.

#### Scenario: Few newer messages
- **WHEN** 10 messages exist after the result
- **THEN** the view shows all 10 after it

#### Scenario: Many newer messages
- **WHEN** more than 100 messages exist after the result
- **THEN** the view shows the 25 that follow the result directly

## REMOVED Requirements

### Requirement: Jumping to the chat
**Reason**: The window can now always reach the newest side, so the "newer messages not shown" status of the jump no longer exists; the requirement is restated as "Entering the chat".
**Migration**: See "Entering the chat".

## ADDED Requirements

### Requirement: Entering the chat
The select action (`Enter` by default) in the context view SHALL close the context view and the results list and make the window's messages the room's loaded messages, with the messages pane focused and its cursor on the result. The room SHALL be detached from the newest messages (see `message-history`), unless the window already ends at the newest message.

#### Scenario: Jump
- **WHEN** the context view of a result is open and the user presses `Enter`
- **THEN** the list and the view are closed, the room shows the window's messages and the cursor is on the result

#### Scenario: Jump keeps the result visible
- **WHEN** the user jumps
- **THEN** the result's row is inside the messages pane

#### Scenario: Window ends at the newest
- **WHEN** the window already holds the newest message of the room and the user jumps
- **THEN** the room is not detached and receives live messages
