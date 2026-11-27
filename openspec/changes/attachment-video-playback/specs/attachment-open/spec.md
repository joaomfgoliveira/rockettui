# Spec Delta

## MODIFIED Requirements

### Requirement: Downloads are made as a normal client
The client SHALL download an attachment only when the user asks for it, except that a video whose reported size is within the inline image download limit MAY be downloaded when its message is drawn in the open room, to show its poster (see `video-playback`). A download SHALL be from the server's own `/file-upload/` path of that attachment, with the user's own credentials, and SHALL NOT request an absolute URL or any other path. The token SHALL NOT appear in the program's arguments or environment. A response over a fixed size limit SHALL be refused and its partial file removed. No request SHALL carry message text, room names or usernames.

#### Scenario: Explicit request only
- **WHEN** a room with a video larger than the inline limit, or with an audio attachment, is opened or scrolled
- **THEN** it is not downloaded

#### Scenario: Small video poster
- **WHEN** a room is opened with a video within the inline limit in view and `inline_video` is on
- **THEN** it is downloaded once for its poster, without a key

#### Scenario: Other host
- **WHEN** an attachment's address is an absolute URL or outside `/file-upload/`
- **THEN** nothing is requested and a status notice says it cannot be opened

#### Scenario: Over the limit
- **WHEN** the reported size or the received bytes exceed the limit
- **THEN** the download stops, the partial file is removed and a status notice says the file is too large

#### Scenario: Token not exposed
- **WHEN** the program is started
- **THEN** its arguments are the configured arguments and the file path, and nothing from the user's credentials
