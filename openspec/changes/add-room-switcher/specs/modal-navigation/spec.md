# Spec Delta

## ADDED Requirements

### Requirement: Room switcher overlay
The room switcher (see `room-switcher`) SHALL be an overlay, not a mode. While it is open the mode badge SHALL keep showing the real mode, and it SHALL take every key before the keymap is consulted. Closing it SHALL leave the mode and the focused pane as they were.

#### Scenario: Badge unchanged
- **WHEN** the switcher is open in Normal mode
- **THEN** the mode badge reads NORMAL

#### Scenario: Keys go to the switcher
- **WHEN** the switcher is open and the user types `j`
- **THEN** `j` is added to the search line and no pane moves
