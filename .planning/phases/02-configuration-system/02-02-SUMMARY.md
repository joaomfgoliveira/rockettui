---
phase: 02-configuration-system
plan: 02
subsystem: ui
tags: [chrono, strftime, ratatui, layout, config, insta-snapshots]

requires:
  - phase: 02-configuration-system
    provides: "Settings (sidebar_width, timestamp_format, key_timeout_ms), App::with_config, config_from sources, temp_config_dir/app_with_config helpers"
provides:
  - "layout::areas(area, sidebar_width, input_rows) and App::areas() as the single geometry source"
  - "ui::timefmt: Zone { Utc, Local } and format_ts (chrono strftime, sanitized, never panics)"
  - "wrap::RowCtx, wrap::time_column, App::row_ctx shared by the renderer and the reducer row model"
  - "App::key_timeout() driving the run loop pending-key deadline; App.zone set to Local in the binary"
  - "Two reviewed full-screen snapshots of non-default config.toml settings"
affects: [02-03 keymap.toml, 02-04 theme.toml, 02-05 theme consumers, 03 history_size and server_url]

actuals:
  tokens: 12800
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "One helper (App::areas, App::row_ctx) builds geometry and row context for both reducer and renderer"
    - "Time zone injected through App.zone; the reducer never reads a clock or a zone"
    - "Compact layout pads every time to the room's widest formatted time (time_column)"

key-files:
  created:
    - src/ui/timefmt.rs
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_sidebar_40.snap
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_custom_config.snap
  modified:
    - src/ui/layout.rs
    - src/ui/mod.rs
    - src/ui/wrap.rs
    - src/ui/messages.rs
    - src/ui/input.rs
    - src/app/mod.rs
    - src/app/messages.rs
    - src/app/sidebar.rs
    - src/lib.rs
    - src/test_util.rs

key-decisions:
  - "sidebar_width is not clamped; a wide sidebar leaves the main column narrow or empty and never panics (A26)"
  - "Binary shows system local time (chrono::Local set once in run); App::new, with_config and tests stay UTC (A2, confirmed by the author 2026-11-16)"
  - "Compact time padding lives inside the dim time span so the 5-span row structure of Phase 1 is unchanged"
  - "A time wider than the room's time column keeps its own width in Compact so the hanging indent still matches the first row"

patterns-established:
  - "Reducer geometry always goes through App::areas(); only ui::view uses layout::areas directly (it has the frame area)"
  - "Never to_string a chrono format: write! into a String and map the error to ?"

requirements-completed: [CONF-07]

plan_head_before: 1703946060cba09825d78341aaa5f660e85d8e1e
plan_head_after: 6d83551869fbcadea4a67635b0524a6c1bc6a728
commits: 3

coverage:
  - id: D1
    description: "sidebar_width in config.toml sets the sidebar width on screen and in every reducer computation; widths beyond the terminal never panic; default keeps all Phase 1 snapshots"
    requirement: "CONF-07"
    verification:
      - kind: unit
        ref: "src/ui/layout.rs#areas_custom_sidebar_width"
        status: pass
      - kind: unit
        ref: "src/ui/mod.rs#layout_80x24_sidebar_40"
        status: pass
      - kind: unit
        ref: "src/ui/mod.rs#sidebar_wider_than_terminal"
        status: pass
      - kind: unit
        ref: "src/app/messages.rs#viewport_follows_sidebar_width"
        status: pass
    human_judgment: false
  - id: D2
    description: "timestamp_format renders every message time through chrono; reducer row model and renderer share App::row_ctx; Compact bodies stay aligned for variable-width formats; out-of-range timestamps render ?"
    requirement: "CONF-07"
    verification:
      - kind: unit
        ref: "src/ui/timefmt.rs (default_matches_phase1, custom_format, out_of_range_is_placeholder, local_zone_formats)"
        status: pass
      - kind: unit
        ref: "src/ui/wrap.rs (time_column_alignment, custom_format_compact_prefix)"
        status: pass
      - kind: unit
        ref: "src/app/messages.rs#row_model_matches_renderer_custom_format"
        status: pass
    human_judgment: false
  - id: D3
    description: "key_timeout_ms replaces the fixed 1 s pending-key timeout; the binary shows local time while App::new and tests stay UTC"
    requirement: "CONF-07"
    verification:
      - kind: unit
        ref: "src/app/mod.rs (key_timeout_from_settings, with_config_defaults_to_utc)"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui config_layout"
        status: pass
    human_judgment: true
    rationale: "Wall-clock timing of the pending key (about 3 s with key_timeout_ms = 3000) and the look of a wider sidebar with local date-times in a real terminal is a human judgment (02-VALIDATION.md Manual-Only Verifications); the task 3 human-check has not been run"
  - id: D4
    description: "Full-screen snapshot of a customised config (sidebar 34, two_line, %a %H:%M) reviewed by eye"
    requirement: "CONF-07"
    verification:
      - kind: unit
        ref: "src/ui/mod.rs#layout_80x24_custom_config"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-11-16
status: complete
---

# Phase 2 Plan 2: config.toml consumers Summary

**sidebar_width, timestamp_format (chrono strftime with an injected zone) and key_timeout_ms now drive the layout, message renderer and run loop, with one App::areas and one App::row_ctx shared by the reducer and the view**

## Performance

- **Duration:** about 15 min (start time was not captured at spawn; estimated)
- **Completed:** 2026-11-16T15:36Z
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 13 (3 created)

## Accomplishments

- Tracer: `sidebar_width = 40` makes the sidebar 40 columns on screen, and the messages viewport width, input rows and sidebar half-page all follow it through `App::areas()`. The default 28 leaves every Phase 1 snapshot byte for byte unchanged. 60 columns on a 40x8, 61x10 or 80x24 screen renders without panicking (messages pane 20 columns wide at 80x24).
- `timestamp_format` formats every message time with chrono in `ui::timefmt`; the UTC `hhmm` placeholder is gone. Out-of-range timestamps render `?`, and the output passes through `sanitize_line` before it is measured or drawn.
- `RowCtx` plus `App::row_ctx` is the only place a row context is built, so scroll math and drawing measure the same rows; Compact pads times to the room's widest formatted time so bodies stay aligned for formats such as `%B`.
- `key_timeout_ms` replaces the fixed 1000 ms constant in the run loop; the binary sets `Zone::Local` once after `App::with_config`, so it shows system local time while tests and `App::new` stay UTC.
- Two reviewed full-screen snapshots (`layout_80x24_sidebar_40`, `layout_80x24_custom_config`); the three Phase 1 80x24 snapshots and the two 52x20 messages snapshots are untouched.

## Task Commits

1. **Task 1 (tracer): sidebar_width end to end** - `9370383` (feat)
2. **Task 2: timestamp_format via chrono, injected zone, shared RowCtx** - `6001f33` (feat)
3. **Task 3: key_timeout_ms, local zone in the binary, custom config snapshot** - `6d83551` (feat)

**Plan metadata:** committed with this summary (docs).

TDD note: Rust tests live in the same files as the code they exercise and reference symbols that did not exist yet (new signatures, new module), so a RED commit that compiles on its own is not possible. As the orchestrator allowed, each task is one commit with the tests written alongside. The plan frontmatter is `type: execute`, so no plan-level TDD gate applies and no `check tdd-red-evidence` records exist. One RED-like failure was observed: my first `time_column_alignment` expectation (15) was an arithmetic slip, and the test reported 16 (10 time + 1 + 3 author + 2); the implementation was right and the test was corrected.

## Files Created/Modified

- `src/ui/timefmt.rs` - `Zone`, `OUT_OF_RANGE`, `format_ts` and its four tests
- `src/ui/layout.rs` - `areas` takes `sidebar_width`; `SIDEBAR_WIDTH` kept as the default
- `src/ui/wrap.rs` - `RowCtx`, `time_column`; `render_rows` and `message_rows` take `&RowCtx`
- `src/ui/messages.rs` - placeholder formatter and its test removed; draws with `app.row_ctx`
- `src/ui/mod.rs` - `pub mod timefmt`, `view` passes the width, new layout and snapshot tests
- `src/ui/input.rs` - test helper uses `app.areas()`
- `src/app/mod.rs` - `zone`, `areas`, `row_ctx`, `key_timeout`, `mod tests`
- `src/app/messages.rs`, `src/app/sidebar.rs` - geometry and row model via `App::areas` and `App::row_ctx`
- `src/lib.rs` - `KEY_TIMEOUT` constant removed, deadline from `app.key_timeout()`, `Zone::Local`
- `src/test_util.rs` - added `config_from_toml`
- `src/ui/snapshots/..._layout_80x24_sidebar_40.snap`, `..._layout_80x24_custom_config.snap` - reviewed screens

## Decisions Made

- Followed the plan for A2, A24, A25 and A26 (see key-decisions in the frontmatter).
- A time wider than the room's `time_col` keeps its own width in Compact; the plan's formula assumes it never happens for a room's own messages, and this keeps the first row and the hanging indent consistent if it does.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `config_from_toml` to src/test_util.rs**
- **Found during:** Task 1
- **Issue:** The plan's tests in three modules each need a `Config` from a config.toml string; the file list does not include src/test_util.rs. Three local copies of the same helper would be duplication.
- **Fix:** One additive helper `config_from_toml(toml) -> Config` (panics with the problem count, never prints the config text). No existing helper changed. Plan 02-03 runs after this plan, so there is no concurrent edit.
- **Files modified:** src/test_util.rs
- **Verification:** all tests that use it pass
- **Committed in:** 9370383

**2. [Naming] Test helper `rows_at` instead of `rows`**
- **Found during:** Task 2
- **Issue:** The plan names the wrap test helper `rows`, but most existing tests bind a local `let rows = ...`, which would shadow it.
- **Fix:** Named the helper `rows_at` (and `count_at` for `message_rows`). Same assertions in every existing test.
- **Files modified:** src/ui/wrap.rs
- **Committed in:** 6001f33

---

**Total deviations:** 2 (1 blocking-file addition, 1 naming)
**Impact on plan:** None on behaviour or scope.

## Issues Encountered

- The task 3 `<human-check>` (real terminal, `sidebar_width = 40`, two-line, `%Y-%m-%d %H:%M`, `key_timeout_ms = 3000`, watch the pending `g` stay about 3 s) was not run: no interactive terminal is available to the executor. The pty scenarios and unit tests cover the mechanics; the look and the wall-clock timing remain for the author's UAT.
- The 8-path task 1 exceeds the 5-file guideline on purpose, as the plan documents (compiler-listed one-line call-site edits).

## Known Stubs

None. `history_size` is stored by plan 02-01 and consumed in Phase 3 by locked decision D-13, not a stub of this plan.

## Threat Flags

None beyond the plan's threat model. T-02-07 (saturating layout, `sidebar_wider_than_terminal`), T-02-08 (every formatted time through `sanitize_line`), T-02-09 (no unwrap, `?` placeholder, `out_of_range_is_placeholder`) and T-02-SC2 (Cargo.toml and Cargo.lock untouched) are implemented and covered.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plans 02-03 (keymap.toml) and later can rely on `App::areas`, `App::row_ctx`, `App::key_timeout` without further seams; plan 02-03's files (src/input/*, src/config/keymap.rs, src/action.rs, src/app/update.rs) were not touched.
- Pending for the author: the manual UAT of the task 3 human-check, and the CONF-07 server-URL part stays deferred to Phase 3 (locked D-12).

## Self-Check: PASSED

- Created files present: src/ui/timefmt.rs and both snapshots (checked with `[ -f ]`).
- Commits `9370383`, `6001f33`, `6d83551` are ancestors of HEAD; `git rev-list --count 1703946..HEAD` = 3.
- `cargo test` 206 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, pty scenarios quit, insert, config_layout and config_error print SMOKE OK, `find src -name '*.snap.new'` empty, `grep -c hhmm` over src/ui/messages.rs and src/ui/wrap.rs is 0.

---
*Phase: 02-configuration-system*
*Completed: 2026-11-16*
