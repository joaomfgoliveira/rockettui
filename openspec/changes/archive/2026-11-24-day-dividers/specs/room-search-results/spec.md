# Spec Delta

## MODIFIED Requirements

### Requirement: Results list
Results SHALL be shown in a read-only list over the messages pane, newest first, each result with its author, time and text drawn like a message in the pane, with a day divider above the first result of each day. The list SHALL be a state of the app and not a mode: the mode badge SHALL keep its value, and the list SHALL take the motion keys and `q`, `Esc` and `Ctrl-c` while it is open. The text of the query SHALL be shown in the list's title.

#### Scenario: Newest first
- **WHEN** a search finds three messages from different days
- **THEN** the list shows the most recent one at the top and the oldest at the bottom

#### Scenario: Mode is unchanged
- **WHEN** the results list opens from Normal mode
- **THEN** the mode badge still reads Normal

#### Scenario: Title shows the query
- **WHEN** the user searched for `deploy`
- **THEN** the list title shows `deploy`

#### Scenario: Rows show only the time
- **WHEN** a result was written on 2026-10-02 at 22:13
- **THEN** its row shows `22:13`, and a divider `2026-10-02` is above it

#### Scenario: Divider where the day changes
- **WHEN** two results are from 2026-11-24 and one is from 2026-10-02
- **THEN** a `2026-11-24` divider is above the first two and a `2026-10-02` divider above the third

#### Scenario: Divider is not a result
- **WHEN** the user moves down through the list
- **THEN** the selection goes from result to result and never rests on a divider
