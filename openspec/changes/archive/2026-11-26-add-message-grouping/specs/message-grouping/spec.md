# Spec Delta

## Purpose

Draws consecutive messages from one author under a single header in the room and the thread pane, so a burst of short messages reads as one block, as in Rocket.Chat's own UI.

## ADDED Requirements

### Requirement: Grouping consecutive messages
In the two-line layout, a message SHALL continue the message before it, and so be drawn without its own header row, when both have the same author, are of the same thread (or both of none), were written on the same local day and the message is no more than `message_grouping_seconds` after the first message of the group. The gap is measured from the first message of the group, not from the message directly before it, so a group never spans more than the gap. A group's header SHALL show the author and the time of its first message; each body SHALL stay indented as under any header.

#### Scenario: Burst under one header
- **WHEN** `joliveira` writes three messages one minute apart
- **THEN** one header with the author and the first message's time is drawn, with the three bodies under it

#### Scenario: Gap
- **WHEN** `joliveira` writes a message 20 minutes after their previous one and the setting is 180
- **THEN** it has its own header with its own time

#### Scenario: Long burst splits
- **WHEN** one author writes a message every 2 minutes for 10 minutes and the setting is 180
- **THEN** the messages at 0 and 2 minutes share the first header, the one at 4 minutes starts a new group (more than 180 s after the first), and so on every second message

#### Scenario: Another author
- **WHEN** a message follows one from a different author
- **THEN** it has its own header, and the author's next message after that has a header too

#### Scenario: Day change
- **WHEN** a message follows one from the same author, within the gap, but on the next local day
- **THEN** a day divider and a header are drawn

### Requirement: What breaks a group
A message SHALL always have its own header when the message before it is a system message, a deleted message or one not yet confirmed by the server, when it is itself one of those, when the unread divider or a day divider is drawn above it, or when the message before it has a thread marker row. Grouping SHALL NOT change which message the cursor is on or what any key acts on.

#### Scenario: Unsent message
- **WHEN** a message is queued, sending or failed
- **THEN** it has its own header, which carries its send-state marker where the time would be

#### Scenario: After a system message
- **WHEN** a message follows a system message from the same author within the gap
- **THEN** it has its own header

#### Scenario: Unread divider
- **WHEN** the unread divider is drawn above a message from the author of the one before it
- **THEN** the message has its own header below the divider

#### Scenario: Thread marker
- **WHEN** the message before has replies, so a thread marker row is drawn under it
- **THEN** the next message has its own header

#### Scenario: Cursor
- **WHEN** the user moves the cursor over a group
- **THEN** it goes from message to message, and the highlight covers only that message's rows

### Requirement: Where grouping applies
Grouping SHALL apply to the room's message pane and to the thread pane, in the two-line layout (the thread pane always uses it). It SHALL NOT apply to the compact layout, to the search results list or to the search-result context view. Setting `message_grouping_seconds` to `0` SHALL turn it off everywhere.

#### Scenario: Compact layout
- **WHEN** `message_layout` is `compact`
- **THEN** every message keeps its own time and author, whatever the setting

#### Scenario: Thread pane
- **WHEN** a thread's replies by one author are close together
- **THEN** the thread pane groups them like the room does

#### Scenario: Search results
- **WHEN** `:search` lists two neighbouring results by one author
- **THEN** each result keeps its own header

#### Scenario: Off
- **WHEN** `message_grouping_seconds = 0`
- **THEN** every message has its own header
