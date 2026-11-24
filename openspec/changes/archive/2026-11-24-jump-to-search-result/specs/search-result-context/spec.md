# Spec Delta

## Purpose

Lets the user read the conversation around a server search result: a read-only window of the room's history, fetched on demand and kept apart from the live message list, so an old result can be seen in context without any gap in the room's loaded messages.

## ADDED Requirements

### Requirement: Context window contents
The context view SHALL show the selected result, up to 25 messages before it and up to 25 messages after it, oldest first, in the same row format as the results list. It SHALL fetch them from the server when opened and SHALL NOT change the room's loaded messages, history state or cursor.

#### Scenario: Result in the middle
- **WHEN** a result has more than 25 messages before it and more than 25 after it, all reachable
- **THEN** the view shows 25 before, the result and 25 after, in time order

#### Scenario: Result near the start of the room
- **WHEN** only 3 messages exist before the result
- **THEN** the view shows those 3 and the result and does not ask for more

#### Scenario: Room's messages untouched
- **WHEN** the user opens and closes a context view
- **THEN** the room's loaded messages and its message cursor are as before

### Requirement: Newer messages only when reachable
The view SHALL include the messages after the result only when the server lets the client reach the ones next to it, which is when at most 100 messages exist after the result. Otherwise the view SHALL end at the result and the status line SHALL say that newer messages are not shown.

#### Scenario: Few newer messages
- **WHEN** 10 messages exist after the result
- **THEN** the view shows all 10 after it

#### Scenario: Many newer messages
- **WHEN** more than 100 messages exist after the result
- **THEN** the view ends at the result and the status line says newer messages are not shown

### Requirement: Selection and scrolling
The result SHALL be selected when the view opens and in view. The move down and up, go to top and bottom, and half-page actions SHALL move the selection by the same rules as the messages pane. The view SHALL be a state of the app and not a mode, and SHALL NOT load more history when scrolled to either end.

#### Scenario: Starts on the result
- **WHEN** the view opens
- **THEN** the selected row is the result and it is visible

#### Scenario: Scroll to the ends
- **WHEN** the user presses `gg` and then `G`
- **THEN** the selection moves to the first and then the last message of the window and no request is sent

#### Scenario: Mode is unchanged
- **WHEN** the view opens from Normal mode
- **THEN** the mode badge still reads Normal

### Requirement: Read-only
While the context view is open, keys that act on messages (send, edit, delete, retry, open prompts) SHALL do nothing.

#### Scenario: Delete does nothing
- **WHEN** the view is open on a result that is the user's own message and the user presses `dd`
- **THEN** nothing is deleted and no prompt opens

### Requirement: Returning
`q`, `Esc` and `Ctrl-c` SHALL close the context view and show the results list again with the same result selected. The open room, the focus and the messages cursor SHALL be as before the view opened.

#### Scenario: Return to the list
- **WHEN** the view is open and the user presses `q`
- **THEN** the results list is shown with the result selected

#### Scenario: Leave the search
- **WHEN** the user presses `q` in the view and then `q` in the list
- **THEN** both are closed and the messages pane shows the room as before

### Requirement: Live messages while viewing
A message that arrives for the room while the view is open SHALL NOT be added to the view. It SHALL be part of the room's messages as usual and be there after the view and the list are closed.

#### Scenario: Message arrives
- **WHEN** a new message arrives for the room while the view is open
- **THEN** the view is unchanged and the message is in the room after the user closes the list

### Requirement: Context errors
A failed request SHALL show an error naming the failure on the status line and SHALL leave the results list as it was. A result that is no longer on the server (deleted since the search) SHALL show that the message was not found. An answer that arrives after the view was cancelled by closing the list, after another room was opened, or after another result was opened SHALL be ignored.

#### Scenario: Request fails
- **WHEN** the context request fails with a network error
- **THEN** the status line shows an error and the results list stays on screen

#### Scenario: Result was deleted
- **WHEN** the result no longer exists on the server
- **THEN** the status line says the message was not found and no view opens

#### Scenario: Late answer
- **WHEN** the user closes the list or opens another room before the answer arrives
- **THEN** the answer is dropped and nothing changes on screen

### Requirement: Context is private
The text, authors and timestamps of the window SHALL never be written to a log, shown in an error, formatted in `Debug` output or stored on disk.

#### Scenario: Failed request
- **WHEN** a context request fails
- **THEN** the error shown and any log line contain no part of a message

### Requirement: Jumping to the chat
The select action (`Enter` by default) in the context view SHALL close the context view and the results list and make the window's messages the room's loaded messages, with the messages pane focused and its cursor on the result. The room SHALL be detached from the newest messages (see `message-history`), and when newer messages were not shown the status line SHALL say that the view ends before the newest.

#### Scenario: Jump
- **WHEN** the context view of a result is open and the user presses `Enter`
- **THEN** the list and the view are closed, the room shows the window's messages and the cursor is on the result

#### Scenario: Newest not reached
- **WHEN** more than 100 messages exist after the result and the user jumps
- **THEN** the status line says the room ends before the newest messages and `G` loads them

#### Scenario: Jump keeps the result visible
- **WHEN** the user jumps
- **THEN** the result's row is inside the messages pane
