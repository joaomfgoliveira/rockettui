# Spec Delta

## Purpose

Remembers which emoji the user picks most and which skin tone they chose, so the emoji box can offer a "Frequently used" section and apply the tone, and keeps both between sessions without ever holding message text.

## ADDED Requirements

### Requirement: Frequently used emoji
The client SHALL count each emoji the user picks, by the name it is sent or inserted under, and SHALL offer the most picked ones as the "Frequently used" section, most picked first and ties broken by the most recent pick. The section SHALL hold at most 16 emoji. A name that is not offered any longer, such as a custom emoji the server dropped, SHALL be left out of the section without being forgotten.

#### Scenario: First pick
- **WHEN** the user has never picked an emoji and picks `:tada:`
- **THEN** the next time the box opens, "Frequently used" holds 🎉

#### Scenario: Most picked first
- **WHEN** the user picked `:tada:` three times and `:+1:` once
- **THEN** "Frequently used" lists 🎉 before 👍

#### Scenario: Tie
- **WHEN** two emoji were each picked twice and one of them was picked last
- **THEN** the one picked last comes first

#### Scenario: At most sixteen
- **WHEN** the user has picked twenty different emoji
- **THEN** "Frequently used" holds sixteen

#### Scenario: Name no longer offered
- **WHEN** a custom emoji the user picked is no longer in the server's list
- **THEN** it is not in "Frequently used", and it comes back if the server lists it again

#### Scenario: Nothing picked yet
- **WHEN** the user has no picks
- **THEN** the box shows no "Frequently used" section and no tab for it

### Requirement: What counts as a pick
Sending a reaction SHALL count as a pick of that emoji. Taking a reaction back SHALL NOT. A pick SHALL be counted when the choice is made, whether or not the server later refuses it.

#### Scenario: Reaction added
- **WHEN** the user adds `:tada:` to a message
- **THEN** `tada` is counted once

#### Scenario: Reaction taken back
- **WHEN** the user takes back a reaction
- **THEN** no count changes

### Requirement: Skin tone choice
The client SHALL keep one skin tone choice: none or one of the five tones. It SHALL start as none, SHALL be changed from the emoji box, and SHALL apply to every emoji that has tones wherever the box offers it or sends it.

#### Scenario: Default
- **WHEN** the user has never chosen a tone
- **THEN** emoji with tones are offered in their default look

#### Scenario: Chosen tone is kept
- **WHEN** the user chooses the third tone and opens the box again, in this session or a later one
- **THEN** emoji with tones are offered with the third tone

### Requirement: State kept between sessions
The picks and the skin tone SHALL be kept in a file named `emoji-recents` under `$XDG_STATE_HOME/rockettui/` when that is an absolute path, else `$HOME/.local/state/rockettui/`. The file SHALL hold only emoji names, counts and the tone, never message text, and SHALL be created readable by its owner only. It SHALL be read once when the session starts and written when a pick or a tone change is made.

#### Scenario: Survives a restart
- **WHEN** the user picks `:tada:` and starts the client again
- **THEN** "Frequently used" holds 🎉

#### Scenario: XDG path
- **WHEN** `$XDG_STATE_HOME` is an absolute path
- **THEN** the file is read and written under `$XDG_STATE_HOME/rockettui/`

#### Scenario: Relative XDG path
- **WHEN** `$XDG_STATE_HOME` is empty or relative
- **THEN** it is ignored and `$HOME/.local/state/rockettui/` is used

#### Scenario: Owner only
- **WHEN** the client creates the file
- **THEN** only its owner can read or write it

### Requirement: A bad or unwritable file is not an error
A missing, unreadable, oversized or malformed file SHALL read as no picks and no tone, without a status message that blocks the user and without stopping the client. Lines that cannot be understood SHALL be skipped and the rest used. A file that cannot be written SHALL leave the picks of the session working in memory. Neither SHALL log or show any name from the file.

#### Scenario: Missing file
- **WHEN** the file does not exist
- **THEN** the box works with no "Frequently used" and the default tone

#### Scenario: Malformed line
- **WHEN** one line of the file is not a name and a count
- **THEN** that line is skipped and the others are used

#### Scenario: Cannot write
- **WHEN** the state directory cannot be created or the file cannot be written
- **THEN** picks still show in "Frequently used" for the rest of the session and the client keeps running

#### Scenario: Oversized file
- **WHEN** the file is larger than 64 KiB
- **THEN** it is not read and reads as empty
