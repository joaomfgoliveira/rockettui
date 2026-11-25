# message-threads Specification

## Purpose

Shows which messages are the start of a thread, keeps thread replies out of the main message flow, and lets the user open a thread in a pane, read it, and see new replies arrive, all from the keyboard.

## Requirements

### Requirement: Thread marker row
A message that has thread replies SHALL be drawn with one extra row under its body and its reaction row, showing the reply count and the time of the last reply, for example `-> 3 replies, last 14:02`. A message without replies SHALL be drawn exactly as before. The row SHALL be indented to the body, highlighted with its message, and plain ASCII.

#### Scenario: Parent with replies
- **WHEN** a message has 3 replies and the last one was posted at 14:02
- **THEN** its marker row reads `-> 3 replies, last 14:02`

#### Scenario: Single reply
- **WHEN** a message has exactly one reply
- **THEN** its marker row says `1 reply`

#### Scenario: No replies
- **WHEN** a message has no replies
- **THEN** it has no marker row

#### Scenario: Both layouts
- **WHEN** `message_layout` is `compact` or `two_line`
- **THEN** the marker row is drawn under the body in both, aligned with the body

### Requirement: Replies stay out of the main flow
A thread reply SHALL NOT be shown in the room's main message flow, whether it comes from history, from search context or live, unless the server marks it as also shown in the channel. A reply that is also shown in the channel SHALL be drawn in the flow with the tag `[thread] ` at the start of its body, the tag and the body text in italics, in both layouts and without an extra row. Hiding a reply SHALL NOT remove it from the thread it belongs to.

#### Scenario: Reply in fetched history
- **WHEN** a fetched page of history contains a thread reply that is not shown in the channel
- **THEN** the main flow does not show it

#### Scenario: Reply also shown in the channel
- **WHEN** a thread reply is marked as also shown in the channel
- **THEN** the main flow shows it with a `[thread] ` tag at the start of its body, in italics

#### Scenario: Tag in both layouts
- **WHEN** a channel-visible reply is drawn in the two-line layout, then in the compact layout
- **THEN** in both the tag starts the body, the text follows it in italics, and wrapped rows line up under the text

#### Scenario: Reply does not move the view
- **WHEN** the user is reading the newest messages and a hidden thread reply arrives live
- **THEN** the main flow does not change and does not scroll

### Requirement: Opening a thread
The key bound to `open_thread` in Normal mode, with the message pane focused, SHALL open the thread of the message under the cursor in the thread pane and focus it. On a reply shown in the channel it SHALL open the parent's thread. On a message with no replies it SHALL do nothing and the status bar SHALL say there is no thread. Opening a thread while another is open SHALL replace it.

#### Scenario: Open from a parent
- **WHEN** the cursor is on a message with 3 replies and the user presses `gt`
- **THEN** the thread pane opens showing the message and its replies, and focus is in the pane

#### Scenario: Open from a channel-visible reply
- **WHEN** the cursor is on a reply that is also shown in the channel and the user presses `gt`
- **THEN** the thread of the reply's parent opens

#### Scenario: No thread
- **WHEN** the cursor is on a message with no replies and the user presses `gt`
- **THEN** no pane opens and the status bar says there is no thread

#### Scenario: Replace
- **WHEN** a thread is open and the user opens another from the message pane
- **THEN** the pane shows the new thread

### Requirement: Thread pane content
The thread pane SHALL show the parent message first, then its replies oldest first, drawn as in the main flow. It SHALL have its own message cursor and scrolling, moved by the usual motions while it is focused. While loading it SHALL show a loading line, and on failure the failure class only, never message text. A thread longer than one page SHALL load further replies as the cursor reaches the edge.

#### Scenario: Order
- **WHEN** a thread with a parent and 3 replies is opened
- **THEN** the pane lists the parent, then the replies from oldest to newest

#### Scenario: Own cursor
- **WHEN** the user presses `j` with the pane focused
- **THEN** the pane's cursor moves and the main flow's cursor does not

#### Scenario: Load failure
- **WHEN** the server rejects the thread request
- **THEN** the pane shows that loading failed, with the failure class and no message text, and the rest of the app keeps working

#### Scenario: Long thread
- **WHEN** a thread has more replies than one page
- **THEN** the further replies are loaded as the user moves through the pane

### Requirement: Live replies in the open thread
A reply to the thread that is open SHALL be appended to the pane, in order, without any command, and a reply already held by id SHALL NOT be duplicated. When the cursor is on the newest reply the pane SHALL follow it as the main flow does. The parent's reply count and last-reply time in the main flow SHALL be updated by a live reply, whether or not the thread is open. An edit, a delete or a reaction on a message of the open thread SHALL be applied in the pane as it is in the main flow.

#### Scenario: Reply to the open thread
- **WHEN** another user replies to the thread that is open
- **THEN** the reply appears at the bottom of the pane

#### Scenario: Reply to a thread that is not open
- **WHEN** another user replies to a thread that is not open
- **THEN** the pane does not change and the parent's marker row in the main flow shows the new count

#### Scenario: Duplicate delivery
- **WHEN** the same reply arrives twice
- **THEN** the pane holds it once

#### Scenario: Edit in the thread
- **WHEN** another user edits a reply in the open thread
- **THEN** the pane shows the new text with the edited mark

### Requirement: Pane focus and closing
While the thread pane is open it SHALL be part of the `<Tab>` cycle, after the message pane, and `Ctrl-w l` from the message pane SHALL focus it. `Esc` in Normal mode with the pane focused, or the key bound to `close_thread`, SHALL close it and return focus to the message pane with the cursor on the message that was opened. Switching to another room SHALL close it. Closing SHALL NOT discard anything the user has typed in the input bar.

#### Scenario: Tab cycle
- **WHEN** the pane is open and the user presses `<Tab>` from the message pane
- **THEN** focus moves to the pane, and `<Tab>` again goes to the sidebar

#### Scenario: Close
- **WHEN** the user presses `Esc` with the pane focused
- **THEN** the pane closes and focus returns to the message pane on the parent message

#### Scenario: Room change
- **WHEN** a thread is open and the user opens another room
- **THEN** the pane is closed

### Requirement: Narrow terminals
When the terminal is too narrow to show the sidebar, the message pane and the thread pane side by side, the thread pane SHALL replace the message pane instead of splitting it, and closing it SHALL bring the message pane back unchanged. The app SHALL NOT crash or draw outside the terminal at any size.

#### Scenario: Wide terminal
- **WHEN** a thread is opened in a terminal of 160 columns
- **THEN** the message pane and the thread pane are both visible

#### Scenario: Narrow terminal
- **WHEN** a thread is opened in a terminal of 80 columns
- **THEN** the pane takes the place of the message pane, and closing it restores the message pane with its cursor and scroll

### Requirement: Thread privacy
Thread content SHALL be handled like any message content: never logged, never put into a status message, never included in an error.

#### Scenario: Failed request
- **WHEN** a thread request fails
- **THEN** no log line, status message or error contains any thread text
