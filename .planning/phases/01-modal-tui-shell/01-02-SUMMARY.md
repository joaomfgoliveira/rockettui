---
phase: 01-modal-tui-shell
plan: 02
subsystem: ui
tags: [rust, ratatui, crossterm, tokio, pty-smoke, sanitize, bracketed-paste, panic-hook]

requires:
  - phase: 01-modal-tui-shell
    provides: "headless TEA core: Action, App, update, keymap rows, sidebar cursor, fake seed Actions (plan 01-01)"
provides:
  - "Runnable binary: thin main (tui::init, run, tui::restore) over the run loop in src/lib.rs"
  - "Pure ui::view and ui::layout::areas (28-col sidebar, messages, input, status, command-line rows) with one small renderer per region"
  - "tui::init / tui::restore: raw mode, alternate screen, bracketed paste, layered panic hook"
  - "event::spawn_input_task mapping crossterm events to Actions (Release filtered)"
  - "Run loop: one bounded channel, seed drained and applied before the first draw, Action::Clock first in every batch, a single update call site"
  - "ui::sanitize::{sanitize, sanitize_line}: the one choke point for untrusted text, wired into every renderer"
  - "Below 40x8 a 'terminal too small' notice replaces the panes"
  - "scripts/smoke_pty.py with shared wait_ready and scenarios quit and panic"
  - "src/test_util.rs (press, app_80x24, render_to_string)"
affects: [01-03, 01-04, 01-05, 01-06]

actuals:
  tokens: 8100
  tasks: 3
  commits: 5
plan_head_before: b248fda9c2b05300fd90da947c556600d79c5cb4
plan_head_after: dbf0278f313e2af46b1e958374ef837bcdde3d7d

tech-stack:
  added: []
  patterns:
    - "Draw only after the queued batch is applied: seed Actions are sent before the input task starts and drained into the first batch"
    - "view(&App) is pure; geometry comes from ui::layout::areas, shared with the reducer in later plans"
    - "Every untrusted string goes through ui::sanitize::sanitize_line before width math or drawing"
    - "pty harness: every scenario starts with wait_ready, which also proves seed-before-first-draw"
    - "Debug-only env hook (ROCKETTUI_DEBUG_PANIC) compiled out of release builds"

key-files:
  created:
    - src/main.rs
    - src/tui.rs
    - src/event.rs
    - src/test_util.rs
    - src/ui/mod.rs
    - src/ui/layout.rs
    - src/ui/sanitize.rs
    - src/ui/sidebar.rs
    - src/ui/messages.rs
    - src/ui/input.rs
    - src/ui/status.rs
    - scripts/smoke_pty.py
  modified:
    - src/lib.rs

key-decisions:
  - "render_to_string walks the TestBackend buffer cells instead of using the backend's Display output, because Display wraps every row in quote marks and hides a row's first characters from starts_with checks"
  - "sanitize returns Cow::Borrowed unless a character actually needs to change, so plain ASCII allocates nothing"
  - "A lone CR becomes LF and CRLF collapses to one LF; U+2028/U+2029 become LF; tab expands to 4 spaces"

patterns-established:
  - "Pane renderers share the shape fn(frame, area, app) and read only &App"
  - "Tests render through crate::test_util::render_to_string and assert on plain text rows"

requirements-completed: [NAV-01, NAV-03, NAV-04]

coverage:
  - id: D1
    description: "Real binary under a pty: first frame lists the seeded rooms (general before NORMAL), j/k accepted, :q exits 0, alternate screen and bracketed paste entered and left"
    requirement: "NAV-03"
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui quit"
        status: pass
    human_judgment: false
  - id: D2
    description: "Five-region layout (28-column sidebar, messages, input bar, status bar, command-line row) and command-line cursor in Command mode only"
    requirement: "NAV-04"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::layout::tests::areas_80x24 ui::tests::layout_80x24_has_all_panes ui::tests::command_mode_renders_cmdline"
        status: pass
    human_judgment: false
  - id: D3
    description: "Mode badge and command line: ':' shows COMMAND and the typed text with the cursor after it; no cursor in Normal mode"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::tests::command_mode_renders_cmdline"
        status: pass
    human_judgment: false
  - id: D4
    description: "Sanitize choke point strips ESC/CSI/OSC/BEL/NUL/DEL/C1, bidi, zero-width and tag characters from every rendered string while emoji ZWJ, variation-selector and regional-indicator sequences survive"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::sanitize::tests ui::tests::hostile_text_is_sanitized_in_render"
        status: pass
    human_judgment: false
  - id: D5
    description: "Tiny terminals (0x0 up to 200x60) never panic; below 40x8 a too-small notice replaces the panes"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::tests::tiny_sizes ui::layout::tests::areas_tiny"
        status: pass
    human_judgment: false
  - id: D6
    description: "A panic restores the terminal (alternate screen left, bracketed paste off) before the panic message prints, exit code 101"
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui panic"
        status: pass
    human_judgment: false
  - id: D7
    description: "Layout reflows on real-terminal resize; Ctrl-c and Esc never quit; terminal is usable after quit and after a deliberate panic"
    verification: []
    human_judgment: true
    rationale: "Real terminal emulators deliver resize and Ctrl-c differently from a pty harness; terminal state after a panic is best confirmed by eye (the plan's human-check)"

duration: 35min
completed: 2026-11-15
status: complete
---

# Phase 1 Plan 02: Terminal Layer and Binary Tracer Summary

**Runnable ratatui binary (sidebar, messages, input bar, status bar, command row) over the headless TEA core, with a seed-before-first-draw run loop, panic-safe terminal lifecycle with bracketed paste, a single sanitize choke point for every rendered string, and a pty smoke harness proving quit and panic end to end.**

## Performance

- **Duration:** about 35 min (start time was not captured; estimated from the session)
- **Completed:** 2026-11-15
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 12 created, 1 modified (src/lib.rs), plus this SUMMARY

## Accomplishments
- Tracer (task 1): `target/debug/rockettui` draws the five-region layout from its very first frame, accepts j/k, quits on `:q` with exit 0, and enters and leaves the alternate screen and bracketed paste; proven by `scripts/smoke_pty.py ... quit`.
- The run loop sends `data::fake::seed()` Actions on the one bounded channel before the input task starts, drains them into the first batch, and applies `Action::Clock(unix_now())` plus the batch through the single `update(&mut app, ..)` call site before the first `terminal.draw`. The harness's `wait_ready` checks that the first `general` precedes the first `NORMAL` in the byte stream, which fails if the draw ever precedes the seed.
- `ui::sanitize` is the one choke point (T-01-01, T-01-02): controls, bidi, zero-width and tag characters are removed, tabs expand, line breaks normalize, and ZWJ, variation-selector and regional-indicator emoji survive. Room names, the room title, authors, bodies, status text and the command line (and the cursor width math) all pass through it.
- Resilience (T-01-04, T-01-06): `terminal too small` below 40x8, saturating geometry, and a layered panic hook; `scripts/smoke_pty.py ... panic` proves exit 101 with the screen restored before `panicked at`.

## Task Commits

1. **Task 1 (tracer): binary end to end** - `7ba093c` (feat)
2. **Task 2: sanitize choke point** - RED `2fc20a0` (test), GREEN `baeb164` (feat)
3. **Task 3: tiny terminals and panic restore** - RED `f03a509` (test), GREEN `dbf0278` (feat)

**Plan metadata:** committed with this SUMMARY (docs).

## Files Created/Modified
- `src/lib.rs` - module tree, `CHANNEL_CAPACITY`, private `unix_now`, `run` loop, debug-only panic hook; test_util declared last
- `src/main.rs` - thin binary: init, run, always restore
- `src/tui.rs` - init/restore with bracketed paste and the layered panic hook
- `src/event.rs` - input task mapping crossterm events to Actions
- `src/ui/mod.rs` - `view`, `pane_block`, `MIN_WIDTH`/`MIN_HEIGHT`, too-small notice
- `src/ui/layout.rs` - `SIDEBAR_WIDTH`, `Areas`, pure `areas`
- `src/ui/sanitize.rs` - `sanitize`, `sanitize_line`, `TAB_WIDTH`
- `src/ui/sidebar.rs`, `messages.rs`, `input.rs`, `status.rs` - one thin renderer per region (`hhmm`, `cmdline_cursor` live here)
- `src/test_util.rs` - `press`, `app_80x24` (moved unchanged from lib.rs), `render_to_string`
- `scripts/smoke_pty.py` - pty harness, shared `wait_ready`, scenarios `quit` and `panic`

## Decisions Made
- `render_to_string` reads buffer cells rather than the backend's `Display`, which wraps each row in quote marks (see Deviations).
- `sanitize` returns `Cow::Borrowed` unless something changes; CRLF collapses to a single LF.
- Subdivision flags degrade to the bare black flag (tag characters are stripped); accepted by the plan.

## TDD Notes
- Task 2 RED (`2fc20a0`): 9 of 12 sanitize/render tests failed on their planned assertions against an identity stub (escape sequences, C1, bidi, tag characters, tab, newlines, NUL/DEL, `sanitize_line`, and `hostile_text_is_sanitized_in_render` failing on `spoofing char U+202E`). `keeps_zwj_and_zwnj`, `keeps_emoji_sequences` and `ascii_is_borrowed` already held for the identity stub; they are pinning tests that guard the GREEN implementation against over-stripping.
- Task 3 RED (`f03a509`): `tiny_sizes` failed on the planned assertion (no `too small` notice at 39x7). `areas_tiny` already passed because the saturating layout from the tracer never panics; it is a pinning test.
- `workflow.tdd_mode` is not enabled and the plan is `type: execute`, so no `gsd_run check tdd-red-evidence` run was performed. RED evidence is the failing `cargo test` output on the named target tests, inspected semantically.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] render_to_string cannot use the TestBackend Display output**
- **Found during:** Task 1 (stage C check, `command_mode_renders_cmdline`)
- **Issue:** The plan says `render_to_string` returns the backend's Display output, but that output wraps every row in quote marks, so `last.starts_with(":q")` failed on `"\":q ..."`.
- **Fix:** `render_to_string` builds the text by walking the buffer cells: one line per row, symbols concatenated, no quoting.
- **Files modified:** src/test_util.rs
- **Verification:** `ui::tests::command_mode_renders_cmdline` and the other two stage-C tests pass.
- **Committed in:** `7ba093c` (task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug in the plan's helper description)
**Impact on plan:** Helper has the intended contract (text rows a test can match); no scope change.

## Issues Encountered
- The first Bash call combined several git commands and was refused by the worktree guard; commands were then split into plain separate calls. No effect on results.
- The `<human-check>` in task 3 (resize in a real terminal, Ctrl-c/Esc, `:q`, `ROCKETTUI_DEBUG_PANIC=1 cargo run`) was not run by this agent. Per the project's `end-of-phase` human-verify mode it is left for phase verification; the pty scenarios cover the automatable part.

## Known Stubs
None that block the plan goal. By design for later plans: the input pane draws an empty box (plan 01-06), the sidebar is a plain room list (plan 01-04), and the messages pane draws one unwrapped line per message with no cursor or scroll (plan 01-05). `Action::Paste` is still ignored by the reducer (plan 01-06).

## Threat Flags
None. No new crates (`Cargo.toml` untouched), no network surface, no print macros under `src/`, and the only wall-clock read is `unix_now()` in `src/lib.rs`.

## Verification Results
- `cargo test`: 40 passed (38 from earlier tasks plus the new tests; 22 from plan 01-01 unchanged)
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `python3 -I scripts/smoke_pty.py target/debug/rockettui quit`: `SMOKE OK: quit`
- `python3 -I scripts/smoke_pty.py target/debug/rockettui panic`: `SMOKE OK: panic`
- Grep gates: `fake::seed()` 1, `tui::restore()` 1, `KeyEventKind::Release` 1, `EnableBracketedPaste` 2, exactly one `update(&mut app` before the first `#[cfg(test)]` in lib.rs, no print/debug macros anywhere in `src/`

## Next Phase Readiness
Ready for plan 01-03 (key sequence matcher and timeout): the run loop's plain `rx.recv()` is the one spot it replaces with a `tokio::select!`. Plans 01-04 and 01-05 each own one renderer file (`sidebar.rs`, `messages.rs`) without touching a shared file. Outstanding for the human at phase verification: the task 3 real-terminal check.

## Self-Check: PASSED

All 13 files exist on disk; commits `7ba093c`, `2fc20a0`, `baeb164`, `f03a509`, `dbf0278` are on this branch.
