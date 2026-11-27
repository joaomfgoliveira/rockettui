# Design

## Context

See proposal.md for motivation. Current state that shapes the approach:

- `Message` (`data/mod.rs`) keeps only a body. `api/map.rs` appends ` [attachment: name]` to it for each file and attachment title; `api/wire.rs` reads only `name` and `title` (`WireFile`, `WireAttach`). `plain_text` is false for such messages.
- Custom emoji already draw images: `kitty.rs` (upload in chunks, Unicode placeholder cells, delete at exit), `emoji_image.rs` (defensive decode, disk cache keyed by name and etag), `Effect::FetchEmojiImage` / `Action::EmojiImageReady` / `EmojiImageFailed`, and capability detection in `tui.rs` (`App.graphics`). The placeholder cells are ordinary text, so wrapping, scrolling and redrawing need no bookkeeping.
- The message pane pre-wraps every message into rows (`wrap::block_lines`), then draws a slice of them. Row heights are known before drawing, which is why custom emoji reserve two columns before their image arrives.
- Attachment files are served by Rocket.Chat under `/file-upload/<id>/<name>` and need the user's auth headers. Attachments of image type carry `image_url`, `image_type`, `image_size` and `image_dimensions`; link previews of other sites also use `image_url`, with an absolute address.
- Elm/TEA: one `Action` enum, one reducer. Action names and settings are user-facing vocabulary.

## Goals / Non-Goals

**Goals:**
- A room with images looks like the web client's: open it, see the pictures, with no jumping rows.
- Reuse the emoji stack (kitty, decode, cache, fetch limits) instead of building a second one.
- Fetching stays that of a normal client, and bytes stay untrusted.

**Non-Goals:**
- Video and audio (the `attachment-open-external` change), animated playback, link-preview images, drawing images outside the main message flow, uploads.
- Sixel or iTerm2 image protocols. Only kitty, as today.

## Decisions

**1. Keep the body text, add an image block under it.**
The caption `[attachment: name]` stays in the body exactly as today; image rows are extra rows appended to the message's block in `wrap::block_lines`. Non-kitty terminals, search results, the thread pane and the failure case therefore need no new path, and `plain_text` and editing rules do not change.
Alternative: replace the caption with the image. Rejected: it changes every surface that shows `body` and loses the file name.

**2. A new `Attachment` list on `Message`.**
`Message.attachments: Vec<Attachment>` with `kind`, `name`, `path`, `mime`, `size`, `dims`. `Debug` prints counts only. A pure function in `api/map.rs` builds it from `file`, `files` and `attachments`, deduplicating the way the caption already does. Only `path`s that start with `/file-upload/` are kept as fetchable; anything else (absolute URLs, link previews) keeps its kind and name but has no path, so nothing downstream can request it.

**3. Reserve the box from metadata, fit the real image inside it.**
Box size = pure function of `dims` (or a default 4:3), the pane's inner width, `image_max_rows` and the cell aspect, assumed 1 : 2 (width : height of a cell). The layout is computed in `wrap` before any fetch, so a ready image cannot move rows. The decoded image is scaled to fit inside the box. Failure is final: the box is removed and the row model re-measured once, the way emoji failures already are.
Alternative: wait for the bytes and reflow. Rejected: the pane would jump while the user reads, and the viewport/cursor math assumes known heights.
Risk accepted: a terminal whose cells are not 1:2 shows slightly distorted boxes; the protocol scales the image to the cell area, so it is stretched, not cropped.

**4. One placement per image, many rows.**
`kitty.rs` gets a placement for `cols x rows` cells: the image is uploaded once with a virtual placement of that size, and the placeholder cells for row `r` and column `c` carry the combining marks for `r` and `c`. The mark table grows from the two entries used by emoji to the protocol's full list (the table has 297 entries; the box is capped well below that). Image ids come from a separate range from emoji ids so neither can delete the other; both go in the single delete-all at exit.

**5. Send small pixels.**
The decoded image is shrunk to at most about 640 px on its longer side before upload for the inline box, whatever its source size; a 12-row box does not need more. This keeps each upload to roughly a megabyte of RGBA in the worst case. The full-size view re-reads the cached bytes and shrinks to the terminal's size (at most about 1600 px), and deletes that image when it closes.

**6. Fetch rules mirror emoji, plus the URL guard.**
New `Effect::FetchAttachmentImage { path, size_hint }` and `Action::AttachmentImageReady` / `AttachmentImageFailed` carrying a redacting reference type (lengths only in `Debug`, like `EmojiRef`). The reducer asks for an image when a message with a fetchable image attachment is drawn in the open room, not before: it keeps a per-session set of paths asked, a small queue and a concurrency limit (same constant style as emoji). `api/mod.rs` builds the request from the configured base URL joined to the path, never from a stored absolute URL, so a server-supplied address cannot send the client to another host. The response is read as a stream and stops at the size cap (proposed 8 MiB); a reported `image_size` over the cap is skipped without a request.

**7. Decoding reuses `emoji_image`, with different limits.**
`decode` gets limits as parameters (`MAX_SIDE` 4096 for attachments, 1024 stays for emoji), keeps the format allow-list (png, gif, webp, jpeg; no ico, which is only meaningful for emoji) and the header check before pixels. A still image is one frame; a gif or webp keeps up to 64 frames when animation is on (see 12), else its first. Decoding runs off the reducer thread, as it does for emoji.

**8. Cache by path and size.**
Under the existing cache directory, in a separate `attachments/` folder, keyed by a hash of the file path and the reported size, since the file-upload path already contains the file id. A changed file gets a new id on the server, so there is no etag to track. Nothing in a file name comes from the server text: the key is a hash, so a path cannot escape the directory. The cache has no size bound in this change; the follow-up is noted under Risks.

**9. Full-size view is a state, not a popup.**
`gp` runs `Action::ShowImage`, which, if the message has a ready image, sets `App.image_view = Some(ImageView { message_id, index })`. While it is set the reducer routes keys to the view (`h`/`l` change image, `Esc`/`q` close) and the UI draws an overlay filling the pane. Closing restores the previous mode and cursor, which the view never touched. The view loads the original file (see 13): from the cache, else with one request under a 32 MiB cap, and keeps the inline image if that fails.

**10. Settings are three plain keys.**
`inline_images` (bool), `image_max_rows` (3-40) and `animate_images` (bool). The box width is not a setting: it follows the pane width.

**11. Privacy.**
No name, path, media type or size is logged or formatted. Image bytes are never printed. The graphics query stays the only gate: no capability means no fetch.

**12. Animation: one image per frame, only while on screen.**
Kitty does not advance an animation drawn with placeholders, so, as for custom emoji, every frame is its own terminal image and the cells carry the id of the current frame, chosen from the clock (`anim_ms`). A frame takes consecutive ids from the attachment range. Decoding keeps at most 64 frames (longer ones are cut short), each shrunk to 320 px on its longer side inline and 800 px in the view, which bounds an animation to about 25 MB of pixels inline. Frames go to the terminal only for animated images near the viewport and for the open view, and are deleted when the image leaves them (the same visibility and bound rule as stills). The run loop redraws on a timer only while an animated image is on screen. Row heights never depend on the frame. `animate_images = false` decodes the first frame only.

**13. Thumbnails: the inline picture is not always the original.**
For images the server makes a thumbnail: `image_url` is `/file-upload/<id>/thumb-<name>` and `title_link` is the original. The attachment keeps both paths (`path`, `full_path`); `files` also lists `thumb-<name>`, which is not listed as an attachment when `<name>` is. The inline box fetches `path`, the view fetches `full_path` (the reported size then belongs to the original, so the inline size cap does not apply to the thumbnail). The caption shows the file's size, `[attachment: name (1.2 MB)]`.

## Risks / Trade-offs

- [Frame memory of animations] -> 64-frame and per-frame pixel caps, frames only for images near the viewport, and `animate_images = false`.
- [Originals can be large] -> 32 MiB cap, header dimension check before decoding, fall back to the thumbnail.

- [A cell aspect other than 1:2 distorts the image] -> Accept and revisit if the user sees it; the terminal can report cell size in pixels, which would make this exact later.
- [Large photos cost bandwidth on every first open of a room] -> The size cap, the reported-size skip and the disk cache bound it; room opening is not blocked because images load in the background.
- [Many images in a long history] -> Fetch is lazy by drawing: only messages in the viewport (plus the already loaded page) are asked for. If the design needs a tighter rule, ask only for rows within a screen of the viewport.
- [The terminal's image memory fills up] -> Keep a bounded number of uploaded images and delete the oldest that are off screen; the decoded pixels can be re-uploaded from memory or cache. Sized in the tasks.
- [Unbounded disk cache] -> Not solved here; add a size or age sweep in a later change if it grows.
- [Wrong `image_dimensions` from the server] -> Fit inside the reserved box; never trust them for allocation, only for layout.
- [Mark table size] -> Capped by `image_max_rows` and the pane width; a box wider than the table is cut to it.
