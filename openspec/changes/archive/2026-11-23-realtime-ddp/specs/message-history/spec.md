# Spec Delta

## MODIFIED Requirements

### Requirement: First-open history
On first open of a room the app SHALL fetch the newest `history_size` messages, cache them per room and position the cursor at the newest message. Reopening SHALL NOT refetch, except that a room marked stale by a realtime reconnect SHALL refetch its newest page on its next open. There SHALL be no startup prefetch of all rooms.

#### Scenario: Reopen
- **WHEN** the user reopens a room that already loaded and is not stale
- **THEN** no new history request is made

#### Scenario: Reopen a stale room
- **WHEN** the user reopens a loaded room that was marked stale by a reconnect
- **THEN** its newest page is fetched and merged by message id, and the room is no longer stale

## ADDED Requirements

### Requirement: Live and fetched messages merge by id
Messages that arrive live and messages from fetched pages SHALL be merged by message id within a room: a held message is never duplicated, a pending sent message is confirmed in place, and order follows message time. Live messages SHALL NOT disturb the cursor message, and older pages SHALL still prepend with the cursor staying on the same message.

#### Scenario: Live message while reading older messages
- **WHEN** the cursor is on an older message and a live message arrives
- **THEN** the cursor stays on the same message and the new message is held at the bottom

#### Scenario: Page overlaps live messages
- **WHEN** a refetched page contains messages that already arrived live
- **THEN** each message appears once
