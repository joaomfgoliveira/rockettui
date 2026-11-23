# Spec Delta

## ADDED Requirements

### Requirement: Editing state of the input
While a message is being edited the app SHALL stay in Insert mode on the input bar, with the input title saying that Enter saves and Esc cancels, and SHALL leave that state when the edit is saved, cancelled or the user switches room. Leaving it SHALL restore the user's previous draft for the room.

#### Scenario: Switch room while editing
- **WHEN** the user opens another room while editing
- **THEN** the edit is cancelled, and the room the user left keeps the draft it had before the edit started

### Requirement: Confirmation prompt
A confirmation prompt SHALL be shown in the status bar and SHALL consume exactly one key: `y` or Enter confirms and any other key cancels without any other effect. While it is open, no other command SHALL run, and the prompt SHALL NOT change the current mode.

#### Scenario: Other key cancels
- **WHEN** a confirmation prompt is open and the user presses `j`
- **THEN** the prompt closes, the cursor does not move and nothing is confirmed

#### Scenario: Repeated delete key
- **WHEN** the user presses `3dd` on their own message
- **THEN** one prompt is opened, not three
