---
phase: 01-modal-tui-shell
plan: 04
subsystem: ui
tags: [ratatui, sidebar, folds, insta, tdd]
status: complete

requires:
  - phase: 01-modal-tui-shell
    provides: key matcher and Normal keymap (01-03), App::open_room, sanitize_line, fake data (01-01/01-02)
provides:
  - Sidebar row model (Unread / Folders / Recent), fold state and cursor over selectable rows
  - Enter on a room opens it and focuses the messages pane; h/l/za/Enter fold folders
  - Sectioned sidebar renderer with kind glyphs, unread and [e2e] markers, reviewed snapshots
affects: [01-05 messages pane, 01-06 full-layout snapshots, Phase 2 keymap/theme]

actuals:
  tokens: 7500
  tasks: 2
  commits: 4
plan_head_before: fd48bb9a4abf0eb19c979d83be63e33b430f78ed
plan_head_after: 1ea416cdd8f8743a4f84569d38fdafe5cb9ae461

tech-stack:
  added: []
  patterns:
    - "Rows are derived from App data by a pure rows(app); only cursor and collapsed folders are stored"
    - "Every room has exactly one home section, precedence Unread > Folder > Recent"

key-files:
  created:
    - src/ui/snapshots/rockettui__ui__sidebar__tests__sidebar_sections_28x20.snap
    - src/ui/snapshots/rockettui__ui__sidebar__tests__sidebar_work_collapsed_28x20.snap
  modified:
    - src/app/sidebar.rs
    - src/ui/sidebar.rs

key-decisions:
  - "A1 placement rule (Unread wins over Folder over Recent) lives only in app::sidebar::rows; a folder row's count is the number of unread member rooms"
  - "h/l act only on folder rows (D-19); za and Enter on folders and za on a child collapse are Claude's discretion (A2)"
  - "Fold changes always leave the cursor on the affected folder row, so row identity is preserved without index bookkeeping"

patterns-established:
  - "Cursor motions index into the list of selectable rows, so headers can never be selected"
  - "Row text is one Line of one style, keeping a room name a contiguous run of identically styled cells (pty readiness check)"

requirements-completed: [NAV-02, NAV-04]

coverage:
  - id: D1
    description: "Sidebar groups rooms into Unread, Folders and Recent, each room once, hidden while its folder is collapsed"
    requirement: NAV-02
    verification:
      - kind: unit
        ref: "src/app/sidebar.rs#sections_order_and_membership, each_room_once, folds"
        status: pass
    human_judgment: false
  - id: D2
    description: "j/k, counts, gg/G, Ctrl-d/Ctrl-u move across selectable rows only and clamp; h/l fold folders only; Enter opens a room"
    requirement: NAV-02
    verification:
      - kind: unit
        ref: "src/app/sidebar.rs#cursor_never_on_header, edges, half_page, child_za_collapses_parent, enter_opens_room, empty_sidebar"
        status: pass
    human_judgment: false
  - id: D3
    description: "Sidebar renders headers, fold markers, kind glyphs, unread bold + marker, [e2e] tag through the sanitizer"
    requirement: NAV-04
    verification:
      - kind: unit
        ref: "src/ui/sidebar.rs#sidebar_sections_28x20, sidebar_work_collapsed_28x20, unread_rooms_are_bold, glyphs_and_markers, hostile_room_name_sanitized"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui quit"
        status: pass
    human_judgment: false

duration: 25 min
completed: 2026-11-15
---

# Phase 1 Plan 04: Sidebar sections, folds and renderer Summary

**Sectioned sidebar (Unread / Folders / Recent) with foldable folders, a cursor that only ever lands on selectable rows, Enter-to-open, and a ratatui renderer with kind glyphs, unread and [e2e] markers, all names sanitized.**

## Accomplishments

- `src/app/sidebar.rs`: `Section`, `SidebarRow`, `SidebarState { cursor, collapsed }`, pure `rows(app)`, `selected_row`, `selected_room`, reworked `apply_motion` and `on_rows_changed`. `src/app/update.rs` needed no change.
- Placement: unread rooms under Unread (newest first), folders in data order with member rooms indented, the rest under Recent; empty sections have no header; a collapsed folder hides its rooms everywhere.
- Motions: j/k/counts, gg/G (`5gg` is the 5th selectable row), Ctrl-d/Ctrl-u (half the sidebar inner height, at least 1), all clamped. h/l fold or unfold folder rows only and are no-ops on room rows (D-19). za and Enter toggle folders; za on an indented room collapses its parent and selects it. Enter on a room calls `App::open_room`.
- `src/ui/sidebar.rs`: List with `Rooms` block, bold dim headers, `▾`/`▸` folder rows with ` (N)` unread count, two-space indent per level, `* ` unread marker with bold, `kind_glyph` (# & ~ + @), ` [e2e]` suffix, highlight `> ` (REVERSED when focused, BOLD otherwise).
- Two insta snapshots were generated, inspected by eye and re-run without `INSTA_UPDATE`; no `.snap.new` files remain.

## Task Commits

1. Task 1 RED: `59ea7be` test(01-04) failing sidebar model tests
2. Task 1 GREEN: `675f4db` feat(01-04) sidebar sections, folds, cursor, Enter
3. Task 2 RED: `a4e1a23` test(01-04) failing renderer tests (snapshots fail without a stored .snap)
4. Task 2 GREEN: `1ea416c` feat(01-04) sectioned renderer and the two snapshots

## Verification

- `cargo test`: 78 passed (9 in `app::sidebar::tests`, 6 in `ui::sidebar::tests`)
- `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean
- `python3 -I scripts/smoke_pty.py target/debug/rockettui quit`: `SMOKE OK: quit`
- Earlier `motion_`, `tracer_`, `focus_` tests still pass

## Deviations from Plan

None - plan executed exactly as written. Two small notes: the Task 2 RED commit message says "and snapshots" although the snapshot files were not yet stored (the snapshot tests failed for lack of them); the extra test `collapsed_folder_shows_closed_marker` was added beyond the five required renderer tests. No files outside `files_modified` were touched.

## Known Stubs

None.

## Threat Flags

None. T-01-13/T-01-14 (sanitized names, app-drawn kind glyph) and T-01-15 (cursor clamping, saturating math) are covered by `hostile_room_name_sanitized`, `kind_glyph` tests and `empty_sidebar` / `cursor_never_on_header`. No new crates.

## Self-Check: PASSED

- FOUND: src/app/sidebar.rs, src/ui/sidebar.rs, both snapshot files
- FOUND commits: 59ea7be, 675f4db, a4e1a23, 1ea416c
