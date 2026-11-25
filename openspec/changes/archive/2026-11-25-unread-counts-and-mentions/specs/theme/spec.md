## ADDED Requirements

### Requirement: Mention badge style
`theme.toml` SHALL offer the element `[sidebar.mention]` for the mention badge in the sidebar, with optional `fg`, `bg` and `modifiers`. The built-in default SHALL draw it in red and bold, distinct from the unread name.

#### Scenario: Styled badge
- **WHEN** `theme.toml` sets `fg = "yellow"` on `sidebar.mention`
- **THEN** the mention badge is drawn in yellow

#### Scenario: Default badge
- **WHEN** `theme.toml` does not mention `sidebar.mention`
- **THEN** the badge is red and bold
