# Spec Delta

## MODIFIED Requirements

### Requirement: Reaction picker style
`theme.toml` SHALL offer the elements `[picker.border]`, `[picker.title]`, `[picker.input]`, `[picker.item]`, `[picker.selected]`, `[picker.own]`, `[picker.hint]`, `[picker.tab]`, `[picker.tab_current]`, `[picker.heading]` and `[picker.footer]`, each with optional `fg`, `bg` and `modifiers`, for the border, the title, the search line, an emoji cell, the selected cell, the entries the user already gave, the hint and empty-state text, a category tab, the tab of the current section, a section heading and the footer line. The defaults SHALL make the selected cell, the current tab and the user's own entries stand out without relying on colour.

#### Scenario: Themed selected row
- **WHEN** `theme.toml` sets `bg = "#334455"` on `picker.selected`
- **THEN** the selected result is drawn on that background

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention the picker
- **THEN** the picker is drawn with the built-in styles and the selected row is distinguishable from the others

#### Scenario: Themed tab
- **WHEN** `theme.toml` sets `fg = "#ffaa00"` on `picker.tab_current`
- **THEN** the tab of the current section is drawn in that colour

#### Scenario: Existing themes keep working
- **WHEN** a `theme.toml` written before these elements existed is loaded
- **THEN** it loads without error and the new elements use the built-in styles
