# Spec Delta

## ADDED Requirements

### Requirement: Thread style
`theme.toml` SHALL offer the elements `[messages.thread]` for the thread marker row, `[messages.thread_reply_hint]` for the `[thread]` tag at the start of a channel-visible reply, and `[thread_pane.border]` for the border of the thread pane, each with the options the other elements of its kind have. The built-in defaults SHALL keep the rows readable and distinguishable from the message body, and SHALL show which pane has focus the same way the other panes do.

#### Scenario: Themed marker
- **WHEN** `theme.toml` sets `fg = "cyan"` on `messages.thread`
- **THEN** the thread marker row is drawn in that color

#### Scenario: Defaults
- **WHEN** `theme.toml` does not mention threads
- **THEN** the marker row is readable and the focused pane's border differs from an unfocused one

#### Scenario: Unknown element
- **WHEN** `theme.toml` names `messages.threads`
- **THEN** the config error screen reports the unknown element
