# Spec Delta

## ADDED Requirements

### Requirement: Thread replies are sends
A reply to a thread SHALL be sent with the same rules as any message: optimistic row, confirmation in place, the per-room ordered queue, failed and unconfirmed states, retry and discard, reconciliation with the incoming copy, the send guards and the quit warning. A reply SHALL carry its thread. Replies and room messages of the same room SHALL share one queue so their order is kept.

#### Scenario: Ordered with room messages
- **WHEN** the user sends a room message and then a thread reply in the same room
- **THEN** they reach the server in that order

#### Scenario: Reconcile
- **WHEN** the live copy of a pending reply arrives
- **THEN** the pending row in the pane is confirmed in place, not duplicated

#### Scenario: Encrypted room
- **WHEN** the user replies in an encrypted room
- **THEN** nothing is sent and the status bar shows the refusal
