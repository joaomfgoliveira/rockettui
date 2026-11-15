---
phase: 01-modal-tui-shell
plan: 05
subsystem: ui
tags: [rust, ratatui, textwrap, unicode-width, insta, vim-motions, scrolling]

requires:
  - phase: 01-modal-tui-shell
    provides: "Matcher, per-pane motion router, App::open_room, layout::areas, sanitize, fake data (plans 01-01 to 01-03)"
provides:
  - "ui::wrap: MessageLayout { Compact, TwoLine }, MIN_BODY, wrap_body, render_rows, message_rows"
  - "app::messages: MsgView { cursor, top_row }, viewport, row_starts, view, apply_motion, follow_newest"
  - "App.msg_views and App.message_layout"
  - "Messages arm of the motion router wired to messages::apply_motion"
  - "Windowed messages pane renderer with cursor highlight and empty states, plus two reviewed 52x20 snapshots"
affects: [01-06, phase-02-config, phase-03-live-data]

actuals:
  tokens: 11500
  tasks: 3
  commits: 6
plan_head_before: fd48bb9a4abf0eb19c979d83be63e33b430f78ed
plan_head_after: 96d86be4dce21246492e4da1dec64935db20d9a5

tech-stack:
  added: []
  patterns:
    - "Reducer and renderer share one row model: ui::wrap::message_rows at the width of layout::areas"
    - "view() recomputes viewport and row heights from app.size on every read, so a Resize can never leave the cursor off screen"
    - "Per-room view stored only after a motion; no entry means the newest default"

key-files:
  created:
    - src/ui/wrap.rs
    - src/app/messages.rs
    - src/ui/snapshots/rockettui__ui__messages__tests__messages_general_newest_52x20.snap
    - src/ui/snapshots/rockettui__ui__messages__tests__messages_long_message_52x20.snap
  modified:
    - src/ui/mod.rs
    - src/ui/messages.rs
    - src/app/mod.rs
    - src/app/update.rs

key-decisions:
  - "The cursor highlight is applied with Buffer::set_style over the whole inner row after the Paragraph renders, because Line::style did not reach the body span's cells"
  - "Ctrl-d/Ctrl-u move the cursor to the message containing the target row and then reuse scroll_into_view, so the resulting top_row is the first row of that message (at most the half page)"
  - "The motion router arm calls crate::app::messages::apply_motion by full path, leaving the shared use line in update.rs untouched for the parallel sidebar plan"

patterns-established:
  - "Render tests draw on a TestBackend exactly the size of the app's own layout::areas rect"
  - "Wide characters: tests skip the hidden second cell when reading text or modifiers from the buffer"

requirements-completed: [NAV-02, NAV-04]

coverage:
  - id: D1
    description: "Messages wrap in the compact layout with a hanging indent, CJK-aware widths, long-token splitting and sanitizing before width math; TwoLine exists in the same enum"
    requirement: "NAV-04"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::wrap::tests"
        status: pass
    human_judgment: false
  - id: D2
    description: "Rooms open at the newest message; j/k, gg/G, Ctrl-d/Ctrl-u and counts move a message cursor with row-based scrolling; h/l are no-ops (D-19); empty and missing rooms are safe; grow and shrink resizes keep the cursor message in the window"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib app::messages::tests"
        status: pass
    human_judgment: false
  - id: D3
    description: "The messages pane draws only the visible window, highlights the cursor message, shows No room open / No messages yet, and leaves no control or bidi characters in the buffer"
    requirement: "NAV-04"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::messages::tests"
        status: pass
    human_judgment: false
  - id: D4
    description: "Scrolling feel and the readability of the hanging indent in a real terminal"
    verification: []
    human_judgment: true
    rationale: "The plan's human-check (cargo run, move through #general, quiet) is a visual judgment and was not run by this agent; it needs plan 01-04's Enter-opens-room to have merged"

duration: 7min
completed: 2026-11-15
status: complete
---

# Phase 1 Plan 05: Messages Pane Summary

**Word-wrapped messages with a hanging indent (Compact default, TwoLine in the same enum), a per-room message cursor with row-based scrolling driven by j/k, gg/G, Ctrl-d/Ctrl-u and counts, and a windowed renderer with cursor highlight, empty states and two reviewed snapshots.**

## Performance

- **Duration:** 7 min (started 2026-11-15T15:10:23Z, finished about 15:18Z)
- **Tasks:** 3 (all TDD, each RED then GREEN)
- **Files modified:** 4 created (2 source, 2 snapshots), 4 modified

## Accomplishments
- `ui::wrap` sanitizes first, wraps with `textwrap`, and builds styled rows in both layouts; `message_rows` is the single height function used by the reducer and the renderer.
- `app::messages` holds `MsgView`, the shared viewport (`layout::areas` inner size), `row_starts`, `view` (clamps and scrolls into view on every read, so resizes in either direction are safe) and `apply_motion` with D-04 / D-05 semantics (`5gg` and `5G` select message 5, half page is `max(1, rows / 2)`, counts multiply, h/l and Select/za are no-ops per D-19).
- `ui::messages::render` draws only `[top_row, top_row + inner height)`, highlights the cursor message (REVERSED when focused, DarkGray background otherwise) across the full row width, and shows `No room open` / `No messages yet`.

## Task Commits

Each task followed RED then GREEN:

1. **Task 1: Wrapping and layout enum** - RED `9f997dd` (test), GREEN `4986337` (feat)
2. **Task 2: Message cursor and row scrolling** - RED `1ea6d19` (test), GREEN `e5d8067` (feat)
3. **Task 3: Messages pane renderer and snapshots** - RED `2339702` (test), GREEN `96d86be` (feat)

**Plan metadata:** committed with this SUMMARY (docs).

## Files Created/Modified
- `src/ui/wrap.rs` - `MessageLayout`, `MIN_BODY`, `wrap_body`, `render_rows`, `message_rows`, 10 tests
- `src/app/messages.rs` - `MsgView`, `viewport`, `row_starts`, `scroll_into_view`, `view`, `apply_motion`, `follow_newest`, 13 tests
- `src/ui/messages.rs` - windowed `render`, 8 new tests (2 snapshot)
- `src/ui/snapshots/*messages_general_newest_52x20.snap`, `*messages_long_message_52x20.snap` - reviewed snapshots on the 52x20 pane of an 80x25 app
- `src/app/mod.rs` - `pub mod messages`, fields `msg_views` and `message_layout`
- `src/app/update.rs` - Messages arm of the motion router
- `src/ui/mod.rs` - `pub mod wrap`

## Decisions Made
- Cursor highlight is applied with `Buffer::set_style` over the inner row after the Paragraph is drawn (see Deviations).
- For Ctrl-d/Ctrl-u the cursor goes to the message containing the target row, then `scroll_into_view` runs as the plan prescribes. A message straddling the new top pulls `top_row` up to its first row, so the window advances by at most the half page and ends aligned to the cursor message. Tests assert this exact relation (`top_row == starts[cursor]`, `0 < top_row <= half`) instead of an unconditional advance of exactly `half`.
- The router arm uses the full path `crate::app::messages::apply_motion` so the shared `use crate::app::{...}` line in update.rs is untouched (plan 01-04 runs in parallel).

## TDD Notes
- RED commits compile with minimal scaffolding (stub wrap functions; stub `view`/`apply_motion`/`follow_newest` with real `viewport` and `row_starts` as test oracles, plus the `App` fields and the router arm). Failures were on the planned assertions: 10 of 10 wrap tests, 11 of 13 messages tests (the two passing are the empty/no-room no-op guards, which hold trivially under a stub), 7 of 9 renderer tests (`messages_no_room` and `hhmm_formats_utc` already held).
- Snapshot tests failed in RED for lack of a snapshot (run with `INSTA_UPDATE=no`, no `.snap.new` left). Snapshots were then generated with `INSTA_UPDATE=always`, read to confirm hanging indent, wide-char wrapping, the long message aligned to the top and a non-blank last inner row, and re-run without it.
- `workflow.tdd_mode` is not enabled and the plan is `type: execute`, so `gsd_run check tdd-red-evidence` was not run; RED evidence is the failing `cargo test` output inspected by hand.
- Test name `motion_gg_G` in the plan is spelled `motion_gg_g_capital` (non-snake-case names trip rustc lints).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Line::style did not highlight the whole cursor row**
- **Found during:** Task 3 (cursor_rows_reversed_when_focused)
- **Issue:** `Line::style(REVERSED)` only reached the prefix spans, not the body span or the trailing blanks, so the highlight was ragged.
- **Fix:** Render the Paragraph unstyled, then `buffer_mut().set_style(...)` the full inner width of every visible cursor row.
- **Files modified:** src/ui/messages.rs
- **Verification:** `cursor_rows_reversed_when_focused` passes (skipping the hidden second cell of wide characters, which ratatui does not style).
- **Committed in:** `96d86be` (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (1 bug). 
**Impact on plan:** None on behavior or files; the highlight now covers the full row as A22 intended.

## Issues Encountered
- Several Bash calls were refused by the worktree command guard when git and file work shared one command; they were split into plain commands and re-run with no effect on results.
- The plan's `<human-check>` (run the app, open #general, move around, open quiet) was not run by this agent. It also depends on plan 01-04's Enter-opens-room, which is being built in parallel. Left for phase verification.

## Known Stubs
None. `App.message_layout` is always `Compact` until the Phase 2 config selector, by design (D-10); `follow_newest` has no caller until plan 01-06, by design.

## Threat Flags
None. No new crates (Cargo.toml untouched), no network surface. T-01-16 (sanitize before wrap, `sanitize_line` for authors), T-01-17 (only the viewport slice is drawn) and T-01-18 (clamp and scroll on every read) are covered by `sanitize_before_wrap`, `hostile_message_sanitized`, `tiny_width_no_panic`, `window_matches_view`, `empty_room_noop`, `resize_clamps_view` and `resize_shrink_scrolls_into_view`.

## Verification Results
- `cargo test`: 94 passed (63 before this plan; 10 wrap, 13 messages, 8 renderer new)
- `cargo test --lib ui::wrap::tests`: 10 passed; `app::messages::tests`: 13 passed; `ui::messages::tests`: 9 passed (includes `hhmm_formats_utc`)
- `cargo test --lib -- app::messages::tests::resize_shrink_scrolls_into_view app::messages::tests::motion_h_l_noop`: 2 passed
- `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean; `find src -name '*.snap.new'`: nothing
- Grep gates: `pub enum MessageLayout` 1, `textwrap::` 2, `pub fn message_rows` 1, `pub mod wrap` 1, `pub struct MsgView` 1, `pub fn follow_newest` 1, `message_rows(` in app/messages.rs 1, `messages::apply_motion(` in update.rs 1, `fn scroll_into_view` 1, `render_rows(` in ui/messages.rs 2, `No messages yet` 2
- `python3 -I scripts/smoke_pty.py target/debug/rockettui quit` and `panic`: SMOKE OK

## Next Phase Readiness
Plan 01-06 can call `messages::follow_newest` after appending a local message and fills the Input arm of the router. Plan 01-04 and this plan touched `src/app/mod.rs` (this plan: one `pub mod` line and two `App` fields) and `src/app/update.rs` (one router arm), so the orchestrator may see adjacent-line merge hunks there. Outstanding for the human at phase verification: the Task 3 visual check in a real terminal.

## Self-Check: PASSED

Files `src/ui/wrap.rs`, `src/app/messages.rs` and both snapshot files exist; commits `9f997dd`, `4986337`, `1ea6d19`, `e5d8067`, `2339702` and `96d86be` are on this branch (6 commits measured from the plan base `fd48bb9`).
