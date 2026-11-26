# Spec Delta

## ADDED Requirements

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
