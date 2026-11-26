# emoji-insert Specification

## Purpose

Lets the user put an emoji in a draft from the keyboard: typing a colon in Insert mode opens the emoji box already searching, and choosing an emoji writes it into the draft where the colon was.

## Requirements

### Requirement: A colon opens the emoji box
Typing `:` in Insert mode SHALL put the colon in the draft and open the emoji box above the input bar, in the searching state with an empty search, when the colon could start a shortcode. The box SHALL work in the room's input bar, in a thread's input bar and while editing a message. A paste SHALL NOT open it.

#### Scenario: Colon opens the box
- **WHEN** the user is in Insert mode and types `:` at the start of the draft or after a space
- **THEN** the draft holds `:`, the box is open above the input bar and the cursor is on its search line

#### Scenario: Thread and edit
- **WHEN** the user types `:` in a thread's input bar or while editing a message
- **THEN** the box opens above that bar

#### Scenario: Paste
- **WHEN** the user pastes text that holds a colon
- **THEN** the box does not open

### Requirement: Colons that do not open the box
A colon SHALL NOT open the box when the character before it is a letter or digit, when it is another colon, or when the cursor is inside inline code or a fenced block (an odd number of backticks before it).

#### Scenario: Time
- **WHEN** the user types `10:30`
- **THEN** the box does not open

#### Scenario: Path
- **WHEN** the user types `std::fs`
- **THEN** the box does not open on either colon

#### Scenario: Code
- **WHEN** the user types a colon after an opening backtick that is not closed yet
- **THEN** the box does not open

#### Scenario: After punctuation
- **WHEN** the user types `(:` or `,:`
- **THEN** the box opens

### Requirement: What is typed goes to the search
While the box is open from a colon, letters, digits, `_`, `+` and `-` SHALL be added to its search and not to the draft, so `j` and `k` are text, and the results SHALL follow at once. `Backspace` SHALL delete a character of the search. `<C-n>`, `<C-p>`, the arrows, `<Tab>`, `<S-Tab>` and `<C-t>` SHALL do what they do in the box anywhere else. No key SHALL reach the keymap.

#### Scenario: Typing searches
- **WHEN** the user types `:` and then `tad`
- **THEN** the search holds `tad`, the results include 🎉 and the draft holds only `:`

#### Scenario: Move and pick
- **WHEN** the user types `:tad`, presses the right arrow and then `Enter`
- **THEN** the second result is put in the draft

#### Scenario: Empty search
- **WHEN** the user types `:` and presses `Enter` at once
- **THEN** the selected emoji of the grid, the first one, is put in the draft

### Requirement: Choosing puts the emoji in the draft
`Enter` SHALL replace the typed `:` with the selected emoji and close the box, leaving the user in Insert mode with the cursor after it. A unicode emoji SHALL be written as the character itself, with the skin tone in use, an alias as the text it is configured to show, and a custom emoji as `:name:`. When nothing matches, `Enter` SHALL close the box and change nothing else.

#### Scenario: Unicode
- **WHEN** the draft is `ship it :` and the user picks 🎉
- **THEN** the draft is `ship it 🎉` and the cursor is after it

#### Scenario: Skin tone
- **WHEN** the tone in use is the third and the user picks 👍
- **THEN** the draft holds the thumbs-up with the third tone

#### Scenario: Alias
- **WHEN** `emoji_aliases` maps `partyparrot` to 🦜 and the user picks it
- **THEN** the draft holds 🦜, which every reader sees

#### Scenario: Custom emoji
- **WHEN** the user picks the custom emoji `rtshipit`
- **THEN** the draft holds `:rtshipit:`

#### Scenario: In the middle of a draft
- **WHEN** the draft is `a : b` with the cursor after the colon and the user picks 🎉
- **THEN** the draft is `a 🎉 b`

#### Scenario: No match
- **WHEN** the search matches nothing and the user presses `Enter`
- **THEN** the box closes and the draft keeps its `:`

### Requirement: Every emoji is offered
The box opened from a colon SHALL offer the unicode emoji the server refuses as a reaction as well, because a character in a message needs no server name. It SHALL NOT list "On this message", because there is no message, and its hint SHALL say that `Enter` inserts.

#### Scenario: Not a reaction
- **WHEN** the user searches for an emoji the server refuses as a reaction
- **THEN** it is a result and can be put in the draft

#### Scenario: No message section
- **WHEN** the box opens from a colon
- **THEN** the first section is "Frequently used" or "Smileys & People"

### Requirement: A character that is not part of a name falls through
A typed character that cannot be part of a shortcode (a space, punctuation, a second `:`), other than the keys above, SHALL close the box and be put in the draft after what had been typed in the search, as if the box had never opened.

#### Scenario: Smiley
- **WHEN** the user types `:` and then `)`
- **THEN** the box closes and the draft holds `:)`

#### Scenario: Word after a colon
- **WHEN** the user types `:` then `tada` and then a space
- **THEN** the box closes and the draft holds `:tada `

#### Scenario: Hand-typed shortcode
- **WHEN** the user types `:tada:` without choosing
- **THEN** the box is closed at the second colon and the draft holds `:tada:`, which is drawn as an emoji in the message

#### Scenario: Paste
- **WHEN** the user pastes text with a space while the box is open
- **THEN** the box closes and the draft receives what was typed in the search followed by the pasted text

### Requirement: Leaving the box without choosing
`Esc` and `<C-c>` SHALL close the box, leave the colon in the draft and keep the user in Insert mode; what was typed in the search SHALL NOT go to the draft. `Backspace` on an empty search SHALL close the box and delete the colon. The box opened from a colon SHALL have no browsing state.

#### Scenario: Escape
- **WHEN** the user types `:tad` and presses `Esc`
- **THEN** the box closes, the draft holds `:` and the mode is still Insert

#### Scenario: Backspace takes the colon back
- **WHEN** the user types `:` and presses `Backspace`
- **THEN** the box closes and the draft is as it was before the colon

#### Scenario: Backspace in a search
- **WHEN** the user types `:ta` and presses `Backspace`
- **THEN** the search holds `t` and the box stays open

### Requirement: The box does not hide the draft
The box opened from a colon SHALL be drawn above the input bar it belongs to, ending at the bar's top edge, so the draft stays visible. It SHALL keep one steady height while open and SHALL be usable at the smallest supported size.

#### Scenario: Draft visible
- **WHEN** the box is open from a colon
- **THEN** the input bar and its draft are drawn below it

#### Scenario: Small terminal
- **WHEN** the terminal is at the smallest supported size
- **THEN** the box is drawn inside the screen and shows its title and the search line

### Requirement: Picks count
An emoji put in the draft SHALL count as a pick of that emoji for "Frequently used", under the name it would be sent under as a reaction when it has one, and under its `:name:` when it is custom.

#### Scenario: Insert counts
- **WHEN** the user puts 🎉 in a draft
- **THEN** 🎉 is one more pick in "Frequently used"

#### Scenario: Escape does not count
- **WHEN** the user closes the box without choosing
- **THEN** no count changes
