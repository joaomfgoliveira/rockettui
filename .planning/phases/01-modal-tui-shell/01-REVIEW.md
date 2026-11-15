---
phase: 01-modal-tui-shell
reviewed: 2026-11-15T00:00:00Z
depth: standard
files_reviewed: 30
files_reviewed_list:
  - .gitignore
  - Cargo.toml
  - scripts/smoke_pty.py
  - src/action.rs
  - src/app/command.rs
  - src/app/input_buf.rs
  - src/app/messages.rs
  - src/app/mod.rs
  - src/app/sidebar.rs
  - src/app/update.rs
  - src/data/fake.rs
  - src/data/mod.rs
  - src/event.rs
  - src/input/key.rs
  - src/input/keymap.rs
  - src/input/matcher.rs
  - src/input/mod.rs
  - src/input/trie.rs
  - src/lib.rs
  - src/main.rs
  - src/test_util.rs
  - src/tui.rs
  - src/ui/input.rs
  - src/ui/layout.rs
  - src/ui/messages.rs
  - src/ui/mod.rs
  - src/ui/sanitize.rs
  - src/ui/sidebar.rs
  - src/ui/status.rs
  - src/ui/wrap.rs
findings:
  critical: 0
  warning: 5
  info: 8
  total: 13
  necessity: 0
status: issues_found
---

# Phase 1: Code Review Report

**Reviewed:** 2026-11-15
**Depth:** standard
**Files Reviewed:** 30
**Status:** issues_found

## Summary

The phase is in good shape. I found no critical issues. The checks I ran against the scope you asked about:

- **Panic resistance.** I ran two throwaway fuzzers on a copy of the repo, not in the tree.
  - An `InputBuffer` fuzzer: 3000 rounds of random edit and motion sequences over mixed Unicode (CJK, ZWJ, combining marks, regional indicators, tabs, newlines, NUL) at random wraps of 1 to 12 columns. It found no panic. It also found no broken invariant: `col` stayed on a char boundary, `visual_rows` and `visual_row_count` agreed, and the cursor row stayed inside the rows.
  - A full-app fuzzer: 400 rounds of random keys, pastes, timeouts and resizes, with a `TestBackend` draw after every step at sizes from 1x1 to 90x30. It found no panic.
- **Escape-sequence injection.** The `sanitize` choke point holds. Every server-controlled or user-controlled string I traced (room, folder and author names, message bodies, the status message, the command line, the pane titles) goes through `sanitize` or `sanitize_line` before it is drawn. C0, DEL and C1 characters are stripped, and the bidi controls are stripped too. See WR-04 for the invisible characters it still lets through.
- **Debug output and logs.** Nothing logs, and every type that holds typed or message text has a redacting `Debug`. This covers `Action`, `Message`, `Chord`, `Outcome`, `Matcher`, `InputBuffer`, and `App` and `StatusMsg`, which have no `Debug` at all. No message content reaches logs or `Debug`.
- **Terminal restore and channel use.** The terminal restore on panic and on error paths is correct, apart from the two gaps in IN-01. The channel is bounded, and every other buffer is capped: the draft at 64 KiB, the command line at 256 characters, and counts at 9999.

The real defects are behavioural. Content can be unreachable in the messages pane (WR-01). The sidebar scroll position is not stable (WR-02). The minimum terminal size is inconsistent with the fixed layout constants (WR-03). `sanitize` misses a set of invisible characters (WR-04). The status bar can hide pending keys and errors (WR-05).

I read `necessity-review.md` and assessed necessity. I found no necessity findings. The guards I looked at defend real trust boundaries or are Phase 2 and Phase 3 seams that locked decisions mandate.

## Warnings

### WR-01: A message taller than the messages pane can never be read to its end

**File:** `src/app/messages.rs:46-62` (and `:136-148`)
**Issue:** `scroll_into_view` aligns a message taller than the viewport to its first row (`end - first > viewport_h` gives `top = first`). This runs on every `j`, `k`, `Ctrl-d` and `Ctrl-u`, so no key can scroll inside such a message. `Ctrl-d` on the tall message adds `delta` to `top_row`, and `scroll_into_view` immediately resets it to `first`.

I verified this on the shipped fake data. At 80x24, the long message in `general` is 20 rows and the viewport is 17 rows. With the cursor on that message, `top_row` stays at 37 through repeated `Ctrl-d` presses, and the last visible row is 54 against a message end of 57. The last 3 rows can never be shown. A pasted stack trace or a log excerpt of a few dozen lines is routine in chat, and today its tail is unreadable in the TUI.

**Fix:** When the cursor message is taller than the viewport, let the intra-message scroll move `top_row` within `first..=end - viewport_h`. For example, in `scroll_into_view`:
```rust
let top = if end - first > viewport_h {
    // keep the message pinned, but allow scrolling inside it
    top_row.clamp(first, end - viewport_h)
} else if first < top_row {
    first
} else if end > top_row.saturating_add(viewport_h) {
    end - viewport_h
} else {
    top_row
};
```
Then make `Ctrl-d` and `Ctrl-u` scroll within the message before they move to the neighbouring message. Alternatively, let `j` and `k` step by row when the current message is taller than the pane. Add a test for the 20-row fake message at 80x24.

### WR-02: The sidebar list scroll offset is rebuilt every frame, so the cursor sticks to the bottom row

**File:** `src/ui/sidebar.rs:85-89`
**Issue:** `ListState::default()` is created inside `render` on every frame, so the scroll offset always restarts at 0. ratatui then picks the minimal offset that shows the selection. Once the cursor is below the fold, it is pinned to the bottom row, and moving up scrolls the whole list under a stationary highlight.

I verified this with a 30-room list in a 10-row pane. The cursor went from index 16 to 15 to 14 and the highlight stayed on screen row 8 each time, while the first visible row changed from room08 to room07 to room06. There is no vim-style `scrolloff` behaviour, and the list jumps. The fake data (8 rooms) fits on screen, so no test sees it. A real server has dozens to hundreds of rooms.

**Fix:** Keep the `ListState` (or just its offset) in `SidebarState` and render with it. Because `view` takes `&App`, either clamp the offset in the reducer, as `messages::view` does, or hold the offset in a `Cell`. Alternatively, compute the window yourself with the same `scroll_into_view` idea used for messages.

### WR-03: Minimum terminal size is inconsistent with the fixed layout constants

**File:** `src/ui/mod.rs:16-18` (with `src/ui/layout.rs:7` and `src/app/input_buf.rs:9`)
**Issue:** `MIN_WIDTH = 40` and `MIN_HEIGHT = 8` do not account for `SIDEBAR_WIDTH = 28` or `MAX_INPUT_ROWS = 5`.
- At 40 columns the main column is 12 wide, so the messages pane has 10 inner columns. The compact prefix `HH:MM author  ` is 13 or more columns wide, and `render_rows` floors the body at `MIN_BODY = 10`. Each row is clipped at the pane edge, so at 40 to roughly 55 columns the message body is never visible.
- At 8 rows with a draft of 5 or more rows, the input bar takes the whole body. I printed `areas(Rect(0,0,40,8), 5)`: `messages.height = 0` and `input.height = 6`. At 10 rows the messages pane is 1 row tall.

The "terminal too small" notice therefore does not guard against terminals that are in fact too small to use. The wrapping and scroll code stays panic-safe (the fuzzer confirmed this), but the UI is silently unusable.

**Fix:** Derive the minimum from the layout, for example `MIN_WIDTH = SIDEBAR_WIDTH + 2 + 13 + MIN_BODY + ...` (about 60). Bound the input rows by the terminal height, for example `input_rows.min(area.height.saturating_sub(MIN_MESSAGES_ROWS + 4) )`, so that the messages pane keeps a minimum height. The bound must live in `layout::areas` or `App::input_rows` so that the reducer and the renderer stay in agreement.

### WR-04: `sanitize` lets several invisible and blank characters through, against its own stated purpose

**File:** `src/ui/sanitize.rs:12-24`
**Issue:** `is_spoofing` is documented as covering "invisible or direction-changing characters that can spoof or hide text". These common ones are missing:
- U+3164, U+115F, U+1160 and U+FFA0 (Hangul fillers)
- U+2800 (Braille blank)
- U+00AD (soft hyphen), U+034F (combining grapheme joiner), U+180E
- U+2061 to U+2064 (invisible math operators)
- U+206A to U+206F (deprecated format controls)
- U+FFF9 to U+FFFB (interlinear annotation)

The Hangul fillers and the Braille blank are the practical ones. They render as blank cells in most terminals, but `unicode-width` assigns them a width, so they can pad or hide text and make one author name look like another. U+00AD and U+034F render zero cells while the width math says 1, and that skews the wrap and cursor math. The escape-sequence part of the choke point is sound. Only this spoofing and width-skew list is incomplete.

**Fix:** Extend `is_spoofing`:
```rust
| 0x00ad | 0x034f | 0x115f | 0x1160 | 0x180e
| 0x2061..=0x2064 | 0x206a..=0x206f
| 0x2800 | 0x3164 | 0xffa0 | 0xfff9..=0xfffb
```
Add rows for these to `strips_bidi_and_zero_width`. As a cheaper alternative, drop every char in Unicode category Cf except ZWJ and ZWNJ, if the author accepts a new dependency.

### WR-05: A long room name pushes the pending keys and error messages out of the status bar

**File:** `src/ui/status.rs:17-35`
**Issue:** The status line is built as `badge + room + pending + status message` in one `Paragraph` with no truncation, so it is clipped at the terminal width. D-09 requires pending keys in the status line, and the status message is the only feedback for `Unknown command: ...`.

A room name of about 50 characters or more (a group DM named after many users, a long team name) pushes both off screen on a 80-column terminal. The fake data has short names (the longest is 13 characters), so no test sees it.

**Fix:** Truncate the room name to a budget with an ellipsis, using display width: `width - badge - pending - status - separators`, never below roughly 8 columns. Or put pending keys and the status message on a right-aligned `Line` and only clip the room name.

## Info

### IN-01: Terminal init panics instead of returning an error, and SIGTERM/SIGHUP skip the restore

**File:** `src/tui.rs:11-12`, `src/main.rs:5`
**Issue:** `tui::init` calls `ratatui::init()`, which is `try_init().expect("failed to initialize terminal")`. Starting without a TTY (a pipe, or cron) produces a panic message, although `main` already returns `io::Result`. Separately, a raw-mode TUI gets no SIGINT from Ctrl-c, but an external `kill` (SIGTERM) or a SIGHUP has no handler. The process then dies with raw mode and the alternate screen still active. Panic and error paths are fine.
**Fix:** Make `init` return `io::Result<DefaultTerminal>` via `ratatui::try_init()`, and call `tui::restore()` on the error path in `main`. Enable tokio's `signal` feature and `select!` on SIGTERM/SIGHUP in `run`, mapping them to `Action::Quit`.

### IN-02: The seed loop sends on the channel it alone will drain, which deadlocks above the channel capacity

**File:** `src/lib.rs:44-48`
**Issue:** The task that owns `rx` awaits `tx.send` for every seed Action before it ever reads `rx`. With `CHANNEL_CAPACITY = 256` and about 10 seed Actions this is fine. The comment calls this the seam that Phase 3 reuses, and a real data source with more than 256 Actions would hang before the first draw.
**Fix:** Apply the seed through `update` directly, as `App::with_fake_data` already does, or spawn the producer as a task instead of sending inline. At least use `try_send` and handle `Full`.

### IN-03: Terminals that send Ctrl-H for Backspace cannot delete

**File:** `src/input/keymap.rs:87,99`
**Issue:** crossterm 0.29 maps byte 0x7F to `Backspace` and 0x08 to `Char('h')` with CONTROL (`event/sys/unix/parse.rs`). Only `<BS>` is bound to `DeleteBack`, so on a terminal or config where the Backspace key sends ^H, deleting does nothing in Insert and Command mode.
**Fix:** Add `(Mode::Insert, "<C-h>", Action::DeleteBack)` and `(Mode::Command, "<C-h>", Action::DeleteBack)`. This matches vim and readline behaviour.

### IN-04: Dead and test-only API

**File:** `src/app/mod.rs:44-47`, `src/ui/status.rs:27`, `src/input/keymap.rs:51`, `src/app/input_buf.rs:145-155`
**Issue:** `StatusMsg::Info` is never constructed; only its match arm exists. `Keymap::lookup`, `InputBuffer::row`, `InputBuffer::col` and `InputBuffer::line_count` are used only by tests. Being `pub` in a library crate hides them from the dead-code lint.
**Fix:** Keep `Info` if Phase 2 needs it. Mark the test-only accessors `#[cfg(test)]`, or drop them. This is not urgent.

### IN-05: `.gitignore` does not cover pending insta snapshots

**File:** `.gitignore:1`
**Issue:** Only `/target` is ignored. `insta` writes `*.snap.new` files next to the snapshot directories whenever an assertion fails, and they would be committed by a careless `git add -A`.
**Fix:** Add `*.snap.new` and `*.pending-snap` to `.gitignore`.

### IN-06: The command line has no horizontal scroll

**File:** `src/ui/status.rs:53-60`
**Issue:** `MAX_CMDLINE` is 256, but `render_cmdline` draws `:` plus the whole text with no scrolling, and `cmdline_cursor` pins the cursor at the right edge. Text typed beyond the terminal width is invisible, so the user is editing blind.
**Fix:** Show only the tail of the command line that fits, and place the cursor at the end of that visible tail.

### IN-07: A paste that is truncated or a character refused at the 64 KiB cap gives no feedback

**File:** `src/app/update.rs:136-139` (and `src/app/input_buf.rs:174`)
**Issue:** `insert_str` returns `truncated`, and the reducer discards it. `insert_char` returns `false` at the cap and the caller discards that too. The user cannot tell that part of a paste was dropped.
**Fix:** When `insert_str` reports truncation, set `app.status_msg = Some(StatusMsg::Info("Paste truncated at 64 KiB".into()))`. That would also give `StatusMsg::Info` a producer (see IN-04).

### IN-08: The folder unread count counts rooms that are not listed under the folder

**File:** `src/app/sidebar.rs:123-127`
**Issue:** An unread room has its home in the Unread section (precedence A1), so it is not drawn under its folder. The folder row's `(n)` still counts every unread room whose id the folder lists. In the fake data, the Work folder shows `(1)` for `dev-team`, which appears only under Unread, and the count is not shown under the folder it belongs to. A room that two folders list is counted in both.
**Fix:** Either say in a comment that the count means "unread rooms that belong to this folder" and is intentional, or count only rooms whose `Home` is this folder. Today the badge points at a room the user cannot find under the folder.

---

_Reviewed: 2026-11-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
