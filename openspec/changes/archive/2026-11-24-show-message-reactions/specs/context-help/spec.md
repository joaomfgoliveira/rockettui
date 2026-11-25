# Spec Delta

## ADDED Requirements

### Requirement: Reactions action on messages
A message that has reactions SHALL list showing its reactions in the context popup, with the keys currently bound to `show_reactions`, after the message's own actions. A message without reactions SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message with reactions
- **WHEN** the cursor is on a message with reactions and the user presses `?`
- **THEN** the popup lists showing reactions with `gr`

#### Scenario: Message without reactions
- **WHEN** the cursor is on a message with no reactions and the user presses `?`
- **THEN** the popup has no row for reactions

#### Scenario: Rebound key
- **WHEN** the user binds `"R" = "show_reactions"` and opens the popup on a message with reactions
- **THEN** the row shows `R`
