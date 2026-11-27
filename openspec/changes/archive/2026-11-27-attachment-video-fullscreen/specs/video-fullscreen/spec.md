# Spec Delta

## Purpose

Watch a video attachment as large as the terminal allows, in an overlay like the full-size image view, with the inline player's keys, and close back to the room as it was.

## ADDED Requirements

### Requirement: A video can be played fullscreen
`play_video_fullscreen` on a message with a video the client may fetch SHALL open an overlay over the whole screen, shaped and placed like the full-size image view, and play the video in it. It SHALL work only where the terminal can show images and `inline_video` is on, and only on a video that the inline box can show (not one whose poster failed). If the video is not downloaded yet it SHALL be downloaded first, as for the inline poster. While the overlay is open the room's mode, focus and cursor SHALL stay as they were. Anything but the messages pane in Normal mode SHALL do nothing.

#### Scenario: Open from a poster
- **WHEN** the cursor is on a video that shows its poster and the user presses `gV`
- **THEN** the overlay opens and the video plays from the start in it

#### Scenario: Open from a placeholder
- **WHEN** the cursor is on a large video that shows only the placeholder and the user presses `gV`
- **THEN** the video is downloaded, and the overlay opens and plays once the first frame is ready

#### Scenario: Not a video
- **WHEN** the user presses `gV` on a message with no video, on a terminal without images, or with `inline_video = false`
- **THEN** a status notice says so in fixed words, as `gv` does

### Requirement: Fullscreen frames are larger than inline ones
Fullscreen playback SHALL show frames at most 15 frames a second, in the shape of the video's poster, sized to the pixels the overlay's picture covers (its cells times the terminal's cell size) and never more than 1280 pixels on a side, with both sides even, and scaled with a sharp (lanczos) filter. Frames SHALL NOT be kept after they are shown. A longer video than 10 minutes SHALL play its first 10 minutes, as inline. While the first frame is not ready the overlay SHALL say it is loading.

#### Scenario: Larger picture
- **WHEN** a 16:9 video plays fullscreen in an overlay whose picture area is 1200 by 800 pixels
- **THEN** its frames are 1200 by 674 pixels and fill the overlay's width

#### Scenario: Loading
- **WHEN** the overlay opens and no frame has arrived
- **THEN** it shows a loading notice, and no row of the room moves

### Requirement: Fullscreen uses the player's keys
While the fullscreen overlay is open the keys SHALL be those of the inline player (`[video]` table): pause and resume, seek 5 and 30 seconds with the same clamping, and stop. Stopping SHALL close the overlay and return to the room with the video showing its poster. `play_video_fullscreen` and `play_video` SHALL also close it. Keys not bound there SHALL do nothing. A status row SHALL show the position, the length and the keys for pause and stop.

#### Scenario: Seek and pause
- **WHEN** the user presses `l`, then `Space`
- **THEN** playback continues 5 seconds later and then freezes, as inline

#### Scenario: Close
- **WHEN** the user presses `Esc`, `q` or `gV`
- **THEN** the overlay closes, the video shows its poster in the room, and `j` and `k` move the cursor again

### Requirement: Fullscreen continues an inline run
When the video is already playing or paused inline, `play_video_fullscreen` SHALL continue from the same position at the larger size, keeping it paused if it was paused. Only one video SHALL play at a time.

#### Scenario: Expand
- **WHEN** a video plays inline at 0:12 and the user presses `gV`
- **THEN** the overlay opens and the video continues from about 0:12

### Requirement: Fullscreen frees what it used
The overlay SHALL close, its `ffmpeg` process SHALL end and its frames in the terminal SHALL be freed when the user stops it, the video ends, the room changes, the message leaves the room, the video can no longer be shown (the terminal or the setting), or the client quits. The room SHALL look as it did before, with no row moved. A resize SHALL refit the picture to the overlay without restarting the video.

#### Scenario: Message deleted
- **WHEN** a live change deletes the message of the video playing fullscreen
- **THEN** the overlay closes and the keys are released

#### Scenario: Resize
- **WHEN** the terminal is resized while the overlay is open
- **THEN** the picture is refitted to the new size and keeps playing

#### Scenario: Quit
- **WHEN** the user quits the client while the overlay is open
- **THEN** no `ffmpeg` process remains

### Requirement: Fullscreen keeps the decoding limits
Fullscreen playback SHALL decode with the same bounded external program as the inline player (run directly, network protocols off, no input, no audio, fixed limits on the picture, frames and time, a timeout for a seek) on the file of the downloads cache. A failure SHALL be a status notice in fixed words, with no file name, path or program output, and SHALL close the overlay. Nothing about the video SHALL be logged.

#### Scenario: Cannot be shown
- **WHEN** the program fails to start or the file cannot be read
- **THEN** the overlay closes and a status notice says the video could not be shown
