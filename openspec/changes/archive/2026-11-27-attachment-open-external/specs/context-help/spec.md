# Spec Delta

## ADDED Requirements

### Requirement: Open-attachment action on messages
A message that has an image, video or audio attachment SHALL list opening it in the context popup, with the keys currently bound to `open_attachment`, among the message's own actions. A message with none SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message with a video
- **WHEN** the cursor is on a message with a video attachment and the user presses `?`
- **THEN** the popup lists opening the attachment with `go`

#### Scenario: Message without a supported attachment
- **WHEN** the cursor is on a message with no image, video or audio attachment and the user presses `?`
- **THEN** the popup has no row for opening an attachment

#### Scenario: Rebound key
- **WHEN** the user binds `"O" = "open_attachment"` and opens the popup on such a message
- **THEN** the row shows `O`

#### Scenario: Unbound
- **WHEN** the user sets `"go" = "none"` and opens the popup on such a message
- **THEN** no row for opening an attachment is shown
