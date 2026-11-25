# room-switcher Specification

## Purpose

Lets the user jump to any joined room from the keyboard: a popup with a search line and a ranked list of rooms, ordered by recent use when the search is empty and by match quality when it is not.

## Requirements

### Requirement: Opening the switcher
`switch_room` SHALL open the room switcher from Normal mode on any pane. Opening it SHALL NOT change the mode, the focused pane or the open room. The search line SHALL start empty and typing SHALL edit it at once, with no extra key to start typing. It SHALL open even before any room is open. With no joined rooms it SHALL show an empty list.

#### Scenario: Open from the sidebar
- **WHEN** the sidebar is focused in Normal mode and the user presses `Ctrl-k`
- **THEN** the switcher opens, the mode badge still reads Normal and the sidebar cursor has not moved

#### Scenario: Open from the messages pane
- **WHEN** the messages pane is focused and the user presses `Ctrl-k`
- **THEN** the switcher opens and the message cursor has not moved

#### Scenario: Typing starts at once
- **WHEN** the switcher has just opened and the user types `g`
- **THEN** `g` is added to the search line and does not act as a Normal mode key

### Requirement: Rooms listed
The switcher SHALL list every joined room the sidebar can list, including rooms inside collapsed folders. It SHALL show each room's name, its kind marker the way the sidebar does, and its unread count and mention badge as the sidebar shows them. Room names are untrusted text and SHALL be sanitized for the terminal. The switcher SHALL NOT list rooms the user has not joined.

#### Scenario: Room in a collapsed folder
- **WHEN** a room sits in a collapsed folder and the switcher opens
- **THEN** the room is listed

#### Scenario: Unread room
- **WHEN** a listed room has 3 unread messages and 1 mention
- **THEN** its row shows `(3)` and `@1` as in the sidebar

#### Scenario: Closed subscription
- **WHEN** a subscription is closed and so is not in the sidebar
- **THEN** it is not in the switcher

### Requirement: Order with an empty search
With an empty search line the switcher SHALL list the rooms visited most recently first, the open room included, with unread rooms before read ones. Within the unread rooms and within the read rooms the order SHALL be most recently visited first. Rooms never visited in this session SHALL follow, ordered by most recent activity, then by name. The first row SHALL be selected when the switcher opens.

#### Scenario: Back to the previous room
- **WHEN** the user opened room A, then room B, and opens the switcher with nothing unread
- **THEN** B is first and A second, so `Ctrl-k` then `Down` then `Enter` returns to A

#### Scenario: Unread boosted
- **WHEN** room C was never visited, has unread messages, and the visited rooms are all read
- **THEN** C is listed before the visited rooms

#### Scenario: Never visited
- **WHEN** no room has been visited yet
- **THEN** rooms are ordered by most recent activity, ties by name

### Requirement: Matching
With a non-empty search line the switcher SHALL list only the rooms whose name contains every character of the query in order (a subsequence match), ignoring case. Rooms SHALL be ranked by match quality: a prefix of the name first, then a match at a word start, then a contiguous run, then scattered characters, ties broken by the empty-search order. The search SHALL run on the client over the rooms it already holds and SHALL NOT make a server request.

#### Scenario: Subsequence
- **WHEN** the user types `gnrl` and a room is named `general`
- **THEN** `general` is listed

#### Scenario: Prefix first
- **WHEN** the user types `dev` and the rooms are `dev-ops` and `team-dev`
- **THEN** `dev-ops` is listed before `team-dev`

#### Scenario: No match
- **WHEN** no room name contains the query as a subsequence
- **THEN** the list is empty and `Enter` does nothing

#### Scenario: Case ignored
- **WHEN** the user types `GEN`
- **THEN** `general` is listed

### Requirement: Navigation and editing
While the switcher is open it SHALL take every key. `Up`, `Ctrl-k` and `Ctrl-p` SHALL move the selection up and `Down`, `Ctrl-j` and `Ctrl-n` SHALL move it down, stopping at the ends. `Backspace` SHALL delete the last character of the query and `Ctrl-u` SHALL clear it. Any edit to the query SHALL select the first row. Pasted text SHALL be added to the query with line breaks removed.

#### Scenario: Move selection
- **WHEN** the user presses `Ctrl-n` twice
- **THEN** the third row is selected

#### Scenario: Vim-style keys
- **WHEN** the user presses `Ctrl-j` twice and then `Ctrl-k` once
- **THEN** the second row is selected

#### Scenario: Stop at the end
- **WHEN** the last row is selected and the user presses `Down`
- **THEN** the selection stays on the last row

#### Scenario: Edit resets selection
- **WHEN** the third row is selected and the user types a character
- **THEN** the first row of the new list is selected

### Requirement: Choosing a room
`Enter` SHALL open the selected room exactly as `Enter` on a sidebar room does, close the switcher and focus the messages pane. Choosing the room that is already open SHALL close the switcher and keep that room as it is. A room opened this way SHALL become the most recently visited.

#### Scenario: Open another room
- **WHEN** the user selects another room and presses `Enter`
- **THEN** that room opens, the messages pane is focused and the switcher is closed

#### Scenario: Open room chosen
- **WHEN** the user presses `Enter` on the room that is already open
- **THEN** the switcher closes and the room's messages and cursor are unchanged

#### Scenario: Draft kept
- **WHEN** the user has an unsent draft in the open room and switches to another room
- **THEN** the draft is kept for the first room, as with any room switch

### Requirement: Closing without choosing
`Esc` and `Ctrl-c` SHALL close the switcher and change nothing else: the open room, the focus, the draft and the recent list stay as they were. `Ctrl-c` SHALL NOT quit.

#### Scenario: Cancel
- **WHEN** the user types a query and presses `Esc`
- **THEN** the switcher closes and the open room is unchanged

#### Scenario: Ctrl-c does not quit
- **WHEN** the switcher is open and the user presses `Ctrl-c`
- **THEN** the switcher closes and the app keeps running

### Requirement: Stale rooms
If the rooms reload or change while the switcher is open, the list SHALL be rebuilt from the current rooms and the selection kept on the same room when it still exists, else on the first row. A room that disappears SHALL never be opened.

#### Scenario: Room leaves while open
- **WHEN** the selected room is removed by a live update
- **THEN** the list is rebuilt and `Enter` opens the new selection, never the removed room

### Requirement: Privacy
The switcher SHALL NOT log or format the search query or room names in diagnostics. The recently visited list SHALL stay in memory and SHALL NOT be written to disk.

#### Scenario: Restart
- **WHEN** the app restarts
- **THEN** no recently visited order is restored
