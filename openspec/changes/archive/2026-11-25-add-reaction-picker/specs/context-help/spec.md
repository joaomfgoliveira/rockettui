# Spec Delta

## ADDED Requirements

### Requirement: React action on messages
A message the user can react to SHALL list reacting in the context popup, with the keys currently bound to `react_message`, after the message's own actions and before the reactions and jump rows. An object the user cannot react to SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message
- **WHEN** the cursor is on a server message and the user presses `?`
- **THEN** the popup lists reacting with `+`

#### Scenario: Order
- **WHEN** the cursor is on a message with reactions and the user presses `?`
- **THEN** the react row comes before the row that shows the reactions

#### Scenario: Not reactable
- **WHEN** the cursor is on a deleted line, or the sidebar or the input bar is focused, and the user presses `?`
- **THEN** the popup has no row for reacting

#### Scenario: Rebound key
- **WHEN** the user binds `"R" = "react_message"` and opens the popup on a message
- **THEN** the row shows `R`

## MODIFIED Requirements

### Requirement: Jump and search actions on messages
Every message, whoever wrote it, SHALL list jumping to the oldest message, jumping to the newest message, searching the room, and jumping to the next and the previous match, each with its live keys and after the message's own actions. An action with no key bound SHALL be left out.

#### Scenario: Jump keys
- **WHEN** the cursor is on any message and the user presses `?`
- **THEN** the popup lists jumping to the oldest message with `gg` and to the newest with `G`

#### Scenario: Someone else's message
- **WHEN** the cursor is on another user's message and the user presses `?`
- **THEN** the popup lists reacting, the two jumps and the search actions as message actions

#### Scenario: Search keys follow the keymap
- **WHEN** the user binds `"<C-f>" = "search_start"` and opens the popup on a message
- **THEN** the search row shows `<C-f>` and `/` if still bound

#### Scenario: Search action unbound
- **WHEN** the user sets `"N" = "none"` and opens the popup on a message
- **THEN** no row for the previous match is shown

#### Scenario: Tall popup
- **WHEN** the cursor is on one of the user's own messages and the terminal is at least 80x24
- **THEN** every listed action is visible and none is clipped
