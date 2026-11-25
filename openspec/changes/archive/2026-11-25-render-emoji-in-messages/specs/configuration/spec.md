# Spec Delta

## MODIFIED Requirements

### Requirement: Emoji aliases
`config.toml` SHALL support an `[emoji_aliases]` table mapping a shortcode name written without colons to the text shown for that emoji, in a reaction and in message text, for example `partyparrot = "🦜"`. An alias value SHALL be accepted only when sanitizing would not change it and it is at most 8 display columns wide; an invalid name or value SHALL be a config error that names the entry. Without the table no alias applies.

#### Scenario: Alias for a custom emoji
- **WHEN** `[emoji_aliases]` sets `partyparrot = "🦜"`
- **THEN** a `:partyparrot:` reaction is drawn as that glyph

#### Scenario: Value too wide
- **WHEN** an alias value is wider than 8 columns
- **THEN** the error screen reports the entry and the allowed width

#### Scenario: Control characters
- **WHEN** an alias value contains a control character
- **THEN** the error screen reports the entry

#### Scenario: Name with colons
- **WHEN** an alias name is written as `:partyparrot:`
- **THEN** the error screen reports that the name is written without colons

#### Scenario: Alias in message text
- **WHEN** `[emoji_aliases]` sets `partyparrot = "🦜"` and a message says `:partyparrot:`
- **THEN** the message is drawn with 🦜
