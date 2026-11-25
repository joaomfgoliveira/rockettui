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

### Requirement: Current reactions with nothing typed
With an empty search, the picker SHALL list the reactions the message already has, in the order of its reaction row, each with its glyph, its `:shortcode:` and its count, and the ones the user gave marked as the user's own. A message without reactions SHALL show a hint to press `/` to search instead.

#### Scenario: Message with reactions
- **WHEN** the picker opens on a message with 👍 from three users, one of them the user, and 🎉 from one other user
- **THEN** it lists 👍 with its count and the own marking, then 🎉 with its count

#### Scenario: Message without reactions
- **WHEN** the picker opens on a message with no reactions
- **THEN** it shows a hint to press `/` to search

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
While browsing, `j`, `k`, `<C-n>`, `<C-p>` and the Down and Up arrows SHALL move the selection through the list, `Enter` SHALL send the selected reaction, and `q` and `Esc` SHALL close the picker without sending. `/` SHALL start a new, empty search and `i` or `a` SHALL continue the current one. Every other key SHALL do nothing, and a paste SHALL be ignored. No key SHALL reach the keymap while the picker is open.

#### Scenario: Move and send
- **WHEN** the user presses `j` and then `Enter` on the reactions of a message
- **THEN** the second reaction is the one sent

#### Scenario: Close
- **WHEN** the user presses `q` or `Esc` while browsing
- **THEN** the picker closes and nothing is sent

#### Scenario: Other keys do nothing
- **WHEN** the user presses `h`, `x` or `:` while browsing
- **THEN** nothing happens and the message cursor and the mode do not change

#### Scenario: Stays in the list
- **WHEN** the user presses `k` on the first result or `j` on the last
- **THEN** the selection stays where it is

#### Scenario: Browse the results of a search
- **WHEN** the user searched for `tad`, pressed `Esc` and then `j`
- **THEN** the selection moves down the results of that search

### Requirement: Keys while searching
While searching, printable characters SHALL be typed into the search, so `j` and `k` are text, and a paste SHALL insert its text on one line. `<C-n>`, `<C-p>` and the Down and Up arrows SHALL move the selection and `Enter` SHALL send the selected reaction. `Backspace` SHALL delete a character, and on an empty search SHALL return to browsing. Changing the search SHALL select the first result. No key SHALL reach the keymap.

#### Scenario: Letters are text
- **WHEN** the user is searching and types `j`
- **THEN** `j` is added to the search and the message cursor does not move

#### Scenario: Move and send
- **WHEN** the user searches for `tad`, presses `<C-n>` and then `Enter`
- **THEN** the second result is the one sent

#### Scenario: Backspace on an empty search
- **WHEN** the search is empty and the user presses `Backspace`
- **THEN** the picker is browsing again and is still open

#### Scenario: Selection resets
- **WHEN** the selection is on the third result and the user types another character
- **THEN** the selection is on the first result of the new list

#### Scenario: Paste
- **WHEN** the user pastes text with control characters while searching
- **THEN** the search holds it on one line with them removed

### Requirement: The hint says what Enter will do
While the picker is open and the pane is wide enough, its bottom border SHALL hint at the keys of the current state and SHALL say what `Enter` does with the selected result: it adds the reaction, or removes it when the user already gave it on the message. The word SHALL change as the selection moves or the search changes, and the hint SHALL be left out when it does not fit.

#### Scenario: Selected reaction is someone else's
- **WHEN** the selected result is a reaction the user has not given on the message
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
The picker SHALL fetch the server's custom emojis once, the first time it opens, and keep them for the session. A failed fetch SHALL NOT stop the picker: it searches unicode emoji and aliases, and the next opening SHALL try the fetch again. A custom emoji SHALL be offered under its name and its aliases and sent under its name.

#### Scenario: First opening
- **WHEN** the picker opens for the first time in a session
- **THEN** the custom emojis are requested and become searchable when they arrive, without closing the picker

#### Scenario: Later openings
- **WHEN** the picker opens again after a successful fetch
- **THEN** no new request is made

#### Scenario: Fetch fails
- **WHEN** the fetch fails
- **THEN** unicode emoji and aliases still search, and the next opening requests the custom emojis again

### Requirement: Picker follows live changes
The open picker SHALL show the message's current reactions as they change, and SHALL close if the message is deleted or is no longer held. The room underneath SHALL keep updating while it is open.

#### Scenario: Reaction arrives while open
- **WHEN** the picker is open with an empty search and another user reacts to the message
- **THEN** the list shows the new reaction without any key

#### Scenario: Message deleted
- **WHEN** the message is deleted while the picker is open
- **THEN** the picker closes

### Requirement: Picker fits the screen
The picker SHALL be drawn over the bottom of the messages and input area at one steady height for as long as it is open, as tall as the screen allows up to a maximum, so the search line stays where it is while results come and go. It SHALL be usable at the smallest supported size. A list longer than the space SHALL scroll to keep the selection visible, and a row wider than the space SHALL be cut with an ellipsis.

#### Scenario: Steady height
- **WHEN** the user types and the number of results changes from a few to many
- **THEN** the picker keeps its size and the search line does not move

#### Scenario: Long result list
- **WHEN** a search has more results than fit
- **THEN** the selection stays visible as the user moves through the list

#### Scenario: Small terminal
- **WHEN** the terminal is at the smallest supported size and the picker is open
- **THEN** the picker is drawn inside the screen and shows its title and the search line

### Requirement: Picker text is sanitized and never logged
Search text, shortcodes and custom emoji names shown by the picker SHALL be sanitized for terminal output. None of them SHALL reach a log, an error text or a status message.

#### Scenario: Hostile custom emoji name
- **WHEN** a custom emoji name from the server contains terminal control characters
- **THEN** they are not passed to the terminal

#### Scenario: Pasted control characters
- **WHEN** the user pastes text with control characters into the search
- **THEN** the search holds it on one line with them removed
