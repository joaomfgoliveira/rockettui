# Design

## Context

`InputBuffer` stores plain text: `lines: Vec<String>`, a byte `col` on a grapheme boundary, and every layout and motion function (`wrap_spans`, `visual_rows`, `cursor_visual`, `view_top`, `left`, `right`, `backspace`, `col_at_display`, ...) walks `grapheme_indices` and measures with `display_width`. The bar draws the rows through `sanitize_line`. Message rows already draw custom emojis: `ui::emoji::draw_shortcodes` finds shortcodes by the `message-emoji` rules and swaps each custom one for `custom_cells(name, look)` (two cells of its image, two stand-in cells, or the text), and `EmojiView::look(name)` says which. `EmojiView` is a small `Copy` value built by `App::emoji_view()`, with `EmojiView::none()` for tests. `sanitize` strips the client's private image-cell characters from any text, so they can never come from the wire or the user. See proposal.md for the motivation.

## Goals / Non-Goals

**Goals:**
- The draft's text, its byte `col` and everything that stores or sends it stay as they are.
- Wrapping, the cursor and the motions agree on the same units, from one place.

**Non-Goals:**
- Drawing unicode shortcodes as glyphs, a markdown preview, or any change to what is sent.

## Decisions

### Keep the text plain; make the layout units coarser
A drawn custom emoji is a *unit* of the layout: the byte range of its `:name:`, drawn two columns wide. The text, `col`, `take`, `insert_str`, the per-room drafts and edit loading are untouched, so nothing can leak a private character into a message and the sanitizer rule keeps holding. Unit edges are always grapheme edges, so a stored `col` is valid even when the look of an emoji changes (an image failing turns the unit back into plain graphemes) and `snap` only has to round down to a unit edge.
*Alternative:* store a private placeholder character in the buffer, as `image_cell` does for drawn message text, and expand it on send. Rejected: `sanitize` strips that range on purpose, `text()` and `take()` would need to expand it, edit loading and pastes would need to collapse it, and a mistake would send a private character to the server.

### One `units()` function, fed the emoji view
`InputBuffer` gains `fn units(&self_line, line_start_state, view: &EmojiView) -> Vec<Unit { start, end, width }>`, where a unit is a grapheme or a drawn custom emoji. The layout functions and the motions call it instead of `grapheme_indices` and take a `&EmojiView` argument (`EmojiView::none()` keeps every existing test and every call that does not draw). `prev_boundary`, `next_boundary`, `floor_boundary`, `col_at_display` and `wrap_spans` become thin walks over the units, so a wrap, a cursor column and a motion cannot disagree.
*Alternative:* store the set of known names in each buffer. Rejected: there is one draft per room and thread, each would need updating when the list loads, and the drawn look still has to come from the app at draw time.

### The scan is shared with messages
The part of `draw_shortcodes` that finds `:name:` by the rules (open and close colon not glued to a letter or digit, name characters, backtick toggling) is extracted as `shortcode_spans(text, state) -> impl Iterator<Item = (Range, name)>` and used by both. The draft passes the backtick state across its lines, since a fenced block spans lines. A span is a unit only when `lookup_found` says the name is custom and `view.look(name)` is not `Look::Text`, which also covers a terminal without images and a failed image. This keeps one definition of "where a shortcode counts".

### Rendering reuses the message path
`ui::input::bar` takes the visual rows and runs each through the same expansion messages use (`custom_cells` for the unit's look, then `expand` into spans with the image id as foreground colour) and calls `keep_image_ids` over the bar after painting, as the message pane does. A unit is never split between rows because `wrap_spans` treats it as one piece, which also gives the "next row" scenario.

### Motions and edits
`left`, `right`, `left_in_line`, `right_in_line` and `backspace` step by unit. `delete_word_back` treats a unit as a word character. `move_line` and `up`/`down` use `col_at_display` over units, so they land on a unit edge. Insert and paste are unchanged. The hardware cursor comes from the same `cursor_visual`, so it sits at the drawn column.

### Images are requested for the draft too
`app::emoji::request_images` already reads the messages on screen and the picker's visible rows; it also scans the live draft with `shortcode_spans`, so a picked or typed emoji gets its image without a message showing it first. The existing once-per-(name, etag) and in-flight limits apply.

## Risks / Trade-offs

- [Every layout function gains an argument; many existing input tests call them] → `EmojiView::none()` makes every old call behave as before, so the existing tests stay as they are and only new tests pass a view.
- [The stand-in and the image have the same width, but a failed image turns a unit back into text, so the rows can change] → the layout is derived on every draw from the same view, nothing stored depends on it, and `col` stays valid; the bar's height follows because it is computed from the same rows.
- [Kitty placeholders in a bar with a reversed or highlighted cell lose their colour id] → `keep_image_ids` is called over the bar, as for messages.
- [Cost of scanning every draw] → drafts are at most 64 KiB and are scanned only when they hold a colon.

## Migration Plan

None. Drafts are plain text and keep their format.

## Open Questions

- Whether Backspace on a drawn emoji should turn it back into editable text (`:rtshipit`) instead of deleting it whole. The spec says whole; changing it later affects one function.
