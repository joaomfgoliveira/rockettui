---
phase: 01-modal-tui-shell
plan: 01
subsystem: core
tags: [rust, ratatui, crossterm, tokio, tea, keymap, vim-notation]

requires: []
provides:
  - "rockettui library crate (edition 2024, rust-version 1.88) with the audited Phase 1 dependency set and a committed Cargo.lock"
  - "Action enum (single TEA message type) with hand-written redacting Debug"
  - "App state plus the pure reducer app::update::update; App::new holds no room data, App::with_fake_data feeds data::fake::seed() through update (D-13 seam)"
  - "input::key: Chord, normalize, parse_keys (full Vim notation, typed ParseError); input::keymap: (Mode, notation, Action) rows, default_bindings, default_keymap"
  - "app::command: table-driven : commands (:q / :quit, unknown-command error); app::sidebar: cursor, selected_room, apply_motion, on_rows_changed"
  - "data: RoomId, FolderId, RoomKind, Room, Folder, Message (redacting Debug) and the 8-room D-13 fake dataset (FAKE_EPOCH, seed())"
  - "cfg(test) test_util (press, app_80x24) inline at the end of src/lib.rs"
affects: [01-02, 01-03, 01-04, 01-05, 01-06, phase-02-config, phase-03-rest]

actuals:
  tokens: 10100
  tasks: 3
  commits: 5
plan_head_before: 6596a3f2046a2850537c857f251cd6deab10a6d3
plan_head_after: 6c23e817687b2ac6d1f7687bbc9e906323796c55

tech-stack:
  added: [ratatui 0.30.2, crossterm 0.29.0 (event-stream), tokio 1.54.1 (rt-multi-thread, macros, sync, time), futures 0.3.34, unicode-width 0.2, unicode-segmentation 1, textwrap 0.16.4, insta 1.49.0 (dev)]
  patterns:
    - "Elm/TEA: one App, one Action enum, one reducer; data enters only as Actions"
    - "Keymap as data rows (Mode, Vim notation, Action) parsed by parse_keys; no per-mode key match in the reducer"
    - "Redacting hand-written Debug for every type that can carry typed, pasted or message text"
    - "Reducer never reads a clock; Action::Clock is the only time input"

key-files:
  created:
    - Cargo.toml
    - Cargo.lock
    - .gitignore
    - src/lib.rs
    - src/action.rs
    - src/app/mod.rs
    - src/app/update.rs
    - src/app/sidebar.rs
    - src/app/command.rs
    - src/input/mod.rs
    - src/input/key.rs
    - src/input/keymap.rs
    - src/data/mod.rs
    - src/data/fake.rs
  modified: []

key-decisions:
  - "Keymap stores one HashMap<Vec<Chord>, Action> per Mode (plan 01-03 swaps these for tries behind the same from_bindings / lookup API)"
  - "parse_angle returns chords already in the canonical normalize() form (SHIFT folded into the char, CONTROL lowercases) so parsed rows match live normalized keys"
  - "Chord-comparing tests use assert!(a == b) rather than assert_eq!, because the redacting Debug hides the char and would make failure output useless"

patterns-established:
  - "Tests drive the real reducer through crate::test_util::press(app, \"<C-w>h\") in Vim notation"
  - "Sidebar assertions use sidebar::selected_room and relative movement, never raw indices"

requirements-completed: [NAV-01, NAV-02, NAV-03]

coverage:
  - id: D1
    description: "Headless tracer: seed Actions through the reducer select general first; j/k move the sidebar selection; ':' enters Command mode; q then Enter sets should_quit"
    requirement: "NAV-03"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::tracer_seed_keys_quit"
        status: pass
    human_judgment: false
  - id: D2
    description: "Sidebar j/k movement clamps at both ends (NAV-02 sidebar j/k)"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::motion_j_k_sidebar"
        status: pass
    human_judgment: false
  - id: D3
    description: "Modal behavior: ':' enters COMMAND, Esc and Ctrl-c cancel and never quit, :q and :quit quit, unknown command errors and clears on next key, empty command is a no-op, Backspace on empty returns to Normal, command line capped at 256 chars"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::mode_ app::update::tests::command_ app::command::tests"
        status: pass
    human_judgment: false
  - id: D4
    description: "Vim-notation grammar (modifiers, named keys, typed ParseError) and keymap built from (Mode, notation, Action) rows"
    verification:
      - kind: unit
        ref: "cargo test --lib input::"
        status: pass
    human_judgment: false
  - id: D5
    description: "No Debug output of Action, Message or Chord carries typed, pasted or message text (T-01-03)"
    verification:
      - kind: unit
        ref: "cargo test --lib action::tests::debug_redacts data::tests::debug_redacts input::key::tests::debug_redacts_typed_char"
        status: pass
    human_judgment: false
  - id: D6
    description: "Full D-13 fake dataset delivered through the seed Action seam: 8 rooms, 5 kinds, 3 unread, 1 encrypted room with messages, exactly one empty room, 2 rooms with 50+ messages, one 600+ character message with an 80-character token"
    verification:
      - kind: unit
        ref: "cargo test --lib data::fake::tests::seed_matches_d13 data::tests::with_fake_data_loads_everything"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-11-15
status: complete
---

# Phase 1 Plan 01: Headless TEA Core Summary

**Rust crate with the headless Elm/TEA core: a pure reducer driven by Vim-notation keymap data rows, table-driven `:q`, sidebar cursor and the full 8-room fake dataset delivered as seed Actions, with redacting Debug on every text-carrying type.**

## Performance

- **Duration:** about 25 min (start time was not captured; estimated from the session)
- **Completed:** 2026-11-15
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 14 created, 0 modified

## Accomplishments
- Tracer (task 1): `cargo init`, the eight audited crates and a committed Cargo.lock; Action, App, reducer, keymap rows, sidebar cursor, command table and the fake data seam, proven end to end by `tracer_seed_keys_quit` (seed Actions -> j/k -> `:q<CR>` -> `should_quit`).
- Full Vim-notation grammar in `parse_keys` (C-/A-/M-/S- modifiers, Tab, Del, Space, lt, arrows, Home/End, PageUp/PageDown) with typed `ParseError`; the built-in keymap is guarded by a unit test.
- Redacting `Debug` for `Chord`, `Action` and `Message`, with sentinel tests (T-01-03).
- 8-room D-13 dataset (all five kinds, Work and People folders, encrypted `secrets`, empty `quiet`, `general` 60 and `dev-team` 55 messages, CJK and emoji bodies, one very long message) behind the unchanged seam, pinned by `seed_matches_d13`.
- Mode, command, resize and clock edge behavior pinned by tests; 22 unit tests pass, clippy `-D warnings` and `cargo fmt --check` clean.

## Task Commits

1. **Task 1 (tracer): headless core end to end** - `762ad2a` (feat)
2. **Task 2: Vim-notation grammar and redacting Debug** - RED `f0d2765` (test), GREEN `1ee6680` (feat)
3. **Task 3: full D-13 dataset and mode/command edge tests** - RED `16db974` (test), GREEN `6c23e81` (feat)

**Plan metadata:** committed separately with this SUMMARY (docs).

## TDD Notes

- Task 2 RED (`f0d2765`): `parse_keys_notation` failed on the planned assertion (`<lt>`, `<S-Tab>`, `<A-CR>`, `<M-x>`, `<Space>` not yet in the grammar). The redaction tests (`debug_redacts`, `debug_redacts_typed_char`) cannot exist in a RED commit because they do not compile without the `Debug` impls; they landed with the GREEN commit. `normalize_cases`, `parse_keys_errors` and `default_keymap_parses` pin behavior the tracer already had.
- Task 3 RED (`16db974`): `seed_matches_d13` and `with_fake_data_loads_everything` failed on the planned assertion (3 rooms vs 8). The mode/command/resize/clock edge tests passed already because the tracer implemented that behavior; they are pinning tests.
- `workflow.tdd_mode` is not enabled and the plan is `type: execute`, so no `gsd_run check tdd-red-evidence` classifier run was performed.

## Decisions Made
- Keymap rows are stored per mode in `HashMap<Vec<Chord>, Action>` (plan 01-03 replaces the maps with tries, same API).
- `parse_keys` yields canonical chords (equal to what `normalize` produces for the same live key), so `<S-x>` is `Char('X')` and `<C-W>` is `Char('w')` + CONTROL.
- Chord comparisons in tests use `assert!(a == b)`: the redacting `Debug` prints `Char(..)`, so `assert_eq!` failure output would be useless.

## Deviations from Plan

None - plan executed exactly as written.

One procedural note, not a plan deviation: the worktree sandbox refused git-dir path access, so the per-plan commit ledger file and spawn-toplevel sentinel (protocol 0a/0c) could not be written. The base commit is known and recorded above (`plan_head_before`), and `commits: 5` was measured with `git rev-list --count 6596a3f..HEAD` before this SUMMARY commit.

## Issues Encountered
None.

## Known Stubs
None. `App::input_rows` returns the constant 1 and `Action::Paste` is ignored in Normal and Command mode by design; plan 01-06 owns both (documented in the plan).

## Threat Flags
None. No network crates (`reqwest`, `hyper`, `tokio-tungstenite`, `ureq` absent from Cargo.lock; tokio has no `net` feature), no print macros under `src/`, no clock reads under `src/app` or `src/input`.

## Verification Results
- `cargo test --lib`: 22 passed
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `cargo tree -i crossterm --depth 0`: single `crossterm v0.29.0`
- Cargo.lock tracked and in HEAD (`git ls-files --error-unmatch`, `git cat-file -e HEAD:Cargo.lock`)
- Grep gates: no `KeyCode::Char('` in the non-test part of update.rs, no print macros, no clock reads, no derived Debug on Action: all 0

## Next Phase Readiness
Ready for plan 01-02 (terminal layer, binary tracer). It moves the inline `test_util` block from `src/lib.rs` into `src/test_util.rs` unchanged, under the same crate path.

## Self-Check: PASSED

All 14 listed files exist on disk; commits `762ad2a`, `f0d2765`, `1ee6680`, `16db974`, `6c23e81` are on this branch (`git log 6596a3f..HEAD`).
