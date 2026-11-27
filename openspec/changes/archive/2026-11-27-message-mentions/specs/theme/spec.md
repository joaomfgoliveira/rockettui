# Spec Delta

## ADDED Requirements

### Requirement: Mention and channel styles
`theme.toml` SHALL offer the elements `[messages.mention]` for another user's `@name`, `[messages.mention_self]` for the user's own `@name`, `[messages.mention_group]` for `@here` and `@all`, and `[messages.channel]` for `#channel`, each with optional `fg`, `bg` and `modifiers`. The built-in defaults SHALL draw the four visibly different from the body text and from each other, with the user's own mention the most prominent.

#### Scenario: Styled mention
- **WHEN** `theme.toml` sets `fg = "yellow"` on `messages.mention`
- **THEN** another user's `@name` is drawn in yellow and the user's own is not

#### Scenario: Styled channel
- **WHEN** `theme.toml` sets `modifiers = ["underlined"]` on `messages.channel`
- **THEN** `#channel` references are underlined

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention these elements
- **THEN** the four kinds are drawn in their built-in styles, and the user's own mention stands out most

#### Scenario: Highlight on the cursor row
- **WHEN** a mention is on the cursor message's rows
- **THEN** it stays distinguishable from the surrounding text
