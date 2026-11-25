# message-reactions Specification

## Purpose

Shows the reactions on a message as a row of emoji with counts, resolves the server's emoji shortcodes to glyphs, and lets the user see who gave each reaction from the keyboard.

## Requirements

### Requirement: Reaction row
A message that has reactions SHALL be drawn with one extra row under its body listing each reaction and its count, in the order the server lists them. A message without reactions SHALL be drawn exactly as before. The row SHALL be indented to the message body, SHALL wrap within the pane width, and SHALL belong to the message: the message cursor highlights it with the rest of the message.

#### Scenario: Message with reactions
- **WHEN** a message has 3 users who reacted with `:+1:` and 1 with `:tada:`
- **THEN** the row under its body reads `👍 3  🎉 1`

#### Scenario: Message without reactions
- **WHEN** a message has no reactions
- **THEN** it has no reaction row

#### Scenario: Narrow pane
- **WHEN** the reactions do not fit on one row at the current pane width
- **THEN** the row wraps onto further rows and no reaction is cut in the middle

#### Scenario: Both layouts
- **WHEN** `message_layout` is `compact` or `two_line`
- **THEN** the reaction row is drawn under the body in both, aligned with the body

### Requirement: Shortcode resolution
Each reaction SHALL be shown as the first of these that applies: the replacement from `emoji_aliases`, the unicode emoji for its shortcode, the raw `:name:`. A skin-tone suffix such as `_tone2` SHALL be applied as a skin tone to the base emoji. A reaction that resolves to nothing SHALL be shown as its raw `:name:` and SHALL still be counted and listed in the popup.

#### Scenario: Known shortcode
- **WHEN** a reaction is `:thumbsup:`
- **THEN** it is shown as the thumbs-up glyph

#### Scenario: Skin tone
- **WHEN** a reaction is `:thumbsup_tone2:`
- **THEN** it is shown as the thumbs-up glyph with that skin tone

#### Scenario: Custom server emoji
- **WHEN** a reaction is `:partyparrot:` and no alias is configured
- **THEN** it is shown as `:partyparrot:`

#### Scenario: Alias wins
- **WHEN** `emoji_aliases` maps `partyparrot` to a glyph and a message has that reaction
- **THEN** the row shows the glyph, and an alias for a name the unicode table knows also overrides the table

### Requirement: Reactions popup
`show_reactions` SHALL open a popup for the message under the cursor in the messages pane, listing each reaction as its glyph (or raw name), its `:shortcode:` and the usernames that gave it. The user's own username SHALL be shown as `you` and the user's own reactions SHALL be highlighted. Any key SHALL close the popup and SHALL NOT act on the room, the input or the keymap. Opening it SHALL NOT change the mode, the focused pane or the selection.

#### Scenario: Open
- **WHEN** the cursor is on a message with reactions and the user presses `gr`
- **THEN** the popup lists every reaction with the users who gave it and the mode badge still reads Normal

#### Scenario: Own reaction
- **WHEN** the user's own username is among those who gave a reaction
- **THEN** the popup shows `you` for them and highlights that reaction

#### Scenario: Any key closes
- **WHEN** the popup is open and the user presses `j`
- **THEN** the popup closes and the cursor has not moved

#### Scenario: Message without reactions
- **WHEN** the cursor is on a message with no reactions and the user presses `gr`
- **THEN** nothing happens and no popup opens

#### Scenario: Other focus
- **WHEN** the sidebar or the input bar is focused and the user presses `gr`
- **THEN** nothing happens

### Requirement: Popup follows live changes
The open popup SHALL show the current reactions of its message. A reaction added or removed while it is open SHALL be reflected without any key, and if the message loses its last reaction the popup SHALL close. The room underneath SHALL keep updating while it is open.

#### Scenario: Reaction arrives while open
- **WHEN** the popup is open and another user adds a reaction to that message
- **THEN** the popup lists the new reaction

#### Scenario: Last reaction removed
- **WHEN** the popup is open and the last reaction on the message is removed
- **THEN** the popup closes

### Requirement: Popup fits the screen
The reactions popup SHALL be drawn over the bottom of the messages and input area, sized to its content up to a maximum height, and SHALL be usable at the smallest supported size. Content that does not fit SHALL be cut with a visible marker and never overflow the screen.

#### Scenario: Many reactions
- **WHEN** the message has more reactions or users than the popup can show
- **THEN** the popup shows the first ones and a marker that more exist

#### Scenario: Small terminal
- **WHEN** the terminal is at the smallest supported size and the popup is open
- **THEN** the popup is drawn inside the screen and shows at least its title

### Requirement: Text from the server is sanitized
Shortcode names and usernames shown in the row and the popup SHALL be sanitized for terminal output like message text, and widths SHALL be measured on what is drawn.

#### Scenario: Hostile shortcode
- **WHEN** a reaction name or a username contains terminal control characters
- **THEN** they are not passed to the terminal

### Requirement: Reactions are never logged
No log, error text or status message SHALL contain reaction names or the usernames of who reacted, and the redacting debug output of a message SHALL NOT include them.

#### Scenario: Debug output
- **WHEN** a message with reactions is formatted for debugging
- **THEN** the output holds at most the number of reactions
