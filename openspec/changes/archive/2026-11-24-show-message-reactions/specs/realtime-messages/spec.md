# Spec Delta

## MODIFIED Requirements

### Requirement: Live edits
When another user edits a message held in a loaded room, the held message SHALL be replaced in place by the edited copy and marked edited. An update to a held message that changes only its reactions SHALL replace the held message's reactions in place, without marking it edited. An update that changes none of its text, its edited state or its reactions (a pin) SHALL leave it untouched.

#### Scenario: Message edited in the open room
- **WHEN** another user edits a message that is held in the open room
- **THEN** the row shows the new text and the edited marker without any command, in the same position

#### Scenario: Reaction on a held message
- **WHEN** the server delivers a held message again with only a reaction added
- **THEN** the row shows the new reaction without any command, the message is not marked edited and the cursor stays on the same message at the same screen row

#### Scenario: Reaction removed from a held message
- **WHEN** the server delivers a held message again with a reaction removed
- **THEN** the row no longer shows it, and a message left with no reactions loses its reaction row

#### Scenario: Update that changes nothing shown
- **WHEN** the server delivers a held message again with the same text, edited state and reactions (a pin)
- **THEN** the row does not change

#### Scenario: Edit of a message not held
- **WHEN** an edit arrives for a message the room does not hold
- **THEN** it is handled like any other live message for that room

#### Scenario: Reaction on a message not held
- **WHEN** a reaction update arrives for a message the room does not hold
- **THEN** it is handled like any other live message for that room
