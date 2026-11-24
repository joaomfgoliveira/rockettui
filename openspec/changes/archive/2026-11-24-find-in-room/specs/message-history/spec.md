# Spec Delta

## MODIFIED Requirements

### Requirement: Older pages
Reaching the oldest loaded message, or a search running out of loaded messages older than the cursor (see `message-search`), SHALL load the next older page of `history_size` messages, prepended with the cursor staying on the same message unless the search then moves it to a match. A page that adds no new message, or an empty first page, SHALL mark the end of history and show a "Beginning of history" status message. One older page SHALL be in flight per room, whoever asked for it.

#### Scenario: End of history
- **WHEN** an older page adds no messages
- **THEN** no further pages are requested for that room

#### Scenario: Search asks for a page
- **WHEN** a search finds no match older than the cursor and the room has more history
- **THEN** one older page is requested, prepended, and the cursor stays on its message unless the page holds a match

#### Scenario: Page already in flight
- **WHEN** an older page is loading because the cursor reached the oldest message, and a search then needs older history
- **THEN** no second request is sent and the search continues with the page that is loading
