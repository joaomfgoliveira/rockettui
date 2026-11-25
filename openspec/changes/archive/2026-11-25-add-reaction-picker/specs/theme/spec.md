# Spec Delta

## ADDED Requirements

### Requirement: Reaction picker style
`theme.toml` SHALL offer the elements `[picker.border]`, `[picker.title]`, `[picker.input]`, `[picker.item]`, `[picker.selected]`, `[picker.own]` and `[picker.hint]`, each with optional `fg`, `bg` and `modifiers`, for the border, the title, the search line, a result row, the selected row, the entries the user already gave and the hint and empty-state text. The defaults SHALL make the selected row and the user's own entries stand out without relying on colour.

#### Scenario: Themed selected row
- **WHEN** `theme.toml` sets `bg = "#334455"` on `picker.selected`
- **THEN** the selected result is drawn on that background

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention the picker
- **THEN** the picker is drawn with the built-in styles and the selected row is distinguishable from the others
