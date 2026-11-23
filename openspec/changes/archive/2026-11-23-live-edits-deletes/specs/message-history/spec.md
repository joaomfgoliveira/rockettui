# Spec Delta

## ADDED Requirements

### Requirement: Edited marker
A message the server reports as edited SHALL be drawn with a visible edited marker, whether it arrived live or in a fetched page. The marker SHALL be themeable like the other message elements.

#### Scenario: Edited message in history
- **WHEN** a fetched page contains a message with an edit time
- **THEN** its row shows the edited marker

### Requirement: Deleted lines
A deleted message SHALL be drawn as one dim line that keeps its place and the original author and time, says "message deleted" and never shows its text or who deleted it. Deleted lines exist only for the running session: history never returns deleted messages, so they are absent after a restart or a room reload.

#### Scenario: Deleted message the server keeps
- **WHEN** the server keeps a deleted message as a deleted-status message (setting "show deleted status"), live or in a fetched page
- **THEN** it is drawn as a deleted line with the original author, without its text and without an edited marker, and never as a system message

#### Scenario: Reload after a delete
- **WHEN** the app restarts and loads a room that had a deleted message
- **THEN** the deleted message is simply not shown

### Requirement: Reconcile edits and deletes after a reconnect
When a refetched page after a reconnect covers a held message, the held copy SHALL take the page's text and edited state. A held server message that falls inside the page's time range and is absent from it SHALL become a deleted line. Held messages older than the page SHALL be left as they are.

#### Scenario: Edited while offline
- **WHEN** a message was edited during an outage and the refetched page contains its new text
- **THEN** the row shows the new text and the edited marker

#### Scenario: Deleted while offline
- **WHEN** a held message inside the refetched page's time range is absent from it
- **THEN** it becomes a deleted line

#### Scenario: Older than the page
- **WHEN** a held message is older than everything in the refetched page
- **THEN** it is unchanged, even if it was deleted or edited meanwhile
