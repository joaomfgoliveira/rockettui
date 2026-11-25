# Proposal

## Why

A server's custom emojis (`:rtshipit:`) are images on the server. Today the client draws them as raw `:name:`, so a message or a reaction that uses one reads the same as a typo, and a room that leans on custom emojis loses much of its meaning. The author uses kitty, whose graphics protocol can draw images as text-like cells, so the images can be shown in the message list itself.

## What Changes

- Load the server's custom emoji list early (when the session starts), not only when the reaction picker first opens, so message text and reactions can tell a custom emoji from an unknown name.
- Resolve a shortcode or reaction name against the custom emojis (by name and by alias) after the user's aliases and the unicode table, as a fourth source.
- Fetch each custom emoji's image from the server once, with the user's own credentials, and keep it for the session and on disk, keyed by the server's `etag` so a changed image is fetched again.
- Draw a custom emoji whose image is ready as a two-cell image inside the message body and the reaction row, using the kitty graphics protocol with Unicode placeholders, so it wraps, scrolls and redraws like text.
- Draw a custom emoji whose image is not ready, failed, or cannot be shown (terminal without the protocol, an image over the limits) as its `:name:` in a theme style, at the same two-cell reservation so rows never reflow when an image arrives.
- Animated images (gif, webp) are sent to the terminal as one image per frame (up to 64) and played by the client, which changes the image id of the cells on a timer; kitty does not animate an image drawn with placeholders.
- Keep server text from ever producing a placeholder cell itself.
- A throwaway spike comes first and gates the rest: it checks that placeholder cells inside a wrapped, scrolling list measure, redraw and clear correctly in kitty. If it fails, the design is revised before anything is built.
- Draw the image in the reaction picker rows and the reactions popup too (their glyph column), and in search results and their context view, fetching what those lists show.
- Out of scope: sixel or iTerm2 images, uploading emojis, and custom emojis in the input bar.

## Capabilities

### New Capabilities
- `custom-emoji-images`: loading the server's custom emojis, fetching and caching their images, deciding when a terminal can show them, the limits on what is decoded, and the fallback when an image cannot be shown.

### Modified Capabilities
- `message-emoji`: a shortcode that names a custom emoji is drawn as that emoji (image or styled fallback) instead of as written, with a fixed two-cell width; the "unknown shortcode" scenario changes accordingly, "Reactions use the server's names too" covers custom names, and a new requirement fixes the two-column width.
- `reaction-picker`: the custom emoji list is loaded when the session starts instead of on the first opening, and the picker uses that same list.

## Impact

- Code: `src/ui/emoji.rs` (resolver gains the custom source), `src/ui/wrap.rs` (span split after wrapping, reaction rows), `src/app/react.rs` (custom list ownership moves out of the picker), `src/api/mod.rs` (image fetch), `src/effect.rs` and `src/action.rs` (fetch Effect, result Action), `src/tui.rs` (terminal capability query before the event loop), `src/config/theme.rs` (a style for the fallback), `src/ui/sanitize.rs` (placeholder characters never pass from server text).
- Dependencies: an image decoder, and either `ratatui-image` or a small hand-written kitty encoder (decided in the design).
- Network and privacy: new authenticated GETs to `/emoji-custom/<name>.<ext>`, the same URLs the web client uses. Image bytes are untrusted input: size and dimension limits apply, and nothing about messages or names is logged.
- Specs: one new capability, two modified.
