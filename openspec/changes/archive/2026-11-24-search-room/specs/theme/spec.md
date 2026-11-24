# Spec Delta

## ADDED Requirements

### Requirement: Search results style
`theme.toml` SHALL offer the elements `[results.border]`, `[results.title]` and `[results.selected]`, each with optional `fg`, `bg` and `modifiers`, that style the border, the title and the selected row of the server search results list. Their built-in defaults SHALL keep the list readable and make the selected row stand out from the others.

#### Scenario: Themed selected row
- **WHEN** `theme.toml` sets `bg = "#334455"` on `results.selected`
- **THEN** the selected result is drawn on that background

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention `results`
- **THEN** the list is drawn with the built-in styles and the selected row is distinguishable from the rest
