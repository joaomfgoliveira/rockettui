# Spec Delta

## MODIFIED Requirements

### Requirement: Opening does not mark read
Opening a room SHALL NOT mark it read on the server. Unread state SHALL refresh on connect and `:reconnect`, and live whenever the server reports a change to the user's subscriptions. A room that is open and receives a live message SHALL stay flagged unread until the server says otherwise.

#### Scenario: Open unread room
- **WHEN** the user opens an unread room
- **THEN** the server-side unread state is unchanged

#### Scenario: Live unread flag
- **WHEN** another user posts in a room that is not open
- **THEN** the room moves to Unread and shows the unread marker without `:reconnect`

#### Scenario: Server clears unread
- **WHEN** the server reports a subscription as read, for example after the user reads it in another client
- **THEN** the room leaves Unread and the marker disappears
