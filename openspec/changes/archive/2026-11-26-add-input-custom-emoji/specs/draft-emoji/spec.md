# Spec Delta

## Purpose

Shows the custom emojis of a draft as the images they will be in the message, while keeping the draft's text exactly as typed and keeping the cursor and the line layout correct.

## ADDED Requirements

### Requirement: Custom emojis in the draft are drawn as images
On a terminal that can show images, a custom emoji written in the draft SHALL be drawn as its image in two columns, or as the two-column stand-in while the image is not ready, in the room's input bar, in a thread's input bar and while editing a message. Anything that is not a custom emoji SHALL be drawn as written.

#### Scenario: Picked emoji
- **WHEN** the draft holds `ship it :rtshipit:` and the server lists that custom emoji
- **THEN** the bar draws `ship it ` followed by the emoji's image

#### Scenario: Alias
- **WHEN** the draft holds `:rtship:` and the server lists that as an alias of `rtshipit`
- **THEN** it is drawn exactly as `:rtshipit:` is

#### Scenario: Image not ready
- **WHEN** the emoji's image has not arrived yet
- **THEN** the bar draws two stand-in cells in its place and the layout does not change when the image arrives

#### Scenario: Unknown name
- **WHEN** the draft holds `:rt_not_an_emoji:` and no custom emoji has that name
- **THEN** it is drawn as written

#### Scenario: Unicode shortcode
- **WHEN** the draft holds `:tada:` and the server has no custom emoji of that name
- **THEN** it is drawn as written

#### Scenario: Inside code
- **WHEN** the draft holds a custom `:name:` between backticks
- **THEN** it is drawn as written

#### Scenario: Glued to a word
- **WHEN** the draft holds `a:rtshipit:`
- **THEN** it is drawn as written

#### Scenario: Terminal without images
- **WHEN** the terminal cannot show images
- **THEN** the draft is drawn as written, as before

#### Scenario: Failed image
- **WHEN** the emoji's image could not be loaded
- **THEN** it is drawn as written, as it is in a message

### Requirement: What counts as a custom emoji in the draft
A shortcode `:name:` in the draft SHALL count as a custom emoji when it counts by the rules of `message-emoji` ("Where a shortcode counts" and "Code is left alone") and its name is the name, or one of the aliases, of a custom emoji the server lists.

#### Scenario: Name rules
- **WHEN** the draft holds `(:rtshipit:)`
- **THEN** the shortcode counts and the parentheses stay as written

### Requirement: Becoming an image as it is typed
A shortcode typed by hand SHALL be drawn as the image from the moment its closing colon is typed, and SHALL go back to being drawn as text if a character of it is deleted.

#### Scenario: Closing colon
- **WHEN** the user types `:rtshipit` and then `:`
- **THEN** the bar draws the image in place of the text

#### Scenario: Before the closing colon
- **WHEN** the draft holds `:rtshipit` with no closing colon
- **THEN** it is drawn as written

#### Scenario: Back to text
- **WHEN** the user presses Backspace on the emoji and the draft is then retyped from `:rtshipi`
- **THEN** the draft is drawn as written until the shortcode is closed again

### Requirement: The draft's text does not change
Drawing SHALL NOT change the text of the draft. A message sent, a message saved after an edit and a draft kept for a room or a thread SHALL carry the text as it was typed or picked, with each shortcode as written, including an alias as the alias.

#### Scenario: Send
- **WHEN** the draft shows an emoji drawn from `:rtshipit:` and the user sends it
- **THEN** the message text is `:rtshipit:` as written

#### Scenario: Alias is kept
- **WHEN** the draft holds `:rtship:` drawn as the image of `rtshipit` and the user sends it
- **THEN** the message text is `:rtship:`

#### Scenario: Edit loads what was typed
- **WHEN** the user edits their message that was sent as `nice :rtshipit:`
- **THEN** the input bar holds `nice :rtshipit:` and draws the emoji as an image

#### Scenario: Draft kept per room
- **WHEN** the user leaves a room with an emoji in the draft and comes back
- **THEN** the draft holds the same text and draws the image

### Requirement: A drawn emoji is one unit for the cursor
While a custom emoji is drawn as an image, the cursor SHALL NOT stand inside it. Moving left or right, the motions of Normal mode in the input bar, Backspace and deleting a word SHALL pass over it or delete it whole, so a single Backspace removes the whole `:name:`.

#### Scenario: Move over it
- **WHEN** the cursor is after a drawn emoji and the user moves left once
- **THEN** the cursor is before it

#### Scenario: Backspace
- **WHEN** the cursor is after a drawn emoji and the user presses Backspace
- **THEN** the whole shortcode is deleted

#### Scenario: Up and down keep the column
- **WHEN** the user moves to another line of the draft
- **THEN** the cursor lands at the nearest column that is not inside a drawn emoji

#### Scenario: Drawn as text
- **WHEN** the emoji is drawn as written, on a terminal without images
- **THEN** the cursor moves through its characters one at a time, as in any text

### Requirement: Layout follows the drawn width
The lines of the draft SHALL wrap by the width drawn, the cursor SHALL be placed by the width drawn, and the scroll and the height of the bar SHALL follow the same rows, so no row is wider than the bar and the hardware cursor is where the text ends. An emoji SHALL NOT be split across two rows.

#### Scenario: Wrap
- **WHEN** a draft holds custom emojis among words at a narrow bar
- **THEN** the lines wrap counting two columns per emoji and no row is wider than the bar

#### Scenario: Not split
- **WHEN** an emoji would start in the last column of a row
- **THEN** it is drawn at the start of the next row

#### Scenario: Cursor column
- **WHEN** the cursor is after `ab` and one drawn emoji
- **THEN** the hardware cursor is four columns from the start of the row

#### Scenario: Bar height
- **WHEN** drawing shorter text makes the draft fit in fewer rows
- **THEN** the bar's height is that of the drawn rows

### Requirement: Images of the draft are requested
The custom emojis written in the draft of the live bar SHALL have their images requested like those of the messages on screen, once per session for each, so an emoji picked or typed becomes an image without the user having seen it in a message first.

#### Scenario: Picked from the box
- **WHEN** the user picks a custom emoji whose image was never asked for
- **THEN** its image is requested and replaces the stand-in when it arrives

#### Scenario: Already loaded
- **WHEN** the emoji's image was loaded before
- **THEN** no request is made and the image is drawn at once
