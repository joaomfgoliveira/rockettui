# Proposal

## Why

The inline video box is at most `image_max_rows` tall and 480 pixels wide, which is small for a screen recording or a demo. Images already have a full-size view (`gp`); a video should have the same: watch it as large as the terminal allows, then drop back to the room.

## What Changes

- A new Normal-mode action `play_video_fullscreen` (default `gV`) on a message with a video the client may fetch: it opens an overlay over the whole screen, in the same place and shape as the full-size image view, and plays the video in it.
- The fullscreen player uses the same player keys as the inline one (the `[video]` table): pause, seek by 5 and 30 seconds, and stop. Stopping closes the overlay and returns to the room as it was.
- Frames are made larger than the inline 480 px: sized to the overlay's pixels (at most 1280 on a side) and scaled with lanczos, still at most 15 frames a second. The shape follows the poster, so a rotated or odd video agrees with itself.
- Pressing `gV` while the inline player runs continues from the same position at the larger size. Pressing it again (or any stop key) closes it.
- A status row with the position, length and the keys is drawn in the overlay, as under the inline box.
- The same safety rules as the inline player: a child `ffmpeg` on the cached file, no shell, bounded sizes, frames not kept, fixed-wording notices, nothing logged. Nothing happens on terminals that cannot show images or with `inline_video = false`.

Out of scope: sound, a toggle key between inline and fullscreen without stopping, picture-in-picture, fullscreen for audio, a setting for the frame size or rate (constants for now), kitty file or shared-memory transmission.

## Capabilities

### New Capabilities
- `video-fullscreen`: the fullscreen overlay player: how it opens, what it draws, its frame size and rate, how it continues an inline run, and how it closes and frees what it used.

### Modified Capabilities
- `keymap`: the new bindable action `play_video_fullscreen` and its default `gV`.
- `context-help`: the message popup lists the fullscreen action on a message with a video.

## Impact

- Builds on `attachment-video-playback`, which is not archived yet: its `Player`, the `ffmpeg` runner and kill-on-drop guard, the two-image-id frame swap, the player key capture and the status row. It should be archived first, or this change applied on top of it.
- `src/app/video.rs` (a fullscreen flag on the player, the new action, the larger frame size), `src/video.rs` (a frame-size limit that is not the constant 480), `src/ui/image_view.rs` or a sibling drawer (the overlay), `src/app/attachments.rs` (`prepare_frame` placement for the overlay), `src/input/` and `src/config/keymap.rs` (the action).
- Features list in the Obsidian vault is updated on archive.
