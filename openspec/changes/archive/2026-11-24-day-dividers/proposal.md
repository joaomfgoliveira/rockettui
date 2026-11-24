# Proposal

## Why

Message rows show only the time of day (`22:13`). Scrolling the live pane or a `:search` list that reaches back days or months, nothing says which day a message is from. A date on every row (or only on old ones) clutters the row; a divider where the day changes keeps rows short and still answers "when". See `configuration` and `room-search-results`.

## What Changes

- Every message row keeps showing only its time, following `timestamp_format` and the local zone.
- A divider line `── YYYY-MM-DD ───…` is drawn above the first message of each day, and again whenever the day changes between two neighbouring messages. The first message of a list also gets one, so the list always says which day it starts on.
- This holds in both the live messages pane and the `:search` results list.
- The divider is not a selectable item: `j`/`k` still move message by message and the cursor highlight covers only the message rows.
- The divider uses the existing `messages_notice` theme element; no new key.

Out of scope: dates on message rows, relative dates ("Today", "Yesterday"), a config key for the divider format.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `configuration`: the "Timestamps in local time" requirement now also says a day divider separates messages from different days.
- `room-search-results`: the results list shows the same dividers.

## Impact

- `ui/wrap.rs`: `divider_row` and `block_rows`; message rows are unchanged.
- `app/messages.rs`: `row_starts_of` counts divider rows, so scrolling and drawing agree.
- `ui/messages.rs`, `ui/results.rs`: draw the divider above each message, outside its cursor highlight.
- Snapshots of every pane that shows messages gain a divider row at the top.
- No new dependency, config key, theme element, clock input or network request.
