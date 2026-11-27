# Tasks

## 1. Carry video and audio metadata

- [x] 1.1 Read `video_url`, `video_type`, `video_size`, `audio_url`, `audio_type`, `audio_size` in `api/wire.rs` and fill `Attachment` for them in `api/map.rs`, with the same `/file-upload/` rule; verify mapping tests for a video, an audio and an attachment with an absolute address
- [x] 1.2 Keep `Debug` redacting for the new data; verify a test that a sentinel path and type never appear

## 2. Settings and keymap

- [x] 2.1 Parse the `[open]` table (`image`, `video`, `audio`, `default`) in `config/settings.rs` with the platform default; verify config tests for each key, the fallback order, an empty list, a non-array, an empty program name and a NUL character
- [x] 2.2 Add `Action::OpenAttachment`, the `open_attachment` vocabulary entry and the `go` default in every preset (`input/keymap.rs`, `config/keymap.rs`); verify keymap tests for default, rebind, unbind and the clash check

## 3. Download

- [x] 3.1 Add the media-type to extension table and the cache path builder (hash of the path plus extension); verify tests that a hostile name or path never leaves the directory and that an unlisted type is refused
- [x] 3.2 Add the streaming download in `api/mod.rs` to a temporary file renamed on success, with the size cap and the reported-size check; verify wiremock tests for success, over the cap (partial file removed), a 404, an absolute address refused, and that the token is sent only as a header to the server
- [x] 3.3 Add `Effect::DownloadAttachment`, its result actions and the in-flight set in the reducer; verify tests for a double press, an already cached file (no request) and a failed download

## 4. Open

- [x] 4.1 Add the spawn effect: direct `Command` with the configured arguments and the file as the last argument, stdio null, reaped in a background task, early failure reported; verify tests with a fake program (a script in a temp directory) for success, missing program, immediate non-zero exit, and a path with spaces passed as one argument
- [x] 4.2 Add the `OpenAttachment` reducer arm and the fixed-wording status notices (none to open, downloading, too large, unsupported type, could not start); verify reducer tests for each notice and that no notice contains a name or path

## 5. Context popup

- [x] 5.1 List the action for a message with an image, video or audio attachment with the live key; verify tests for listed, not listed, rebound and unbound

## 6. Integration

- [x] 6.1 Run `cargo fmt --check`, `cargo clippy` and `cargo test`; verify they pass
- [x] 6.2 Ask the user to check `go` on a video, an audio message and an image on their local Docker server with their own player (kitty terminal); the checks are theirs to run
- [ ] 6.3 Update the Features list in the Obsidian vault (tick or add the line, refresh `Progress` and `Last updated`) when the change is archived
