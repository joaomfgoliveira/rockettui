# Tasks

## 1. Carry attachment metadata

- [x] 1.1 Read `image_url`, `image_type`, `image_size`, `image_dimensions` (and the `file`/`files` id, name and type) in `api/wire.rs`; verify a wire test per shape (attachment with all fields, with only a title, file only)
- [x] 1.2 Add `Attachment` and `Message.attachments` in `data/mod.rs`, built in `api/map.rs`, keeping only `/file-upload/` paths as fetchable and dropping absolute and link-preview addresses; verify mapping tests, including the dedupe and the unchanged body text and `plain_text`
- [x] 1.3 Keep `Message`'s `Debug` redacting for the new field (counts only); verify a test that a sentinel name and path never appear
- [x] 1.4 Add an image attachment message to the docker fixtures used by the mapping tests; verify it parses

## 2. Settings and keymap

- [x] 2.1 Add `inline_images` and `image_max_rows` to `config/settings.rs` with defaults and the range error; verify config tests for default, set, and out of range
- [x] 2.2 Add `Action::ShowImage`, the `show_image` vocabulary entry and the `gp` default in every preset (`input/keymap.rs`, `config/keymap.rs`); verify keymap tests for default, rebind, unbind and the clash check

## 3. Terminal side

- [x] 3.1 Extend `kitty.rs` with a `cols x rows` placement, the full row/column mark table and a separate image id range; verify unit tests for a 1x1, a wide and a tall placement and for ids not overlapping emoji ids
- [x] 3.2 Make `emoji_image::decode` take its limits as parameters and keep the emoji behavior unchanged; add `MAX_SIDE` 4096 and a shrink step for attachments; verify existing emoji tests still pass and new ones for an oversize header, a corrupt file, an animated image and ico being refused
- [x] 3.3 Add the attachment cache (hash of path and size under `attachments/`); verify tests for hit, miss, unwritable directory and that a hostile path cannot leave the directory

## 4. Fetching

- [x] 4.1 Add the fetch to `api/mod.rs` (base URL plus `/file-upload/` path, user headers, streamed with a size cap); verify wiremock tests for success, over the cap, a non-image body, a 404 and that an absolute address is refused
- [x] 4.2 Add `Effect::FetchAttachmentImage`, `Action::AttachmentImageReady`/`Failed` (redacting `Debug`) and the effect runner; verify tests for ready, failed, and the cache short-circuit
- [x] 4.3 In the reducer, ask for images when their message is drawn in the open room only: once per path, a concurrency limit, nothing when graphics are unsupported or `inline_images` is off; verify reducer tests for each rule and for a reported size over the cap

## 5. Drawing

- [x] 5.1 Add the pure box-size function (dimensions, pane width, max rows, 1:2 cell aspect, default box) in `ui/`; verify unit tests for wide, tall, tiny, missing dimensions and a narrow pane
- [x] 5.2 Append image rows (placeholder notice, then image cells) to the message block in `ui/wrap.rs`; strip placement characters from server text in `sanitize`; verify snapshot tests for pending, ready, failed (box removed), two images, the cursor row and the unsupported terminal
- [x] 5.3 Upload ready images before drawing and delete the oldest off-screen ones past a bound in `tui.rs`; verify tests of the bound and that exit deletes everything

## 6. Full-size view

- [x] 6.1 Add `App.image_view`, the `ShowImage` reducer arm (notices for no image, not ready, unsupported terminal) and key routing for `h`, `l`, `Esc`, `q`; verify reducer tests for open, change, close, and the three notices
- [x] 6.2 Draw the overlay and upload the larger image from the cache, deleting it on close; verify snapshot tests of the overlay and that no request is made
- [x] 6.3 List the action in the context popup for a message with an image, with the live key; verify tests for listed, not listed, rebound and unbound

## 7. Thumbnails and caption

- [x] 7.1 Read `title_link` and the file `size`, keep `Attachment.full_path`, drop `thumb-<name>` when `<name>` is listed, and caption `[attachment: name (size)]`; verify mapping tests for the thumbnail shape and the size labels
- [x] 7.2 Load the original (`full_path`) for the full-size view from the cache, else one request under a 32 MiB cap, falling back to the thumbnail; do not apply the inline size cap to a thumbnail; verify effect and reducer tests

## 8. Animation

- [x] 8.1 Add the `animate_images` setting (default true) and its tests (default, off, wrong type)
- [x] 8.2 Let the attachment decode keep up to 64 frames at 320 px (inline) and 800 px (view) when animation is on, first frame otherwise; verify tests for a gif, an animated webp, too many frames, a still image and animation off
- [x] 8.3 Give each frame its own terminal id from the attachment range, choose the id of the current frame from `anim_ms` when drawing, and keep row heights frame independent; verify drawing tests that no row moves between frames
- [x] 8.4 Send frames only for animated images near the viewport and for the open view, free them when they leave (same bound and visibility rule as stills), and redraw on a timer only while one is on screen; verify tests of upload, free, the bound and no timer when none is on screen
- [x] 8.5 Play the animation in the full-size view and stop and free it on close; verify view tests

## 9. Integration

- [x] 9.1 Run `cargo fmt --check`, `cargo clippy` and `cargo test`; verify they pass
- [ ] 9.2 Ask the user to check inline images, no row jumping while they load, `gp` (full quality), animated gifs and webps inline and in the view, and a room with a large photo by eye against their local Docker server (kitty terminal); the checks are theirs to run
- [ ] 9.3 Update the Features list in the Obsidian vault (tick or add the line, refresh `Progress` and `Last updated`) when the change is archived
