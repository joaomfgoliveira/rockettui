# Spec Delta

## Purpose

Lets the user list the threads of the open room, filter them by followed and unread state, search them, and open one in the thread pane, all from the keyboard.

## ADDED Requirements

### Requirement: Opening the thread list
The key bound to `list_threads` in Normal mode SHALL open an overlay listing the threads of the open room. With no room open, or in an encrypted room, it SHALL do nothing and the status bar SHALL say why. The list SHALL show a loading line until the first page arrives.

#### Scenario: Open
- **WHEN** a room is open and the user presses `gT`
- **THEN** the overlay opens and lists the room's threads once loaded

#### Scenario: No room
- **WHEN** no room is open and the user presses `gT`
- **THEN** no overlay opens and the status bar says no room is open

#### Scenario: Room with no threads
- **WHEN** the open room has no threads
- **THEN** the overlay says there are no threads

### Requirement: Rows
Each row SHALL show the parent message's author, the first line of its text shortened to the row width, the reply count and the time of the last reply, and `following` when the user follows the thread. Rows SHALL be ordered by last reply time, newest first, as the server returns them. Text from the server SHALL be sanitized like message text before drawing.

#### Scenario: Row content
- **WHEN** a thread by `ana` with 3 replies, last at 14:02, is listed
- **THEN** its row shows `ana`, the start of her text, `3 replies` and `14:02`

#### Scenario: Long text
- **WHEN** the parent's text is longer than the row
- **THEN** it is cut to fit and the counts stay visible

### Requirement: Filters
The list SHALL offer three filters, all threads, followed threads and unread threads, starting on all. `<Tab>` SHALL cycle to the next filter and `<S-Tab>` to the previous. Changing the filter SHALL reload the list from its start and show the active filter's name in the overlay title. The unread filter SHALL list the threads with replies the user has not read, as the server reports them.

#### Scenario: Followed only
- **WHEN** the user presses `<Tab>` once
- **THEN** the title shows the followed filter and only followed threads are listed

#### Scenario: Unread only
- **WHEN** the user selects the unread filter
- **THEN** only threads the server reports with unread replies are listed

### Requirement: Search
Typing in the overlay SHALL search the threads of the open room by text on the server, with a short delay after the last key before the request, and SHALL show the matching threads under the active filter. An empty search SHALL list the filter's threads. The search text SHALL NOT be logged or written into any error.

#### Scenario: Search
- **WHEN** the user types `deploy`
- **THEN** the list shows the threads that match `deploy`

#### Scenario: Clear
- **WHEN** the user deletes the text
- **THEN** the list returns to the filter's full list

### Requirement: Navigation and paging
`Down` and `Up` (and `<C-n>` and `<C-p>`) SHALL move the selection and `Esc` SHALL close the overlay. When the selection nears the end of the loaded rows and the server has more, the next page SHALL load without any command. A failed page SHALL show the failure class only and SHALL keep the rows already loaded.

#### Scenario: Paging
- **WHEN** the room has 120 threads and the user moves to the end of the first page
- **THEN** the next page is loaded and the selection continues

#### Scenario: Failed page
- **WHEN** a later page fails
- **THEN** the loaded rows stay and the overlay shows that loading more failed

### Requirement: Choosing a thread
`Enter` SHALL close the overlay and open the selected thread in the thread pane, focused, as `open_thread` does. This SHALL work when the parent message is not loaded in the main flow: the parent SHALL be fetched for the pane. The main flow SHALL NOT move. If the parent can no longer be fetched the status bar SHALL say the thread is gone and the overlay SHALL stay open.

#### Scenario: Choose
- **WHEN** the user selects a row and presses `Enter`
- **THEN** the overlay closes and the thread pane shows that thread

#### Scenario: Parent not loaded
- **WHEN** the chosen thread's parent is far older than the loaded history
- **THEN** the pane still opens and the main flow does not move

#### Scenario: Deleted parent
- **WHEN** the parent was deleted since the list loaded
- **THEN** the status bar says the thread is gone and the overlay stays open

### Requirement: Thread list privacy
The thread list SHALL NOT log message text or search text, and errors SHALL name the failure class only.

#### Scenario: Failed search
- **WHEN** a search request fails
- **THEN** no log line or message contains the search text
