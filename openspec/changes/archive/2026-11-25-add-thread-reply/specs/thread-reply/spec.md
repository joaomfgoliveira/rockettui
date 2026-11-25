# Spec Delta

## Purpose

Lets the user answer a thread, start a thread on a message, choose whether a reply is also shown in the channel, and edit, delete and react to messages inside the thread pane, all from the keyboard.

## ADDED Requirements

### Requirement: Replying from the pane
With the thread pane focused, the key that enters Insert mode SHALL move focus to an input bar with the thread as its target. When the thread is shown beside the messages it SHALL have an input bar of its own under it, next to the room's input bar, and each bar SHALL show its own draft; when the thread takes the place of the message pane there SHALL be one input bar, which names the thread as its target. While a thread is the target the bar's title SHALL name it as a reply. `Enter` SHALL send the text as a reply to that thread. `Esc` SHALL leave Insert mode as usual and keep the target and the text.

#### Scenario: Start typing
- **WHEN** the thread pane is focused and the user presses `i`
- **THEN** the input bar is focused, in Insert mode, and its title says it is a reply in the thread

#### Scenario: Send
- **WHEN** the user types `ok` and presses `Enter`
- **THEN** a reply `ok` is sent to the open thread and appears at the bottom of the pane

#### Scenario: Leave and return
- **WHEN** the user presses `Esc` and later `i` again
- **THEN** the text and the thread target are still there

#### Scenario: Own input bar
- **WHEN** the thread is shown beside the messages
- **THEN** the thread has its own input bar, and the room's input bar keeps the room's draft

### Requirement: Starting a thread
The key bound to `reply_in_thread` in Normal mode with the message pane focused SHALL open the thread pane on the message under the cursor, even when it has no replies, and start a reply as above. Sending the first reply SHALL create the thread, after which the message SHALL show a thread marker row. On a reply shown in the channel it SHALL act on the reply's parent. On a system message, a deleted message or in an encrypted room it SHALL refuse with a status message.

#### Scenario: First reply
- **WHEN** the cursor is on a message with no replies, the user presses `gR`, types `hi` and presses `Enter`
- **THEN** a thread is created with that reply and the message shows `1 reply`

#### Scenario: Existing thread
- **WHEN** the message already has replies and the user presses `gR`
- **THEN** its thread opens and a reply can be typed

#### Scenario: System message
- **WHEN** the cursor is on a system message and the user presses `gR`
- **THEN** nothing opens and the status bar says it cannot be replied to

### Requirement: Also send to the channel
In Insert mode with a thread as the target, the key bound to `toggle_reply_to_channel` SHALL toggle whether the reply is also shown in the channel. The state SHALL be visible in the input bar title and SHALL reset to off after each reply is sent and when the target changes. A reply sent with it on SHALL appear in the main flow with its thread-reply hint and in the pane.

#### Scenario: Toggle
- **WHEN** the user presses `<C-b>` while writing a thread reply
- **THEN** the input bar title shows that the reply will also go to the channel

#### Scenario: Reset after send
- **WHEN** the reply is sent
- **THEN** the option is off for the next reply

#### Scenario: Outside a thread
- **WHEN** the input bar targets the room and the user presses `<C-b>`
- **THEN** nothing changes

### Requirement: Drafts per thread
A draft SHALL belong to its room or to its thread. Switching between the room input and a thread reply, closing the pane or opening another thread SHALL keep each draft and SHALL NOT mix text between them. Drafts SHALL stay in memory only and SHALL NOT be written to disk or logged. An empty draft SHALL NOT be stored. A reply that is not yet sent SHALL stay queued when its pane is closed and SHALL show again when the thread is reopened.

#### Scenario: Two drafts
- **WHEN** the user has typed `a` in the room input and `b` in a thread reply and closes the pane
- **THEN** the room input still holds `a` and reopening the thread brings back `b`

#### Scenario: Quit warning
- **WHEN** a thread reply is queued, being sent, failed or unconfirmed and the user quits
- **THEN** the existing quit warning counts it, whether or not its thread is open

### Requirement: Actions on pane messages
Edit, delete and react SHALL work on the message under the cursor in the thread pane under the same rules, keys and confirmation as in the main flow. A user's own reply SHALL be editable and deletable; deleting a thread's parent SHALL follow the server's rules and the pane SHALL show the result.

#### Scenario: Edit own reply
- **WHEN** the cursor is on the user's own reply in the pane and the user presses `e`
- **THEN** the reply is loaded into the input bar for editing

#### Scenario: React in the pane
- **WHEN** the user presses `+` on a reply in the pane and picks an emoji
- **THEN** the reaction appears on that reply

#### Scenario: Someone else's reply
- **WHEN** the user presses `e` on another user's reply
- **THEN** nothing is edited and the existing refusal is shown

### Requirement: Thread reply failures
A reply that fails, or whose outcome is unknown, SHALL be shown as a failed row in the pane with the existing retry and discard keys, and SHALL NOT appear in the main flow. Retrying SHALL NOT create a duplicate when the first attempt did reach the server.

#### Scenario: Failed reply
- **WHEN** the server rejects a reply
- **THEN** the pane shows it as failed and `r` retries it

#### Scenario: Unknown outcome
- **WHEN** a reply times out and then arrives through the live stream
- **THEN** the pane shows it once, as sent
