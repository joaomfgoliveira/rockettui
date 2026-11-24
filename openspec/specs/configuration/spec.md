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
`config.toml` SHALL support `keymap_preset` (`vim` or `vim-extras`), `sidebar_width` (10-60), `history_size` (1-1000, default 50), `timestamp_format` (strftime-style, default `%H:%M`), `message_layout` (`compact` by default, or `two_line`) and `key_timeout_ms` (100-5000, default 1000). Out-of-range or invalid values SHALL be config errors.

#### Scenario: Sidebar width
- **WHEN** the user sets `sidebar_width = 40`
- **THEN** the sidebar is 40 columns wide

#### Scenario: Invalid width
- **WHEN** the user sets `sidebar_width = 5`
- **THEN** the error screen reports the value and the allowed range

### Requirement: Timestamps in local time
The app SHALL render message timestamps in the system local time using the configured format. A message row SHALL show only its time. A divider line with the date as `YYYY-MM-DD` in the local time zone SHALL be drawn above the first message of each day, including the first message of the list, wherever the messages pane or the results list shows messages.

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
