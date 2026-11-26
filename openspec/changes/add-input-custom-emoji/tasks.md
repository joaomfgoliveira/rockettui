# Tasks

## 1. Shared scan

- [x] 1.1 Extract the shortcode scan of `ui::emoji::draw_shortcodes` into `shortcode_spans` that carries the backtick state in and out; verify the whole existing `message-emoji` test suite passes unchanged
- [x] 1.2 Add `units()` to the input buffer: graphemes plus drawn custom emojis (custom by name or alias, look not `Text`, width 2); verify with unit tests for a known name, an alias, an unknown name, `a:name:`, a name inside backticks and a fenced block across lines, and a terminal without images

## 2. Layout and motions

- [x] 2.1 Make `wrap_spans`, `visual_rows`, `visual_row_count`, `cursor_visual`, `view_top`, `settle` and `half_page` use units and take the emoji view, with `EmojiView::none()` at the existing call sites; verify the existing input tests pass unchanged and add tests for wrap by drawn width, not splitting a unit, and the cursor column after `ab` and an emoji
- [x] 2.2 Make `left`, `right`, `left_in_line`, `right_in_line`, `backspace`, `delete_word_back`, `move_line` and `col_at_display` step by unit, and `snap` round to a unit edge; verify with tests for each scenario of "A drawn emoji is one unit for the cursor", including a single Backspace deleting the whole `:name:`
- [x] 2.3 Pass `app.emoji_view()` from `edit_input`, `input_viewport` and the cursor code; verify the reducer tests for Insert editing still pass and one new test covers Backspace on an emoji through the reducer

## 3. Drawing

- [x] 3.1 Draw the rows of the bar through the messages' expansion, with stand-in and image cells, and call `keep_image_ids` over the bar, in the room bar and the thread bar; verify with insta snapshots for an image, a stand-in, a terminal without images and a narrow bar
- [x] 3.2 Request the images of the live draft in `request_images`; verify with a reducer test that a picked custom emoji asks for its image once, and none when it was already loaded
- [x] 3.3 Check the draft's text stays as typed: send, edit save, per-room draft and an alias; verify with tests that the sent text equals the typed text for `:rtship:`

## 4. Integration

- [x] 4.1 Run `cargo test`, `cargo clippy` and `cargo fmt --check`; verify all pass
- [x] 4.2 Ask the user to try it in kitty against the local Docker server: pick a custom emoji with `:`, type one by hand, move over it, wrap a long draft, edit a message that holds one; verify by the user's confirmation

## Workflow follow-up

- Update `/home/joliveira/Documents/Obsidian Vault/Personal/Projects/Rockettui/Features.md` when the change is archived.
