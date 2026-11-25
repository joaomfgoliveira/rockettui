# Design

## Context

See `proposal.md` for why. Today an emoji is a character: `draw_shortcodes` rewrites `:name:` into text, `wrap_body` wraps that string with `textwrap`, `highlight_row` slices it by byte ranges into spans, and `reaction_rows` builds `"{glyph} {n}"` strings. Every width is measured with `unicode-width`, and `App::row_ctx` is the one place a `RowCtx` is built, so the pane and the scroll model share one `render_rows`. The picker already owns the custom list (`CustomEmojis`, `Effect::LoadCustomEmojis`, `Client::custom_emojis`). The HTTP client disables redirects and sends the auth headers on every request. `tui.rs` initialises ratatui and bracketed paste, and nothing queries the terminal.

The design leans on one property of the kitty graphics protocol: with Unicode placeholders, an image is shown wherever cells hold `U+10EEEE` followed by combining marks that give the cell's row and column inside the image, and the image id is in the cell's foreground colour. A placeholder cell is therefore text, so ratatui's diff, scrolling and clearing handle it without any placement bookkeeping. Whether `unicode-width` and `textwrap` measure such cells as one column each is an assumption that the spike (task 1) must confirm before the rest is built.

## Goals / Non-Goals

**Goals:**
- Custom emojis drawn inline in bodies and reaction rows, wrapping and scrolling like text.
- Row counts that never change when an image arrives (a fixed two-column reservation).
- The existing string pipeline kept: no rewrite of wrapping, highlighting or the row model.
- Level 1 (the styled `:name:`) as the always-available fallback, which also ships first.

**Non-Goals:**
- Images in the picker list or the reactions popup, animation, sixel and iTerm2 protocols, emoji upload.
- Sizing images to the font beyond a fixed two-by-one cell box.
- A general image-in-chat facility (attachments, previews).

## Decisions

### D1. Unicode placeholders, not placed images
Placed images (cursor-positioned, deleted by id) need the client to track every image's screen position through wrapping, scrolling, popups drawn over the list and ratatui's diff redraw. Placeholders move that work to the terminal. Alternative: ratatui-image's widget model, one widget per image at computed coordinates, which brings the bookkeeping back and cannot sit inside a wrapped `Line`. Rejected.

### D2. The pipeline stays string-based; the span split is at the end
`draw_shortcodes` emits, for a custom emoji whose image is ready, a run of two placeholder cells; for the fallback, the `:name:` cut or padded to two columns' worth of text. After wrapping, the step that builds spans (`highlight_row` for bodies, `reaction_rows` for reactions) splits a row at each placeholder run and gives that run a style whose foreground is the image id, in place of the body style. No other stage learns about images.
- Alternative: segment the body into `Text` / `Emoji` values and wrap those. Cleaner types, but it rewrites `wrap_body` (a `textwrap` call), `highlight_row` and the reaction items for a gain the placeholder encoding makes unnecessary. It is the plan B if the spike shows the width assumption fails.
- The fallback is chosen when the text is built, so the drawn text depends on the image state. That is why the reservation is fixed: the fallback and the placeholder run are both exactly two columns.

### D3. Two columns while an image can still arrive, the full name otherwise
On a terminal that can show images, a custom emoji takes two columns from the first draw: the placeholder run once the image is ready, and a two-column stand-in in the custom style until then. A failed image falls back to the full `:name:`, and because a failure is final for the session the message is re-measured once. On a terminal that cannot show images the full `:name:` is drawn from the start and never changes, so such a terminal loses nothing against today.
- Alternative: always two columns. Rejected: it would hide the name on terminals without graphics.
- **Decided (spike):** the stand-in is two NBSP (`U+00A0`) cells in the custom style, so it is two columns and has no break opportunity. The spike measured that two spaces change wrapping (spaces are break points) and that a `:`-led cut name does too (`:` is a break-class character); two NBSP, or two plain letters, matched the placeholder run's wrapping at every width from 4 to 60.

### D4. Ownership of the list moves from the picker to the session
`CustomEmojis` moves out of `app/react.rs` to a small `app/emoji.rs` owner with the same three states. `Effect::LoadCustomEmojis` is issued when the rooms have loaded, and again when a room opens while the state is `NotLoaded`. The picker reads the list from the same place and stops issuing the effect itself. `RowCtx` gains a view of it so resolution inside `render_rows` is a pure function of state, like aliases are today.

### D5. Fetching and decoding are an Effect and an Action
`Effect::FetchEmojiImage { name, etag, ext }` → `Action::EmojiImageReady { name, image }` or `Action::EmojiImageFailed { name }`. Which emojis to fetch is decided in the reducer when a message or reaction naming one is first drawn (a set of requested names prevents duplicates), with a small concurrency cap and a queue. Decoding runs off the UI task, with a byte limit on the response and a pixel limit on the decoded image, and for animated formats at most 64 frames, each shrunk like the first (each frame is an image of its own and the client swaps the id the cells carry on a 40 ms timer while an animated emoji is on screen; the terminal's own animation commands did not play on placeholder images). The decoded and uploaded image lives in `App` once per emoji, never per occurrence. The reducer stays one function and the channel stays one channel.

### D6. Cache on disk under the XDG cache dir
`~/.cache/rockettui/emoji/<name>-<etag>.<ext>`, written atomically, read on a later session when the listed `etag` matches. A cache that cannot be used degrades to memory only. The name is sanitised to a file-name-safe form before it touches a path, because it is server text.

### D7. Terminal support is decided once, before the event loop
Query the terminal for the kitty graphics protocol and cell size at startup, with a short timeout, before raw-mode input is shared with the event stream. The result (`Supported(font size)` or `Unsupported`) is stored in `App` and read by the resolver. Nothing is sent to a terminal that did not answer yes. This sits in `tui.rs` next to bracketed paste, which also has to be undone on exit paths.

### D8. Dependency: decided by the spike
Option A, `ratatui-image` 11.x (targets ratatui ^0.30.1): does the query, the upload and the placeholder encoding. Option B, a hand-written encoder: base64 chunked upload, the placeholder row and column diacritic table, and the capability query, a few hundred lines with a learning value in line with the project's DDP client. The spike builds the placeholder run by hand either way, since D2 needs the characters themselves and not a widget; if the crate cannot give the cells as text, B is chosen. The image decoder (`image` crate with only png, gif and webp enabled) is needed in both.

**Decided (spike):** Option B, a hand-written encoder. The spike's chunked base64 upload, virtual placement (`a=T,U=1,c=2,r=1`) and placeholder cells took about 40 lines and displayed correctly in kitty, inside wrapped lines, while scrolling and under a popup, with no ghosts. `ratatui-image` is not needed. The capability query (task 4.1) is still to write. Spike findings: a placeholder cell is one column and one grapheme, a run is two, and `textwrap` never splits a run at widths of 4 or more (the app never wraps under `MIN_BODY` 10).

### D9. Server text cannot produce placeholders
`sanitize` and `sanitize_line` already strip control characters from message bodies, usernames and room names. They additionally drop `U+10EEEE` and the placement marks, so only the client's own emission after sanitising can contain them. The order matters: sanitising happens in `wrap_body` after `draw_shortcodes`, so the emission has to move after it or `wrap_body` has to be told to keep the client's own cells. Task 2 settles this with a test that a body containing the characters draws nothing.

### D10. What the servers answered (measured, task 3.5)
`dev/probe_emoji_image.py` creates a real 1x1 png emoji as admin and asks for it on the local Docker servers, 8.9.1 and 9.0.0 (same answers on both):
- `GET /emoji-custom/<name>.<ext>` is at the server root, not under `/api/v1`. It answers 200 with `Content-Type: image/png` and the bytes.
- It answers the same with the `X-User-Id`/`X-Auth-Token` headers, with no headers at all, with the listed `etag` as a query, with another `etag` and with none. So the path needs no credentials and the `etag` is only a cache key. The client still sends its headers, as on every request (the default headers of the one HTTP client); this is no extra exposure to the server that already holds them, and redirects stay off so they never reach another host.
- A name that does not exist is not a 404: the server answers 200 with a generated 50x50 `image/svg+xml` placeholder. The decoder accepts only png, gif and webp, so this is refused as "not an image" and the emoji gets the fallback.
- The list entry has `extension` (`png` here) and `etag`; both are kept on `CustomEmoji`.

**Decided (implementation):** the cell size from the capability query (D7) is not needed: an emoji is a fixed two-by-one cell box and the terminal scales the image into it. The query asks only whether images are supported (`a=q`), and it is sent only when `TERM`, `KITTY_WINDOW_ID` or `TERM_PROGRAM` say the terminal may be kitty or ghostty and no multiplexer is in between, so no other terminal receives it. Images are fetched only when the terminal answered yes.

## Risks / Trade-offs

- [Width assumption fails: `unicode-width` or `textwrap` count placeholder cells wrongly] → The spike checks it first. Plan B is D2's alternative.
- [Terminal draws placeholders badly inside some other layer, such as tmux, or in kitty versions that differ] → Support is queried, not assumed. Anything unanswered gets the fallback.
- [Image state changes the drawn text, so a cached row or the scroll model could go stale] → Both columns of every reservation are fixed, and `RowCtx` is the single input. A test asserts `message_rows` is equal with and without images ready.
- [Burst of requests in a busy room] → Fetch only what is drawn, concurrency cap, a queue, once per name and `etag`.
- [Hostile image: huge, malformed, decompression bomb] → Byte limit before decode, pixel limit on the header before decoding, at most 64 frames of an animation, no panics on decode errors.
- [Redirects are off, and emoji paths on some servers redirect, such as to a CDN] → An emoji that redirects shows the fallback. Not worked around, so the token is never sent to another host.
- [Whether the emoji path needs the auth headers is unverified] → Send them as for every other request unless the local server shows otherwise; checked against the Docker server in task 5.
- [The emission-after-sanitise order in D9 is easy to get wrong] → A dedicated test; the spec's last requirement states the property.
- [Visual correctness cannot be checked by an agent] → Snapshots cover layout and the fallback; a final task asks the author to look in kitty.

## Migration Plan

No data or config migration. A new optional theme element for the fallback style, with a default, so no existing theme breaks. The cache directory is created on first use and can be deleted at any time. Rollback is reverting the change: nothing persistent depends on it.

Ships in three slices, each usable on its own: (1) list ownership and the styled fallback (this is Level 1), (2) fetch, decode and cache, (3) the capability query and placeholder drawing.

## Open Questions

- Fallback look while an image loads (D3): a blank in the custom style or a cut name. A visual choice that does not change the specs.
- Whether the picker rows and the popup should get images next, or stay as `:name:`.
