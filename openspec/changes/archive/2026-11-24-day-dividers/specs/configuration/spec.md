# Spec Delta

## MODIFIED Requirements

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
