# Spec Delta

## ADDED Requirements

### Requirement: Thread list overlay
The thread list (see `thread-list`) SHALL be an overlay, not a mode. While it is open the mode badge SHALL keep showing the real mode, and it SHALL take every key before the keymap is consulted. Closing it SHALL leave the mode and the focused pane as they were, unless a thread was chosen, which focuses the thread pane.

#### Scenario: Badge unchanged
- **WHEN** the thread list is open in Normal mode
- **THEN** the mode badge reads NORMAL

#### Scenario: Keys go to the overlay
- **WHEN** the thread list is open and the user types `j`
- **THEN** `j` is added to the search line and no pane moves

#### Scenario: Close without choosing
- **WHEN** the user presses `Esc`
- **THEN** the overlay closes and the focus is where it was
