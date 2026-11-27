# Spec Delta

## Purpose

Show a video attachment in its own box in the message flow: a poster, then playback with pause and seek, without sound, using the user's `ffmpeg` and only on terminals that can show images.

## ADDED Requirements

### Requirement: Videos have an inline box
A video attachment that the client may fetch SHALL get a box in the message flow of the open room, under the message's caption, where the terminal can show images and `inline_video` is on. The box SHALL have a default 16:9 shape (the server reports no video size) at the pane width and at most `image_max_rows` tall, plus one row for a status line. Until its poster is in memory the video SHALL take a single line (the placeholder), so an unloaded video leaves no empty box; once the poster is there the full box and its status row SHALL be drawn, and from then on (uploading, playing, pausing, stopping) no row moves. While idle, the status row SHALL say how to play it (`video · gv to play`), and the poster SHALL carry a play button so it reads as a video. A video with no `/file-upload/` path, a terminal that cannot show images or `inline_video = false` SHALL leave the caption only. Only the open room's message flow SHALL draw video boxes, not threads, search results or the full-size view.

#### Scenario: Placeholder is one line
- **WHEN** a room is opened that has a large video whose poster is not loaded
- **THEN** it shows one dim line and no empty box

#### Scenario: Box fixed once the poster is there
- **WHEN** the poster arrives
- **THEN** the full box and its status row appear once, and later states (playing, paused, stopped) move no row

#### Scenario: Unsupported terminal
- **WHEN** the terminal cannot show images
- **THEN** the video shows only its caption and `gv` says images cannot be shown

#### Scenario: Turned off
- **WHEN** the user sets `inline_video = false`
- **THEN** no video box is drawn and nothing is downloaded for it

### Requirement: The box shows a placeholder, a poster, or the playing video
A video box SHALL be in one of four states: a placeholder (nothing is downloaded yet), a poster (one still frame), playing and paused. The placeholder SHALL say that the video can be loaded with its live key and give its size when the server reports one. `play_video` SHALL step the video under the cursor from placeholder to poster, from poster to playing, and from playing or paused back to the poster. At most one video SHALL be playing or paused at a time: starting another SHALL stop the first.

#### Scenario: Step from placeholder
- **WHEN** the cursor is on a video that shows the placeholder and the user presses `gv`
- **THEN** the video is downloaded and its poster takes the box

#### Scenario: Step to playing and back
- **WHEN** the cursor is on a video that shows its poster and the user presses `gv`, and later `gv` again
- **THEN** the first press starts playback and the second stops it and shows the poster again

#### Scenario: Another video
- **WHEN** one video plays and the user starts another
- **THEN** the first returns to its poster

#### Scenario: Not a video
- **WHEN** the user presses `gv` on a message with no video, or outside the messages pane
- **THEN** a status notice says there is no video, or nothing happens outside the pane

### Requirement: Posters load automatically only for small videos
A video whose reported size is within the inline image download limit SHALL have its poster loaded when its message is drawn in the open room, once per file per session. A larger video, or one with no reported size, SHALL show the placeholder until the user asks for it with `play_video`. The poster SHALL be one frame taken about one second into the video (the first frame for a video shorter than that), kept in the cache so that a later session does not run `ffmpeg` again. A poster that cannot be made SHALL leave the caption only and be final for the session.

#### Scenario: Small video
- **WHEN** a room is opened with a 1.5 MB video in view
- **THEN** its poster appears without any key

#### Scenario: Large video
- **WHEN** the room has a 40 MB video
- **THEN** it shows the placeholder with its size and nothing is downloaded until `gv`

#### Scenario: Cached poster
- **WHEN** the same video is shown in a later session
- **THEN** its poster is read from the cache and `ffmpeg` is not run

#### Scenario: Poster fails
- **WHEN** the file cannot be decoded
- **THEN** only the caption is drawn and the client goes on

### Requirement: Playback shows frames at the video's pace
Playing SHALL show the video's frames in its box at the pace of the video, at most 15 frames a second and at most 480 pixels wide, from the downloaded file, without sound. Frames SHALL NOT be kept after they are shown. A video longer than 10 minutes SHALL play its first 10 minutes. When the video ends, the box SHALL return to its poster.

#### Scenario: Plays in the box
- **WHEN** the user starts a poster
- **THEN** its frames replace each other in the same box and no row moves

#### Scenario: End of video
- **WHEN** playback reaches the end
- **THEN** the box shows the poster again and the keys are released

#### Scenario: Long video
- **WHEN** a video is 25 minutes long
- **THEN** it plays its first 10 minutes only

### Requirement: The player captures the keys until it is stopped
While a video plays or is paused it SHALL take the keys, and keys not bound in the player's key table SHALL do nothing. The defaults SHALL be: `Space` to pause and resume, `h` and `l` to seek 5 seconds back and forward, `H` and `L` to seek 30 seconds, and `Esc`, `q` and `Ctrl-c` to stop. A seek SHALL stay inside the video, and seeking while paused SHALL show the new frame and stay paused. A row under the box SHALL show the position and the length (`0:12 / 0:48`) and the keys for pause and stop. Stopping SHALL give the keys back to the Normal mode keymap as they were, with the cursor where it was.

#### Scenario: Pause and resume
- **WHEN** the user presses `Space` while a video plays, and again later
- **THEN** the picture freezes and the position stops, then both go on from there

#### Scenario: Seek
- **WHEN** the user presses `l` at 0:12 of a 0:48 video
- **THEN** playback continues from 0:17

#### Scenario: Seek past the ends
- **WHEN** the user seeks back at 0:02, or forward within 5 seconds of the end
- **THEN** it goes to the start, or to the end where the video stops

#### Scenario: Other keys
- **WHEN** the user presses `j` while a video plays
- **THEN** nothing happens and the cursor stays

#### Scenario: Keys given back
- **WHEN** the user presses `Esc` or `q`
- **THEN** the video shows its poster and `j` and `k` move the cursor again

### Requirement: Playback stops and frees what it used
Playback SHALL stop, its `ffmpeg` process SHALL be ended and its frame in the terminal SHALL be freed when the user stops it, the video reaches its end, the room is changed, the message leaves the room or scrolls out of view (a new message can push it), the video can no longer be shown (the terminal or the setting) or the client quits. Nothing SHALL keep running after the client exits.

#### Scenario: Room change
- **WHEN** the user switches room while a video plays
- **THEN** it stops and nothing of it stays in the terminal

#### Scenario: Message deleted
- **WHEN** the playing video's message is deleted by a live change
- **THEN** playback stops and the keys are released

#### Scenario: Pushed out of view
- **WHEN** new messages push the playing video far out of view
- **THEN** playback stops and the keys are released

#### Scenario: Quit
- **WHEN** the user quits the client while a video plays
- **THEN** no `ffmpeg` process remains

### Requirement: Media is decoded by a bounded external program
The client SHALL decode video only with the user's `ffmpeg` and `ffprobe`, run directly (never through a shell) on a file of the downloads cache with a client-built name, with network protocols disabled, no standard input, no audio, a fixed limit on the picture size, the number of frames and the running time, and a timeout for the poster and for a seek. A missing program, a failure to start it or a file it cannot read SHALL be a status notice in fixed words, with no file name, path or program output, and SHALL leave the box as it was. Nothing about the video SHALL be logged.

#### Scenario: ffmpeg missing
- **WHEN** `ffmpeg` is not installed and the user presses `gv` on a video
- **THEN** a status notice says the program is missing and the box stays a placeholder

#### Scenario: Hostile file
- **WHEN** the file is not a video, or claims a size over the limits
- **THEN** it is not played, a status notice says it could not be shown and the client goes on

#### Scenario: Notices hold no server text
- **WHEN** any step fails
- **THEN** the notice does not contain the file name, the path or the program's output
