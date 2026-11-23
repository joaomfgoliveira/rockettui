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
Opening a room SHALL NOT mark it read on the server. Unread state SHALL refresh only on connect and `:reconnect`.

#### Scenario: Open unread room
- **WHEN** the user opens an unread room
- **THEN** the server-side unread state is unchanged

### Requirement: Encrypted rooms
Encrypted rooms SHALL be listed with the encrypted marker. Opening one SHALL show an "encrypted, not supported" placeholder and SHALL NOT request history or send messages.

#### Scenario: Open encrypted room
- **WHEN** the user opens an encrypted room
- **THEN** the placeholder is shown and no history request is made
