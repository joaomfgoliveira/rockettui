---
phase: 01-modal-tui-shell
plan: 06
subsystem: ui
tags: [ratatui, crossterm, unicode-segmentation, unicode-width, insta, pty-smoke, modal-input]

requires:
  - phase: 01-modal-tui-shell
    provides: "sidebar motions (01-04), messages pane with follow_newest (01-05), keymap/matcher/reducer (01-01..01-03), run loop that stamps Action::Clock (01-02)"
provides:
  - "InputBuffer: multiline, grapheme-aware, sanitized, capped, redacting Debug, hard-wrapped visual rows with exact cursor math"
  - "Insert mode end to end: i/a/o, D-14 editing keys, Enter appends a local message from me (D-15), safe one-insert paste"
  - "D-18 Normal-mode motions inside the input bar (h/l, j/k, gg/G, Ctrl-d/Ctrl-u)"
  - "input bar renderer that grows 1..5 rows then scrolls, with the hardware-cursor rules"
  - "reviewed 80x24 full-screen snapshots (Normal, Insert multi-line, Command) and the pty `insert` smoke scenario with a screen model"
affects: [phase-02-config-keymap, phase-04-send-messages]

actuals:
  tokens: 17500
  tasks: 3
  commits: 6
plan_head_before: 583b9a4f2988f4ed0f89d8a1502ca182a80f37fa
plan_head_after: 31a23f6f8d82651042d605dfe7b1575d3d217d86

tech-stack:
  added: []
  patterns:
    - "Hand-rolled input buffer behind the Action channel (no raw key events reach it); the reducer calls its methods through one edit_input helper that also settles the view"
    - "Pure geometry shared by reducer and renderer: input_rows / input_viewport / view_top mirror app::messages::view"
    - "SubmitInput is the single seam Phase 4 swaps from a local append to an API send"

key-files:
  created:
    - src/app/input_buf.rs
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_normal.snap
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_insert_multiline.snap
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_command.snap
  modified:
    - src/app/mod.rs
    - src/app/update.rs
    - src/action.rs
    - src/input/keymap.rs
    - src/ui/input.rs
    - src/ui/mod.rs
    - scripts/smoke_pty.py

key-decisions:
  - "InputBuffer exposes settle(width, height) and the reducer commits the effective view top after every motion and Insert edit, so scrolling stays put between keys instead of re-deriving from a stale stored top"
  - "Cursor-line extra visual row rule uses 'the last visual row of the cursor line is full' (width >= pane width) rather than 'line width is a multiple of the width'; identical for ASCII, and exact when wide characters leave a row short"
  - "up/down tie-break: when two grapheme boundaries are equally near the wanted display column, the left one wins"
  - "insert_str builds the new lines in one splice so a 64 KiB paste of newlines is not quadratic"
  - "SubmitInput and the Insert editing actions are guarded to Insert mode in the reducer"

patterns-established:
  - "Draft editing never bypasses ui::sanitize: insert_char refuses line-break chars and drops sanitizer-dropped chars; insert_str sanitizes then splits on LF"
  - "pty smoke scenarios drain output while typing (type_keys) and assert on a parsed screen model, not on raw byte substrings"

requirements-completed: [NAV-01, NAV-02, NAV-04]

coverage:
  - id: D1
    description: "Multiline InputBuffer with grapheme-aware editing, sanitized typed and pasted text, 64 KiB cap, never a line break inside a stored line, redacting Debug"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib app::input_buf::tests (19 tests)"
        status: pass
    human_judgment: false
  - id: D2
    description: "i/a/o enter Insert from any pane; Esc and Ctrl-c return to Normal and to the previous pane and never quit; edge and idempotency probes hold"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::mode_"
        status: pass
    human_judgment: false
  - id: D3
    description: "Enter in Insert mode appends a local message from me stamped with App.clock_secs, clears the draft, stays in Insert and jumps the messages pane to it; blank draft is a no-op; no open room shows the error and keeps the draft"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::submit_"
        status: pass
      - kind: other
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui insert"
        status: pass
    human_judgment: false
  - id: D4
    description: "D-18: with the input bar focused in Normal mode h/l, j/k, gg/G and Ctrl-d/Ctrl-u move inside the draft without changing text, mode, focus, room or sidebar selection; every h/j/k/l/gg/G/Ctrl-d/Ctrl-u press has a stated effect in every pane"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::input_normal_ (5 tests) and app::input_buf::tests::half_page_only_when_scrolling"
        status: pass
    human_judgment: true
    rationale: "A25 choices (h/l stop at line ends, gg/G land on column 0, Ctrl-d/Ctrl-u only when the draft is taller than the bar) need author confirmation in a real terminal"
  - id: D5
    description: "Paste is one insert, sanitized, CRLF-normalized, capped, joined onto one line in Command mode, and never submits"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::paste_"
        status: pass
    human_judgment: true
    rationale: "Bracketed paste delivery depends on the terminal emulator (A7); the reducer side is proven, the terminal side needs the manual UAT"
  - id: D6
    description: "Input bar grows 1..5 rows then scrolls to keep the cursor visible; hardware cursor shown only in Insert, in Normal with the input bar focused, and on the command line in Command mode"
    requirement: "NAV-04"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::input::tests::hardware_cursor_rules ui::tests::input_grows_to_five_rows"
        status: pass
    human_judgment: false
  - id: D7
    description: "Reviewed full-screen 80x24 snapshots in Normal, Insert (multi-line draft) and Command modes"
    requirement: "NAV-04"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::tests::layout_80x24"
        status: pass
    human_judgment: false
  - id: D8
    description: "Alt-Enter newline delivery, Ctrl-key interception and resize behaviour in the author's real terminal"
    verification: []
    human_judgment: true
    rationale: "Terminal and multiplexer behaviour (A6, A7) cannot be asserted from a pty with a synthetic byte stream; the plan's human-check covers it"

duration: 7min
completed: 2026-11-15
status: complete
---

# Phase 1 Plan 06: Modal Input Shell Summary

**Hand-rolled multiline grapheme-aware InputBuffer wired through the Action channel: Insert mode (i/a/o, Alt-Enter/Ctrl-j newlines, Enter appends a local message), D-18 Normal-mode motions in the input bar, safe paste, a growing 1-5 row input bar with correct hardware cursor rules, reviewed full-screen snapshots and a pty insert smoke run.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-11-15T15:21:29Z
- **Completed:** 2026-11-15T15:28:50Z
- **Tasks:** 3 (all TDD, 6 commits)
- **Files modified:** 11 (4 created, 7 modified; 3 of the created files are insta snapshots)

## Accomplishments

- `InputBuffer` (src/app/input_buf.rs): grapheme-cluster editing (typing, Backspace, Ctrl-w, Ctrl-u, arrows, Home/End), sanitized `insert_char` / `insert_str`, 64 KiB cap on a char boundary, no line break ever stored inside a line, redacting `Debug`, character-level hard wrap with exact `cursor_visual`, a pure `view_top`, and `half_page` that only acts when the draft is taller than the bar.
- Insert mode end to end: `i`/`a`/`o` from any pane, `Esc`/`Ctrl-c` restore the previous pane and never quit, Enter appends a `local-N` message from `me` stamped by `App.clock_secs` (no clock read in the reducer) and follows it in the messages pane, paste is one insert and never submits.
- D-18: with the input bar focused in Normal mode, `h`/`l` (within the line), `j`/`k`, `gg`/`G`, `Ctrl-d`/`Ctrl-u` and counts act on the draft only.
- `ui::input`: view window, dim "press i to write" hint, every row through `sanitize_line`, hardware cursor in Insert and in Normal-with-input-focus only.
- Three reviewed 80x24 snapshots and a pty `insert` scenario backed by a small screen model; all three smoke scenarios and the whole phase gate are green.

## Task Commits

Each TDD task has a RED commit then a GREEN commit:

1. **Task 1: InputBuffer and D-18 input motions**
   - RED `1f0f977` (test): 19 buffer tests + 5 `input_normal_` tests against an `unimplemented!()` stub
   - GREEN `2707d53` (feat): buffer, `App.input_rows`/`input_viewport`, Input arm of the motion router
2. **Task 2: Insert mode, submit, paste, input renderer**
   - RED `1a01d2e` (test): 11 reducer tests + `hardware_cursor_rules` with the new Action variants not yet handled
   - GREEN `c7b5e85` (feat): keymap rows, reducer arms, renderer and `cursor_position`
3. **Task 3: Snapshots, smoke insert scenario, phase gate**
   - RED `2ed72f4` (test): three snapshot tests (no `.snap` files) + `input_grows_to_five_rows`
   - GREEN `31a23f6` (feat): reviewed snapshots + `scripts/smoke_pty.py` screen model and `insert` scenario

**Plan metadata:** committed separately (docs: complete plan)

## TDD Gate Compliance

All three tasks have a `test(01-06)` commit followed by a `feat(01-06)` commit (ancestry verified by `git log`), no REFACTOR commit was needed. RED evidence, with the semantic assessment:

- Task 1 RED (`1f0f977`): `cargo test --lib` gave 109 passed, 24 failed. The failures are the 24 target tests; they compile and run, then panic inside the `unimplemented!()` stub (buffer tests) or fail an assertion because the Input arm of the router was empty (`input_normal_*`, e.g. position unchanged after `h`). Semantic assessment: the target tests executed and failed because the feature did not exist; no syntax, import or fixture fault. Rust has no `workflow.tdd_mode` RED-evidence classifier format here (tdd.md "Known gap - Rust"), so the evidence is the plain `cargo test` output.
- Task 2 RED (`1a01d2e`): 133 passed, 12 failed (11 reducer tests + `hardware_cursor_rules`), all on assertions: Insert mode never entered, no message appended, `cursor_position` stub returned `None`. Semantic assessment: valid RED.
- Task 3 RED (`2ed72f4`): the three snapshot tests failed because no reviewed `.snap` existed (insta wrote `.snap.new`, deleted before the GREEN commit). `input_grows_to_five_rows` passes at RED by design: it locks behavior delivered in Task 2 (growth to 5 rows) at the full-screen level. The smoke `insert` scenario has no RED commit; it is harness code verified by running it.

## Files Created/Modified

- `src/app/input_buf.rs` - the multiline buffer, motions, wrap geometry and 19 tests
- `src/app/mod.rs` - `input`, `local_seq`, `LOCAL_AUTHOR`, `input_rows()` from the draft, `input_viewport()`
- `src/app/update.rs` - Input arm of the motion router (D-18), `EnterInsert`, Insert editing, `Paste`, `SubmitInput`, `edit_input` helper, 16 new tests
- `src/action.rs` - 11 new unit variants with redacting `Debug` arms
- `src/input/keymap.rs` - Normal `i a o`; 14 Insert rows
- `src/ui/input.rs` - input bar renderer, `cursor_position`, `hardware_cursor_rules`
- `src/ui/mod.rs` - three snapshot tests and `input_grows_to_five_rows`
- `src/ui/snapshots/*layout_80x24_{normal,insert_multiline,command}.snap` - reviewed screens
- `scripts/smoke_pty.py` - `Screen` model, `type_keys`, scenario `insert`

## Decisions Made

- Added `InputBuffer::settle` and call it from the reducer after every Input motion and Insert edit, so the stored view top follows the cursor and scrolling does not jump back (the plan's `view_top` stays pure and unchanged).
- The "extra empty visual row" for a cursor at the end of a full row is keyed on the last wrapped row being full, not on the line width being a multiple of the pane width; same result for ASCII, correct for CJK lines that leave a row one column short.
- `up`/`down`/`move_line` break display-column ties toward the left boundary.
- The editing Actions and `SubmitInput` are guarded to Insert mode in the reducer.

## Deviations from Plan

None - plan executed exactly as written (the additions above are implementation details inside the plan's stated behavior; no files outside `files_modified` were touched).

## Issues Encountered

None. The sandbox rejected two compound shell commands; they were split into separate calls with no effect on the result.

## Known Stubs

None. The "press i to write" text is the intended empty-state hint, not a placeholder.

## User Setup Required

None - no external service configuration required.

## Threat Flags

None - no new network endpoints, auth paths, file access or trust-boundary schema. The plan's mitigations are implemented: T-01-19 (cap, `cap_enforced`), T-01-20 (sanitize in `insert_char`/`insert_str` and `sanitize_line` in the renderer; `insert_str_normalizes`, `insert_char_filters`, `insert_char_never_breaks_line`), T-01-21 (`debug_redacts`), T-01-22 (`paste_is_one_insert_never_submits`), T-01-24 (counts clamp; `input_normal_*`, `half_page_only_when_scrolling`), T-01-SC6 (Cargo.toml untouched).

## Next Phase Readiness

- Phase 1 plan set is complete: modal shell with sidebar, messages, multiline input, status bar and command line.
- Phase 4 swaps the body of `submit_input` (src/app/update.rs) for an API send behind the same `SubmitInput` action.
- Open for the author (human-check, assumptions A6, A7, A25): Alt-Enter delivery in the real terminal (Ctrl-j is the bound fallback), bracketed paste, and the D-18 input-bar choices. Run `cargo run` through the checklist in 01-06-PLAN.md Task 3.

---
*Phase: 01-modal-tui-shell*
*Completed: 2026-11-15*

## Self-Check: PASSED
