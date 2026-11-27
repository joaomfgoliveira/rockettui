# Design

## Context

See proposal.md for motivation. The inline player of `attachment-video-playback` (not archived yet) is the base, as it stands in the code:

- **One player.** `App.video: Option<Player>` holds the room, message id, run number, position, the two terminal image ids, the shown id, the newest unsent frame and the run's handle. While it is `Some` every key goes to a small matcher over the `[video]` table plus the Normal-mode keys of `play_video`. Seeks start a new run (new generation, old process killed); a kill-on-drop guard owns each `ffmpeg`.
- **Frames.** `video::spawn_playback` runs `ffmpeg` to raw RGBA of a fixed `W` by `H` (the poster's size, so no probe is needed), a reader thread paces them and sends `Action::VideoFrame`. `app::video::prepare_player` runs before every draw: it transmits the newest frame under the hidden of the two ids, places it, and switches the cells to it. The frame size is capped by `frame_len` at 480 by 480.
- **Overlay.** The full-size image view (`app::image_view`, `ui::image_view`) is a state, not a mode: it takes every key, covers the whole screen (`area`, `inner`), fits an image in it (`placement`) and draws placeholder cells with its own image id. It closes when its message goes away.

## Goals / Non-Goals

**Goals:**
- Fullscreen reuses the player: the same state, keys, runner, id swap, guard and status row, so the safety and cleanup rules hold without a second implementation.
- Closing fullscreen leaves the room exactly as before.

**Non-Goals:**
- A toggle between inline and fullscreen that keeps playing across a close, sound, a setting for size or rate, or a faster transmission path (kitty file or shared memory).

## Decisions

**1. Fullscreen is a flag on the existing player, not a second player.**
`Player` gains `fullscreen: bool`. There is still one video at a time and one `App.video`. The overlay is drawn whenever `fullscreen` is true, as the image view is drawn whenever `App.image_view` is `Some`; no new `Mode`, no new `App` field. Alternative: a separate `FullscreenPlayer` in its own module, as `image_view` is separate from the inline box. It would duplicate the runs, seeks, keys, ids and cleanup that took most of the inline change, and two players could then run at once.

**2. A bigger frame, from the poster's shape, without a probe.**
The reader needs the exact frame size before the first frame. The fullscreen size is the poster's aspect fitted in the overlay's pixels (cells times the cell size, each side at most 1280), both sides even, computed in the reducer from the poster the client already holds. No `ffprobe` is needed, and a rotated phone video still agrees with its own poster. A source smaller than 720 is scaled up by `ffmpeg` (a little wasted work, no wrong picture). The limit becomes a parameter: `frame_len` checks against a given side (`MAX_SIDE` 480 inline, 1280 fullscreen), `Playback` carries it, and nothing larger than 1280 by 1280 by 4 bytes is ever allocated. Alternative: probe the source size and pick `min(source, 720)`. It avoids the upscale but adds a process to every start. Alternative: a setting. The numbers are constants, as for the inline player.

**3. Rate stays at 15 frames a second.**
At 1280 by 720 that is about 55 MB/s of pixels and 74 MB/s of base64 (less on a smaller overlay) through the pty: fine for a local kitty, rough over SSH. The existing rule helps: the reader drops a frame when two are still unhandled, and a slow terminal write slows the loop, so the pace degrades by dropping frames instead of queueing them. If it is choppy the next step is the file or shared-memory transmission noted in the inline design (`t=f`, `t=s`); the transmit helper changes and nothing else. The manual check in the tasks decides whether 720 and 15 stand.

**4. `gV` continues an inline run; otherwise it starts at the beginning.**
Not playing: `gV` on a ready poster (or a placeholder, after the download) starts a fullscreen run at 0 with the bigger size. Playing or paused inline: it sets `fullscreen` and starts a new run (a new generation, like a seek) at the current position and the larger size, keeping the paused state; the old picture is not shown in the overlay (its size differs), which says it is loading until the first new frame. In the capture, `gV` while fullscreen closes it (it is another key of the `play_video` family).

**5. Stopping closes everything.**
Every stop key, `play_video` and `play_video_fullscreen` end the player as the inline one does: the process is killed, both ids are deleted, the keys are returned. There is no "back to inline" state. Esc closes to the room with the poster showing, which is what the proposal says and the simplest thing to explain and test.

**6. The overlay draws like the image view, with the player's ids.**
`ui::image_view::area`, `inner` and `placement` give the overlay and the cells for a frame of the player's size. The overlay is drawn over the room, whose rows are unchanged. While fullscreen the inline box keeps drawing the poster (not the player's ids), so closing changes nothing under it. The overlay's status row is the inline `status_text` at the overlay's width. Before the first frame the overlay shows a dim `loading...`. The poster is not shown in the overlay: a terminal image has one virtual placement per id, and re-placing the poster at the overlay's size would resize the inline box under it.

**7. A resize refits without restarting.**
`prepare_player` remembers the cells it placed the shown id in. When the overlay's cells change and no new frame is pending (a paused video), it places the shown id again. A playing video re-places with its next frame anyway. The frame size does not change; it is fitted inside the new overlay.

**8. Lifecycle is the inline rule, minus the viewport.**
`sync` stops the player for a changed room, a message that left, video no longer allowed, and quit, as inline. The "pushed far from the viewport" check is skipped while fullscreen: the overlay covers the room and new messages cannot hide a video the user is looking at.

**9. The download is reused.**
Fullscreen uses the file the poster already put in the downloads cache; `gV` on a placeholder asks for the same single effect (download, poster, then the run) with the fullscreen flag remembered, so a second download never happens.

## Risks / Trade-offs

- [Throughput over the pty] -> 720 px and 15 fps are constants; frame dropping under load; kitty file or shared memory is the known next step. The user's check in kitty settles it.
- [An upscaled small source] -> A source under 720 is enlarged by `ffmpeg`; the picture is right, the cost is small.
- [A new run on every expand] -> A moment of `loading...` when expanding an inline run; keyframe seek is fast (see the inline design).
- [Two terminal-image sizes under one id] -> Each transmit carries its own width and height, and each frame is placed again, so a size change between inline and fullscreen is handled by the same per-frame placement.
- [Depends on an unarchived change] -> The proposal says so; archive `attachment-video-playback` first or apply on top of it.

## Migration Plan

No settings and no data. The action is new and defaults to `gV`, which is unused. Rollback is removing the change.

## Open Questions

- Whether 720 and 15 are right in kitty on the author's machine (the manual check), and whether the overlay should show the poster until the first frame (it cannot, see decision 6, without a second image id).
