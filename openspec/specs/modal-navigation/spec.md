# modal-navigation Specification

## Purpose

Defines the modal, Vim-style interaction model and the three-pane layout of rockettui, so that every action is reachable from the keyboard.

## Requirements

### Requirement: Three-pane layout
The app SHALL show a sidebar of rooms, a messages pane and an input bar, with a status bar showing the current mode, the current room and any pending keys, and a command-line row below it.

#### Scenario: Launch
- **WHEN** the app starts
- **THEN** the sidebar, messages pane, input bar and status bar are visible

### Requirement: Modes
The app SHALL provide Normal, Insert and Command modes and show the current mode. `i`, `a` and `o` enter Insert mode on the input bar from any pane, `:` enters Command mode, and `Esc` or `Ctrl-c` returns to Normal mode without quitting.

#### Scenario: Leave Insert mode
- **WHEN** the user presses `Esc` in Insert mode
- **THEN** the app returns to Normal mode and to the previously focused pane

#### Scenario: Ctrl-c does not quit
- **WHEN** the user presses `Ctrl-c`
- **THEN** the current mode is cancelled and the app keeps running

### Requirement: Motions
In Normal mode, `j`/`k` SHALL move the cursor, `gg`/`G` SHALL jump to the first or last item, and `Ctrl-d`/`Ctrl-u` SHALL move half a page. A numeric prefix SHALL repeat the motion.

#### Scenario: Counted motion
- **WHEN** the user types `5j` in the sidebar
- **THEN** the cursor moves down five rows

#### Scenario: Horizontal keys in lists
- **WHEN** the user presses `h` or `l` on a message or on a sidebar room row
- **THEN** nothing happens, while on a folder row `h`/`l` fold and unfold it

#### Scenario: Motions in the input bar
- **WHEN** the input bar is focused in Normal mode
- **THEN** the motions act on the input buffer (`h`/`l` move the cursor, `j`/`k` move between lines, `gg`/`G` jump to first or last line)

### Requirement: Pane focus
`Ctrl-w` followed by `h`, `j` or `l` SHALL move focus between panes. `Enter` on a sidebar room SHALL open it and move focus to the messages pane.

#### Scenario: Open room from sidebar
- **WHEN** the user presses `Enter` on a room in the sidebar
- **THEN** the room opens and the messages pane is focused

### Requirement: Multi-key sequences
Multi-key bindings SHALL be matched with a timeout. Pending keys SHALL be shown in the status bar. After the timeout an ambiguous prefix fires or is cancelled.

#### Scenario: Pending keys
- **WHEN** the user types `g` and waits
- **THEN** `g` is shown as pending, and after the timeout the pending sequence is cleared

### Requirement: Command mode
Command mode SHALL dispatch from a table of commands. `:q` and `:quit` SHALL quit (with undelivered messages the first one warns, see `message-sending`; `:q!` and `:quit!` always quit), `:reconnect` and `:retry` SHALL reload rooms, `:reload` SHALL reload the open room's newest messages (see `message-history`), `:help` SHALL open the help overlay (see `help`), `:search <text>` SHALL search the open room on the server (see `room-search-results`), and an unknown command SHALL show an error in the status bar without quitting.

#### Scenario: Quit
- **WHEN** the user runs `:q` with nothing undelivered
- **THEN** the app exits

#### Scenario: Unknown command
- **WHEN** the user runs `:nope`
- **THEN** the status bar shows an error and the app keeps running

#### Scenario: Help command
- **WHEN** the user runs `:help`
- **THEN** the help overlay opens and the app returns to Normal mode

#### Scenario: Search command
- **WHEN** the user runs `:search deploy` with a room open
- **THEN** the app returns to Normal mode and the results of the server search are shown

#### Scenario: Reload command
- **WHEN** the user runs `:reload` with a room open
- **THEN** the app returns to Normal mode and the room's newest messages are requested

### Requirement: Multiline input
The input bar SHALL grow with its content up to a maximum height, support the usual editing keys, and treat pasted text safely.

#### Scenario: Multiline draft
- **WHEN** the user inserts a newline in Insert mode
- **THEN** the input bar grows to show both lines

### Requirement: Editing state of the input
While a message is being edited the app SHALL stay in Insert mode on the input bar, with the input title saying that Enter saves and Esc cancels, and SHALL leave that state when the edit is saved, cancelled or the user switches room. Leaving it SHALL restore the user's previous draft for the room.

#### Scenario: Switch room while editing
- **WHEN** the user opens another room while editing
- **THEN** the edit is cancelled, and the room the user left keeps the draft it had before the edit started

### Requirement: Confirmation prompt
A confirmation prompt SHALL be shown in the status bar and SHALL consume exactly one key: `y` or Enter confirms and any other key cancels without any other effect. While it is open, no other command SHALL run, and the prompt SHALL NOT change the current mode.

#### Scenario: Other key cancels
- **WHEN** a confirmation prompt is open and the user presses `j`
- **THEN** the prompt closes, the cursor does not move and nothing is confirmed

#### Scenario: Repeated delete key
- **WHEN** the user presses `3dd` on their own message
- **THEN** one prompt is opened, not three

### Requirement: Room switcher overlay
The room switcher (see `room-switcher`) SHALL be an overlay, not a mode. While it is open the mode badge SHALL keep showing the real mode, and it SHALL take every key before the keymap is consulted. Closing it SHALL leave the mode and the focused pane as they were.

#### Scenario: Badge unchanged
- **WHEN** the switcher is open in Normal mode
- **THEN** the mode badge reads NORMAL

#### Scenario: Keys go to the switcher
- **WHEN** the switcher is open and the user types `j`
- **THEN** `j` is added to the search line and no pane moves

### Requirement: Thread list pane
The thread list (see `thread-list`) SHALL be a pane in the thread column, not a mode and not an overlay. While it is focused the mode badge SHALL keep showing the real mode. It SHALL be part of the `<Tab>` cycle after the message pane while it is shown, `Ctrl-w l` from the message pane SHALL focus it, and the focus keys SHALL leave it. Its keys SHALL be resolved through the keymap like those of any pane, except `Esc` and `q` (close), which it handles first; `move_left` and `move_right` select the filter there. Its search line SHALL be a state of the pane, not a mode: while it is open it SHALL take typed text, and the mode badge SHALL NOT change.

#### Scenario: Badge unchanged
- **WHEN** the thread list is focused in Normal mode
- **THEN** the mode badge reads NORMAL

#### Scenario: Keys go through the keymap
- **WHEN** the thread list is focused and the user presses `j`
- **THEN** the selection moves, as the keymap binds `j`

#### Scenario: Typing in the search line
- **WHEN** the search line is open and the user types `j`
- **THEN** `j` is added to the text and nothing moves

#### Scenario: Leave without closing
- **WHEN** the user presses `Ctrl-w h` with the list focused
- **THEN** focus moves to the message pane and the list stays shown

#### Scenario: Tab from the messages
- **WHEN** the list is shown and the user presses `<Tab>` from the message pane
- **THEN** the list is focused
