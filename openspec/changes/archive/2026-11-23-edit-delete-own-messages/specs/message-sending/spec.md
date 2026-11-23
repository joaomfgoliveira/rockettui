# Spec Delta

## MODIFIED Requirements

### Requirement: Retry and discard
In Normal mode with the messages pane focused, `r` on the failed or unconfirmed row under the cursor SHALL retry it, and `dd` (the `delete_message` action, see `message-editing`) SHALL discard it; `discard_send` remains a bindable action that only discards. Both are rebindable. A retry SHALL first check whether the server already has the message and SHALL NOT resend if it does, and if the resend errors SHALL check once more before marking it failed or unconfirmed.

#### Scenario: Already delivered
- **WHEN** the user retries an unconfirmed message the server already stored
- **THEN** it is marked sent and not posted again

#### Scenario: Discard a failed row
- **WHEN** the user presses `dd` on a failed message
- **THEN** it is removed locally with no prompt and no request to the server

## ADDED Requirements

### Requirement: Edits and deletes are not sends
An edit or a delete of an existing message SHALL NOT enter the per-room send queue, SHALL NOT change the send state of any message, and SHALL NOT wait for or hold back queued sends.

#### Scenario: Edit while a send is in flight
- **WHEN** the user saves an edit while a message of the same room is being sent
- **THEN** the edit request goes out without waiting and the send queue is unaffected
