# Proposal

## Why

A custom emoji picked from the emoji box (`add-emoji-insert`) lands in the draft as `:rtshipit:`, ten characters of text, while the same emoji in a message is a two-column image. The user cannot tell what they are about to send until it is sent. Messages already draw custom emojis as images, so the draft can too.

## What Changes

- On a terminal that can show images, a custom emoji written in the draft (`:name:` or `:alias:` of one the server lists) is drawn as its image, two columns wide, in the room's input bar, the thread's input bar and while editing. Until the image is ready it is drawn as the same two-column stand-in the messages use.
- The text of the draft does not change: sending, editing, saving a draft per room and the thread reply all carry `:name:` exactly as it was typed or picked.
- The cursor treats a drawn emoji as one unit: it never lands inside it, and left, right and Backspace pass over it whole.
- Lines wrap, the cursor column, the scroll and the bar's height follow the drawn width, so nothing is wider than the bar.
- A shortcode typed by hand becomes an image when its closing colon is typed. Inside backticks, and on a terminal without images, nothing is drawn differently.
- The images of the custom emojis in the draft are requested like those of the messages on screen.

Out of scope: drawing unicode shortcodes (`:tada:`) as glyphs in the draft (they are put in as characters by the box already), a preview of formatting, and changing what is sent.

## Capabilities

### New Capabilities
- `draft-emoji`: how custom emojis are drawn in the input bar, how the cursor and the layout treat them, and that the draft's text stays as typed.

### Modified Capabilities

None. `message-emoji` keeps the rules for messages; this capability reuses them for the draft by reference.

## Impact

- `src/app/input_buf.rs` (layout and motions take the emoji view and treat a drawn custom emoji as one unit), `src/ui/emoji.rs` (the scan that finds shortcodes is shared with the draft), `src/ui/input.rs` (draw the rows with image cells and keep their ids), `src/app/emoji.rs` (`request_images` also looks at the draft), `src/app/update.rs` (callers pass the view).
- No new dependency and no change to the effects or the wire.
- Independent of `add-emoji-box-grid`; most useful after `add-emoji-insert`.
