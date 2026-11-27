# Proposal

## Why

A video message is only `[attachment: clip.mp4 (1.5 MB)]` in the message flow. `go` can hand the file to an outside player, but the user wants to watch it in the terminal, in the message's own box, the way image previews and animated gifs already appear. kitty can show it: the client already draws images from placeholder cells, keeps a box fixed from metadata so rows never move, and redraws on a timer for animations. What is missing is a source of frames that does not keep every frame in memory.

## What Changes

- A video attachment gets an inline box, reserved from a default 16:9 shape (Rocket.Chat reports no video size), drawn in the open room's message flow only, on terminals that can show images.
- The box has states: a **placeholder** (`[video 40 MB: gv to load]`), a **poster** (one frame about one second in), **playing** and **paused**.
- A video of at most the inline image cap (8 MiB) gets its poster automatically, from the download cache `go` already uses. A larger one shows the placeholder until the user asks.
- A new Normal-mode action `play_video` (default `gv`) steps through the states: placeholder, load the poster, play, stop (back to the poster). One video is active at a time.
- Frames come from `ffmpeg` run as a child process on the cached file, writing raw frames to a pipe. The client reads them at the video's pace and sends each to the terminal under one image id, so the cells never change. `ffprobe` gives the duration once.
- While a video plays or is paused it captures the keys until the user stops it. A new `[video]` table in `keymap.toml` binds the player actions: `video_pause` (`Space`), `video_seek_back` / `video_seek_forward` (`h`/`l`, 5 s), `video_seek_back_far` / `video_seek_forward_far` (`H`/`L`, 30 s) and `video_stop` (`Esc`, `q`, `Ctrl-c`). A row under the box shows the position and the keys.
- New settings: `inline_video` (default `true`) and the `ffmpeg` and `ffprobe` program names.
- No sound in this version. A missing `ffmpeg` is a status notice, not an error.

Out of scope: audio, video in threads, search results and the `gp` view, subtitles, playlists, streaming without downloading, other terminals than kitty, a player for audio attachments.

## Capabilities

### New Capabilities
- `video-playback`: the inline video box, its states, loading the poster, playing, pausing, seeking and stopping, the limits on what ffmpeg is asked to do, and what is freed when it ends.

### Modified Capabilities
- `keymap`: the new bindable actions (`play_video` in Normal mode, and the player actions in the new `[video]` table) with their defaults, and the player table's rules.
- `configuration`: the `inline_video`, `ffmpeg` and `ffprobe` settings.
- `attachment-open`: downloads are no longer only on an explicit request: a video within the inline image cap is downloaded to show its poster.
- `context-help`: the message popup lists the next step of `play_video` ("load video", "play video", "stop video") with its live key.

## Impact

- Builds on `attachment-open-external` (the downloads cache, `download::file_name`, `Client::download_attachment`) and `attachment-image-preview` (the box, the placeholder cells, the image id range, the redraw timer), both archived.
- A new external dependency on `ffmpeg` and `ffprobe` (found on `PATH`, or named in the config). This is the second place the client starts a process, and the first that reads untrusted media with a large native parser; the design calls out the limits and the flags used.
- `src/effect.rs` and a new module for the frame reader; `src/app/` for the player state and key routing; `src/ui/` for the box states and the status row; `src/config/` and `src/input/` for the settings and the `[video]` key table; `src/kitty.rs` reuses `transmit` and `place` with a single re-sent id.
- Features list in the Obsidian vault is updated on archive.
