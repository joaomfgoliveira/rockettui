# Spec Delta

## ADDED Requirements

### Requirement: Reloading a room
`:reload` SHALL reload the newest messages of the open room from the server. In a detached room it SHALL end the detached state and show the newest messages with the cursor on the newest. In a loaded room it SHALL fetch the newest messages again and merge them by id, keeping the cursor, the older pages already loaded and the room's unsent messages. In a room whose first load failed it SHALL retry the load. It SHALL say on the status line that it is reloading.

#### Scenario: Leave a detached room
- **WHEN** the room is detached after a jump from a search result and the user runs `:reload`
- **THEN** the room is no longer detached and shows its newest messages with the cursor on the newest

#### Scenario: Refresh a loaded room
- **WHEN** the room is loaded and the user runs `:reload`
- **THEN** the newest messages are requested and any message the room lacks is added, with the cursor and the older pages kept

#### Scenario: Retry a failed room
- **WHEN** the room's first load failed and the user runs `:reload`
- **THEN** the load is requested again

#### Scenario: Unsent messages are kept
- **WHEN** the room holds a failed or queued message and the user runs `:reload`
- **THEN** that message is still in the room, in the same state

#### Scenario: Status line
- **WHEN** the user runs `:reload` and a request is sent
- **THEN** the status line says the newest messages are being reloaded and the line clears on the next key

### Requirement: Reload refusals
`:reload` SHALL change nothing and show a one-line message when no room is open, when the open room is encrypted, or when the room is already loading its messages.

#### Scenario: No room open
- **WHEN** no room is open and the user runs `:reload`
- **THEN** the status line asks to open a room and nothing is requested

#### Scenario: Encrypted room
- **WHEN** the open room is encrypted and the user runs `:reload`
- **THEN** the status line says encrypted rooms are not supported and nothing is requested

#### Scenario: Already loading
- **WHEN** the room is still loading its first page and the user runs `:reload`
- **THEN** the status line says it is already loading and no second request is sent
