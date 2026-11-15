---
status: complete
phase: 01-modal-tui-shell
source: [01-VERIFICATION.md]
started: 2026-11-15T15:30:00Z
updated: 2026-11-15T15:57:19.026Z
---

## Current Test

[testing complete]

## Tests

### 1. Resize behavior in a real terminal
expected: Dragging the terminal smaller and larger re-lays out the sidebar, messages pane and input bar on the next frame with no garbage or panic. Below 40x8 a 'terminal too small' notice replaces the panes. The message cursor stays visible after a resize. Note WR-03: between 40 and about 60 columns the messages body is clipped, so judge usability, not just absence of a panic.
result: pass

### 2. Ctrl-w interception by terminal or multiplexer (assumption A7)
expected: In Normal mode, Ctrl-w then h, j or l moves focus to sidebar, input or messages and the thick border follows. If the terminal or tmux/zellij eats Ctrl-w (for example closing a tab or window), record it and decide on an alternative binding.
result: pass

### 3. 1 s key-timeout feel (D-06)
expected: Press g alone: the pending g shows in the status line and clears after about 1 s. Press Ctrl-w alone: same. Typing 5 then j moves 5 rows. A pending count or sequence is dropped after the timeout without any visible glitch.
result: pass

### 4. Alt-Enter newline delivery and bracketed paste (assumptions A6, A25)
expected: In Insert mode, Alt-Enter and Ctrl-j insert a newline and Enter submits. Pasting a multi-line block from the system clipboard lands in the draft as one insert and does not submit any message. Terminals that send Alt-Enter differently should be noted.
result: pass
note: "User asked whether newline should be Shift-Enter instead of Alt-Enter (needs kitty keyboard protocol; not decided)"

### 5. D-18 input-bar choices
expected: With focus on the input bar in Normal mode, h/l move the cursor within the line, j/k move between lines of a multi-line draft, gg/G jump to the first and last line, and the hardware cursor is visible. The input bar grows from 1 to 5 rows and then scrolls. The author confirms these choices feel right.
result: pass

### 6. Moving around #general
expected: Launch the app, press Enter on #general, then use j/k, gg/G, Ctrl-d/Ctrl-u, 5j and 3<C-d>. The cursor highlight is clear and the long Japanese and 20-row messages wrap with a hanging indent. NOTE WR-01: the last 3 rows of the 20-row long message cannot be scrolled into view; confirm whether that is acceptable for Phase 1.
result: pass

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
