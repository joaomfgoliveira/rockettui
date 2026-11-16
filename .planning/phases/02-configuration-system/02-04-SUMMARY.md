---
phase: 02-configuration-system
plan: 04
subsystem: config
tags: [toml, theme, ratatui, colors, modifiers, border-types, markers, insta-snapshots]

requires:
  - phase: 02-configuration-system (plan 01)
    provides: Reader/Problem diagnostics, parse_table, load_from_sources, shared error screen
  - phase: 02-configuration-system (plan 02)
    provides: App::areas, App::row_ctx, config_from_toml test helper
  - phase: 02-configuration-system (plan 03)
    provides: keymap::build in load_from_sources (only the theme.toml check_syntax call was left)
provides:
  - theme.toml parsing into Theme (16 style elements, two border types, Markers), defaults equal to the Phase 1 look
  - D-08 colors (named, 256-index as integer or string, #rrggbb) and D-09 modifiers with line:col errors
  - Config.theme and App.theme; sidebar, status bar, command line, input bar and borders drawn from app.theme
  - pane_block(title, focused, theme) and validated [markers] (T-02-14)
affects: [02-05 messages pane and author palette theming, 06 any new styled element must add a theme path]

actuals:
  tokens: 15000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Defaults are Rust values (Theme::default), never embedded TOML; a theme.toml only overrides what it names"
    - "StyleSpec.resolve builds a Style from its fields (never Style::patch): a set modifiers list replaces the default, an empty list clears it, an absent one keeps it"
    - "Element vocabulary is one list (ELEMENT_PATHS) plus one match (style_slot_mut); a new styled element is one line in each"
    - "Marker strings are rejected, not sanitized, when sanitize_line would change them; widths are display widths"

key-files:
  created:
    - src/config/theme.rs
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_themed_chrome.snap
  modified:
    - src/config/mod.rs
    - src/app/mod.rs
    - src/ui/status.rs
    - src/ui/sidebar.rs
    - src/ui/input.rs
    - src/ui/mod.rs
    - src/ui/messages.rs
    - src/test_util.rs

key-decisions:
  - "Theme element names (sidebar.cursor, status.error, input.hint, border.focused, ...) are the user-facing theme.toml vocabulary; accepted by the author as A29 and rated costly to rename"
  - "The error screen keeps fixed built-in styles and never reads the theme (A30, confirmed by the author 2026-11-16)"
  - "A rejected marker keeps its default and records one Error; unknown marker keys warn through Reader::warn_unknown only"
  - "Fold-width Warning is positioned at the first fold_* value seen and compares the resulting widths, so setting one fold marker that differs from the default of the other also warns"
  - "Color parsing accepts exactly what ratatui Color::from_str accepts plus integers 0-255 (so reset and bright-/grey spellings also work); the offending value is never echoed"

patterns-established:
  - "Theme problems carry a key path: status.mode.fg, border.focused.type, markers.unread"
  - "Style assertions read cells from test_util::render_buffer; theme inputs come from test_util::config_from_theme"

requirements-completed: [CONF-06, CONF-03]

plan_head_before: 00aeb89fa21e0eb651a7f3d8206ffeb203bf5e7c
plan_head_after: 101a3f98a67866cbbd99317a71ab725fff243582
commits: 3

coverage:
  - id: D1
    description: "theme.toml [status.mode] with an index fg, a hex bg and an italic modifier recolors the mode badge cells end to end, replacing reversed and bold"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/config/theme.rs#tracer_status_mode_from_dir"
        status: pass
    human_judgment: false
  - id: D2
    description: "D-08 colors (names, index as integer or string, #rrggbb) and D-09 modifiers (clear, inherit, order and repeat irrelevant, unknown name is an error) with theme.toml line:col and key path; unknown tables, elements and keys only warn"
    requirement: "CONF-03"
    verification:
      - kind: unit
        ref: "src/config/theme.rs (color_forms, modifiers_clear_and_inherit, unknown_theme_keys_warn, wrong_shapes_are_errors)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Sidebar rows and cursor, status bar spans, command line and input bar draw with styles from app.theme; defaults equal the Phase 1 styles and every Phase 1 snapshot is unchanged"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/config/theme.rs (chrome_elements_parse, defaults_match_phase1_chrome)"
        status: pass
      - kind: unit
        ref: "src/ui/sidebar.rs#themed_cursor_and_unread"
        status: pass
      - kind: unit
        ref: "src/ui/status.rs (themed_error_style, themed_cmdline_and_room)"
        status: pass
      - kind: unit
        ref: "src/ui/input.rs (themed_hint, themed_draft_text)"
        status: pass
    human_judgment: false
  - id: D4
    description: "[border.focused] and [border.unfocused] set pane border and title style and type (plain, rounded, double, thick); other types are config errors"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/config/theme.rs#border_types"
        status: pass
      - kind: unit
        ref: "src/ui/mod.rs (themed_border_corners, border_style_colors_border_and_title)"
        status: pass
    human_judgment: false
  - id: D5
    description: "[markers] sets fold, unread, cursor, encrypted markers and the five kind glyphs; control, bidi and over-wide values are errors; the not-unread placeholder is as wide as the unread marker; fold width mismatch warns"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/config/theme.rs (markers_validation, marker_defaults_match_phase1)"
        status: pass
      - kind: unit
        ref: "src/ui/sidebar.rs#themed_markers"
        status: pass
    human_judgment: false
  - id: D6
    description: "Equal values are never collisions and spelling every value at its default equals leaving it unset; empty theme.toml and empty element tables inherit defaults"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/config/theme.rs (equal_values_allowed, empty_tables_inherit)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Full 80x24 screen with rounded focused and double unfocused borders and custom fold, unread and cursor markers, reviewed by eye"
    requirement: "CONF-06"
    verification:
      - kind: unit
        ref: "src/ui/mod.rs#layout_80x24_themed_chrome"
        status: pass
    human_judgment: false
  - id: D8
    description: "A theme.toml error reaches the shared error screen with file, line:col and key path"
    requirement: "CONF-03"
    verification:
      - kind: e2e
        ref: "ad-hoc pty probe: fg = 300 under [status.mode] printed theme.toml:2:6 status.mode.fg: invalid color (...)"
        status: pass
    human_judgment: false
  - id: D9
    description: "How the colors, rounded and double borders and markers look in a real terminal theme, and that read rooms stay aligned under the unread marker"
    requirement: "CONF-06"
    verification: []
    human_judgment: true
    rationale: "The task 3 human-check (02-VALIDATION.md Manual-Only Verifications) needs an interactive terminal; buffer tests cannot judge the look"

duration: 6min
completed: 2026-11-16
status: complete
---

# Phase 2 Plan 4: theme.toml chrome theming Summary

**theme.toml with D-08 colors (named, 256-index, hex), D-09 per-element fg/bg/modifiers tables, border types and validated sidebar markers, driving the sidebar, status bar, command line, input bar and borders while the defaults reproduce the Phase 1 look**

## Performance

- **Duration:** 6 min (about 5 min 20 s of wall clock)
- **Started:** 2026-11-16T15:43:57Z
- **Completed:** 2026-11-16T15:49:17Z
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 10 (2 created)

## Accomplishments

- Tracer: a theme.toml `[status.mode]` with `fg = 208`, `bg = "#102030"` and `modifiers = ["italic"]` makes the rendered badge cells Indexed(208) on Rgb(16, 32, 48), exactly ITALIC (no reversed, no bold); with no theme the badge is unchanged. The tracer verify (full test suite, clippy, fmt, all snapshots) re-ran green before expansion.
- `Theme` has 16 style elements (`sidebar.{header,folder,room,unread,cursor,cursor_unfocused}`, `status.{mode,room,pending,info,error}`, `cmdline.text`, `input.{text,hint}`, `border.{focused,unfocused}`), two border types and `Markers`; every default equals the old literal, so all Phase 1 and plan 02-02 snapshots pass untouched.
- Colors: names (`light-blue`), index as integer or string (`208`, `"208"`), `#rrggbb`; 300, -1, `"nope"`, `true`, 1.5 are one Error at theme.toml line:col with key `status.mode.fg`, never echoing the value. `modifiers = []` clears, absent inherits, order and repeats do not matter, unknown names list the valid ones.
- Borders: `type` is accepted only in `[border.*]` (thick when focused, plain otherwise by default); `pane_block(title, focused, theme)` applies type and style to border and title.
- Markers: every value must survive `sanitize_line` unchanged (ESC, bidi, tab and newline are Errors), at most 4 display columns (encrypted 12), empty allowed; the not-unread placeholder is `" ".repeat(width(unread))`; unequal fold widths warn.
- A reviewed `layout_80x24_themed_chrome` snapshot (rounded focused sidebar, double other panes, `v `/`> ` folders, `! ` unread with aligned read rooms, `» ` cursor).
- 258 tests pass (236 before this plan, 22 added), clippy and fmt clean, pty scenarios `quit` and `config_error` print SMOKE OK.

## Task Commits

1. **Task 1 (tracer): theme.toml [status.mode] end to end** - `62759a7` (feat)
2. **Task 2: sidebar, status bar, command line and input bar elements** - `c22350f` (feat)
3. **Task 3: border styles and types, markers and glyphs, themed snapshot** - `101a3f9` (feat)

**Plan metadata:** committed with this summary (docs).

TDD note: Rust tests live in the same files as the code they exercise and name symbols that did not exist yet (Theme fields, new signatures), so a RED commit that compiles on its own is not possible. As the orchestrator allowed, each TDD task is one commit with the tests written alongside; there are no separate RED/GREEN commits and no `check tdd-red-evidence` records. The plan frontmatter is `type: execute`, so no plan-level gate applies.

## Files Created/Modified

- `src/config/theme.rs` - Theme, Markers, StyleSpec, color/modifier/border-type/marker parsing, `parse`, ELEMENT_PATHS, MODIFIER_NAMES, width caps, 17 tests
- `src/config/mod.rs` - `Config.theme`, `pub mod theme`, `theme::parse` replaces the theme.toml `check_syntax`
- `src/app/mod.rs` - `App.theme`
- `src/ui/status.rs` - badge, room, pending, info, error and command line read the theme; first tests
- `src/ui/sidebar.rs` - row styles, highlight, fold/unread/cursor/encrypted markers and glyphs from the theme
- `src/ui/input.rs` - hint and draft styles, themed borders
- `src/ui/mod.rs` - `pane_block` with theme, three new tests, themed snapshot test
- `src/ui/messages.rs` - passes the theme to `pane_block`
- `src/test_util.rs` - `render_buffer` and `config_from_theme` helpers
- `src/ui/snapshots/rockettui__ui__tests__layout_80x24_themed_chrome.snap` - reviewed screen

## Decisions Made

- Followed the plan for A6, A7, A29 and A30 (see key-decisions in the frontmatter).
- The fold-width Warning compares the resulting widths of both fold markers, positioned at the first fold value seen, so it also fires when only one fold marker is set to a width that differs from the other's default.
- `kind_glyph` stays in `ui::sidebar` as the one source of default glyphs; `Markers::default` calls it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `render_buffer` and `config_from_theme` to src/test_util.rs**
- **Found during:** Tasks 1 and 2
- **Issue:** the plan's tests in four modules assert cell colors and need a rendered buffer and a Config built from a theme.toml string; src/test_util.rs is not in the plan's file lists. Local copies in each module would be duplication.
- **Fix:** two additive helpers; no existing helper changed (same approach as `config_from_toml` in plan 02-02).
- **Files modified:** src/test_util.rs
- **Committed in:** 62759a7, c22350f

**2. [Naming] Extra tests beyond the plan's list**
- **Found during:** Tasks 2 and 3
- **Issue:** the plan names no test for the command-line style, the input draft style, the border and title colors, or the Phase 1 marker defaults, although D-09 says every element gets a key.
- **Fix:** added `themed_cmdline_and_room`, `themed_draft_text`, `border_style_colors_border_and_title`, `marker_defaults_match_phase1`, `wrong_shapes_are_errors`. No behaviour change.
- **Committed in:** c22350f, 101a3f9

**3. [Count] The plan's per-module test counts were off**
- The plan's `fails_when` thresholds assumed 2 existing ui::input tests; the module has 1. Observed totals: task 2 filter 33 passed (needs 30), task 3 filter 52 passed (needs 48); both thresholds met.

---

**Total deviations:** 3 (1 blocking-file addition, 2 naming or count notes)
**Impact on plan:** None on behaviour or scope.

## Issues Encountered

- The task 3 `<human-check>` (real terminal, theme.toml with cursor bg, mode badge, rounded cyan border and a `●` unread marker, then `fg = 300`) was not run: no interactive terminal is available to the executor. Substitute: an ad-hoc pty probe with `fg = 300` showed the error screen with `theme.toml:2:6 status.mode.fg: invalid color (use a name like "light-blue", an index 0-255, ...`. The look in a real terminal theme is left for the author's UAT.
- Running the binary without a tty panics inside ratatui terminal init (pre-existing, not part of this plan; the error screen needs a terminal).

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. T-02-14 (markers rejected unless `sanitize_line` leaves them unchanged; `markers_validation`), T-02-16 (width caps 4 and 12, placeholder derived from the unread marker; `themed_markers`) and T-02-SC4 (Cargo.toml and Cargo.lock untouched) are implemented and covered. T-02-15 is accepted as planned.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 02-05 can add the messages pane and author palette by appending paths to `ELEMENT_PATHS` and `style_slot_mut`, and a palette parse beside `parse_markers`; `pane_block` already takes the theme and `ui::messages` already passes it.
- Pending for the author: the task 3 manual check (colors, rounded and double borders, markers in a real terminal), and the earlier pending checks of plans 02-02 and 02-03.

## TDD Gate Compliance

Not satisfied by design as separate RED/GREEN commits (see TDD note); each TDD task has tests and implementation in one commit.

## Self-Check: PASSED

- Created files present: src/config/theme.rs and the themed_chrome snapshot (checked with `[ -f ]`).
- Commits `62759a7`, `c22350f`, `101a3f9` are ancestors of HEAD; `git rev-list --count 00aeb89..HEAD` = 3.
- `cargo test` 258 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, pty scenarios quit and config_error print SMOKE OK, `find src -name '*.snap.new'` empty. Acceptance greps all met (theme::parse 1, pub theme: 1, theme.status_mode 1, old badge literal 0 in status.rs, theme.sidebar_ 6, theme.input_ 2, theme.status_error 1, border_type_focused in src/ui/mod.rs, theme.markers in sidebar.rs, sanitize_line in theme.rs). Cargo.toml and Cargo.lock untouched.

---
*Phase: 02-configuration-system*
*Completed: 2026-11-16*
