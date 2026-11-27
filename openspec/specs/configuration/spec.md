# configuration Specification

## Purpose

Defines how rockettui loads and validates its configuration files and which user settings `config.toml` offers.

## Requirements

### Requirement: Config location and defaults
The app SHALL read `config.toml`, `keymap.toml` and `theme.toml` from `~/.config/rockettui/`, honoring `$XDG_CONFIG_HOME`. Any or all files MAY be absent, and the app SHALL then use built-in defaults.

#### Scenario: No config files
- **WHEN** the app starts with no config files present
- **THEN** it starts normally with defaults

### Requirement: Invalid config refuses to start
If a present config file is invalid, the app SHALL NOT start normally. It SHALL show an in-TUI error screen listing every problem found across all three files (file, line and column when available, key, message), and any key press SHALL quit with exit code 1, and the full report SHALL also be printed to stderr.

#### Scenario: Several mistakes at once
- **WHEN** `config.toml` has an out-of-range value and `theme.toml` has a bad color
- **THEN** one error screen lists both and a keypress exits non-zero

### Requirement: Unknown keys are tolerated
Unknown keys and sections SHALL be ignored on a normal start, and listed as warnings on the error screen only when a real error is also present. Connection and permission warnings are a separate channel, printed to stderr on exit.

#### Scenario: Extra key
- **WHEN** `config.toml` contains an unknown key and nothing else is wrong
- **THEN** the app starts normally

### Requirement: Display settings
`config.toml` SHALL support `keymap_preset` (`vim` or `vim-extras`), `sidebar_width` (10-60), `history_size` (1-1000, default 50), `timestamp_format` (strftime-style, default `%H:%M`), `message_layout` (`compact` by default, or `two_line`), `key_timeout_ms` (100-5000, default 1000) and `mark_read_delay_ms` (0-60000, default 3000; `0` marks an open room read at once) and `message_grouping_seconds` (0-3600, default 180; `0` turns message grouping off, see `message-grouping`). Out-of-range or invalid values SHALL be config errors.

#### Scenario: Sidebar width
- **WHEN** the user sets `sidebar_width = 40`
- **THEN** the sidebar is 40 columns wide

#### Scenario: Invalid width
- **WHEN** the user sets `sidebar_width = 5`
- **THEN** the error screen reports the value and the allowed range

#### Scenario: Grouping gap
- **WHEN** the user sets `message_grouping_seconds = 120`
- **THEN** messages of one author more than 2 minutes apart have their own headers

#### Scenario: Invalid grouping gap
- **WHEN** the user sets `message_grouping_seconds = 100000`
- **THEN** the error screen reports the value and the allowed range

#### Scenario: Mark read delay
- **WHEN** the user sets `mark_read_delay_ms = 5000`
- **THEN** an open unread room is marked read after 5 seconds

#### Scenario: Invalid mark read delay
- **WHEN** the user sets `mark_read_delay_ms = 100000`
- **THEN** the error screen reports the value and the allowed range

### Requirement: Timestamps in local time
The app SHALL render message timestamps in the system local time using the configured format. A message header SHALL show only its time; a message that continues a group (see `message-grouping`) has no header and so shows no time of its own. A divider line with the date as `YYYY-MM-DD` in the local time zone SHALL be drawn above the first message of each day, including the first message of the list, wherever the messages pane or the results list shows messages.

#### Scenario: Grouped message
- **WHEN** a message continues the one before it
- **THEN** it shows no time and the group's header shows the time of its first message

#### Scenario: Custom format
- **WHEN** `timestamp_format = "%H:%M:%S"`
- **THEN** message rows show hours, minutes and seconds

#### Scenario: Divider for the first message
- **WHEN** a room's messages are shown
- **THEN** a divider with the date of the first message is above it

#### Scenario: Divider when the day changes
- **WHEN** a message written on 2026-11-24 follows one written on 2026-11-23
- **THEN** a `2026-11-24` divider is drawn between them

#### Scenario: No divider within a day
- **WHEN** two neighbouring messages were written on the same local day
- **THEN** no divider is drawn between them

#### Scenario: Cursor skips dividers
- **WHEN** the user moves the message cursor over a day change
- **THEN** it goes from message to message and the divider is not highlighted

### Requirement: Follow on send
`config.toml` SHALL support `follow_on_send` accepting `always` (default; jump to the newest message after sending) or `if_at_bottom` (follow only if the newest row was already visible before the send). Other values SHALL be config errors.

#### Scenario: Default
- **WHEN** the user sends a message while scrolled up and the setting is default
- **THEN** the messages pane jumps to the newest message

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

### Requirement: Inline image settings
`config.toml` SHALL support `inline_images` (boolean, default `true`) and `image_max_rows` (3-40, default 12). With `inline_images = false` no attachment image is fetched or drawn inline and `[attachment: name]` is shown as without the feature. `image_max_rows` is the tallest an inline image may be, in rows. Invalid or out-of-range values SHALL be config errors.

#### Scenario: Turned off
- **WHEN** the user sets `inline_images = false`
- **THEN** a room with image attachments shows only their captions and requests no image

#### Scenario: Taller boxes
- **WHEN** the user sets `image_max_rows = 20`
- **THEN** an image that needs more than 12 rows may take up to 20

#### Scenario: Invalid rows
- **WHEN** the user sets `image_max_rows = 100`
- **THEN** the error screen reports the value and the allowed range

### Requirement: Animated image setting
`config.toml` SHALL support `animate_images` (boolean, default `true`). With `animate_images = false` an animated image attachment is shown as its first frame, inline and in the full-size view. A value that is not a boolean SHALL be a config error.

#### Scenario: Turned off
- **WHEN** the user sets `animate_images = false`
- **THEN** a gif shows its first frame and no redraw timer runs for it

#### Scenario: Invalid value
- **WHEN** the user sets `animate_images = "no"`
- **THEN** the error screen reports the key and that true or false is expected
