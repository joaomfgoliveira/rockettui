# Proposal

## Why

Someone who sends several messages in a row gets a full author and time header for every one of them, which makes a busy room tall and noisy to read. Rocket.Chat's own UI groups such messages under one header, using the admin setting `Message_GroupingPeriod` (in seconds; its default is believed to be 300, not confirmed). The terminal client should read the same way.

## What Changes

- In the two-line layout, a message that follows another from the same author within a configurable gap of the group's first message is drawn without its own header row: the author and time show once, and each message body sits under it, indented as before.
- A message after a longer gap, from another author, across a day change, or after an unread divider starts a new group with its own header and time.
- A new `config.toml` key `message_grouping_seconds` sets the gap (default 180; `0` turns grouping off; out of range or invalid values are config errors).
- Grouping applies to the room's message pane and to the thread pane. The compact layout, the search results list and the search-result context view are unchanged.
- The cursor, selection, edit, delete and reaction keys keep acting on one message at a time, grouped or not.

## Capabilities

### New Capabilities
- `message-grouping`: when consecutive messages share one header, what breaks a group, and where it applies.

### Modified Capabilities
- `configuration`: adds `message_grouping_seconds` to the display settings, and a row that belongs to a group has no time of its own.

## Impact

- `src/config/settings.rs`: the new key, its range and default.
- `src/ui/wrap.rs`: the row drawing and the row count learn whether a message continues the one before it.
- `src/app/messages.rs`, `src/app/thread.rs`: the row model that scrolling uses stays in step with the drawing.
- `src/ui/messages.rs`, `src/ui/thread.rs`: pass the previous message to the drawing.
- No new dependencies. Unrelated to `add-thread-list`, though it shares `ui/wrap.rs` and the thread pane drawn there.
