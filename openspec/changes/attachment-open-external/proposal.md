# Proposal

## Why

Rocket.Chat plays video and audio in the message list and opens images in a viewer. A terminal client cannot play media itself without a large amount of decoding code, but the user's desktop already has players. Today a video or voice message is only `[attachment: clip.mp4]`, with no way to see or hear it. A key that downloads the file and hands it to the user's own player closes that gap for video and audio, and for images on terminals that cannot draw them.

## What Changes

- Read the video and audio attachment metadata (path, media type, size) the server sends, next to the image metadata.
- Add a Normal-mode action `open_attachment`, bound to `go`, that downloads the first image, video or audio attachment of the cursor message to the client's cache, as a normal client, and opens it with a configured external program. The terminal UI keeps running while the program runs.
- The program comes from `config.toml` as an argument list (never a shell string): per kind (`image`, `video`, `audio`) with a `default` fallback, and `xdg-open` (`open` on macOS) when nothing is set. The file path is passed as the last argument and nothing else from the server is.
- Show progress and failures as status notices, and never open a file whose type is not an image, video or audio.
- List the action in the context popup for a message with such an attachment.

Out of scope: playing or decoding media inside the terminal, poster frames or waveforms, streaming without downloading, opening other file types (documents, archives), cleaning the download cache, uploads.

## Capabilities

### New Capabilities
- `attachment-open`: downloading an attachment of a message and opening it with an external program.

### Modified Capabilities
- `keymap`: new bindable action `open_attachment`, default `go` in Normal mode.
- `context-help`: a message with an image, video or audio attachment lists the action with its live key.
- `configuration`: new `[open]` table with the opener argument lists.

## Impact

- Depends on `attachment-image-preview` (the `Attachment` metadata and the `/file-upload/` path rule). Archive that change first; the keymap delta below is written on top of its vocabulary.
- `src/api/wire.rs`, `src/api/map.rs`, `src/data/mod.rs`: video and audio fields on `Attachment`.
- `src/api/mod.rs`, `src/effect.rs`, `src/action.rs`: a streaming download to disk and a spawn effect.
- `src/config/settings.rs`, `src/config/keymap.rs`, `src/input/keymap.rs`: the `[open]` table, the action and its key.
- A new way for the app to run another program: the config's argument list is the user's own, but it is the first place the client starts a process, so it is called out in the design.
- Features list in the Obsidian vault is updated on archive.
