# reaction-picker Specification

## Purpose

Lets the user react to a message from the keyboard: a popup with a search line that finds unicode emoji, the user's aliases and the server's custom emojis, and sends or takes back the chosen reaction.

## Requirements

### Requirement: Opening the picker
`react_message` SHALL open the reaction picker for the message under the cursor when the messages pane is focused on a message the server holds that is neither a deleted line nor a system message. Opening it SHALL NOT change the mode, the focused pane or the selection. On any other pane, on a pending or failed local message, a deleted line or a system message it SHALL do nothing.

#### Scenario: Open
- **WHEN** the cursor is on another user's message and the user presses `+`
- **THEN** the picker opens, the mode badge still reads Normal and the cursor has not moved

#### Scenario: Own message
- **WHEN** the cursor is on one of the user's own server messages and the user presses `+`
- **THEN** the picker opens

#### Scenario: Opens browsing
- **WHEN** the user presses `+` on a message
- **THEN** the picker is in the browsing state, so the next key moves or chooses and is not text

#### Scenario: Message that cannot be reacted to
- **WHEN** the cursor is on a deleted line, a system message or a message still being sent, and the user presses `+`
- **THEN** nothing happens

#### Scenario: Other focus
- **WHEN** the sidebar or the input bar is focused and the user presses `+`
- **THEN** nothing happens

### Requirement: Refusals before opening
The picker SHALL NOT open in a detached room, and SHALL NOT open while the user's own username is not known yet, because whether a reaction is added or taken back depends on it. Each refusal SHALL show a status message that says why.

#### Scenario: Detached room
- **WHEN** the room shows older messages and the user presses `+`
- **THEN** the picker does not open and the status line says the room is showing older messages

#### Scenario: Username not known
- **WHEN** the own username has not arrived yet and the user presses `+`
- **THEN** the picker does not open and the status line says the username is not loaded yet

### Requirement: Searching the emoji
While searching, the list SHALL follow the typed text at once. The search SHALL look through unicode emoji by name and by every shortcode, the user's `[emoji_aliases]`, and the server's custom emojis, matching the typed text anywhere in a name case-insensitively, ignoring a leading or trailing colon. Results SHALL be ranked exact match first, then names that start with the text, then the rest, each group in a stable order.

#### Scenario: Unicode by shortcode
- **WHEN** the user searches for `tad`
- **THEN** the results include 🎉 `:tada:` and the exact and prefix matches come before the others

#### Scenario: Colons typed
- **WHEN** the user searches for `:tada:`
- **THEN** the results are those of `tada`

#### Scenario: Alias
- **WHEN** `emoji_aliases` maps `partyparrot` to 🦜 and the user searches for `parrot`
- **THEN** `:partyparrot:` is a result, shown with its alias glyph

#### Scenario: Custom emoji
- **WHEN** the server has a custom emoji `ship_it` and the user searches for `ship`
- **THEN** `:ship_it:` is a result, drawn as its name

#### Scenario: No match
- **WHEN** the typed text matches nothing
- **THEN** the picker says there are no matches and Enter sends nothing

### Requirement: Browsing and searching
The picker SHALL have two states, as a vim buffer has. It SHALL open in the browsing state, where keys move and choose and nothing is typed. `/` and `i` SHALL start searching, and `Esc` or `Backspace` on an empty search SHALL return to browsing. `<C-c>` SHALL close the picker from either state. The state SHALL be visible: only while searching SHALL the search line show the hardware cursor and be drawn as live input, and the hint SHALL name the keys of the current state.

#### Scenario: Opens browsing
- **WHEN** the picker opens
- **THEN** it is browsing: the message's reactions are listed and typed letters are not text

#### Scenario: Start searching
- **WHEN** the user presses `/` while browsing
- **THEN** the picker is searching with an empty search and the cursor is on the search line

#### Scenario: Continue a search
- **WHEN** the user left a search with `Esc` and presses `i`
- **THEN** the picker is searching again with the same text

#### Scenario: Leave searching
- **WHEN** the user presses `Esc` while searching
- **THEN** the picker is browsing, the search text and its results are kept, and nothing is sent

### Requirement: Keys while browsing
While browsing, `h`, `j`, `k`, `l`, the arrows, `<C-n>` and `<C-p>` SHALL move the selection through the grid (`<C-n>` and `<C-p>` as down and up), `<Tab>` and `<S-Tab>`, and `H` and `L`, SHALL jump to the previous and next section, `Enter` SHALL send the selected reaction, and `q` and `Esc` SHALL close the box without sending. `/` SHALL start a new, empty search and `i` or `a` SHALL continue the current one. Every other key SHALL do nothing, and a paste SHALL be ignored. No key SHALL reach the keymap while the box is open.

#### Scenario: Move and send
- **WHEN** the user presses `l` and then `Enter` on the reactions of a message
- **THEN** the second reaction is the one sent

#### Scenario: Close
- **WHEN** the user presses `q` or `Esc` while browsing
- **THEN** the box closes and nothing is sent

#### Scenario: Other keys do nothing
- **WHEN** the user presses `x` or `:` while browsing
- **THEN** nothing happens and the message cursor and the mode do not change

#### Scenario: Stays in the list
- **WHEN** the user presses `k` on the first row or `l` on the last emoji
- **THEN** the selection stays where it is

#### Scenario: Browse the results of a search
- **WHEN** the user searched for `tad`, pressed `Esc` and then `l`
- **THEN** the selection moves to the next result of that search

### Requirement: Keys while searching
While searching, printable characters SHALL be typed into the search, so `h`, `j`, `k` and `l` are text, and a paste SHALL insert its text on one line. `<C-n>`, `<C-p>` and the arrows SHALL move the selection through the grid, `<Tab>` and `<S-Tab>` SHALL jump between sections, `<C-t>` SHALL cycle the skin tone, and `Enter` SHALL send the selected reaction. `Backspace` SHALL delete a character, and on an empty search SHALL return to browsing. Changing the search SHALL select the first result. No key SHALL reach the keymap.

#### Scenario: Letters are text
- **WHEN** the user is searching and types `j`
- **THEN** `j` is added to the search and the message cursor does not move

#### Scenario: Move and send
- **WHEN** the user searches for `tad`, presses the right arrow and then `Enter`
- **THEN** the second result is the one sent

#### Scenario: Backspace on an empty search
- **WHEN** the search is empty and the user presses `Backspace`
- **THEN** the box is browsing again and is still open

#### Scenario: Selection resets
- **WHEN** the selection is on the third result and the user types another character
- **THEN** the selection is on the first result of the new grid

#### Scenario: Paste
- **WHEN** the user pastes text with control characters while searching
- **THEN** the search holds it on one line with them removed

### Requirement: The hint says what Enter will do
While the box is open and the pane is wide enough, its bottom border SHALL hint at the keys of the current state and SHALL say what `Enter` does with the selected emoji: it adds the reaction, or removes it when the user already gave it on the message. The word SHALL change as the selection moves or the search changes, and the hint SHALL be left out when it does not fit.

#### Scenario: Selected reaction is someone else's
- **WHEN** the selected emoji is a reaction the user has not given on the message
- **THEN** the hint says that Enter adds

#### Scenario: Selected reaction is the user's own
- **WHEN** the user moves the selection to a reaction they already gave
- **THEN** the hint says that Enter removes

#### Scenario: Hint follows the state
- **WHEN** the user starts searching and later leaves the search
- **THEN** the hint names the searching keys while searching and the browsing keys while browsing

#### Scenario: Narrow pane
- **WHEN** the pane is too narrow for the hint next to the title
- **THEN** the hint is not drawn

### Requirement: Sending or taking back a reaction
`Enter` on a reaction SHALL send it for the message, adding it when the user is not among those who gave it on that message and taking it back when the user is. The picker SHALL close on `Enter`. The row and the reactions popup SHALL change from the message the server sends back, not from a local guess.

#### Scenario: Add
- **WHEN** the user selects `:tada:` on a message where they did not give it and presses `Enter`
- **THEN** a reaction is added for the user, the picker closes, and the row shows it once the server confirms

#### Scenario: Take back
- **WHEN** the user selects a reaction they already gave and presses `Enter`
- **THEN** their reaction is taken back and the row stops counting them once the server confirms

#### Scenario: Toggle from the empty list
- **WHEN** the search is empty, the user selects one of the message's own reactions and presses `Enter`
- **THEN** that reaction is taken back

### Requirement: Refused or failed reaction
A reaction the server refuses, or that cannot be sent, SHALL show one fixed status line that names the failure class and never the server's text, and SHALL leave the message as it was. The status line SHALL NOT hold the search text, the shortcode or any reaction name.

#### Scenario: Server refuses the name
- **WHEN** the server answers that the emoji is not valid
- **THEN** the status line says the reaction was not sent and the row is unchanged

#### Scenario: Network failure
- **WHEN** the request cannot reach the server
- **THEN** the status line says the reaction may not have been sent

### Requirement: Custom emojis from the server
The picker SHALL search the server's custom emojis from the list the client loads when the session starts and keeps for the session, and SHALL NOT request it again once it has loaded. When that list has not loaded, the picker SHALL work from unicode emoji and aliases and ask for the list again, and a custom emoji that arrives while the picker is open SHALL become searchable without closing it. A custom emoji SHALL be offered under its name and its aliases and sent under its name.

#### Scenario: First opening
- **WHEN** the picker opens for the first time in a session and the session's list has loaded
- **THEN** the custom emojis are searchable at once and no request is made by the picker

#### Scenario: Later openings
- **WHEN** the picker opens again after a successful load
- **THEN** no new request is made

#### Scenario: Fetch fails
- **WHEN** the session's list could not be loaded
- **THEN** unicode emoji and aliases still search, and opening the picker requests the list again

#### Scenario: List arrives while open
- **WHEN** the picker opens while the list is still loading and the list then arrives
- **THEN** the custom emojis become searchable without closing the picker

### Requirement: Picker follows live changes
The open picker SHALL show the message's current reactions as they change, and SHALL close if the message is deleted or is no longer held. The room underneath SHALL keep updating while it is open.

#### Scenario: Reaction arrives while open
- **WHEN** the picker is open with an empty search and another user reacts to the message
- **THEN** the list shows the new reaction without any key

#### Scenario: Message deleted
- **WHEN** the message is deleted while the picker is open
- **THEN** the picker closes

### Requirement: Picker fits the screen
The box SHALL be drawn over the bottom of the messages and input area at one steady height for as long as it is open, as tall as the screen allows up to a maximum, so the search line stays where it is while results come and go. Each emoji SHALL take a cell of three columns, two for the emoji and one blank, and a row SHALL hold as many cells as fit. When the height is short the box SHALL drop first the footer and then the tabs, keeping the search line and at least one row of the grid. It SHALL be usable at the smallest supported size. A grid taller than the space SHALL scroll to keep the selection visible.

#### Scenario: Steady height
- **WHEN** the user types and the number of results changes from a few to many
- **THEN** the box keeps its size and the search line does not move

#### Scenario: Long result list
- **WHEN** the selection moves down through the sections
- **THEN** the grid scrolls and the selection stays visible

#### Scenario: Cells fit the width
- **WHEN** the pane is 52 columns wide
- **THEN** a row holds as many three-column cells as fit inside the borders and no row is wider than the box

#### Scenario: Short screen
- **WHEN** the screen is too short for the footer and the tabs
- **THEN** the footer goes first, then the tabs, and the search line and one grid row remain

#### Scenario: Small terminal
- **WHEN** the terminal is at the smallest supported size and the box is open
- **THEN** the box is drawn inside the screen and shows its title and the search line

### Requirement: Picker text is sanitized and never logged
Search text, shortcodes and custom emoji names shown by the picker SHALL be sanitized for terminal output. None of them SHALL reach a log, an error text or a status message.

#### Scenario: Hostile custom emoji name
- **WHEN** a custom emoji name from the server contains terminal control characters
- **THEN** they are not passed to the terminal

#### Scenario: Pasted control characters
- **WHEN** the user pastes text with control characters into the search
- **THEN** the search holds it on one line with them removed

### Requirement: Sections with nothing typed
With an empty search the box SHALL list every emoji it offers as a grid in sections, in this order, leaving out a section that is empty: "On this message" (the reactions the message already has, in the order of its reaction row), "Frequently used", "Smileys & People", "Animals & Nature", "Food & Drink", "Travel & Places", "Activities", "Objects", "Symbols", "Flags" and "Custom" (the server's custom emojis). Each section SHALL start on a new row under its heading.

#### Scenario: Message with reactions
- **WHEN** the box opens on a message with 👍 and 🎉
- **THEN** the first section is "On this message" with 👍 and 🎉, followed by the other sections

#### Scenario: Message without reactions
- **WHEN** the box opens on a message with no reactions
- **THEN** there is no "On this message" section and the first section is "Frequently used", or "Smileys & People" when nothing was picked yet

#### Scenario: Every emoji is there
- **WHEN** the user scrolls through all the sections
- **THEN** every unicode emoji the server accepts as a reaction appears once in its section

#### Scenario: No custom emojis
- **WHEN** the server has no custom emojis, or the list has not loaded
- **THEN** there is no "Custom" section

#### Scenario: Own reaction marked
- **WHEN** the user gave one of the reactions listed in "On this message"
- **THEN** its cell is drawn in the user's own style

### Requirement: The first selection
When the box opens the selection SHALL be on the first emoji of the first section. Changing the search SHALL select the first result.

#### Scenario: Opens on the first reaction
- **WHEN** the box opens on a message with reactions
- **THEN** the selection is on its first reaction

#### Scenario: Opens on the first emoji
- **WHEN** the box opens on a message with no reactions and nothing was ever picked
- **THEN** the selection is on the first emoji of "Smileys & People"

### Requirement: Category tabs
Under the search line the box SHALL show one tab for each section that is not empty, in section order, each drawn as one emoji, and SHALL mark the tab of the section the selection is in. While a search has results, the tabs SHALL show none as current.

#### Scenario: Current section marked
- **WHEN** the selection moves from "Smileys & People" into "Animals & Nature"
- **THEN** the mark moves to the second tab

#### Scenario: Empty section has no tab
- **WHEN** nothing was picked yet
- **THEN** there is no tab for "Frequently used"

#### Scenario: Searching
- **WHEN** the user has typed a search that has results
- **THEN** no tab is marked as current

### Requirement: Moving in the grid
The selection SHALL move through the grid by cell and by row. Left and right SHALL move to the previous and next emoji in reading order, across row ends and from one section into the next. Up and down SHALL move one row in the same column, skipping section headings, and SHALL land on the last cell of a row that has fewer cells. Moving past the first or last emoji SHALL leave the selection where it is.

#### Scenario: Down in the same column
- **WHEN** the selection is on the third cell of a row and the user moves down
- **THEN** the selection is on the third cell of the next row

#### Scenario: Short last row
- **WHEN** the selection is in column eight and the row below has five cells
- **THEN** moving down selects the fifth cell of that row

#### Scenario: Across sections
- **WHEN** the selection is on the last emoji of a section and the user moves right
- **THEN** the selection is on the first emoji of the next section

#### Scenario: Edges
- **WHEN** the selection is on the first emoji and the user moves left or up
- **THEN** the selection stays where it is

### Requirement: Jumping between sections
`<Tab>` and `<S-Tab>` SHALL jump the selection to the first emoji of the next and the previous section, and the grid SHALL scroll to keep the selection and its section heading visible.

#### Scenario: Next section
- **WHEN** the user presses `<Tab>`
- **THEN** the selection is on the first emoji of the next section and the grid shows its heading

#### Scenario: Previous section
- **WHEN** the user presses `<S-Tab>` in the middle of a section
- **THEN** the selection is on the first emoji of the previous section

#### Scenario: Last section
- **WHEN** the selection is in the last section and the user presses `<Tab>`
- **THEN** the selection stays where it is

### Requirement: Footer names the selection
The bottom row inside the box SHALL show the selected emoji as `:name:`, sanitized, followed by how many users gave it on the message and `you` when the user did, and on the right the skin tone in use. It SHALL say there are no matches when a search has no results.

#### Scenario: Selected reaction
- **WHEN** the selection is on 🎉, given by two users including the user
- **THEN** the footer reads `:tada:` with the count 2 and `you`

#### Scenario: Plain emoji
- **WHEN** the selection is on an emoji nobody gave on the message
- **THEN** the footer shows its `:name:` and no count

#### Scenario: Custom emoji
- **WHEN** the selection is on a custom emoji
- **THEN** the footer shows its `:name:` and says it is custom

#### Scenario: No match
- **WHEN** a search matches nothing
- **THEN** the footer says there are no matches and Enter sends nothing

### Requirement: Skin tone in the box
`<C-t>` SHALL cycle the skin tone through none and the five tones, in both the browsing and the searching state, and SHALL apply the new tone at once to every emoji that has tones in the grid and in the results. An emoji with tones SHALL be sent under the name the server accepts for the tone in use. An emoji without tones SHALL NOT change.

#### Scenario: Cycle
- **WHEN** the user presses `<C-t>` while browsing
- **THEN** 👍 and the other emoji with tones are drawn with the next tone and the footer names it

#### Scenario: Works while searching
- **WHEN** the user presses `<C-t>` while searching
- **THEN** the tone changes and the typed text is untouched

#### Scenario: Sent with the tone
- **WHEN** the tone in use is the second and the user selects 👍 and presses `Enter`
- **THEN** the reaction sent is the thumbs-up with the second tone

#### Scenario: Emoji without tones
- **WHEN** a tone is in use
- **THEN** 🎉 is drawn and sent as it always is

### Requirement: Results in the grid
While the search holds text, the box SHALL show its results in the same grid under one "Results" section, in the order of the ranking of the search, with no other section. An empty search SHALL show the sections again.

#### Scenario: Results section
- **WHEN** the user searches for `tad`
- **THEN** the grid holds one "Results" section with 🎉 among the first cells

#### Scenario: Back to the sections
- **WHEN** the user deletes the whole search
- **THEN** the sections are shown again

### Requirement: Custom emojis in the grid
A custom emoji SHALL take one cell like any other emoji: its image in the two glyph columns, or two stand-in cells while the image is not ready, on a terminal that cannot show images, or when the image failed. The images of the custom emojis in the rows on screen SHALL be requested, once per session for each, and the ones that are not on screen SHALL NOT.

#### Scenario: Image in the cell
- **WHEN** the "Custom" section is on screen and an image is ready
- **THEN** its cell shows the image and the footer names it when selected

#### Scenario: Stand-in
- **WHEN** the image has not arrived yet, or the terminal cannot show images
- **THEN** its cell shows two stand-in cells and the footer still names it

#### Scenario: Only what is on screen
- **WHEN** the server has hundreds of custom emojis
- **THEN** only the images of the rows on screen are requested
