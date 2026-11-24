# message-search Specification

## Purpose

Lets the user find text in the messages of the open room from the keyboard, with Vim-style `/`, `n` and `N`, using only the messages already held in memory plus older history pages fetched on demand.

## Requirements

### Requirement: Search prompt
With the messages pane focused and a room open, the search start action SHALL open a prompt on the command row showing `/` and the typed text. Typing, Backspace, `Ctrl-u` and paste SHALL edit the query, and the query length SHALL be capped. The prompt SHALL NOT change the mode, the focus or the cursor, and SHALL take every key until it closes.

#### Scenario: Open the prompt
- **WHEN** the messages pane is focused on an open room and the user presses `/`
- **THEN** the command row shows `/` and the mode badge still reads Normal

#### Scenario: Typing is not a command
- **WHEN** the prompt is open and the user types `j`
- **THEN** `j` is added to the query and the message cursor does not move

#### Scenario: Not available elsewhere
- **WHEN** the sidebar or the input bar is focused, or no room is open, and the search start action runs
- **THEN** no prompt opens

### Requirement: Running and cancelling a search
`Enter` SHALL close the prompt and run the search for the typed query. `Esc` and `Ctrl-c` SHALL close it and change nothing. Backspace on an empty query SHALL close it. An empty query on `Enter` SHALL close the prompt and leave any earlier search as it was.

#### Scenario: Cancel
- **WHEN** the prompt is open with text and the user presses `Esc`
- **THEN** the prompt closes, the cursor has not moved and no search is active

#### Scenario: Empty query keeps the old search
- **WHEN** a search is active and the user opens the prompt and presses `Enter` with nothing typed
- **THEN** the prompt closes and the earlier search and its highlight remain

### Requirement: Matching rule
A search SHALL match its query as plain text, not a regular expression, against the text of the room's messages. It SHALL be case-insensitive unless the query contains an uppercase letter, in which case it SHALL be case-sensitive. Deleted lines and encrypted placeholders SHALL never match.

#### Scenario: Smart case, lowercase query
- **WHEN** the query is `hello` and a message says `Hello there`
- **THEN** that message matches

#### Scenario: Smart case, uppercase query
- **WHEN** the query is `Hello` and a message says `hello there`
- **THEN** that message does not match

#### Scenario: Plain text
- **WHEN** the query is `a.c` and a message says `abc`
- **THEN** that message does not match

#### Scenario: Deleted line
- **WHEN** a deleted line is the only place the query could appear
- **THEN** it does not match

### Requirement: First match
Running a search SHALL move the messages cursor to the nearest matching message at or older than the cursor, and show it in view. When none is held there, the search SHALL continue into older history as described for `n`.

#### Scenario: Nearest match
- **WHEN** the cursor is on the newest message and several older messages match
- **THEN** the cursor moves to the newest of the matching messages

#### Scenario: Cursor already on a match
- **WHEN** the cursor message itself matches
- **THEN** the cursor stays on it

#### Scenario: Nothing matches
- **WHEN** no loaded message matches and the room has no older history
- **THEN** the cursor does not move and the status line says nothing was found

### Requirement: Next and previous match
The search next action SHALL move the cursor to the next matching message toward older messages, and the search previous action toward newer ones. Neither SHALL wrap around. With no active search they SHALL do nothing and say so on the status line. A count before the key SHALL repeat the jump.

#### Scenario: Next toward older
- **WHEN** a search is active and the user presses `n`
- **THEN** the cursor moves to the nearest matching message older than the cursor message

#### Scenario: Previous toward newer
- **WHEN** a search is active, the cursor is on a match and the user presses `N`
- **THEN** the cursor moves to the nearest matching message newer than the cursor message

#### Scenario: No wrap
- **WHEN** the cursor is on the newest match and the user presses `N`
- **THEN** the cursor does not move and the status line says there is no newer match

#### Scenario: No active search
- **WHEN** no search is active and the user presses `n`
- **THEN** nothing moves and the status line says there is no search

### Requirement: Loading older history while searching
When the search next action, or a new search, finds no match among the loaded messages older than the cursor, the app SHALL fetch one older page, search it, and repeat until a match is found, the room has no more history, or the search is abandoned. Finding a match SHALL stop the fetching. Reaching the end without a match SHALL say so on the status line and leave the cursor where it was.

#### Scenario: Match in an older page
- **WHEN** `n` finds nothing older in memory and the next older page contains a match
- **THEN** that page is loaded and the cursor moves to the matching message

#### Scenario: Several pages
- **WHEN** the first older page has no match and the second does
- **THEN** both pages are loaded and the cursor ends on the match

#### Scenario: End of history
- **WHEN** older pages run out with no match
- **THEN** the status line says nothing more was found, no further page is requested and the cursor has not moved

#### Scenario: Failed page
- **WHEN** an older page fails to load during the search
- **THEN** the search stops and the status line reports the failure

#### Scenario: Abandoned
- **WHEN** the user opens another room or clears the search while a page is loading
- **THEN** no further page is requested for the search

### Requirement: Match highlighting
Every occurrence of the query in the messages shown SHALL be drawn with the search match style, including on the cursor row and in live messages that arrive after the search. A new search SHALL replace the old highlight. `Esc` in Normal mode SHALL clear the search and its highlight, and opening another room SHALL clear it too.

#### Scenario: Highlight all occurrences
- **WHEN** a search is active and two visible messages contain the query
- **THEN** the query text is drawn in the match style in both

#### Scenario: Live message
- **WHEN** a search is active and a live message containing the query arrives
- **THEN** its occurrence is drawn in the match style and the cursor does not move

#### Scenario: Clear
- **WHEN** a search is active and the user presses `Esc` in Normal mode
- **THEN** no text is drawn in the match style and `n` reports there is no search

#### Scenario: Room change
- **WHEN** a search is active and the user opens another room
- **THEN** the new room shows no highlight

### Requirement: Search content is never exposed
The query and the matched text SHALL NOT be written to logs or error messages, and SHALL NOT appear in the `Debug` output of any app type. The prompt and status lines SHALL show the query only on the screen.

#### Scenario: Debug output
- **WHEN** an action or effect carrying a query is formatted with `Debug`
- **THEN** the output does not contain the query text
