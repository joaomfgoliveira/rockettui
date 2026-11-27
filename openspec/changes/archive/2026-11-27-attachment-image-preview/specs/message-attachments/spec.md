# Spec Delta

## ADDED Requirements

### Requirement: Attachment metadata is carried on messages
The client SHALL keep, for each attachment and file of a message, its kind (image, video, audio or other), its name, the server path of the file, its media type, its size in bytes and, for an image, its pixel dimensions, when the server reports them, and the path of the original file when the picture the server lists is a thumbnail of it. This data SHALL NOT be printed, logged or formatted into an error. A message without attachments, or an attachment missing any of these fields, SHALL still be shown as before.

#### Scenario: Image with metadata
- **WHEN** a message arrives with an image attachment that reports a path, type, size and dimensions
- **THEN** the message keeps them and its text still shows `[attachment: name]`

#### Scenario: Thumbnail and original
- **WHEN** a message lists a file `photo.png` and its thumbnail `thumb-photo.png`
- **THEN** one attachment is kept, with the thumbnail path for the inline box and the original path for the full view, and the caption is `[attachment: photo.png (size)]`

#### Scenario: Missing fields
- **WHEN** an attachment reports only a title
- **THEN** it is kept as an attachment of unknown kind and the message is drawn as it is today

#### Scenario: Debug output
- **WHEN** a message with attachments is formatted with `Debug`
- **THEN** only counts appear, never a name, path or type

### Requirement: Images are drawn inline in the message flow
When the terminal supports the kitty graphics protocol with Unicode placeholders and `inline_images` is on, the client SHALL draw each image attachment of a message in the main message flow, on rows under the message text, below its `[attachment: name (size)]` caption, the size left out when the server reports none. The image SHALL keep its aspect ratio and fit in a box no wider than the message area and no taller than `image_max_rows` rows. Messages SHALL keep scrolling, wrapping and the cursor highlight as for text rows.

#### Scenario: Image in a room
- **WHEN** a room is opened whose message has a ready image attachment
- **THEN** the image is drawn under the message's text inside the box, and the caption stays above it

#### Scenario: Tall image
- **WHEN** an image is taller than it is wide and would exceed `image_max_rows`
- **THEN** it is scaled down so its height is `image_max_rows` rows and its width follows

#### Scenario: Several images
- **WHEN** a message has two image attachments
- **THEN** both are drawn, in the order the server lists them, one below the other

#### Scenario: Cursor on an image
- **WHEN** the cursor is on a message with an image
- **THEN** the cursor highlight covers its rows without hiding or reversing the image

### Requirement: The box is reserved before the image arrives
The rows an image takes SHALL be decided from its reported pixel dimensions, or from a fixed default box when none are reported, before any bytes are fetched. While the image is not ready its box SHALL be drawn blank with a short dim notice, and when it arrives the same rows SHALL show it, so that no row of the message flow moves. An image whose actual shape differs from the reported one SHALL be fitted inside the reserved box.

#### Scenario: Placeholder then image
- **WHEN** the room is opened and the image has not arrived
- **THEN** its box is reserved, and when the image arrives the same rows show it and nothing scrolls

#### Scenario: No dimensions reported
- **WHEN** the server gives no dimensions for an image
- **THEN** a fixed default box is reserved and the image is fitted inside it

#### Scenario: Image failed
- **WHEN** the fetch or the decoding fails
- **THEN** the box is removed once and only the caption remains, as a failure is final for the session

### Requirement: Images are fetched as a normal client
The client SHALL fetch an attachment image only when the message that holds it is drawn in the open room, from the server's own `/file-upload/` path of that attachment, with the user's own credentials, and at most once per path per session. A path that is not a `/file-upload/` path of the connected server, including any absolute URL or a link preview from another site, SHALL NOT be requested. At most a small fixed number of fetches SHALL run at once, and the rest SHALL wait. No request SHALL carry message text, room names or usernames.

#### Scenario: Visible image
- **WHEN** a room is opened whose messages hold three image attachments
- **THEN** each is requested once from its `/file-upload/` path

#### Scenario: Other host
- **WHEN** an attachment's image address is an absolute URL or points outside `/file-upload/`
- **THEN** nothing is requested and only its caption is drawn

#### Scenario: Image over the size limit
- **WHEN** the server reports a size over the download limit
- **THEN** no request is made and only the caption is drawn

#### Scenario: Many at once
- **WHEN** a room has more images than the limit allows at once
- **THEN** the excess wait for a free slot and none is dropped

#### Scenario: Not drawn
- **WHEN** the terminal cannot show images, or `inline_images` is off
- **THEN** no image is requested for drawing

### Requirement: Attachment images are cached
A fetched image SHALL be kept for the session and on disk under the client's cache directory, keyed by the room-independent file path and size the server reports, and read from disk on a later session without a request. A cache that cannot be written SHALL NOT stop the client.

#### Scenario: Next session
- **WHEN** the client starts again and opens the same room
- **THEN** the image is read from disk and not requested

#### Scenario: Cache cannot be written
- **WHEN** the cache directory cannot be created or written
- **THEN** images are kept in memory for the session and no error stops the client

### Requirement: Untrusted image bytes are limited
Image bytes from the server SHALL be treated as untrusted. A response over a fixed size, an image over fixed pixel dimensions read from its header, or bytes that do not decode as png, gif, webp or jpeg SHALL be refused, and the attachment SHALL be drawn as its caption. A decoding failure SHALL NOT crash the client and SHALL NOT show the server's text. An animated image SHALL keep at most a fixed number of frames, each within a fixed pixel size, and only its first when animation is off.

#### Scenario: Oversized response
- **WHEN** the response is larger than the limit
- **THEN** the download stops, nothing is decoded and only the caption is drawn

#### Scenario: Decompression bomb
- **WHEN** the header of an image reports dimensions over the limit
- **THEN** no pixel is decoded and only the caption is drawn

#### Scenario: Not an image
- **WHEN** the bytes are not a decodable image, whatever the reported type
- **THEN** only the caption is drawn and the client goes on

### Requirement: Images are drawn only where they can be shown
The client SHALL draw an attachment image only when the startup graphics query showed support for the kitty graphics protocol with Unicode placeholders, with the same rules as custom emoji. Otherwise the message SHALL be drawn exactly as it is without this feature and no graphics data SHALL be sent.

#### Scenario: Other terminal
- **WHEN** the terminal does not support the protocol
- **THEN** the message shows `[attachment: name]` only and no image is fetched

#### Scenario: Server text cannot draw an image
- **WHEN** a message, file name or title contains the placement characters of the graphics protocol or a graphics escape sequence
- **THEN** they are removed or drawn as inert text and nothing is drawn as an image

### Requirement: Open the image at full size
A Normal-mode action `show_image`, bound to `gp` by default, SHALL open the first image of the message under the cursor in an overlay that fits the terminal, at the largest size that keeps the aspect ratio. With several images on the message, `l` and `h` SHALL move to the next and previous one. `Esc` or `q` SHALL close the overlay and return to the message flow. A message with no image, or one whose image is not ready, SHALL show a status notice and change nothing. The overlay SHALL show the original file, read from the cache or fetched once from its `/file-upload/` path under a fixed size limit, and SHALL fall back to the inline picture when that fails.

#### Scenario: Open and close
- **WHEN** the cursor is on a message with a ready image and the user presses `gp`
- **THEN** the image is drawn as large as the terminal allows, and `Esc` returns to the message flow with the cursor where it was

#### Scenario: Several images
- **WHEN** the message has three images and the overlay is open on the first
- **THEN** `l` shows the second and `h` returns to the first

#### Scenario: Message without an image
- **WHEN** the cursor is on a message with no image and the user presses `gp`
- **THEN** a status notice says there is no image and nothing else changes

#### Scenario: Image not ready
- **WHEN** the image is still loading or failed and the user presses `gp`
- **THEN** a status notice says so and nothing else changes

#### Scenario: Terminal without images
- **WHEN** the terminal cannot show images and the user presses `gp`
- **THEN** a status notice says images cannot be shown here

### Requirement: Animated images play
A gif or animated webp attachment SHALL play, inline in the message flow and in the full-size view, when `animate_images` is on and the terminal can show images. Only animations on screen or near it SHALL have their frames in the terminal; frames of one that leaves SHALL be freed. Row heights SHALL NOT depend on the frame shown. An animation with more frames than the limit SHALL play its first frames only. With `animate_images` off, or for any image that is not animated, the first frame SHALL be shown.

#### Scenario: Inline animation
- **WHEN** a room is opened with a ready animated gif in view
- **THEN** its frames play in the same box and no row moves

#### Scenario: Animation in the view
- **WHEN** the user presses `gp` on a message with an animated image
- **THEN** the full-size view plays it, and closing the view stops it and frees its frames

#### Scenario: Off screen
- **WHEN** an animated image scrolls far out of view
- **THEN** its frames are freed in the terminal and no redraw is scheduled for it

#### Scenario: Too many frames
- **WHEN** an animation has more frames than the limit
- **THEN** it plays its first frames only and the client goes on

#### Scenario: Animation off
- **WHEN** `animate_images` is false
- **THEN** an animated image shows its first frame
