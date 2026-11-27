# Design

## Context

See proposal.md for motivation. What the change builds on, as the code stands:

- **Box and images.** An image box is reserved in `wrap` from metadata (`ui::attachment::box_size`), drawn as placeholder cells whose foreground colour carries a terminal image id (`kitty::cell_at`, `id_color`), and shown by `kitty::transmit` and `kitty::place`. `app::attachments::prepare_frame` runs before every draw and is the one place that writes image commands to the terminal. Ready images live in `AttachmentImages` under their server path.
- **Animation.** A gif keeps every frame as its own terminal image and the cells pick the id from the clock (`anim_ms`). That holds frames in memory and in the terminal, so it cannot carry a video (a 10 s clip at 15 fps is 150 frames).
- **Downloads.** `Client::download_attachment` and `download::file_name` already put a file in `<cache>/rockettui/downloads` under a client-built name. `Effect::OpenFile` already starts a program directly with detached streams.
- **Keys while something has the keyboard.** The full-size image view is a state (`App.image_view`); while it is open the reducer sends it every key before the keymap.
- Elm/TEA: one `Action` enum, one reducer. Effects do the IO and answer with actions. `tokio` has no `process` feature enabled; processes are `std::process` children, as for `open_file`.

## Goals / Non-Goals

**Goals:**
- A video looks like the other attachments (a box that never moves rows), and plays in it with pause and seek.
- Frames are streamed and dropped, so memory and terminal use are bounded whatever the video's length.
- Untrusted media reaches `ffmpeg` only through one narrow, bounded call site, and nothing lingers after the player stops.

**Non-Goals:**
- Sound (see Decision 9), exact frame seeking, hardware decoding, a decoder written in Rust, frame-perfect timing, running over SSH smoothly.

## Decisions

**1. `ffmpeg` as a child process, not a library.**
`ffmpeg`/`ffprobe` are run directly (argument list, no shell), as `open_file` does. Alternatives: the `ffmpeg-next` crate (links C libraries at build time, a heavy build dependency, and the parser then runs inside the client's own process, so a crash or exploit takes the client with it); a pure Rust decoder (nothing mature for h264). A child process keeps the parser outside the client, gives a clean kill, and keeps the build unchanged.

**2. The poster is a normal ready image.**
Loading a poster runs `ffmpeg` for one frame (`-ss 1`, retried at `0` when the video is shorter), as PNG on stdout, capped in size, then `emoji_image::decode` (the existing defensive decoder) makes the `Rgba`. The result is stored in `AttachmentImages` under the video's path as `ImageState::Ready`, so uploading, placement, the near-the-viewport rule and freeing all work as for images. The PNG is cached under `attachments/` with the key `(video path + "#poster", size)`, so a later session reads it and runs nothing. The poster is made at the playback width (`min(480, source width)`), so it also tells playback the frame size (see 5).

**3. A video box is an image box with a fixed shape and one extra row.**
`box_size` is called with a fixed 16:9 pixel size (1280 by 720 as "reported" dimensions), so the box depends on the pane width and `image_max_rows` only. The poster or the video is `fitted` inside it. One more row is reserved under every video box for the status line (blank unless the video is active), so activating a video never changes a row count. The placeholder is drawn like the "loading image..." notice: text in the first row, blank rows after.

**4. Automatic poster, explicit everything else.**
`request_images` also asks for the poster of a video whose reported size is within `MAX_ATTACHMENT_BYTES` (8 MiB), once per path, under the same concurrency limit as images. Larger or unknown-size videos wait for `play_video`. The first press on a placeholder queues the download and then the poster; a press on a poster starts playback. Downloading goes through the existing `download_attachment` (so the `attachment-open` rules and the 200 MiB cap apply) and is shared with `go`: a file already in the cache is not fetched again.

**5. Playback: a reader thread paces frames; the reducer only stores the newest.**
Starting playback spawns `ffmpeg` on the cached file with output `rawvideo`, `rgba`, `-vf fps=15,scale=W:H` where `W` by `H` are the poster's size (so no `ffprobe` is needed for the frame size, and a rotated phone video still agrees with its own poster), `-t 600`, `-an -sn -dn`, `-nostdin`, `-protocol_whitelist file`, `-loglevel error`, stdout piped, stdin and stderr null. A reader thread (`spawn_blocking`) reads exactly `W*H*4` bytes at a time from the pipe and sleeps until each frame's time (`start + n/15 s`), then sends `Action::VideoFrame { generation, index, pixels }` over a small bounded channel. The thread, not `ffmpeg`, owns the pace: the pipe fills when the thread is not reading, which makes `ffmpeg` wait, so pausing needs no signal. Alternative: `ffmpeg -re` and let it pace (after a pause it would burst to catch up). `ffprobe` is run once for the duration, only to show the position and clamp seeks; a video with no duration plays until the pipe ends.

**6. Two image ids, swapped per frame.**
The cells of the box carry one id; sending new pixels under the id that is being shown can drop its placement or flicker, depending on the terminal. So the player has two ids (A and B), reserved once from the attachment range like the view's id. Each frame is transmitted to the id that is not shown, placed, and the next draw's cells use it; the older one is then free for the next frame. `prepare_frame` writes the transmit and place before the draw, exactly as it does for stills. The cells therefore only change colour, and rows never move.

**7. The player is a state that captures keys, with its own key table.**
`App.video: Option<Player>` holds the room, message id, path, phase (playing or paused), generation, position, duration, the two ids, which is shown and the newest frame. While it is `Some` the reducer routes every key to a small matcher over a `[video]` key table before the Normal keymap, as `image_view` does, and unmatched keys do nothing. The table is a fourth trie in `Keymap`, built from the defaults and the `[video]` rows of `keymap.toml` with the same two-pass rules; it is not a new `Mode`, so nothing that matches on `Mode` changes. The six actions are accepted only in that table. `play_video` is an ordinary Normal-mode action (`gv`).

**8. Seeking restarts the process.**
A seek bumps the generation, kills the child (the guard's `Drop` kills and reaps), and starts `ffmpeg` again with `-ss <time>` before `-i` (fast, keyframe-accurate, which is close enough for 5 and 30 s steps). Frames of an older generation that are still in the channel are ignored. Seeking while paused starts the process, takes the first frame, shows it and holds. The position is the seek time plus `index / 15`.

**9. No sound.**
`-an`. A Rust client has no audio output without a new dependency, and a second process (an audio-only player) would need synchronising with the frame pace, which drifts. It is said in the help and in the proposal rather than half-done.

**10. Bounded and killed.**
One player at a time. The child is wrapped in a guard that kills and waits on `Drop`, so replacing the player, changing room, a deleted message, an error and quitting all end it, and the app dropping on exit ends it too. The poster and `ffprobe` have timeouts (killed when over), a seek has a first-frame timeout, and the output of each is capped. If the playing box falls far from the viewport (new messages push it) the reducer stops it, so a hidden video never decodes.

**11. Failures are fixed words.**
New notices: the program is missing, the file could not be shown, the video could not be loaded. None carries a name, path or program output; stderr is null, so there is nothing to carry. A poster that fails is `ImageState::Failed` for the session, as for images.

**12. Settings.**
`inline_video` (bool) and `ffmpeg`, `ffprobe` (program names), in `config.toml`, parsed like the other settings. The numbers (15 fps, 480 px, 10 min, 5 and 30 s, timeouts) are constants, not settings.

## Risks / Trade-offs

- [Untrusted media in a large native parser] -> Child process (not in-client), `-protocol_whitelist file` (no network fetches from a crafted playlist), no stdin, no audio, fixed picture/frame/time limits, a timeout and a kill, only types from the download table, client-built file names, and a hostile file only ever costs a failed box.
- [A pty can be too slow for 15 frames a second of 480 px] -> About 8 MB/s of pixels; fine for a local kitty, rough over SSH. Width and fps are constants chosen to fit; if it is choppy the next step is kitty's file or shared-memory transmission (`t=f`, `t=s`) for a local terminal, which keeps the design (the transmit helper changes, nothing else).
- [Replacing pixels under a shown id behaves differently per terminal] -> Decision 6 avoids it by double buffering; a spike checks the actual behaviour in kitty before the frame upload is finished.
- [Seek by restarting costs a moment] -> Keyframe seek is fast; the old frame stays on screen until the new generation's first frame arrives.
- [A zombie or runaway `ffmpeg`] -> The kill-on-drop guard, one player at a time, the cap on running time, and a test that no process remains after stop and quit.
- [`ffmpeg` not installed] -> A fixed notice on first use, the box stays a placeholder; videos still open with `go`.
- [Memory per frame] -> One frame in the reducer plus a channel of two; the frame is dropped once sent to the terminal.
- [Rows pushed by live messages while playing] -> The captured keys cannot scroll, but a stop-when-far rule frees the decode.

## Migration Plan

New settings default to on and read nothing when absent. No data migration. Rollback is removing the change; the downloads and poster files in the cache are harmless.

## Open Questions

- Whether the poster should be taken at 1 s or at a fraction of the duration (needs `ffprobe` before the poster). The spec says about one second; either satisfies it.
- Whether the status row should also show a bar. The text position and length satisfy the spec.
