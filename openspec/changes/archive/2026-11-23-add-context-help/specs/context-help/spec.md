# Spec Delta

## Purpose

Tells the user which actions are available for the currently selected object, with the keys of their active keymap, without leaving the screen they are on.

## ADDED Requirements

### Requirement: Opening and closing the context popup
The app SHALL open a popup at the bottom of the screen with `?` in Normal mode. Any key SHALL close it, and that key SHALL NOT act on the room, the input or the keymap. Opening it SHALL NOT change the mode, the focused pane or the selection.

#### Scenario: Open
- **WHEN** the user presses `?` in Normal mode
- **THEN** the popup is shown and the mode badge still reads Normal

#### Scenario: Any key closes
- **WHEN** the popup is open and the user presses `j`
- **THEN** the popup closes and the cursor has not moved

#### Scenario: Full help from the popup
- **WHEN** the popup is open and the user presses `?`
- **THEN** the popup closes and the full help overlay opens

### Requirement: Popup follows the selected object
The popup SHALL name the selected object and list only the actions that apply to it. The object is the sidebar row under the cursor when the sidebar is focused, the message under the cursor when the messages pane is focused, and the input bar when it is focused.

#### Scenario: Object follows focus
- **WHEN** the sidebar is focused on a room and the user presses `?`, then closes the popup, focuses the input bar and presses `?`
- **THEN** the first popup names a room and the second names the input bar

### Requirement: Actions per object kind
A room SHALL list opening it. A folder SHALL list opening and closing it. A section header SHALL list no action beyond moving. One of the user's own plain-text server messages SHALL list edit and delete. A failed or unconfirmed message of the user's SHALL list retry and discard. Any other message SHALL list no action beyond moving. The input bar SHALL list entering Insert mode and moving in the draft.

#### Scenario: Own message
- **WHEN** the messages pane is focused on one of the user's own plain-text server messages and the user presses `?`
- **THEN** the popup names a message of the user's and lists edit and delete with their keys

#### Scenario: Failed message
- **WHEN** the cursor is on a failed message and the user presses `?`
- **THEN** the popup lists retry and discard, and not edit

#### Scenario: Someone else's message
- **WHEN** the cursor is on another user's message and the user presses `?`
- **THEN** the popup names a message and lists no message actions

#### Scenario: Folder
- **WHEN** the sidebar cursor is on a folder and the user presses `?`
- **THEN** the popup names a folder and lists opening and closing it

#### Scenario: Nothing selected
- **WHEN** no room is open and the messages pane is focused
- **THEN** the popup says there is nothing selected

### Requirement: Popup follows the live keymap
Each listed action SHALL show every key currently bound to it in Normal mode, in Vim notation. An action with no key bound SHALL be left out, and a `cmd:` binding SHALL NOT be listed as an action. The popup SHALL always end with how to reach the full help.

#### Scenario: Rebound key
- **WHEN** the user binds `"E" = "edit_message"` and opens the popup on their own message
- **THEN** the edit row shows `E`

#### Scenario: Unbound action
- **WHEN** the user sets `"e" = "none"` and opens the popup on their own message
- **THEN** no edit row is shown

### Requirement: Popup fits the screen
The popup SHALL be drawn over the bottom of the messages and input area, sized to its content up to a maximum height, and SHALL be usable at the smallest supported size. Incoming messages and connection changes SHALL keep updating the room underneath while it is open.

#### Scenario: Small terminal
- **WHEN** the terminal is at the smallest supported size and the popup is open
- **THEN** the popup is drawn inside the screen and shows at least the object name

#### Scenario: Message arrives while open
- **WHEN** the popup is open and a live message arrives
- **THEN** the message is added to its room and the popup stays open

### Requirement: Popup styling
Every element of the popup (border, title, key column, description) SHALL be themeable through `theme.toml` with built-in defaults, and SHALL show only text built into the app, never message content, author names or room names.

#### Scenario: Themed key column
- **WHEN** `theme.toml` sets `fg` on the popup key element
- **THEN** the keys in the popup are drawn in that color
