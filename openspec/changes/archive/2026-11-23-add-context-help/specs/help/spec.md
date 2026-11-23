# Spec Delta

## MODIFIED Requirements

### Requirement: Opening and closing help
The app SHALL open a help overlay over the messages pane with `:help`, with the key bound to `show_help` (none by default), or with `?` while the context popup is open (see `context-help`). `q`, `Esc`, `Ctrl-c` and the keys bound to `show_help` or `show_context` SHALL close it. Opening it SHALL NOT change the current mode.

#### Scenario: Open from Normal mode
- **WHEN** the user has bound `"H" = "show_help"` in `[normal]` and presses `H`
- **THEN** the help overlay is shown and the mode badge still reads Normal

#### Scenario: Open from the popup
- **WHEN** the user presses `?` in Normal mode and then `?` again
- **THEN** the help overlay is shown and the mode badge still reads Normal

#### Scenario: Open from the command line
- **WHEN** the user runs `:help`
- **THEN** the help overlay is shown

#### Scenario: Close
- **WHEN** the help overlay is open and the user presses `q`
- **THEN** the overlay closes and the focused pane and room are as before
