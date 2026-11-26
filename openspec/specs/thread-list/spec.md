# thread-list Specification

## Purpose

Lets the user list the threads of the open room in a side column, filter them by followed and unread state, search them, and open one in the thread pane, all from the keyboard.

## Requirements

### Requirement: Opening the thread list
The key bound to `list_threads` in Normal mode SHALL open the thread list of the open room in the thread column and focus it. The column is the thread pane's place: beside the message pane when the terminal is wide enough for both, in place of it when it is not, and closing the list SHALL bring the message pane back unchanged. With no room open, or in an encrypted room, it SHALL do nothing and the status bar SHALL say why. The list SHALL show a loading line until the first page arrives. When a thread is open in the column, the list SHALL take its place.

#### Scenario: Open
- **WHEN** a room is open and the user presses `gT`
- **THEN** the column opens focused and lists the room's threads once loaded, with the messages still visible beside it on a wide terminal

#### Scenario: Narrow terminal
- **WHEN** the terminal is too narrow to show both and the user presses `gT`
- **THEN** the list takes the place of the message pane, and closing it restores the message pane with its cursor and scroll

#### Scenario: No room
- **WHEN** no room is open and the user presses `gT`
- **THEN** nothing opens and the status bar says no room is open

#### Scenario: Room with no threads
- **WHEN** the open room has no threads
- **THEN** the list says there are no threads

#### Scenario: Over an open thread
- **WHEN** a thread is open in the column and the user presses `gT`
- **THEN** the list replaces it

### Requirement: Rows
Each row SHALL be three lines under a thin divider between rows: the parent message's author with the date and hour of the last reply at the right, the first line of the parent's text shortened to the row width, and `following` (when the user follows the thread) at the left with the reply count at the right. Rows SHALL be ordered by last reply time, newest first, as the server returns them. The selected row SHALL be highlighted like the cursor message of a message pane. Text from the server SHALL be sanitized like message text before drawing.

#### Scenario: Row content
- **WHEN** a thread by `ana` with 3 replies, last at 2026-11-14 14:02, is listed
- **THEN** its row shows `ana` and `2026-11-14 14:02`, the start of her text, and `3 replies`

#### Scenario: Followed
- **WHEN** the user follows the thread
- **THEN** `following` is on the row's last line, left of the reply count

#### Scenario: Long text
- **WHEN** the parent's text is longer than the row
- **THEN** it is cut to fit and the date and the reply count stay visible

### Requirement: Filters
The list SHALL offer three filters, all threads, followed threads and unread threads, starting on all. While the list is focused, the keys bound to `move_right` (`l`) and `move_left` (`h`) SHALL select the next and the previous filter, wrapping around; `<Tab>` is left to the pane cycle. Changing the filter SHALL reload the list from its start and show the active filter's name in the pane title. The unread filter SHALL list the threads with replies the user has not read, as the server reports them.

#### Scenario: Followed only
- **WHEN** the user presses `l` once
- **THEN** the title shows the followed filter and only followed threads are listed

#### Scenario: Unread only
- **WHEN** the user selects the unread filter
- **THEN** only threads the server reports with unread replies are listed

### Requirement: Search
`/` with the list focused SHALL open a search line in the pane. Typed text SHALL search the threads of the open room by text on the server, with a short delay after the last key before the request, and SHALL show the matching threads under the active filter. `Enter` SHALL keep the text and return to the list keys; `Esc` SHALL clear the text, list the filter's threads again and return to the list keys. An empty search SHALL list the filter's threads. The search text SHALL NOT be logged or written into any error.

#### Scenario: Search
- **WHEN** the user presses `/` and types `deploy`
- **THEN** the list shows the threads that match `deploy`

#### Scenario: Keep
- **WHEN** the user presses `Enter` in the search line
- **THEN** the text stays, the matching threads stay listed and `j` moves the selection

#### Scenario: Clear
- **WHEN** the user presses `Esc` in the search line
- **THEN** the text is cleared and the list returns to the filter's full list

#### Scenario: Typing is text
- **WHEN** the search line is open and the user types `j`
- **THEN** `j` is added to the text and the selection does not move

### Requirement: Navigation and paging
With the list focused, the motion keys (`j` and `k` or `Down` and `Up`, `gg` and `G`, the half-page keys) SHALL move the selection as they are bound in the keymap. `Esc` or `q` SHALL close the list and return focus to the pane it was opened from. When the selection nears the end of the loaded rows and the server has more, the next page SHALL load without any command. A failed page SHALL show the failure class only and SHALL keep the rows already loaded.

#### Scenario: Move
- **WHEN** the user presses `j`
- **THEN** the selection moves down one row

#### Scenario: Close
- **WHEN** the user presses `Esc` with the list focused and no search line open
- **THEN** the column closes and focus is where it was before `gT`

#### Scenario: Paging
- **WHEN** the room has 120 threads and the user moves to the end of the first page
- **THEN** the next page is loaded and the selection continues

#### Scenario: Failed page
- **WHEN** a later page fails
- **THEN** the loaded rows stay and the list shows that loading more failed

### Requirement: Choosing a thread
`Enter` SHALL replace the list with the selected thread in the same column, focused, as `open_thread` does. This SHALL work when the parent message is not loaded in the main flow: the parent SHALL be fetched for the pane. The main flow SHALL NOT move. If the parent can no longer be fetched the status bar SHALL say the thread is gone and the list SHALL stay shown. Closing the thread (see `message-threads`) SHALL return to the list with its filter, search text, loaded rows and selection as they were.

#### Scenario: Choose
- **WHEN** the user selects a row and presses `Enter`
- **THEN** the column shows that thread, focused

#### Scenario: Parent not loaded
- **WHEN** the chosen thread's parent is far older than the loaded history
- **THEN** the thread still opens and the main flow does not move

#### Scenario: Deleted parent
- **WHEN** the parent was deleted since the list loaded
- **THEN** the status bar says the thread is gone and the list stays shown

#### Scenario: Back to the list
- **WHEN** the user presses `Esc` in the thread that was opened from the list
- **THEN** the list is shown again on the same filter, text and selection

### Requirement: Thread list privacy
The thread list SHALL NOT log message text or search text, and errors SHALL name the failure class only.

#### Scenario: Failed search
- **WHEN** a search request fails
- **THEN** no log line or message contains the search text
