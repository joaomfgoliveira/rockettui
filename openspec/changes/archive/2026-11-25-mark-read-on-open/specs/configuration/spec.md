# Spec Delta

## MODIFIED Requirements

### Requirement: Display settings
`config.toml` SHALL support `keymap_preset` (`vim` or `vim-extras`), `sidebar_width` (10-60), `history_size` (1-1000, default 50), `timestamp_format` (strftime-style, default `%H:%M`), `message_layout` (`compact` by default, or `two_line`), `key_timeout_ms` (100-5000, default 1000) and `mark_read_delay_ms` (0-60000, default 3000; `0` marks an open room read at once). Out-of-range or invalid values SHALL be config errors.

#### Scenario: Sidebar width
- **WHEN** the user sets `sidebar_width = 40`
- **THEN** the sidebar is 40 columns wide

#### Scenario: Invalid width
- **WHEN** the user sets `sidebar_width = 5`
- **THEN** the error screen reports the value and the allowed range

#### Scenario: Mark read delay
- **WHEN** the user sets `mark_read_delay_ms = 5000`
- **THEN** an open unread room is marked read after 5 seconds

#### Scenario: Invalid mark read delay
- **WHEN** the user sets `mark_read_delay_ms = 100000`
- **THEN** the error screen reports the value and the allowed range
