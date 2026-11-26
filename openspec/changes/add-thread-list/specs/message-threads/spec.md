# Spec Delta

## MODIFIED Requirements

### Requirement: Pane focus and closing
While the thread pane is open it SHALL be part of the `<Tab>` cycle, after the message pane, and `Ctrl-w l` from the message pane SHALL focus it. `Esc` in Normal mode with the pane focused, or the key bound to `close_thread`, SHALL close it and return focus to the message pane with the cursor on the message that was opened. When the thread was opened from the thread list (see `thread-list`), closing it SHALL show the thread list again, focused, with its filter, search text and selection as they were, instead of returning to the message pane. Switching to another room SHALL close it, and the thread list with it. Closing SHALL NOT discard anything the user has typed in the input bar.

#### Scenario: Tab cycle
- **WHEN** the pane is open and the user presses `<Tab>` from the message pane
- **THEN** focus moves to the pane, and `<Tab>` again goes to the sidebar

#### Scenario: Close
- **WHEN** the user presses `Esc` with the pane focused
- **THEN** the pane closes and focus returns to the message pane on the parent message

#### Scenario: Close a thread opened from the list
- **WHEN** the user opened the thread from the thread list and presses `Esc` with the pane focused
- **THEN** the thread list is shown again, focused, on the same filter, text and selection

#### Scenario: Room change
- **WHEN** a thread is open and the user opens another room
- **THEN** the pane is closed
