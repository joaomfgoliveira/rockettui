# Spec Delta

## ADDED Requirements

### Requirement: Loading newer history while searching
In a detached room (see `message-history`), when the search previous action finds no loaded match newer than the cursor, the app SHALL fetch newer pages one at a time and search each until a match is found, the newest message is reached, or the search is abandoned. Without a match the cursor SHALL NOT move and the status line SHALL say so.

#### Scenario: Match in a newer page
- **WHEN** `N` finds nothing newer in memory in a detached room and the next newer page contains a match
- **THEN** that page is loaded and the cursor moves to the matching message

#### Scenario: Several pages
- **WHEN** the first newer page has no match and the second does
- **THEN** both pages are loaded and the cursor ends on the match

#### Scenario: Newest reached without a match
- **WHEN** newer pages run out with no match
- **THEN** the status line says there is no newer match, no further page is requested and the cursor has not moved

#### Scenario: Failed page
- **WHEN** a newer page fails to load during the search
- **THEN** the search stops and the status line reports the failure

#### Scenario: Abandoned
- **WHEN** the user opens another room or clears the search while a newer page is loading
- **THEN** no further page is requested for the search

#### Scenario: Room that is not detached
- **WHEN** `N` finds nothing newer in a room that is not detached
- **THEN** the status line says there is no newer match and nothing is requested
