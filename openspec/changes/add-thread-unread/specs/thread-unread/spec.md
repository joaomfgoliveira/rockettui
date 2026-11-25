# Spec Delta

## Purpose

Shows which threads have replies the user has not read, on the thread itself and on its room in the sidebar, and marks a thread read when the user reads it, keeping this in step with the server.

## ADDED Requirements

### Requirement: Unread mark on a thread
A thread whose id the server lists as unread for the user SHALL show `new` at the end of its marker row. A thread that is not listed SHALL show nothing extra. The state SHALL be the server's and SHALL NOT be computed from the replies the app receives.

#### Scenario: Unread thread
- **WHEN** the server lists a thread as unread for the user
- **THEN** its marker row reads `-> 3 replies, last 14:02, new`

#### Scenario: Read thread
- **WHEN** the thread is not in the server's unread list
- **THEN** the marker row has no `new`

#### Scenario: Reply in a closed thread
- **WHEN** another user replies to a thread that is not open and the server reports it unread
- **THEN** the parent's marker row gains `new` without any command

### Requirement: Thread badge in the sidebar
A room with unread threads SHALL show a badge `T` followed by the number of its unread threads, after the unread count and the mention badge. When the server reports the user mentioned in an unread thread, the badge SHALL use the mention style. A zero count SHALL show no badge. A count over 99 SHALL be shown as `99+`.

#### Scenario: Badge
- **WHEN** a room has 2 unread threads and no unread messages
- **THEN** its row shows `T2`

#### Scenario: With count and mention
- **WHEN** a room has 3 unread messages, 1 mention and 2 unread threads
- **THEN** its row shows `(3)`, `@1` and `T2`, in that order

#### Scenario: Mention in a thread
- **WHEN** the user is mentioned in one of the unread threads
- **THEN** the `T` badge is drawn in the mention style

### Requirement: Rooms with unread threads
A room that has unread threads but no unread messages SHALL be listed in the Unread section of the sidebar, in bold, with its thread badge. The folder header count SHALL keep counting unread rooms. Room unread counts SHALL stay the server's and SHALL NOT include thread replies the server does not count.

#### Scenario: Only a thread is unread
- **WHEN** a room has no unread messages and one unread thread
- **THEN** it is in Unread, bold, showing `T1`

#### Scenario: Thread read
- **WHEN** the last unread thread of such a room is read
- **THEN** the room leaves Unread

### Requirement: Marking a thread read
A thread that is open in the pane SHALL be marked read on the server after it has stayed open for a short delay, using the same delay rule as the room mark-read. Closing the pane or opening another thread before the delay SHALL cancel the mark. A reply that arrives live in a thread that is open and focused SHALL be marked read too. The request is best effort: its failure SHALL NOT be shown as an error and the thread SHALL stay unread.

#### Scenario: Read after a delay
- **WHEN** the user opens an unread thread and keeps it open past the delay
- **THEN** the thread is marked read and `new` disappears

#### Scenario: Quick close
- **WHEN** the user opens an unread thread and closes it at once
- **THEN** the thread stays unread

#### Scenario: Failure
- **WHEN** the mark-read request fails
- **THEN** no error is shown and the thread stays unread

### Requirement: Unread state stays in step
A live subscription update SHALL carry the thread unread state to the rooms and the parents. A field the update does not contain SHALL keep the value the app holds; an update with none of the thread fields SHALL change nothing. A thread read from another client SHALL lose its `new` mark and its share of the badge without any command.

#### Scenario: Read elsewhere
- **WHEN** the user reads the thread in the web client
- **THEN** `new` and its badge count disappear in rockettui

#### Scenario: Partial update
- **WHEN** an update arrives with no thread fields
- **THEN** the badges and marks stay as they were

### Requirement: Thread unread privacy
Thread unread handling SHALL NOT log or show message text, and errors SHALL name the failure class only.

#### Scenario: Failed mark
- **WHEN** a mark-read request fails
- **THEN** no log line or message contains thread text
