# Spec Delta

## MODIFIED Requirements

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
