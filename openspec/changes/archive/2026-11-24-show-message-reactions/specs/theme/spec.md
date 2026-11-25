# Spec Delta

## ADDED Requirements

### Requirement: Reaction row style
`theme.toml` SHALL offer the elements `[messages.reactions]` for the reaction row and `[messages.reactions_own]` for the reactions the user gave, each with optional `fg`, `bg` and `modifiers`. The built-in defaults SHALL keep the row readable, make the user's own reactions stand out from the others, and not depend on the terminal drawing the emoji.

#### Scenario: Themed reaction row
- **WHEN** `theme.toml` sets `fg = "yellow"` on `messages.reactions`
- **THEN** the reaction row is drawn in that color

#### Scenario: Own reaction in the row
- **WHEN** `theme.toml` does not mention reactions and the user has reacted to a message
- **THEN** that reaction in the row is distinguishable from the others

### Requirement: Reactions popup style
`theme.toml` SHALL offer the elements `[reactions.border]`, `[reactions.title]`, `[reactions.name]`, `[reactions.users]` and `[reactions.own]`, each with optional `fg`, `bg` and `modifiers`, that style the border, the title, the shortcode column, the user list and the user's own entries of the reactions popup. Their built-in defaults SHALL keep the popup readable.

#### Scenario: Themed user list
- **WHEN** `theme.toml` sets `fg = "cyan"` on `reactions.users`
- **THEN** the usernames in the popup are drawn in that color

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention reactions
- **THEN** the popup is drawn with the built-in styles and the user's own entries are distinguishable
