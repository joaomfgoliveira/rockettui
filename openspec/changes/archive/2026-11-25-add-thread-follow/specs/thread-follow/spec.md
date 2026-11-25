# Spec Delta

## Purpose

Shows whether the user follows a thread and lets the user follow or unfollow it from the keyboard, so the server's notifications for that thread match what the user wants.

## ADDED Requirements

### Requirement: Follow state shown
A thread marker row and the thread pane header SHALL show whether the user follows the thread: a followed thread SHALL show the word `following`, an unfollowed thread SHALL show nothing extra. The state SHALL be the server's: a thread whose follower list contains the user's own account is followed.

#### Scenario: Followed thread
- **WHEN** the server lists the user among the followers of a thread with 3 replies
- **THEN** its marker row reads `-> 3 replies, last 14:02, following`

#### Scenario: Unfollowed thread
- **WHEN** the user is not among the followers
- **THEN** the marker row has no follow text

#### Scenario: Pane header
- **WHEN** the user follows the open thread
- **THEN** the pane header shows `following`

### Requirement: Toggling follow
The key bound to `toggle_follow_thread` SHALL follow the thread when the user does not follow it and unfollow it when they do. It SHALL act on the thread of the message under the cursor in the message pane, or on the open thread when the thread pane is focused. On a message with no thread it SHALL do nothing and the status bar SHALL say there is no thread. The state SHALL change on screen when the server accepts the request.

#### Scenario: Follow
- **WHEN** the cursor is on an unfollowed thread parent and the user presses `gf`
- **THEN** the thread is followed and the marker row shows `following`

#### Scenario: Unfollow
- **WHEN** the user presses `gf` on a followed thread
- **THEN** the thread is unfollowed and `following` disappears

#### Scenario: In the pane
- **WHEN** the thread pane is focused and the user presses `gf`
- **THEN** the open thread's follow state toggles

#### Scenario: No thread
- **WHEN** the cursor is on a message with no replies and the user presses `gf`
- **THEN** nothing is sent and the status bar says there is no thread

### Requirement: Failed toggle
When the server rejects or does not answer a follow or unfollow request, the follow state SHALL stay as it was and the status bar SHALL show that the change failed, naming the failure class only. Pressing the key twice before the first answer SHALL send one request at a time and SHALL NOT leave the state inconsistent with the server.

#### Scenario: Rejected
- **WHEN** the server rejects a follow request
- **THEN** the thread stays unfollowed and the status bar says following failed

#### Scenario: Double press
- **WHEN** the user presses `gf` twice quickly
- **THEN** the second press is ignored until the first request finishes

### Requirement: Follow state stays in step
When the follow state of a thread changes on the server, whether by the user's other client, by the user's own reply or by the server's automatic following, the state shown SHALL follow it without any command. A fetched thread or parent SHALL carry the server's current state.

#### Scenario: Changed elsewhere
- **WHEN** the user unfollows the thread from the web client
- **THEN** `following` disappears from the thread in rockettui

#### Scenario: Reload
- **WHEN** a room is reloaded
- **THEN** every thread shows the server's current follow state

### Requirement: Follow privacy
Follow requests, state and errors SHALL NOT contain or log message text or the token.

#### Scenario: Failed request
- **WHEN** a follow request fails
- **THEN** no log line, status message or error contains message text
