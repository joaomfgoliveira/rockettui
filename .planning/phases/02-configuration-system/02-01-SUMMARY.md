---
phase: 02-configuration-system
plan: 01
subsystem: config
tags: [toml, chrono, xdg, config-validation, error-screen, ratatui, pty-smoke]

requires:
  - phase: 01-modal-tui-shell
    provides: App/Action/reducer, Keymap::from_bindings, MessageLayout, sanitize_line, pty smoke harness
provides:
  - src/config module (paths, diag, settings, Config, load) with a pure core and a thin IO shell
  - config.toml settings of D-12/D-13 validated with file:line:col problems
  - App::with_config and run(terminal, config); main loads config before the terminal
  - In-TUI config error screen (refuse to start, any key exits 1, report on stderr)
  - Test helpers temp_config_dir and app_with_config; XDG-isolated pty scenarios config_layout and config_error
affects: [02-02 config.toml consumers, 02-03 keymap.toml, 02-04 theme.toml, 02-05 theme consumers, 03 server_url and history_size]

actuals:
  tokens: 16300
  tasks: 3
  commits: 3

tech-stack:
  added: [toml 1.2 (parse, std), chrono 0.4 (clock, std)]
  patterns:
    - "Pure core (Sources -> Result<Config, Report>) plus IO shell; env passed as arguments"
    - "Spanned DeTable walk with a Reader accumulating Problems (no serde derive)"
    - "Problem text built from toml message() and span() only, never the error Display"

key-files:
  created:
    - src/config/mod.rs
    - src/config/diag.rs
    - src/config/paths.rs
    - src/config/settings.rs
    - src/ui/error_screen.rs
    - src/ui/snapshots/rockettui__ui__error_screen__tests__error_screen_80x24.snap
  modified:
    - Cargo.toml
    - Cargo.lock
    - src/lib.rs
    - src/main.rs
    - src/app/mod.rs
    - src/test_util.rs
    - src/ui/mod.rs
    - scripts/smoke_pty.py

key-decisions:
  - "message_layout wired end to end first (tracer); the other keys were added without moving any seam"
  - "Unknown key names are echoed at most 32 characters on screen and stderr"
  - "Error screen uses fixed built-in styles, never the user theme; one documented exception to the TEA constraint (show_config_errors reads crossterm directly)"
  - "TimeFormat validates through StrftimeItems plus a write! sample render, so no user format can panic via to_string"

patterns-established:
  - "Config problems are collected per file and sorted by (file, position); discovery order is kept for ties"
  - "Every problem string passes through sanitize_line before it is drawn or printed"

requirements-completed: [CONF-01, CONF-02, CONF-03, CONF-07]

plan_head_before: 58b0b02ea55696586686e085c58e3f42b2a9dee2
plan_head_after: a2d21f48f46beef4bc56d3f43ed7db975869e302
commits: 3

coverage:
  - id: D1
    description: "XDG config dir resolution and read-only, hardened file reads; absent files give defaults and nothing is created"
    requirement: "CONF-01"
    verification:
      - kind: unit
        ref: "src/config/paths.rs#config_dir_resolution"
        status: pass
      - kind: unit
        ref: "src/config/paths.rs#read_source_hardening"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#absent_files_default"
        status: pass
    human_judgment: false
  - id: D2
    description: "With no config files the app is exactly Phase 1 (169 prior tests and 7 snapshots unchanged); empty files behave as absent"
    requirement: "CONF-02"
    verification:
      - kind: unit
        ref: "cargo test (193 passed, includes the 169 Phase 1 tests)"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#empty_files_are_absent"
        status: pass
    human_judgment: false
  - id: D3
    description: "message_layout = two_line in config.toml changes the running binary's message layout"
    requirement: "CONF-07"
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui config_layout"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#tracer_two_line_from_dir"
        status: pass
    human_judgment: false
  - id: D4
    description: "All config.toml keys validated with the D-13 ranges; invalid values, types, enums and strftime formats are errors with file, line:col and key; unknown keys only warn"
    requirement: "CONF-07"
    verification:
      - kind: unit
        ref: "src/config/settings.rs#bounds_inclusive"
        status: pass
      - kind: unit
        ref: "src/config/settings.rs#timestamp_format_validation"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#unknown_keys_warn_only"
        status: pass
    human_judgment: false
  - id: D5
    description: "Every problem across the three files is collected in one pass, sorted, and no source line is ever echoed"
    requirement: "CONF-03"
    verification:
      - kind: unit
        ref: "src/config/mod.rs#multiple_errors_one_pass"
        status: pass
      - kind: unit
        ref: "src/config/diag.rs#no_source_echo"
        status: pass
    human_judgment: false
  - id: D6
    description: "Invalid config: error screen lists everything, any key exits 1, terminal restored, sanitized report on stderr, no App started"
    requirement: "CONF-03"
    verification:
      - kind: unit
        ref: "src/ui/error_screen.rs (error_screen_80x24, truncates_with_more_line, hostile_text_sanitized, quits_only_on_key_press, tiny_sizes_no_panic)"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui config_error"
        status: pass
    human_judgment: true
    rationale: "Readability of the error screen in a real terminal is a human judgment (02-VALIDATION.md Manual-Only Verifications); the task 3 human-check has not been run"

duration: 25min
completed: 2026-11-16
status: complete
---

# Phase 2 Plan 1: Configuration pipeline and error screen Summary

**Pure TOML config core (toml 1.2 spanned tree, chrono strftime validation) feeding App::with_config, with config.toml message_layout proven in the real binary and an in-TUI error screen that lists every problem and exits 1**

## Performance

- **Duration:** about 25 min (start time was not captured at spawn; estimated)
- **Completed:** 2026-11-16T15:30Z
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 14 (6 created)

## Accomplishments

- Tracer: `message_layout = "two_line"` in `$XDG_CONFIG_HOME/rockettui/config.toml` makes the running binary draw author + time header rows with the body indented below; proven by the `config_layout` pty scenario and a unit test. No files means Phase 1 exactly.
- All six config.toml keys of D-12 (`keymap_preset`, `sidebar_width`, `history_size`, `timestamp_format`, `message_layout`, `key_timeout_ms`) are validated with the D-13 ranges; every problem carries file, line:col and key. `history_size` is stored in `App.settings` and consumed by nothing (Phase 3).
- One report for three files: every syntax error (`parse_recoverable`) plus every semantic error, sorted by file then position; unknown keys and tables (`server_url` included) are warnings shown only next to a real error.
- Hardened reads: non-regular files, files over 1 MiB, non-UTF-8 and unreadable files are errors; a missing or vanished file is absent. Nothing under the config dir is ever written.
- Error screen (`ui::error_screen`) with fixed styles, truncation line, reviewed 80x24 snapshot, and the `config_error` pty scenario (exit 1, ALT_ON before ALT_OFF, report on stderr after restore, no `NORMAL` ever drawn).
- pty harness now runs every scenario with an isolated, normally empty `XDG_CONFIG_HOME`.

## Task Commits

1. **Task 1 (tracer): message_layout end to end** - `79d91f0` (feat)
2. **Task 2: validate every config.toml key, one-pass collection, hardened reads** - `806ce22` (feat)
3. **Task 3: in-TUI config error screen** - `a2d21f4` (feat)

**Plan metadata:** committed with this summary (docs).

TDD note: the tests live in the same files as the code they exercise (Rust `#[cfg(test)]` modules) and reference symbols that did not exist yet, so a RED commit that compiles on its own is not possible. Per the orchestrator's note, each TDD task is one commit with the tests written alongside; no separate RED/GREEN commits and no `check tdd-red-evidence` records. The tests were first run green; a failing run was seen only for the tracer test, whose first version had a too-loose Compact matcher (see Issues Encountered).

## Files Created/Modified

- `src/config/mod.rs` - `Config`, `Sources`, `load`, `load_from_dir`, `load_from_sources`; cross-file tests
- `src/config/diag.rs` - `ConfigFile`, `Severity`, `Problem`, `Report`, `line_col`, `Reader` (`string`, `one_of`, `int_in`, `warn_unknown`), `parse_table`, `check_syntax`
- `src/config/paths.rs` - `config_dir`, hardened `read_source`, `read_sources`, `MAX_CONFIG_BYTES`
- `src/config/settings.rs` - `Settings`, `KeymapPreset`, `TimeFormat` (validating `parse`), range constants, `parse`
- `src/ui/error_screen.rs` - `render`, `quits_on`, tests and snapshot
- `src/lib.rs` - `pub mod config`, `run(terminal, config)`, `show_config_errors`
- `src/main.rs` - `main() -> ExitCode`, config loaded before the terminal, error branch
- `src/app/mod.rs` - `App.settings`, `App::with_config` (`App::new` delegates)
- `src/test_util.rs` - `temp_config_dir`, `app_with_config`
- `scripts/smoke_pty.py` - XDG isolation, `config_layout`, `config_error`
- `Cargo.toml`, `Cargo.lock` - `toml` and `chrono` (default features off; no serde, no networking crate)

## Decisions Made

- Unknown key names are echoed at most 32 characters (the prohibition covers values, but a key name is also user text; cheap to cap).
- Error counts line uses real plurals (`3 errors, 1 warning in <dir>`) instead of the literal `error(s)`.
- `Report::new` sorts with `sort_by_key((file, pos))`; `None` positions sort first because `Option` orders `None` before `Some`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Tracer test matched a time inside a message body as the Compact layout**
- **Found during:** Task 1
- **Issue:** My first `has_compact_prefix` matched `HH:MM` + space + letter anywhere, so the fake message "standup moved to 10:30 today" counted as Compact and the two-line test failed.
- **Fix:** Anchored the match on a pane border character, as the smoke scenario already does, and added a positive control (default config draws Compact).
- **Files modified:** src/config/mod.rs
- **Verification:** `config::tests::tracer_two_line_from_dir` passes
- **Committed in:** 79d91f0

**2. [Rule 2 - Missing critical] Cap on echoed unknown key names**
- **Found during:** Task 3
- **Issue:** `warn_unknown` echoed a whole user-written key name; the plan's prohibition limits echoed user text to 32 characters.
- **Fix:** Truncate the shown name to 32 characters.
- **Files modified:** src/config/diag.rs
- **Committed in:** a2d21f4

**3. [Rule 1 - Lint] clippy::type_complexity on a test table**
- **Found during:** Task 2
- **Fix:** Introduced a local `type Case` alias in `bounds_inclusive`.
- **Committed in:** 806ce22

---

**Total deviations:** 3 auto-fixed (2 bug/lint, 1 missing critical)
**Impact on plan:** None on scope. Plan behaviour and file list unchanged.

## Issues Encountered

- The task 3 `<human-check>` (run the real binary in a terminal with an invalid config and judge readability) was not run: no interactive terminal is available to the executor. The pty scenario covers the mechanics (screen content, any key exits 1, restore, stderr report); readability remains for the author's UAT.
- The 5-file-per-task guideline is exceeded by task 1 on purpose, as the plan documents.

## Known Stubs

None. `Config.keymap` is `default_keymap()` and keymap.toml/theme.toml are syntax-checked only; both are intentional seams for plans 02-03 and 02-04, not a stub that blocks this plan's goal.

## Threat Flags

None beyond the plan's threat model. T-02-01 (no source echo), T-02-02 (sanitize before draw and print), T-02-03 (metadata-first reads), T-02-04 (relative XDG ignored), T-02-05 (strftime validated without panic paths) and T-02-SC1 (toml and chrono only, networking-crate guard reads 0) are implemented and covered by tests.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plans 02-02 and 02-03 (wave 2) can plug into `Settings`, `Config`, `Reader` and `App.settings` without changing any seam. `Cargo.toml` is final for Phase 2.
- Pending for the author: the manual error-screen readability check (task 3 human-check).

## TDD Gate Compliance

Not satisfied by design for RED/GREEN as separate commits (see TDD note above); the plan frontmatter is `type: execute`, so no plan-level gate applies. Each TDD task has its tests and implementation in one commit.

## Self-Check: PASSED

- Created files present: src/config/{mod,diag,paths,settings}.rs, src/ui/error_screen.rs, the error screen snapshot (checked with `[ -f ]`).
- Commits `79d91f0`, `806ce22`, `a2d21f4` are ancestors of HEAD; `git rev-list --count 58b0b02..HEAD` = 3.
- `cargo test` 193 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, five pty scenarios print SMOKE OK, `find src -name '*.snap.new'` empty.

---
*Phase: 02-configuration-system*
*Completed: 2026-11-16*
