# Tasks

## 1. Keymap

- [x] 1.1 Add `Action::PlayVideoFullscreen`, the `play_video_fullscreen` vocabulary entry, its help description and the `gV` default in every preset; update the tests that list every `g` binding; verify keymap tests for default, rebind, unbind and the clash check

## 2. Frame size

- [x] 2.1 Make the frame limit a parameter (`MAX_SIDE` 480 inline, a new 1280 for fullscreen) in `frame_len` and `Playback`, and add the pure `fullscreen_size(poster, target)` (poster aspect fitted in the overlay's pixels, even sides, never over the limit); verify unit tests for wide, tall, tiny, square and extreme shapes, and that `spawn_playback` refuses a size over its limit

## 3. Player

- [x] 3.1 Add `Player.fullscreen` and `play_video_fullscreen`: start a fullscreen run at 0 on a ready poster (and after the download for a placeholder), and expand a running inline player by starting a new run at the current position with the larger size, keeping paused; the inline `gv`, the stop keys and `gV` close it; verify reducer tests for each start, the expand from playing and from paused, the keys, and the notices for no video, no images and `inline_video` off
- [x] 3.2 Keep the size across seeks and give the capture trie the new action (`gV` closes a fullscreen player and expands an inline one); verify tests for a seek in fullscreen keeping the size and for a rebound or unbound key

## 4. Overlay

- [x] 4.1 Draw the overlay while the player is fullscreen (the image view's `area`, `inner` and `placement`, the player's ids, a title, the status row at the overlay's width, a dim `loading...` before the first frame), and keep the inline box drawing its poster; verify snapshot tests for loading, playing, paused, a narrow screen, and that no row of the room moves
- [x] 4.2 Place frames in the overlay's cells in `prepare_player` and place the shown id again when the cells change with no new frame (resize); verify tests for the placement, the same ids alternating, a resize while paused and while playing, and that closing deletes both ids

## 5. Lifecycle

- [x] 5.1 Close fullscreen for a room change, a deleted message, a terminal or setting that no longer allows it and quit, and skip the far-from-viewport rule while it is open; verify reducer tests for each, a test that new messages do not close it, and with the fake `ffmpeg` that no process is left after close and after the app is dropped

## 6. Popup and checks

- [x] 6.1 List `play_video_fullscreen` in the context popup on a message with a fetchable video (not one whose poster failed), with the live keys; verify tests for listed, no video, rebound and unbound
- [x] 6.2 Add redaction tests for the new paths (no path, name or program output in notices, rows or `Debug`) and run `cargo fmt --check`, `cargo clippy` and `cargo test`; verify they pass
- [ ] 6.3 Ask the user to check against their local Docker server in kitty: `gV` from a poster and from a placeholder, expanding an inline run, sharpness and smoothness at the overlay's pixel size (up to 1280) and 15 fps, `Space`, `h`, `l`, `H`, `L`, `Esc`, `q` and `gV` to close, a resize while playing and while paused, and that `ffmpeg` is gone afterwards; the checks are theirs to run
- [ ] 6.4 Update the Features list in the Obsidian vault (tick or add the line, refresh `Progress` and `Last updated`) when the change is archived

## Workflow follow-up

- Archive `attachment-video-playback` first (or apply this on top of it), then archive this one with `/openspec-archive-change` after the checks, and sync the specs.
