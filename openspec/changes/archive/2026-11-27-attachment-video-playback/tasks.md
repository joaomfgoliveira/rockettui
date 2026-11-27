# Tasks

## 1. Settings

- [x] 1.1 Add `inline_video` (default true), `ffmpeg` and `ffprobe` (default program names) to `config/settings.rs`, rejecting a wrong type, an empty name and a NUL character; verify config tests for default, each key set, and each invalid value

## 2. Keymap

- [x] 2.1 Add `Action::PlayVideo`, the `play_video` vocabulary entry and the `gv` default in every preset (`input/keymap.rs`, `config/keymap.rs`, help text); update the tests that list every `g` binding; verify keymap tests for default, rebind, unbind and the clash check
- [x] 2.2 Add the six player actions (`video_pause`, `video_seek_back`, `video_seek_forward`, `video_seek_back_far`, `video_seek_forward_far`, `video_stop`), the `[video]` table with its defaults, and a fourth trie in `Keymap`, accepting the six only there and refusing a table with no key for `video_stop`; verify keymap tests for the defaults, a rebind, an unbind, an action in the wrong table, no way to stop, and that Normal mode is unchanged

## 3. Calling ffmpeg

- [x] 3.1 Add the pure argument builders for the poster, the playback (with `-ss`, `-t`, `fps`, explicit scale) and `ffprobe` duration, with `-nostdin`, `-protocol_whitelist file`, `-an -sn -dn` and no shell, plus the frame-size and frame-time arithmetic and the duration parser; verify unit tests of every argument list and of the arithmetic
- [x] 3.2 Add a child-process guard that kills and reaps on drop, and `Effect::LoadVideoPoster` (poster at 1 s, retry at 0, PNG from stdout capped in size, decoded with the existing defensive decoder, cached under `attachments/`, timeouts); verify tests with a fake `ffmpeg` script for success, missing program, non-zero exit, a timeout, an oversized output, a cache hit that runs nothing, and that no process remains
- [x] 3.3 Add the duration probe and the playback reader (a thread that reads whole frames, paces them at 15 per second, answers `Action::VideoFrame` with a generation, ends with `Action::VideoEnded`, and stops on pause or kill); verify tests with a fake `ffmpeg` script for pacing, pause not reading, a seek's new generation, end of stream and a short last frame

## 4. Poster and box

- [x] 4.1 Reserve the video box (fixed 16:9, `image_max_rows`, plus one status row) and draw the placeholder (with the size) and the poster in the open room's flow only, nothing without graphics or with `inline_video` off; verify snapshot tests for placeholder, poster, unsupported terminal and the setting off, and that no row moves between states
- [x] 4.2 Ask for the poster of a video within the inline cap when its message is drawn, once per path under the existing concurrency limit, and on `play_video` for a larger one (download through the shared downloader, then the poster); store it as `ImageState` under the video path; verify reducer tests for the size rule, once per path, the big video waiting for the key, and a failed poster staying failed
- [x] 4.3 Change the `attachment-open` download rule in the code so a small video is downloaded without a key and anything else still is not; verify a test that a room with an audio file and a large video requests nothing

## 5. Player

- [x] 5.1 Add `App.video` (room, message id, phase, generation, position, duration, the two ids, shown id, newest frame) and the `play_video` step (placeholder to poster, poster to playing, playing or paused back to the poster, one at a time), with fixed-wording notices for no video, ffmpeg missing and could not be shown; verify reducer tests for each step, another video taking over, and the notices
- [x] 5.2 Send each new frame to the terminal under the hidden of the two ids, place it, and draw the cells with the other (`prepare_frame`), freeing both ids when the player ends; verify tests that a frame is transmitted once, that the ids alternate, that the cells change colour only, and that stopping deletes both
- [x] 5.3 Draw the status row (`0:12 / 0:48` and the keys, sanitized, fixed width) under the box while the video is active; verify snapshot tests for playing, paused, unknown duration and a narrow pane

## 6. Controls

- [x] 6.1 Route every key to the `[video]` table while a player is active (pause and resume, seek by 5 and 30 seconds with clamping, stop on `Esc`, `q` and `Ctrl-c`, everything else does nothing), and give the keys back on stop; verify reducer tests for each default key, a rebound key, an unbound one, other keys doing nothing, the cursor unchanged and Normal mode back after stop
- [x] 6.2 Implement a seek as a new generation (kill, start again at the new time, ignore frames of the older one, show the new first frame, stay paused if paused) and the end of the video returning to the poster; verify tests for seeking at the ends, seeking while paused, stale frames being dropped and the end

## 7. Lifecycle

- [x] 7.1 Stop and free the player when the room changes, its message is deleted, the terminal or setting no longer allows it, its box is pushed far from the viewport, and on quit; verify reducer tests for each, and a test with the fake `ffmpeg` that no process is left after stop and after the app is dropped
- [x] 7.2 List `play_video` in the context popup with the next step's wording and the live key, only for a message with a fetchable video; verify tests for placeholder, poster, playing, no video, rebound and unbound

## 8. Integration

- [x] 8.1 Add redaction tests for the new effects and actions (no path, name or program output in `Debug`) and run `cargo fmt --check`, `cargo clippy` and `cargo test`; verify they pass
- [x] 8.2 Ask the user to check against their local Docker server in kitty: a small video's poster appears by itself, a large one shows the placeholder, `gv` loads and plays it in the box without rows moving or flicker (this also confirms the two-id swap works in kitty), `Space`, `h`, `l`, `H`, `L`, `Esc` and `q`, leaving the room while it plays, and that `ffmpeg` is gone afterwards; the checks are theirs to run
- [x] 8.3 Update the Features list in the Obsidian vault (tick or add the line, refresh `Progress` and `Last updated`) when the change is archived

## Workflow follow-up

- Archive with `/openspec-archive-change` after the checks, then sync the specs.
