# context-help Specification

## Purpose

Tells the user which actions are available for the currently selected object, with the keys of their active keymap, without leaving the screen they are on.

## Requirements

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

### Requirement: Popup for search results
While the search results list or a result's context view is open, `?` SHALL open the context popup for it, whichever pane is focused. The popup SHALL name the list ("Search results") or the view ("Search result in context") and list the keys of its actions from the live keymap, plus the fixed `q Esc` row. The titles of the list and of the view SHALL hint at `Enter`, `?` and `q`.

#### Scenario: Popup over the list
- **WHEN** the results list is open and the user presses `?`
- **THEN** the popup names the search results and lists select, the motions and `q Esc`

#### Scenario: Popup over the context view
- **WHEN** a context view is open and the user presses `?`
- **THEN** the popup names the result in context and says select opens it in the chat

#### Scenario: Any key closes it
- **WHEN** the popup is open over the list and the user presses `j`
- **THEN** the popup closes and the selection has not moved

#### Scenario: Title hint
- **WHEN** the list or the view is open
- **THEN** its title mentions `Enter`, `?` and `q`

### Requirement: Reactions action on messages
A message that has reactions SHALL list showing its reactions in the context popup, with the keys currently bound to `show_reactions`, after the message's own actions. A message without reactions SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message with reactions
- **WHEN** the cursor is on a message with reactions and the user presses `?`
- **THEN** the popup lists showing reactions with `gr`

#### Scenario: Message without reactions
- **WHEN** the cursor is on a message with no reactions and the user presses `?`
- **THEN** the popup has no row for reactions

#### Scenario: Rebound key
- **WHEN** the user binds `"R" = "show_reactions"` and opens the popup on a message with reactions
- **THEN** the row shows `R`

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
