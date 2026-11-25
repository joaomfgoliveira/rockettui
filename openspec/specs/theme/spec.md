# theme Specification

## Purpose

Defines how users restyle every UI element, borders, markers and author colors through `theme.toml`.

## Requirements

### Requirement: Colors
Colors SHALL accept named ANSI colors (e.g. `cyan`, `light-blue`), a 256-color index and `#rrggbb`. Named colors SHALL remain the defaults.

#### Scenario: Hex color
- **WHEN** `theme.toml` sets `fg = "#ff8800"` on an element
- **THEN** that element is drawn in that color

### Requirement: Per-element styles
`theme.toml` SHALL offer per-element tables grouped by pane (for example `[sidebar.cursor]`, `[status.error]`), each with optional `fg`, `bg` and `modifiers`. Unset fields SHALL inherit the built-in style. Every styled element SHALL be themeable.

#### Scenario: Partial override
- **WHEN** only `fg` is set for `sidebar.cursor`
- **THEN** its other attributes keep the default

### Requirement: Borders and markers
The theme SHALL control border style and the marker strings (fold, unread, encrypted, sending, failed, unconfirmed, and similar). Markers SHALL be rejected when sanitizing would change them, and SHALL be at most 4 columns wide (12 for the encrypted marker).

#### Scenario: Wide marker
- **WHEN** a marker exceeds its width limit
- **THEN** the config error screen reports it

### Requirement: Author palette
`author_palette` SHALL accept 1-64 colors that replace the default per-author colors. Each username SHALL map to a stable palette entry. An explicit `fg` on `messages.author` SHALL win over the palette.

#### Scenario: Stable author color
- **WHEN** the same author appears on several messages
- **THEN** every row shows the author in the same color

### Requirement: Vocabulary stability
Theme element names and marker keys SHALL be treated as user-facing vocabulary. Renaming one is a breaking change.

#### Scenario: Error screen style
- **WHEN** the config error screen is shown
- **THEN** it uses fixed built-in styles and does not read the user theme

### Requirement: Search match style
`theme.toml` SHALL offer a `[search.match]` element with optional `fg`, `bg` and `modifiers` that styles the matched text of an active search in the messages pane. Its built-in default SHALL make a match stand out from body text, from the message cursor row and from a dimmed row.

#### Scenario: Themed match
- **WHEN** `theme.toml` sets `bg = "#ff8800"` on `search.match`
- **THEN** the matched text of an active search is drawn on that background

#### Scenario: Match on the cursor row
- **WHEN** a match is on the row under the message cursor
- **THEN** the match is still drawn in the match style and is distinguishable from the rest of the cursor row

### Requirement: Search results style
`theme.toml` SHALL offer the elements `[results.border]`, `[results.title]` and `[results.selected]`, each with optional `fg`, `bg` and `modifiers`, that style the border, the title and the selected row of the server search results list. Their built-in defaults SHALL keep the list readable and make the selected row stand out from the others.

#### Scenario: Themed selected row
- **WHEN** `theme.toml` sets `bg = "#334455"` on `results.selected`
- **THEN** the selected result is drawn on that background

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention `results`
- **THEN** the list is drawn with the built-in styles and the selected row is distinguishable from the rest

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

### Requirement: Reaction picker style
`theme.toml` SHALL offer the elements `[picker.border]`, `[picker.title]`, `[picker.input]`, `[picker.item]`, `[picker.selected]`, `[picker.own]` and `[picker.hint]`, each with optional `fg`, `bg` and `modifiers`, for the border, the title, the search line, a result row, the selected row, the entries the user already gave and the hint and empty-state text. The defaults SHALL make the selected row and the user's own entries stand out without relying on colour.

#### Scenario: Themed selected row
- **WHEN** `theme.toml` sets `bg = "#334455"` on `picker.selected`
- **THEN** the selected result is drawn on that background

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention the picker
- **THEN** the picker is drawn with the built-in styles and the selected row is distinguishable from the others
