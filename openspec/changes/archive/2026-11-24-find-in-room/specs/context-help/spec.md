# Spec Delta

## MODIFIED Requirements

### Requirement: Actions per object kind
A room SHALL list opening it. A folder SHALL list opening and closing it. A section header SHALL list no action beyond moving. One of the user's own plain-text server messages SHALL list edit and delete. A failed or unconfirmed message of the user's SHALL list retry and discard. The input bar SHALL list entering Insert mode and moving in the draft. Messages also list the actions of the next requirement.

#### Scenario: Own message
- **WHEN** the messages pane is focused on one of the user's own plain-text server messages and the user presses `?`
- **THEN** the popup names a message of the user's and lists edit and delete with their keys, followed by the jumps and search actions

#### Scenario: Failed message
- **WHEN** the cursor is on a failed message and the user presses `?`
- **THEN** the popup lists retry and discard, and not edit

#### Scenario: Someone else's message
- **WHEN** the cursor is on another user's message and the user presses `?`
- **THEN** the popup names a message and lists no edit, delete, retry or discard

#### Scenario: Folder
- **WHEN** the sidebar cursor is on a folder and the user presses `?`
- **THEN** the popup names a folder and lists opening and closing it

#### Scenario: Nothing selected
- **WHEN** no room is open and the messages pane is focused
- **THEN** the popup says there is nothing selected

## ADDED Requirements

### Requirement: Jump and search actions on messages
Every message, whoever wrote it, SHALL list jumping to the oldest message, jumping to the newest message, searching the room, and jumping to the next and the previous match, each with its live keys and after the message's own actions. An action with no key bound SHALL be left out.

#### Scenario: Jump keys
- **WHEN** the cursor is on any message and the user presses `?`
- **THEN** the popup lists jumping to the oldest message with `gg` and to the newest with `G`

#### Scenario: Someone else's message
- **WHEN** the cursor is on another user's message and the user presses `?`
- **THEN** the popup lists only the two jumps and the search actions as message actions

#### Scenario: Search keys follow the keymap
- **WHEN** the user binds `"<C-f>" = "search_start"` and opens the popup on a message
- **THEN** the search row shows `<C-f>` and `/` if still bound

#### Scenario: Search action unbound
- **WHEN** the user sets `"N" = "none"` and opens the popup on a message
- **THEN** no row for the previous match is shown

#### Scenario: Tall popup
- **WHEN** the cursor is on one of the user's own messages and the terminal is at least 80x24
- **THEN** every listed action is visible and none is clipped
