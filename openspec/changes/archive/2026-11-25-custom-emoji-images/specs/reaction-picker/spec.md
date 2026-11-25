# Spec Delta

## MODIFIED Requirements

### Requirement: Custom emojis from the server
The picker SHALL search the server's custom emojis from the list the client loads when the session starts and keeps for the session, and SHALL NOT request it again once it has loaded. When that list has not loaded, the picker SHALL work from unicode emoji and aliases and ask for the list again, and a custom emoji that arrives while the picker is open SHALL become searchable without closing it. A custom emoji SHALL be offered under its name and its aliases and sent under its name.

#### Scenario: First opening
- **WHEN** the picker opens for the first time in a session and the session's list has loaded
- **THEN** the custom emojis are searchable at once and no request is made by the picker

#### Scenario: Later openings
- **WHEN** the picker opens again after a successful load
- **THEN** no new request is made

#### Scenario: Fetch fails
- **WHEN** the session's list could not be loaded
- **THEN** unicode emoji and aliases still search, and opening the picker requests the list again

#### Scenario: List arrives while open
- **WHEN** the picker opens while the list is still loading and the list then arrives
- **THEN** the custom emojis become searchable without closing the picker
