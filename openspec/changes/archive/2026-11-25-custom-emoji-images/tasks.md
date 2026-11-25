# Tasks

## 1. Spike (gate: do not start group 2 until this is answered)

- [x] 1.1 Write a throwaway binary under `dev/` (not part of the crate's `src/`) that uploads one PNG to kitty and draws a two-cell placeholder run inside a `textwrap`ped `ratatui` line, and verify by running it in kitty that the image shows inside the text.
- [x] 1.2 In the same spike, measure the placeholder run with `unicode-width` and `textwrap` and verify with an assertion that it counts as two columns and is never split across rows.
- [x] 1.3 In the same spike, scroll a list that holds the run, draw a popup over it and close the popup, and verify there are no ghost images or stray cells after redraws.
- [x] 1.4 Decide D8 (`ratatui-image` or a hand-written encoder) and D3's stand-in look from what was measured, record both in `design.md` under Decisions, and verify the design still says what will be built. If 1.2 or 1.3 failed, revise D2 to the segment alternative before going on.

## 2. Session-owned custom emoji list and the styled fallback (Level 1)

- [x] 2.1 Move `CustomEmojis` out of `app/react.rs` into a session-level owner, load the list when the rooms have loaded and again on opening a room while it is `NotLoaded`, and verify with reducer tests for the three scenarios of "The custom emoji list is loaded at session start".
- [x] 2.2 Make the picker read that list and stop issuing its own load, and verify the "Custom emojis from the server" scenarios of `reaction-picker` with tests, including a list arriving while the picker is open.
- [x] 2.3 Add the custom source to the shortcode resolver (name and alias, after unicode, aliases and server names) and a theme element for the custom-emoji style with a default, and verify with unit tests for the `message-emoji` scenarios (custom, by alias, unicode wins) and that an existing theme file still loads.
- [x] 2.4 Draw custom emojis in bodies and in the reaction row as the full `:name:` in the custom style, leave code spans alone, and verify with snapshots; verify the popup and picker still show `:name:`.
- [x] 2.5 Update `openspec` docs in `README` or the help text where emoji are described, and verify the text matches the behavior.

## 3. Fetch, decode and cache

- [x] 3.1 Add `Client::emoji_image` (authenticated GET of the server's emoji path, redirects stay off, byte limit while reading) and verify with `wiremock` tests: success, over the limit, 404, a redirect, and that no request carries message text.
- [x] 3.2 Add the decoder (first frame only, pixel limit read from the header before decoding, only png, gif and webp) and verify with tests for a good image, an oversized one, a non-image and an animated one.
- [x] 3.3 Add the disk cache under the XDG cache dir (file name safe from the sanitised name and `etag`, atomic write, memory-only when unwritable) and verify with tests for a hit, a changed `etag`, an unwritable directory and a hostile name such as `../x`.
- [x] 3.4 Add `Effect::FetchEmojiImage`, `Action::EmojiImageReady` and `Action::EmojiImageFailed`, with a requested-names set, a concurrency cap and a queue, issued from the reducer when a drawn message or reaction first names an emoji, and verify with reducer tests for "once per name", "unused is not requested" and "many at once".
- [x] 3.5 Verify against the local Docker Rocket.Chat (8.9.1 and 9.0.0) with `rtshipit` that the image is fetched, that the emoji path works with and without the auth headers, and record what was found in `design.md`.

## 4. Terminal support and image drawing

- [x] 4.1 Query the terminal for kitty graphics support and cell size at startup in `tui.rs` with a short timeout, store `Supported` or `Unsupported` in `App`, and verify the client starts normally with no answer, with an answer of no, and sends no graphics data in either case.
- [x] 4.2 Strip the placement characters from server text in `sanitize` and `sanitize_line`, and verify with tests for a body, a username, a room name and a reaction name containing them, and for a raw graphics escape sequence staying inert.
- [x] 4.3 Emit the two-cell stand-in or the placeholder run for a custom emoji on a supporting terminal, after sanitising, and split the spans at each run with the image id as the foreground, in both `highlight_row` and `reaction_rows`; verify with tests that `message_rows` is the same with and without the image ready, that no row is wider than the pane, and that a failed image is re-measured once.
- [x] 4.4 Upload each decoded image once when it is ready and keep it per emoji in `App`, and verify with a test that five uses of one emoji upload once.
- [x] 4.5 Run the full test suite, `cargo clippy` and `cargo fmt --check`, and verify all pass.

## 5. Integration check

- [x] 5.1 Look in kitty at a room with `rtshipit` in a body, in a reaction, wrapped at a narrow width, scrolled, with the reaction popup over it, and while images are loading; the author confirms there are no ghosts, no reflow when images arrive, and that the fallback reads well.
- [ ] 5.2 Look in a terminal without graphics support (or with `TERM` that does not answer) and confirm the full `:name:` is drawn and nothing else changes.

## 6. Follow-up asked after the first look

- [x] 6.1 Draw the image in the glyph column of the reaction picker and the reactions popup, in search results and their context view, request the images those lists show, and keep the selected row from reversing the image cells; verified with render tests.
- [x] 6.2 Keep all frames of an animated gif or webp (at most 64, delays floored at 20 ms), upload each as its own image and swap the id the cells carry on a timer (kitty's own animation did not play on placeholder images); verified with decoder, id and timing tests.
- [x] 6.3 The author looks in kitty at the picker, the popup, a search result and an animated emoji (`dev/demo_emoji.py up` creates `rtspin`).

## Workflow follow-up

- Archive the change after the author's check in 5.1 and 5.2 and sync the specs.
