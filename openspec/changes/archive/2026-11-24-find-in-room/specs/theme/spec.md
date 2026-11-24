# Spec Delta

## ADDED Requirements

### Requirement: Search match style
`theme.toml` SHALL offer a `[search.match]` element with optional `fg`, `bg` and `modifiers` that styles the matched text of an active search in the messages pane. Its built-in default SHALL make a match stand out from body text, from the message cursor row and from a dimmed row.

#### Scenario: Themed match
- **WHEN** `theme.toml` sets `bg = "#ff8800"` on `search.match`
- **THEN** the matched text of an active search is drawn on that background

#### Scenario: Match on the cursor row
- **WHEN** a match is on the row under the message cursor
- **THEN** the match is still drawn in the match style and is distinguishable from the rest of the cursor row
