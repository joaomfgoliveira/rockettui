# Spec Delta

## ADDED Requirements

### Requirement: Show-image action on messages
A message that has an image attachment SHALL list opening it in the context popup, with the keys currently bound to `show_image`, among the message's own actions. A message with no image SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Message with an image
- **WHEN** the cursor is on a message with an image and the user presses `?`
- **THEN** the popup lists opening the image with `gp`

#### Scenario: Message without an image
- **WHEN** the cursor is on a message with no image and the user presses `?`
- **THEN** the popup has no row for opening an image

#### Scenario: Rebound key
- **WHEN** the user binds `"P" = "show_image"` and opens the popup on a message with an image
- **THEN** the row shows `P`

#### Scenario: Unbound
- **WHEN** the user sets `"gp" = "none"` and opens the popup on a message with an image
- **THEN** no row for opening an image is shown
