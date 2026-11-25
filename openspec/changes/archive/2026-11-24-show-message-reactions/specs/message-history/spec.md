# Spec Delta

## ADDED Requirements

### Requirement: Reactions in fetched pages
A message in a fetched page SHALL carry the reactions the server reports for it, so its reaction row is drawn the same whether the message arrived live or in a page. A message whose reactions the server does not report SHALL have none.

#### Scenario: Reactions in history
- **WHEN** a room is opened and a fetched message has reactions
- **THEN** its row shows them under the body

#### Scenario: Merge keeps one copy
- **WHEN** a fetched page overlaps a live message that already carries reactions
- **THEN** the message appears once, with the reactions of the newer copy

### Requirement: Reconcile reactions after a reconnect
When a refetched page after a reconnect covers a held message, the held copy SHALL take the page's reactions, as it takes the page's text and edited state. Held messages older than the page SHALL be left as they are. The cursor SHALL stay on the same message at the same screen row.

#### Scenario: Reacted while offline
- **WHEN** a message gained a reaction during an outage and the refetched page contains it
- **THEN** the row shows the reaction and the message is not marked edited

#### Scenario: Reaction removed while offline
- **WHEN** a reaction was removed during an outage and the refetched page no longer has it
- **THEN** the row no longer shows it

#### Scenario: Older than the page
- **WHEN** a held message is older than everything in the refetched page
- **THEN** its reactions are unchanged
