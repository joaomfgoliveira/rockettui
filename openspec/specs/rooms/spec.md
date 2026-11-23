# rooms Specification

## Purpose

Defines how the sidebar lists the user's rooms and how rooms are opened, including encrypted rooms that the client cannot read.

## Requirements

### Requirement: Joined rooms only
The sidebar SHALL list the user's joined channels, private groups, discussions, teams and DMs. DMs SHALL be named by the other user's username. Closed subscriptions, omnichannel rooms and subscriptions without a room record SHALL be skipped. There SHALL be no directory browsing or joining.

#### Scenario: Closed DM
- **WHEN** a subscription has `open: false`
- **THEN** it does not appear

### Requirement: Sidebar sections
The sidebar SHALL group rooms into Unread, Folders and the rest, with the rest ordered by most recent activity. Folders SHALL be Favorites (favorited rooms without a category) followed by the user's custom categories. Empty folders SHALL be omitted, and Folders SHALL fold and unfold. A room that is unread appears in Unread rather than in its folder.

#### Scenario: Unread room
- **WHEN** a subscription has unread messages
- **THEN** the room is shown in Unread with an unread marker (a flag only, no counts)

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

### Requirement: Encrypted rooms
Encrypted rooms SHALL be listed with the encrypted marker. Opening one SHALL show an "encrypted, not supported" placeholder and SHALL NOT request history or send messages.

#### Scenario: Open encrypted room
- **WHEN** the user opens an encrypted room
- **THEN** the placeholder is shown and no history request is made
