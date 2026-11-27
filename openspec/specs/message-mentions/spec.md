# message-mentions Specification

## Purpose

Highlights user, group and channel references in message text so they stand out while reading, and lets the user open a referenced channel from the keyboard.

## Requirements

### Requirement: References in message text are highlighted
Text in a message body that the server reports as a mention of a user or a reference to a channel SHALL be drawn in its own style: another user's `@name`, the user's own `@name`, `@here` or `@all`, and `#channel`. The words are shown exactly as written; only the style changes. System messages, deleted lines and text inside code spans SHALL be left unstyled.

#### Scenario: Another user
- **WHEN** a message says `thanks @bob` and the server lists `bob` in its mentions
- **THEN** `@bob` is drawn in the user-mention style

#### Scenario: Own name
- **WHEN** a message says `ping @me` and `me` is the user's own username
- **THEN** `@me` is drawn in the own-mention style, distinct from another user's

#### Scenario: Group mention
- **WHEN** a message says `heads up @here` or `@all`
- **THEN** that word is drawn in the group-mention style

#### Scenario: Channel
- **WHEN** a message says `see #dev-chat` and the server lists `dev-chat` in its channels
- **THEN** `#dev-chat` is drawn in the channel style

#### Scenario: Not a mention
- **WHEN** a message says `mail bob@example.com` and the server lists no mentions
- **THEN** no part of the text is highlighted

#### Scenario: Code span
- **WHEN** a mention appears inside a code span
- **THEN** it is drawn as plain code text

### Requirement: Highlighting without the server's lists
When a message carries no mentions or channels list, such as older history, `@name` and `#name` tokens SHALL be highlighted by their shape alone, and a token SHALL NOT be highlighted when it directly follows a letter or digit.

#### Scenario: Message without lists
- **WHEN** a message without the lists says `cc @carol`
- **THEN** `@carol` is drawn in the user-mention style

#### Scenario: Glued to a word
- **WHEN** such a message says `bob@example.com`
- **THEN** `@example.com` is not highlighted

### Requirement: Highlights wherever message text appears
The same highlighting SHALL apply to message text in the main flow, the thread pane, search results and the draft being typed in the input bar, and SHALL stay correct when a line wraps. A highlight SHALL NOT change what is sent or stored.

#### Scenario: In a thread
- **WHEN** a thread reply mentions `@bob`
- **THEN** `@bob` is highlighted in the thread pane

#### Scenario: In the draft
- **WHEN** the user types `hi @bob` in the input bar
- **THEN** `@bob` is highlighted and the text sent is `hi @bob`

#### Scenario: Wrapped mention
- **WHEN** a mention is split across two rows by wrapping
- **THEN** both parts are drawn in the mention style

### Requirement: Open a referenced channel
In Normal mode, the `open_channel` action on a message that references channels SHALL open the room named. With one channel it SHALL open that room directly. With several it SHALL show a picker of them, and choosing one opens that room. On a message with no channel reference it SHALL do nothing.

#### Scenario: One channel
- **WHEN** the cursor message references `#dev-chat`, which is in the sidebar, and the user presses `gc`
- **THEN** the `dev-chat` room opens

#### Scenario: Several channels
- **WHEN** the cursor message references `#dev-chat` and `#ops`
- **THEN** pressing `gc` shows a picker of both, and choosing `ops` opens that room

#### Scenario: No channel
- **WHEN** the cursor message references no channel and the user presses `gc`
- **THEN** nothing happens

### Requirement: Rooms the user is not in
When the referenced room is not one the user is subscribed to, `open_channel` SHALL show a notice and change nothing. The client SHALL NOT join a room on the user's behalf.

#### Scenario: Not subscribed
- **WHEN** the cursor message references `#secret-project`, which is not in the user's rooms, and the user presses `gc`
- **THEN** a notice says the room is not one of the user's rooms, and the open room does not change

### Requirement: Message text stays private
Mentions and channel names SHALL NOT be written to logs or to any `Debug` or `Display` output of a message.

#### Scenario: Debug output
- **WHEN** a message with mentions is formatted with `Debug`
- **THEN** no username or channel name from it appears in the output
