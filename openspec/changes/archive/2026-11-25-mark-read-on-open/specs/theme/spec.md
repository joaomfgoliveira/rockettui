# Spec Delta

## ADDED Requirements

### Requirement: Unread divider style
`theme.toml` SHALL offer the element `[messages.unread_divider]` for the unread divider line, with optional `fg`, `bg` and `modifiers`. The built-in default SHALL draw it in red, distinct from the date divider.

#### Scenario: Custom color
- **WHEN** `theme.toml` sets `fg = "yellow"` on `messages.unread_divider`
- **THEN** the unread divider is drawn in yellow

#### Scenario: Default
- **WHEN** `theme.toml` does not mention `messages.unread_divider`
- **THEN** the divider is red
