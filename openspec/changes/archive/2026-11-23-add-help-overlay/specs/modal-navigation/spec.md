# Spec Delta

## MODIFIED Requirements

### Requirement: Command mode
Command mode SHALL dispatch from a table of commands. `:q` and `:quit` SHALL quit (with undelivered messages the first one warns, see `message-sending`; `:q!` and `:quit!` always quit), `:reconnect` and `:retry` SHALL reload rooms, `:help` SHALL open the help overlay (see `help`), and an unknown command SHALL show an error in the status bar without quitting.

#### Scenario: Quit
- **WHEN** the user runs `:q` with nothing undelivered
- **THEN** the app exits

#### Scenario: Unknown command
- **WHEN** the user runs `:nope`
- **THEN** the status bar shows an error and the app keeps running

#### Scenario: Help command
- **WHEN** the user runs `:help`
- **THEN** the help overlay opens and the app returns to Normal mode
