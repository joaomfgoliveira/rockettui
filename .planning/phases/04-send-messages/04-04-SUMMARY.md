---
phase: 04-send-messages
plan: 04
subsystem: ui
tags: [theme, markers, send-state, render-rows, insta, tdd]

requires:
  - phase: 04-send-messages
    provides: SendState and Message.send (plan 04-02), the pump and Failed marking
provides:
  - "theme.toml vocabulary: [markers] sending / failed / unconfirmed and [messages.pending] / [messages.failed]"
  - Markers.sending/failed/unconfirmed (defaults "…", "!", "?") and Theme.messages_pending (dim) and Theme.messages_failed (red)
  - render_rows draws a Queued, Sending, Failed or Unconfirmed message with the state's marker in place of its time and the state's style on time cell, author and body, in Compact and TwoLine
  - App::row_ctx raises the room's time column to the widest unsent marker so every body keeps its x
  - insta snapshots of the pending row and of every send state in both layouts
affects: [04-05, 04-06, 04-07, 04-08]

actuals:
  tokens: 21000
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "One match on msg.send (send_look) returns the time-cell text and the optional state style, so render_rows and message_rows cannot diverge"
    - "A marker is measured and padded exactly like a time; row_ctx owns the room-wide column"

key-files:
  created:
    - src/ui/snapshots/rockettui__ui__messages__tests__send_pending_52x20.snap
    - src/ui/snapshots/rockettui__ui__messages__tests__send_states_compact_52x20.snap
    - src/ui/snapshots/rockettui__ui__messages__tests__send_states_two_line_52x20.snap
  modified:
    - src/config/theme.rs
    - src/ui/wrap.rs
    - src/app/mod.rs
    - src/ui/messages.rs

key-decisions:
  - "The state style replaces the author palette, bold and the body style outright (author, marker and body all take messages_pending or messages_failed), so an unconfirmed message is never mistaken for a delivered one"
  - "Queued and Sending share the sending marker and messages.pending; Failed and Unconfirmed differ only by marker (! versus ?) and share messages.failed"
  - "In TwoLine the marker takes the time's place in the header (`author  marker`); the server's copy gets its time back"

patterns-established:
  - "Pattern: a pty-free end-to-end theme test drives the reducer with press(), then Action::MessageSent, and asserts cell styles in the rendered buffer"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "A Queued or Sending row is drawn dim with the sending marker where its time would be, configurable by [markers] sending and [messages.pending], and returns to the normal look when the server's copy replaces it"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/ui/wrap.rs#pending_row_compact, pending_rows_count_equals_render"
        status: pass
      - kind: integration
        ref: "src/ui/messages.rs#pending_theme_end_to_end, send_pending_52x20"
        status: pass
    human_judgment: false
  - id: D2
    description: "Failed (!) and Unconfirmed (?) rows are drawn in messages.failed (red) in both layouts and never look like a delivered message; the cursor highlight still applies on top"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/ui/wrap.rs#failed_and_unconfirmed_rows"
        status: pass
      - kind: integration
        ref: "src/ui/messages.rs#send_states_compact_52x20, send_states_two_line_52x20, cursor_over_failed_row"
        status: pass
    human_judgment: false
  - id: D3
    description: "A marker wider than the formatted time never moves a body; the new names are validated like the existing vocabulary"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/ui/wrap.rs#wide_marker_keeps_body_column"
        status: pass
      - kind: unit
        ref: "src/config/theme.rs#send_vocabulary_parses, send_marker_defaults, every_element_is_themeable, equal_values_allowed"
        status: pass
    human_judgment: false
  - id: D4
    description: "Whether the default glyphs and dim/red styles read well in the author's own terminal"
    requirement: MSG-02
    verification: []
    human_judgment: true
    rationale: "Assumption A8: appearance in a real terminal is a visual judgment no test asserts; all three markers and both styles are configurable"

duration: 15min
completed: 2026-11-19
status: complete
plan_head_before: dc160b511bcbf2a1a4630079a72ef8ec5a030094
plan_head_after: 4d5c2fa1524e4b390249b01130d9beb7fe767af1
commits: 3
---

# Phase 4 Plan 4: Send State Look Summary

**Pending messages are drawn dim with a `…` marker in place of their time, failed ones with `!` and unconfirmed ones with `?` in red, in both message layouts, all five names settable in theme.toml, with every body staying on the room's shared column.**

## Performance

- **Duration:** about 15 min
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 4 source files, 3 snapshots created

## Accomplishments

- theme.toml gains `[markers] sending`, `failed`, `unconfirmed` (validated by the existing `parse_markers`: control or bidi characters and anything over 4 columns is one error naming the key) and the elements `messages.pending` (default dim) and `messages.failed` (default red foreground), each with `fg`, `bg` and `modifiers`.
- `render_rows` asks one function, `send_look`, for the time-cell text and the optional state style. Queued and Sending take the sending marker and `messages_pending`; Failed and Unconfirmed take their marker and `messages_failed`; Sent keeps the time and the normal author/body styling. Author and body still go through `sanitize_line` (D-14).
- `App::row_ctx` raises the room's time column to the widest unsent marker (`unsent_marker_column`), so a marker wider than the formatted time never shifts a body. `message_rows` still equals `render_rows(..).len()`.
- The messages pane needed no change: it already draws through `render_rows` with the shared `RowCtx`, and the `messages.cursor` highlight is applied over the rows afterwards (`cursor_over_failed_row`: highlight on, red kept).

## Task Commits

1. **Task 1 (tracer): `[markers] sending` + `[messages.pending]` -> Theme -> row_ctx -> render_rows -> pending row in the pane** - `486366f` (feat)
2. **Task 2 (TDD): failed and unconfirmed look**
   - RED `3891609` (test)
   - GREEN `4d5c2fa` (feat)

Tracer feedback gate: `workflow.auto_advance` is off and `human_verify_mode` is `end-of-phase`; the tracer's `<verify>` is automated only, so it was re-run end to end (theme/wrap/messages tests, build plus the `send`, `config_layout` and `insert` pty scenarios, full test/clippy/fmt) and passed. "Tracer verified end-to-end - expanding".

## TDD notes (Task 2)

- **RED (`3891609`):** `Markers.failed`, `Markers.unconfirmed` and `Theme.messages_failed` were added as empty stubs (empty strings, `Style::default()`, not parsed, not drawn) so the tests compile. Nine tests failed on their planned assertions: `send_marker_defaults` (`failed == "!"`), `send_vocabulary_parses`, `every_element_is_themeable` (24 elements), `equal_values_allowed` (complete `[markers]` table), `failed_and_unconfirmed_rows` (marker `!` in the first cell), `wide_marker_keeps_body_column` (`time_col == 4`), `send_states_compact_52x20` and `send_states_two_line_52x20` (marker glyphs absent) and `cursor_over_failed_row` (red under the cursor). Cargo's console report is not one of the formats `gsd_run check tdd-red-evidence` reads and `workflow.tdd_mode` is off, so the classifier was not run; semantic assessment: each target executed and failed on the intended assertion, none on a compile, setup or fixture error.
- **GREEN (`4d5c2fa`):** real defaults, MARKER_KEYS/ELEMENT_PATHS/slot arms, the failed and unconfirmed arms of `send_look`, and the failed and unconfirmed widths in `unsent_marker_column`; the two snapshots were created with `INSTA_UPDATE=always` and checked by eye. 394 tests pass.
- **REFACTOR:** none needed. One clippy lint in a RED test (`manual_pattern_char_comparison`) was fixed in the GREEN commit.

## Verification

- `cargo test --lib -- config::theme ui::wrap ui::messages`: 74 passed (plan floors: 30 and 36).
- `cargo test`: 394 passed; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean; `find src -name '*.snap.new'` prints nothing.
- pty scenarios `send`, `config_layout`, `insert`: three `SMOKE OK`.
- Snapshot review: `send_pending_52x20` differs from the general snapshot only by the scroll caused by the new bottom row and by that row (`…     me     hello pending`; the focus moved to the input, so the border is the unfocused one); the compact snapshot shows `09:34`, `…`, `…`, `!`, `?` with all bodies at x 14; the two-line snapshot shows each marker in the header after the author.

## Files Created/Modified

- `src/config/theme.rs` - three marker keys, two elements, defaults, slot arms, and the vocabulary tests
- `src/ui/wrap.rs` - `send_look`, the state rule in `render_rows`, module doc, and four tests
- `src/app/mod.rs` - `unsent_marker_column` and its use in `row_ctx`
- `src/ui/messages.rs` - six tests and the every-state fixture
- three `.snap` files under `src/ui/snapshots/`

## Decisions Made

- A pending or failed row takes the state style for author, marker and body wholesale (no palette color, no bold author), so the whole row reads as not delivered; the plan said "author, marker and body" in that style.
- `sanitize_line` is applied to the marker as well as to author and body when it is drawn, although `parse_markers` already rejects anything it would change (defence in depth, no cost).
- Shared `messages.failed` for Failed and Unconfirmed, as the plan specifies; only the marker tells them apart.

## Deviations from Plan

None - plan executed exactly as written. (The plan's `send_pending_52x20` is built "like messages_general_newest_52x20" at 52x20; the snapshot border is the unfocused one because `i` moves focus to the input, which is the expected result of the plan's `ihello pending<CR>`.)

## Issues Encountered

None. One test helper in Task 1 first compared byte offsets (the `…` is three bytes) and passed by accident; it was corrected to compare display columns before the commit.

## Known Stubs

None. The RED stubs for `failed`, `unconfirmed` and `messages_failed` were replaced in the GREEN commit.

## Threat Flags

None beyond the plan's register. T-04-13 (marker strings) is mitigated by the existing `parse_markers` path and tested by `send_vocabulary_parses` for all three keys; T-04-14 (an undelivered message drawn as delivered) is mitigated by the single `send_look` match and covered by cell-style assertions and snapshots for every state in both layouts. T-04-SC-04 holds: `Cargo.toml` is unchanged.

## Next Phase Readiness

Plan 04-03 (parallel, wave 3) sets `Unconfirmed`; once it lands, those rows already render with `?` and the red style. Later plans can restyle all five names in theme.toml; the vocabulary is user-facing, so renaming one later would silently drop a user's setting.

## Self-Check: PASSED

The three snapshots and the four modified source files exist and `git status` is clean; the commits `486366f`, `3891609` and `4d5c2fa` are ancestors of HEAD (`commits: 3` measured from `plan_head_before` before this docs commit). The acceptance greps all meet their floors and `find src -name '*.snap.new'` prints nothing.
