# Spec Delta

## Purpose

Lets the user search the whole history of the open room on the server with `:search <text>` and read the results in a list over the messages pane, without leaving the keyboard. It complements the in-memory find of `message-search`, which only sees loaded messages.

## ADDED Requirements

### Requirement: Search command
The command `:search <text>` SHALL ask the server to search the open room for `<text>`, sent exactly as typed after the command name with surrounding whitespace removed. It SHALL work with a room open, from any pane focus. It SHALL NOT change the mode, the focus or the messages cursor.

#### Scenario: Run a search
- **WHEN** a room is open and the user runs `:search deploy`
- **THEN** the server is asked to search that room for `deploy` and the results list opens

#### Scenario: Query keeps its inner spacing
- **WHEN** the user runs `:search  two   words `
- **THEN** the query sent is `two   words`

#### Scenario: Server syntax passes through
- **WHEN** the user runs `:search from:ana has:star`
- **THEN** the text `from:ana has:star` is sent unchanged, so the server applies its own query syntax

### Requirement: Refused searches
The app SHALL refuse the command, send no request and show an error on the status line when the query is empty, when no room is open, or when the open room is end-to-end encrypted. A refused search SHALL leave any results list that is already open as it was.

#### Scenario: Empty query
- **WHEN** the user runs `:search` with nothing after it
- **THEN** the status line says a search text is needed and no request is sent

#### Scenario: No room open
- **WHEN** no room is open and the user runs `:search hello`
- **THEN** the status line says a room must be open and no request is sent

#### Scenario: Encrypted room
- **WHEN** the open room is encrypted and the user runs `:search hello`
- **THEN** the status line says encrypted rooms cannot be searched and no request is sent

### Requirement: Results list
Results SHALL be shown in a read-only list over the messages pane, newest first, each result with its author, time and text drawn like a message in the pane. The list SHALL be a state of the app and not a mode: the mode badge SHALL keep its value, and the list SHALL take the motion keys and `q`, `Esc` and `Ctrl-c` while it is open. The text of the query SHALL be shown in the list's title.

#### Scenario: Newest first
- **WHEN** a search finds three messages from different days
- **THEN** the list shows the most recent one at the top and the oldest at the bottom

#### Scenario: Mode is unchanged
- **WHEN** the results list opens from Normal mode
- **THEN** the mode badge still reads Normal

#### Scenario: Title shows the query
- **WHEN** the user searched for `deploy`
- **THEN** the list title shows `deploy`

### Requirement: Navigating and closing results
While the list is open, the move down and up, go to top and bottom, and half-page actions SHALL move a selection in the list by the same rules as the messages pane. `q`, `Esc` and `Ctrl-c` SHALL close the list. Closing SHALL leave the open room, the focus and the messages cursor as they were. Keys that act on messages (send, edit, delete, retry, open prompts) SHALL do nothing while the list is open.

#### Scenario: Scroll
- **WHEN** the list holds many results and the user presses `j` then `G`
- **THEN** the selection moves one result down and then to the last loaded result

#### Scenario: Close
- **WHEN** the list is open and the user presses `Esc`
- **THEN** the list closes and the messages pane shows the room as before

#### Scenario: Read-only
- **WHEN** the list is open and the user presses `dd` on a result that is the user's own message
- **THEN** nothing is deleted and no prompt opens

### Requirement: Paging
The first request SHALL ask for one page of results. When the selection reaches the last loaded result and the server may hold more, the app SHALL request the next page and append it, with no request in flight twice. A page that returns fewer results than were asked for SHALL mark the end of the results, and no further request SHALL be made.

#### Scenario: Next page
- **WHEN** a full page is shown and the user moves the selection to its last result
- **THEN** the next page is requested and its results are appended after the last one

#### Scenario: End of results
- **WHEN** a page comes back with fewer results than a full page
- **THEN** no further page is requested, however far the user scrolls

#### Scenario: No duplicate request
- **WHEN** a next-page request is in flight and the user presses `j` on the last result again
- **THEN** no second request is sent

### Requirement: Search outcomes on the status line
A search that finds nothing SHALL show that nothing was found, and no list SHALL open. A failed request SHALL show an error naming the failure, and a room the user cannot read SHALL show that access was refused. A failed next-page request SHALL keep the results already shown. An answer that arrives after the list was closed, after another search started, or after another room was opened SHALL be ignored.

#### Scenario: No results
- **WHEN** the server returns no message for the query
- **THEN** the status line says nothing was found and the list does not open

#### Scenario: Request fails
- **WHEN** the request fails with a network error
- **THEN** the status line shows an error and no list opens

#### Scenario: Next page fails
- **WHEN** a next-page request fails
- **THEN** the results already shown stay and the status line shows an error

#### Scenario: Late answer
- **WHEN** the user closes the list or opens another room before an answer arrives
- **THEN** the answer is dropped and nothing changes on screen

### Requirement: Case rule
A server search SHALL be case-insensitive, and the app SHALL NOT apply the smart-case rule of `/`.

#### Scenario: Uppercase query
- **WHEN** the user runs `:search Deploy` and a message says `deploy now`
- **THEN** that message is in the results

### Requirement: Query and results are private
The query text and the results SHALL never be written to a log, shown in an error, formatted in `Debug` output or stored on disk.

#### Scenario: Failed request
- **WHEN** a search request fails
- **THEN** the error shown and any log line contain no part of the query or of a result text
