# attachment-open Specification

## Purpose
Download an image, video or audio attachment of a message to the client's cache, as a normal client, and open it with the user's own external program.

## Requirements

### Requirement: Open an attachment with an external program
A Normal-mode action `open_attachment`, bound to `go` by default, SHALL download the first image, video or audio attachment of the message under the cursor and open it with the configured external program. The terminal UI SHALL stay responsive and usable while the download runs and after the program starts. A message with no such attachment SHALL show a status notice and change nothing.

#### Scenario: Video message
- **WHEN** the cursor is on a message with a video attachment and the user presses `go`
- **THEN** the file is downloaded and the video opens in the configured player while the client keeps running

#### Scenario: Several attachments
- **WHEN** the message has two attachments of the supported kinds
- **THEN** the first one in the order the server lists them is opened

#### Scenario: No supported attachment
- **WHEN** the cursor is on a message with no image, video or audio attachment and the user presses `go`
- **THEN** a status notice says there is nothing to open and nothing else changes

#### Scenario: Image on a terminal without graphics
- **WHEN** the terminal cannot draw images and the user presses `go` on a message with an image
- **THEN** the image is downloaded and opened in the configured program

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

### Requirement: Downloaded files are kept safely
A downloaded file SHALL be written under the client's cache directory with a name built by the client from a hash of the attachment's path and an extension chosen from a fixed list for its media type, never from the server's file name. Only image, video and audio media types of that list SHALL be opened; any other type SHALL be refused with a status notice. A file that is already in the cache SHALL be opened without a new request.

#### Scenario: Hostile file name
- **WHEN** an attachment's name is `../../x.sh`
- **THEN** the file is stored under the cache directory with a client-built name and the name is never used as a path

#### Scenario: Unsupported type
- **WHEN** the attachment's media type is not an image, video or audio type of the list
- **THEN** it is not opened and a status notice says the type is not supported

#### Scenario: Already downloaded
- **WHEN** the user opens the same attachment twice
- **THEN** the second open starts the program without a request

#### Scenario: Cache cannot be written
- **WHEN** the cache directory cannot be created or written
- **THEN** a status notice says the file could not be saved and the client goes on

### Requirement: The program is configured as an argument list
The program SHALL be run directly with the configured arguments followed by the file path as the last argument, and never through a shell. Its standard input, output and error SHALL be detached from the terminal so it cannot draw over the interface, and the client SHALL NOT wait for it to finish before taking input. A program that cannot be started, or that exits with an error right away, SHALL produce a status notice that does not include the file name or any server text.

#### Scenario: Program not installed
- **WHEN** the configured program does not exist
- **THEN** a status notice says the program could not be started and the client goes on

#### Scenario: Window program
- **WHEN** the program opens its own window
- **THEN** the client's terminal is not disturbed

#### Scenario: File name with shell characters
- **WHEN** the file name or path holds spaces or shell characters
- **THEN** it is passed as one argument, unchanged

### Requirement: Progress is shown
While a download runs the status line SHALL say so, and when it ends it SHALL say whether the program was started or what failed. Pressing `go` again while a download of the same attachment runs SHALL NOT start a second one.

#### Scenario: Running
- **WHEN** a download is in progress
- **THEN** the status line shows that the file is being downloaded

#### Scenario: Double press
- **WHEN** the user presses `go` twice on the same message while it downloads
- **THEN** one download runs and one program starts
