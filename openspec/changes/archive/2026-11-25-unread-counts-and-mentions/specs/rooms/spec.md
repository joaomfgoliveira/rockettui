## MODIFIED Requirements

### Requirement: Sidebar sections
The sidebar SHALL group rooms into Unread, Folders and the rest, with the rest ordered by most recent activity. Folders SHALL be Favorites (favorited rooms without a category) followed by the user's custom categories. Empty folders SHALL be omitted, and Folders SHALL fold and unfold. A room that is unread appears in Unread rather than in its folder. The Unread section SHALL keep the same order whether or not its rooms have mentions.

#### Scenario: Unread room
- **WHEN** a subscription has unread messages
- **THEN** the room is shown in Unread with an unread marker and its unread count after the name

#### Scenario: Mentions do not reorder
- **WHEN** a room in Unread gains a mention
- **THEN** it keeps its place in the section and only its badge changes

## ADDED Requirements

### Requirement: Unread count and mention badge
An unread room SHALL show the server's unread message count in parentheses after its name, for example `general (3)`. When the room has mentions, a badge `@N` SHALL follow the count, where N is the user mentions plus the group mentions (`@all`, `@here`) the server reports. A badge SHALL NOT be shown when N is zero. A room the server flags as alerting with a count of zero SHALL stay in Unread and SHALL show no count. A count over 99 SHALL be shown as `99+`, and so SHALL a mention total. The folder header SHALL keep showing the number of unread rooms in the folder, not messages.

#### Scenario: Count and mention
- **WHEN** a room has 3 unread messages, 1 user mention and 1 group mention
- **THEN** the row shows `(3)` followed by `@2`

#### Scenario: Count without mention
- **WHEN** a room has 12 unread messages and no mentions
- **THEN** the row shows `(12)` and no badge

#### Scenario: Flagged without a count
- **WHEN** the server flags a room as alerting and its unread count is zero
- **THEN** the room is in Unread, bold, with no count and no badge

#### Scenario: Large count
- **WHEN** a room has 250 unread messages
- **THEN** the row shows `(99+)`

#### Scenario: Read room
- **WHEN** a room is read
- **THEN** it shows no count and no badge
