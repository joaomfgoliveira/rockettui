# message-emoji Specification

## Purpose

Draws the emoji shortcodes people write in messages as emoji, following the server's own rules for what counts as a shortcode, and leaves the stored text alone.

## Requirements

### Requirement: Shortcodes in message text are drawn as emoji
A shortcode `:name:` in the body of a message SHALL be drawn as its emoji when it resolves, by the same rules as a reaction: the user's `[emoji_aliases]` first, then the unicode table, then the names the server uses, with a `_toneN` suffix applied as a skin tone, then the server's custom emojis by name and by alias. A shortcode that does not resolve SHALL be drawn exactly as written. System messages and deleted lines SHALL be left as they are, and so SHALL emoji already written as characters.

#### Scenario: Known shortcode
- **WHEN** a message says `ship it :tada:`
- **THEN** the row reads `ship it 🎉`

#### Scenario: Skin tone
- **WHEN** a message says `:thumbsup_tone2:`
- **THEN** it is drawn as the thumbs-up with that skin tone

#### Scenario: Alias
- **WHEN** `emoji_aliases` maps `partyparrot` to 🦜 and a message says `:partyparrot:`
- **THEN** it is drawn as 🦜

#### Scenario: Name the server uses
- **WHEN** a message says `:flag_af:`
- **THEN** it is drawn as the Afghanistan flag

#### Scenario: Unknown shortcode
- **WHEN** a message says `:rt_not_an_emoji:` and no source knows it
- **THEN** it is drawn as `:rt_not_an_emoji:`

#### Scenario: Custom emoji
- **WHEN** a message says `:rtshipit:` and the server lists a custom emoji of that name
- **THEN** it is drawn as that emoji's image, or as its fallback while the image is not ready

#### Scenario: Custom emoji by its alias
- **WHEN** a message says `:rtship:` and the server lists that as an alias of `rtshipit`
- **THEN** it is drawn exactly as `:rtshipit:` is

#### Scenario: Unicode wins over custom
- **WHEN** the server has a custom emoji named `tada` and a message says `:tada:`
- **THEN** it is drawn as 🎉, because the unicode table comes before the server's custom emojis

#### Scenario: Emoji already a character
- **WHEN** a message contains 🎉 as a character
- **THEN** it is drawn as written

#### Scenario: Message still being sent
- **WHEN** the user has just sent `thanks :+1:` and the row is still pending
- **THEN** the row shows 👍 like the confirmed message will

### Requirement: Where a shortcode counts
A shortcode SHALL count only when the colon that opens it is not preceded by a letter or digit and the colon that closes it is not followed by one, and when what is between the colons is one or more letters, digits, `_`, `+` or `-`. Anything else SHALL stay as written.

#### Scenario: Times and ratios
- **WHEN** a message says `at 10:100:20`
- **THEN** it is drawn unchanged

#### Scenario: Punctuation around it
- **WHEN** a message says `(:tada:)` or `:tada:,`
- **THEN** the shortcode is drawn as the emoji and the punctuation stays

#### Scenario: Two in a row
- **WHEN** a message says `:tada::+1:`
- **THEN** both are drawn as emoji

#### Scenario: Glued to a word
- **WHEN** a message says `a:tada:` or `:tada:b`
- **THEN** it is drawn unchanged

#### Scenario: Spaces inside
- **WHEN** a message says `:not a code:`
- **THEN** it is drawn unchanged

### Requirement: Code is left alone
A shortcode inside inline code (between backticks) or a fenced code block SHALL be drawn exactly as written.

#### Scenario: Inline code
- **WHEN** a message says ``use `:smile:` here``
- **THEN** `:smile:` is drawn as written and the backticks stay

#### Scenario: Fenced block
- **WHEN** a message holds a fenced block containing `:tada:`
- **THEN** it is drawn as written

### Requirement: Only the drawing changes
The stored text of a message SHALL NOT change. Editing a message SHALL load the text as it was typed, with its shortcodes. Lines SHALL wrap by the width of what is drawn, so an emoji takes the room it takes on screen, and the number of rows a message takes SHALL be the same in the pane and in the row model that scrolling uses.

#### Scenario: Edit shows what was typed
- **WHEN** the user edits their own message that was sent as `nice :tada:`
- **THEN** the input bar holds `nice :tada:`

#### Scenario: Wrapping by drawn width
- **WHEN** a message holds shortcodes that draw wider or narrower than they were written
- **THEN** the lines wrap by the drawn width and no row is wider than the pane

### Requirement: Reactions use the server's names too
A reaction named with a name the server uses for an emoji SHALL be drawn as that emoji in the reaction row, in the reactions popup and in the picker, like a name from the unicode table. A reaction named with a custom emoji SHALL be drawn in the reaction row as that emoji's image or its fallback, and in the popup and the picker as its image in the glyph column (the stand-in while it is on its way) followed by its `:name:`; where the image cannot be shown they show the `:name:` alone.

#### Scenario: Reaction added under a server name
- **WHEN** a message has a reaction named `flag_af`
- **THEN** the row shows the Afghanistan flag with its count, and the popup and the picker draw it the same way

#### Scenario: Reaction with a custom emoji
- **WHEN** a message has a reaction named `rtshipit`
- **THEN** the row shows that emoji's image, or its fallback while the image is not ready, followed by the count, and the popup and the picker show that image (or no glyph) and `:rtshipit:`

### Requirement: A custom emoji takes a fixed width
On a terminal that can show images, a custom emoji SHALL take two columns in a body and in a reaction, whether it is drawn as an image or is still waiting for one. On a terminal that cannot, it takes the width of its `:name:`. Lines SHALL wrap by the width drawn, and the number of rows a message takes SHALL be the same in the pane and in the row model that scrolling uses.

#### Scenario: Wrapping around an image
- **WHEN** a message holds custom emojis among words at a narrow pane
- **THEN** the lines wrap counting two columns per custom emoji and no row is wider than the pane

#### Scenario: Image arrives later
- **WHEN** an image becomes ready after its message was drawn
- **THEN** the message keeps the same rows and the scroll position does not change

#### Scenario: Code is left alone
- **WHEN** a message says ``use `:rtshipit:` here``
- **THEN** the shortcode inside the backticks is drawn as written and takes its written width
