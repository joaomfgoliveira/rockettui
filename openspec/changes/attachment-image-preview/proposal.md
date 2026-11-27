# Proposal

## Why

An image sent to a room shows up in rockettui as `[attachment: photo.png]`. In the Rocket.Chat web client you open the room and see the picture. For a client meant for daily use, images are a large part of what people send, and the terminal this app is built for (kitty) can already draw them: custom emoji use the kitty graphics protocol today.

## What Changes

- Read the attachment metadata the server sends (file URL, original file URL, type, size, pixel dimensions) onto each message, instead of only the name. Nothing about it is ever printed or logged.
- Show the caption as `[attachment: name (size)]` and list an image once: the `thumb-<name>` copy the server makes is not listed.
- Draw an image attachment inline in the message flow, under the message text, inside a box with a maximum size. The box is reserved from the server's reported dimensions before the bytes arrive, so rows do not move when the image lands.
- Fetch the image automatically when its message is drawn (same rule as custom emoji), as a normal client: only from the server's own `/file-upload/` paths, with the user's credentials, a size cap before download, a dimension cap before decode, and a disk cache.
- Add a Normal-mode action `show_image`, bound to `gp`, that opens the image under the cursor message at the largest size the terminal allows; `Esc` closes it. The inline picture can be a thumbnail the server made, so the view loads the original file (from the cache, else one request) and falls back to the thumbnail.
- Play animated gif and webp images, inline and in the full-size view, only while they are on screen, with frame and pixel limits.
- Keep `[attachment: name]` as the caption line, and as the whole drawing on terminals that cannot show images or when an image failed.
- Settings `inline_images` (on by default) and `image_max_rows` to turn the feature off or cap the box height, and `animate_images` (on by default) to show only the first frame of animations.
- List the action in the context popup for a message with an image.

Out of scope: video and audio (see `attachment-open-external`), link-preview images from other websites, image upload, drawing images in the thread pane, search results and the draft.

## Capabilities

### New Capabilities
- `message-attachments`: the metadata carried for a message's attachments, inline image drawing, loading and caching of attachment images, and the full-size view.

### Modified Capabilities
- `keymap`: new bindable action `show_image`, default `gp` in Normal mode.
- `context-help`: a message with an image lists the action with its live key.
- `configuration`: new settings `inline_images`, `image_max_rows` and `animate_images`.
- `message-history`: attachments are no longer always text only; an image attachment can also be drawn as an image, and its caption carries the size.

## Impact

- `src/api/wire.rs`, `src/api/map.rs`, `src/data/mod.rs`: parse and carry attachment metadata on `Message` (redacting `Debug`).
- `src/api/mod.rs`, `src/effect.rs`, `src/action.rs`, `src/emoji_image.rs`: a download-and-decode effect and its actions, sharing the cache and decoding code with emoji.
- `src/kitty.rs`, `src/tui.rs`: multi-row placement (a longer row/column mark table), image ids kept apart from emoji ids, cleanup at exit.
- `src/ui/wrap.rs`, `src/ui/messages.rs`: image rows in the message block; a new overlay for the full view.
- `src/config/settings.rs`, `src/config/keymap.rs`, `src/input/keymap.rs`: the settings, the action and its key.
- New vocabulary (action name, setting names) becomes stable once shipped.
- Features list in the Obsidian vault is updated on archive.
