# Spec Delta

## ADDED Requirements

### Requirement: Thread action on messages
A message that has thread replies, and a reply shown in the channel, SHALL list opening its thread with its live key, among the message's own actions. A message with no thread SHALL NOT list it. An action with no key bound SHALL be left out. In the thread pane the popup SHALL list closing the thread.

#### Scenario: Parent with replies
- **WHEN** the cursor is on a message with replies and the user presses `?`
- **THEN** the popup lists opening the thread with `gt`

#### Scenario: No thread
- **WHEN** the cursor is on a message with no replies and the user presses `?`
- **THEN** the popup does not list the thread action

#### Scenario: Unbound
- **WHEN** the user sets `"gt" = "none"` and opens the popup on a message with replies
- **THEN** no row for opening the thread is shown

#### Scenario: In the pane
- **WHEN** the thread pane is focused and the user presses `?`
- **THEN** the popup lists closing the thread with `Esc`
