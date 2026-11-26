# Spec Delta

## ADDED Requirements

### Requirement: Thread unread style
`theme.toml` SHALL offer the elements `[sidebar.thread_badge]` for the thread badge, `[sidebar.thread_badge_mention]` for a thread badge with a mention, and `[messages.thread_unread]` for the marker row, thread list line and border label of unread threads, each with optional `fg`, `bg` and `modifiers`. The plain badge SHALL default to the style of the room's row and the unread style SHALL default to the red of the unread divider. The mention variant SHALL stand out from the plain badge.

#### Scenario: Themed badge
- **WHEN** `theme.toml` sets `fg = "magenta"` on `sidebar.thread_badge`
- **THEN** the thread badge is drawn in that color

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention these elements
- **THEN** the mention variant is distinguishable from the plain badge

#### Scenario: Unknown element
- **WHEN** `theme.toml` names `sidebar.thread_badges`
- **THEN** the config error screen reports the unknown element
