# Spec Delta

## MODIFIED Requirements

### Requirement: Text-only rendering
Messages SHALL render as text, sanitized for terminal output, except that custom emojis and image attachments MAY be drawn as images where the terminal can show them (see `custom-emoji-images` and `message-attachments`). System messages SHALL render as a dim one-liner and attachments as `[attachment: name (size)]` (the size only when the server reports it, a server-made thumbnail not listed), which an image attachment keeps as its caption. Messages SHALL wrap with a hanging indent in the compact layout or use a header line in the two-line layout, with bodies aligned at a shared column.

#### Scenario: Attachment
- **WHEN** a message has a file attachment
- **THEN** the row shows the attachment tag with its name and size

#### Scenario: Control characters
- **WHEN** a message contains terminal control characters
- **THEN** they are not passed to the terminal
