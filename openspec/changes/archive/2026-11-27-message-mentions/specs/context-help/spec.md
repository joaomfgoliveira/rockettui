# Spec Delta

## ADDED Requirements

### Requirement: Open-channel action on messages
A message that references a channel SHALL list opening it in the context popup, with the keys currently bound to `open_channel`, among the message's own actions. A message with no channel reference SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message with a channel
- **WHEN** the cursor is on a message that references `#dev-chat` and the user presses `?`
- **THEN** the popup lists opening the channel with `gc`

#### Scenario: Message without a channel
- **WHEN** the cursor is on a message with no channel reference and the user presses `?`
- **THEN** the popup has no row for opening a channel

#### Scenario: Rebound key
- **WHEN** the user binds `"C" = "open_channel"` and opens the popup on a message that references a channel
- **THEN** the row shows `C`

#### Scenario: Unbound
- **WHEN** the user sets `"gc" = "none"` and opens the popup on such a message
- **THEN** no row for opening a channel is shown
