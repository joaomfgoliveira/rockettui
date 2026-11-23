# message-editing Specification

## Purpose

Lets the user fix or remove their own sent messages from the keyboard, and guarantees that nobody else's message can be edited or deleted through the client.

## Requirements

### Requirement: Only own server messages
Edit and delete SHALL act only on a message the server holds (not a pending, failed or unconfirmed local one) whose author is the user's own account, and that is a plain text message. On any other message, including anyone else's, the edit and delete commands SHALL do nothing: no prompt, no request to the server and no change to the screen. They SHALL also do nothing while the own username is not yet known, and in an encrypted room.

#### Scenario: Someone else's message
- **WHEN** the cursor is on a message written by another user and the user presses the edit key or `dd`
- **THEN** nothing happens: no editing starts, no prompt opens and no request is sent

#### Scenario: Message with an attachment
- **WHEN** the cursor is on the user's own message that carries a file or attachment and the user presses the edit key
- **THEN** nothing happens, so the attachment text shown in the row is never written into the message

#### Scenario: Own username unknown
- **WHEN** the own username has not loaded yet and the user presses the edit key or `dd` on any message
- **THEN** nothing happens

### Requirement: Edit through the input bar
In Normal mode with the messages pane focused, the edit key (default `e`) on one of the user's own messages SHALL load its text into the input bar in Insert mode, mark the input as editing that message and show that in the input title. Enter SHALL send the new text as an edit of that message, and `Esc` SHALL cancel the edit and restore the draft the user had before.

#### Scenario: Start editing
- **WHEN** the user presses `e` on their own message
- **THEN** the input shows that message's text, the mode is Insert and the input title says that Enter saves and Esc cancels

#### Scenario: Cancel
- **WHEN** the user presses `Esc` while editing
- **THEN** editing ends without a request and the input shows the draft the user had before editing

#### Scenario: Existing draft kept
- **WHEN** the user had a half-typed draft, edits a message and saves it
- **THEN** the half-typed draft is back in the input afterwards

### Requirement: Saving an edit
Saving SHALL send the edit without using the send queue and SHALL leave the old text on screen until the server accepts it. On acceptance the row SHALL show the new text with the edited marker in the same position, whether the confirmation arrives from the request or from the live stream. Blank text or text equal to the original SHALL end the edit without a request.

#### Scenario: Edit accepted
- **WHEN** the user saves a changed text and the server accepts it
- **THEN** the row shows the new text and the edited marker, in the same position, once

#### Scenario: Nothing to save
- **WHEN** the user saves the unchanged or blank text
- **THEN** editing ends and no request is sent

### Requirement: Refused or failed edit
When an edit fails (the server refuses it, for example because of its time limit or settings, or the request does not complete), the app SHALL show one fixed status line for the kind of failure, never server text or the message text, SHALL keep editing open with the typed text intact, and SHALL leave the displayed message unchanged.

#### Scenario: Server refuses the edit
- **WHEN** the server refuses an edit
- **THEN** the status bar shows a fixed message, the row keeps its old text and the input still holds the new text for another try

### Requirement: Delete with confirmation
In Normal mode with the messages pane focused, `dd` on one of the user's own server messages SHALL open a confirmation prompt in the status bar and SHALL NOT send anything yet. `y` or Enter SHALL confirm; any other key SHALL cancel and be consumed. On confirmation the app SHALL request the delete, and when the server accepts it the row SHALL become the deleted line in the same position.

#### Scenario: Confirm
- **WHEN** the user presses `dd` on their own message and then `y`
- **THEN** the message is deleted on the server and the row becomes a deleted line in the same position

#### Scenario: Cancel
- **WHEN** the user presses `dd` and then any key other than `y` or Enter
- **THEN** nothing is deleted and the prompt closes

#### Scenario: Row changed while the prompt was open
- **WHEN** the row under the prompt stops being one of the user's own server messages before the user confirms
- **THEN** confirming sends nothing

### Requirement: Refused or failed delete
When a delete fails, the app SHALL show one fixed status line for the kind of failure, never server text, and the row SHALL stay as it was.

#### Scenario: Server refuses the delete
- **WHEN** the server refuses the delete
- **THEN** the status bar shows a fixed message and the message stays visible and unchanged

### Requirement: Nothing logged
No status message, error or log line produced by editing or deleting SHALL contain message text.

#### Scenario: Failure while editing
- **WHEN** an edit request fails
- **THEN** the message shown names the failure kind only
