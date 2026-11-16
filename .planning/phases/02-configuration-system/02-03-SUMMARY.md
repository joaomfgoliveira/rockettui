---
phase: 02-configuration-system
plan: 03
subsystem: config
tags: [toml, keymap, keybindings, trie, vim-extras, lockout-guard, pty-smoke]

requires:
  - phase: 01-modal-tui-shell
    provides: Keymap/Trie/Matcher, Vim key notation (parse_keys, format_keys), command table, pty smoke harness
  - phase: 02-configuration-system (plan 01)
    provides: Reader/Problem diagnostics, parse_table, Settings.keymap_preset, load_from_sources, error screen
provides:
  - keymap.toml loading per mode on top of the vim / vim-extras preset, with a curated action vocabulary
  - none unbinding with trie pruning, cmd:<command> bindings validated against the command table
  - load-time errors for bad notation, unknown action or command, wrong types, duplicate chords, prefix clashes, self-lockout and text-eating sequences; warnings for dead chords and unknown tables
  - Action::FocusNext, FocusPrev, RunCommand and the reducer arms for them
affects: [02-04 theme.toml, 02-05 theme consumers, 04 command-line features that add COMMANDS rows]

actuals:
  tokens: 11563
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Rows applied in two passes (all none removals, then all binds), sorted by file offset, never in TOML table order"
    - "Exhaustive name_of match with no wildcard: every new Action variant must choose whether it is bindable"
    - "User-written text echoed in a problem is capped at 32 characters (key paths, names, values)"

key-files:
  created:
    - src/input/actions.rs
    - src/config/keymap.rs
  modified:
    - src/input/mod.rs
    - src/input/keymap.rs
    - src/input/trie.rs
    - src/config/mod.rs
    - src/action.rs
    - src/app/update.rs
    - scripts/smoke_pty.py

key-decisions:
  - "Action names are the 31 curated snake_case names of D-07 plus focus_next and focus_prev; the vocabulary is now a user-facing contract (renaming one breaks a keymap.toml)"
  - "Rows are ordered by their byte offset in the file, so 'later' in a duplicate report means later in the file even though the toml crate iterates keys sorted"
  - "Trie::clashes is a plain pairwise prefix scan over entries() (n is under 100 per mode) instead of a DFS; same result, simpler to verify"
  - "Duplicate and text-eating checks apply to none rows as well, following the plan text literally"

patterns-established:
  - "keymap problems carry a key path of the form normal.\"<C-w>h\" and the offset of the key (or of the value for value errors)"
  - "Lockout guard runs on the resulting keymap, so it also catches a lockout produced by none rows on a preset"

requirements-completed: [CONF-04, CONF-05, CONF-03]

plan_head_before: 1ed6eb91f77b9451b57b6995f218ddf31d8b2b8c
plan_head_after: eb614e1d5e71d64c78ac329e8da8ac2ddf73738a
commits: 3

coverage:
  - id: D1
    description: "keymap.toml [normal] row rebinds a key from the file into the running binary (';' enters Command mode, ';q<CR>' quits) while the preset rows are kept"
    requirement: "CONF-05"
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui config_keymap"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#tracer_rebind_semicolon"
        status: pass
    human_judgment: false
  - id: D2
    description: "Curated action vocabulary: every name round-trips, internal actions have no name, none and cmd: are not names"
    requirement: "CONF-05"
    verification:
      - kind: unit
        ref: "src/input/actions.rs#names_round_trip"
        status: pass
      - kind: unit
        ref: "src/input/actions.rs#internal_actions_unnamed"
        status: pass
    human_judgment: false
  - id: D3
    description: "keymap_preset vim equals the Phase 1 defaults; vim-extras adds only Tab and Shift-Tab pane focus cycling, rendered focus follows"
    requirement: "CONF-04"
    verification:
      - kind: unit
        ref: "src/config/keymap.rs#preset_vim_equals_default"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#preset_vim_extras_adds_tab"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#vim_extras_tab_moves_focus_rendered"
        status: pass
      - kind: unit
        ref: "src/app/update.rs#focus_cycle"
        status: pass
    human_judgment: false
  - id: D4
    description: "cmd:<command> binds a key to a validated ':' command; RunCommand never prints its text in Debug"
    requirement: "CONF-05"
    verification:
      - kind: unit
        ref: "src/config/keymap.rs#cmd_binding_runs_command"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#cmd_unknown_command_error"
        status: pass
      - kind: unit
        ref: "src/action.rs#debug_redacts"
        status: pass
    human_judgment: false
  - id: D5
    description: "none unbinds and prunes empty trie branches; removals precede binds regardless of TOML order; unbinding an unbound key is silent"
    requirement: "CONF-05"
    verification:
      - kind: unit
        ref: "src/input/trie.rs#remove_prunes"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#none_unbinds"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#none_gg_then_bind_g"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#unbind_unbound_is_noop"
        status: pass
    human_judgment: false
  - id: D6
    description: "Duplicate spellings, prefix clashes, self-lockout and text-eating sequences are config errors with keymap.toml line:col and key path; dead chords and unknown tables are warnings; all land on the shared error screen"
    requirement: "CONF-03"
    verification:
      - kind: unit
        ref: "src/config/keymap.rs#duplicate_spellings"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#prefix_clash_names_both"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#lockout_guard"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#insert_sequence_rejected"
        status: pass
      - kind: unit
        ref: "src/config/keymap.rs#dead_chords_warn"
        status: pass
    human_judgment: false
  - id: D7
    description: "Key feel of Tab/Shift-Tab and J/K rebinding in a real terminal, and readability of the clash message on the error screen"
    requirement: "CONF-05"
    verification: []
    human_judgment: true
    rationale: "The task 3 human-check (02-VALIDATION.md Manual-Only Verifications) needs an interactive terminal; the executor ran an ad-hoc pty probe of the error screen only"

duration: 5min
completed: 2026-11-16
status: complete
---

# Phase 2 Plan 3: keymap.toml loading and validation Summary

**keymap.toml per-mode tables layered on the vim / vim-extras preset, with a curated snake_case action vocabulary, none unbinding, cmd:<command> bindings and load-time detection of duplicates, prefix clashes, self-lockout and text-eating sequences**

## Performance

- **Duration:** 5 min (318 s)
- **Started:** 2026-11-16T15:36:40Z
- **Completed:** 2026-11-16T15:42Z
- **Tasks:** 3 (1 tracer, 2 TDD)
- **Files modified:** 9 (2 created)

## Accomplishments

- Tracer: a `[normal]` row `";" = "enter_command"` in a real keymap.toml makes `;q<CR>` quit the running binary through Command mode (`config_keymap` pty scenario); the preset rows stay.
- `input::actions`: 31 curated names plus `none` and `cmd:<command>`; `name_of` is an exhaustive match, so internal actions (typed characters, raw keys, resize, clock, paste, loaded data, key timeout, counted, RunCommand) can never be bound by name.
- `keymap_preset = "vim-extras"` adds `<Tab>` -> focus_next and `<S-Tab>` -> focus_prev (Sidebar -> Messages -> Input -> Sidebar and back); the rendered thick border moves, asserted at column 28.
- `none` removes a preset binding and prunes empty trie branches (after unbinding gg, g is no longer a prefix); all removals apply before all binds.
- Problems reported with keymap.toml line:col and key path: bad notation, unknown action, unknown or empty `cmd:` command, wrong types, same-file duplicate chords (naming both spellings and the earlier line), prefix clashes (naming both bindings in Vim notation), lockout of Insert, Command or Normal mode, and Insert/Command sequences that start with a plain character. Dead chords (leading count digit, `<C-i>`, `<C-m>`) and unknown tables are warnings.
- 236 tests pass (206 before this plan, 30 added here), clippy and fmt clean, five pty scenarios print SMOKE OK.

## Task Commits

1. **Task 1 (tracer): keymap.toml rebinding end to end** - `5ccbabc` (feat)
2. **Task 2: vim-extras preset and cmd:<command> bindings** - `53616b8` (feat)
3. **Task 3: none unbinding, two-pass apply, clash/duplicate/lockout/sequence validation** - `eb614e1` (feat)

**Plan metadata:** committed with this summary (docs).

TDD note: the tests live in the same files as the code they exercise (Rust `#[cfg(test)]` modules) and name symbols that did not exist yet, so a RED commit that compiles on its own is not possible. Per the orchestrator's note each TDD task is one commit with tests written alongside; there are no separate RED/GREEN commits and no `check tdd-red-evidence` records. The plan frontmatter is `type: execute`, so no plan-level gate applies.

## Files Created/Modified

- `src/input/actions.rs` - ACTION_NAMES, action_from_name, exhaustive name_of
- `src/config/keymap.rs` - `build(preset, src, out)`: read rows, sort by offset, reject duplicates and text-eaters, warn dead chords, two-pass apply, report clashes, lockout guard, `cmd:` validation
- `src/input/trie.rs` - `Trie::remove` (pruning), `entries`, `clashes`
- `src/input/keymap.rs` - `vim_extras_bindings`, `Keymap::bind/unbind/clashes/bindings`
- `src/action.rs` - `FocusNext`, `FocusPrev`, `RunCommand(String)` with redacted Debug
- `src/app/update.rs` - reducer arms for the three variants, two tests
- `src/config/mod.rs` - `load_from_sources` calls `keymap::build`
- `src/input/mod.rs` - `pub mod actions`
- `scripts/smoke_pty.py` - scenario `config_keymap`

## Decisions Made

- The 31-name vocabulary is final for Phase 2; names beyond the D-07 examples were chosen by the planner and are now user-facing.
- Rows are ordered by file offset so "later" is deterministic; the toml crate's sorted iteration is never relied on.
- `Trie::clashes` is a pairwise scan of `entries()` rather than a DFS (equivalent, simpler).
- Duplicate and text-eating checks include `none` rows, as the plan text reads literally.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] config_keymap scenario looked for the COMMAND badge in raw output bytes**
- **Found during:** Task 1
- **Issue:** ratatui redraws only changed cells, so "NORMAL" to "COMMAND" is emitted as a partial update and `b"COMMAND"` never appears in the byte stream.
- **Fix:** After typing `;` the scenario waits (`read_until`) for COMMAND in the 24x80 `Screen` model, then types `q` and Enter.
- **Files modified:** scripts/smoke_pty.py
- **Verification:** `config_keymap` prints SMOKE OK
- **Committed in:** 5ccbabc

**2. [Rule 1 - Bug] Own test data tripped the new clash check**
- **Found during:** Task 3
- **Issue:** three of my tests used keys the preset already binds as prefixes (`<C-w>`, `j`, `z`), so they produced a legitimate clash error instead of the case under test.
- **Fix:** switched the test data to unbound keys (`<C-q>`, `xy`, `x`/`xx`). Production code unchanged.
- **Files modified:** src/config/keymap.rs
- **Committed in:** eb614e1

---

**Total deviations:** 2 auto-fixed (both Rule 1, both in test code)
**Impact on plan:** None. Behaviour and file list as planned.

## Issues Encountered

- The task 3 `<human-check>` (real terminal: Tab, Shift-Tab, J, K, then the `g` clash) was not run: no interactive terminal. Substitute probe: a pty run with a keymap containing `g = "goto_top"` and `jk = "cancel"` in [insert] showed the error screen listing both errors with `keymap.toml:2:1` and `keymap.toml:7:1` and exiting 1 on a key. At 80 columns the clash line is cut at `unbind one with "n` by the existing error screen's truncation, so the author should judge readability in UAT (the file position and both bindings are visible; only the tail of the hint is lost).
- Preset note (E-CONF-04): `build` receives the preset from `settings.keymap_preset`; when `config.toml` has an invalid `keymap_preset`, that field keeps its default (`vim`), so keymap.toml is still checked against `vim` in the same pass.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. T-02-10 (lockout guard, `lockout_guard`), T-02-11 (curated names only, `internal_actions_unnamed`, `cmd:` checked against COMMANDS and dispatched only through `command::execute`), T-02-12 (`RunCommand(..)` redacted, `debug_redacts`), T-02-13 (`insert_sequence_rejected`) and T-02-SC3 (Cargo.toml and Cargo.lock untouched) are implemented and covered.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- keymap.toml is parsed and validated; plan 02-04 (theme.toml) only needs to replace the remaining `check_syntax` call for theme.toml in `load_from_sources`.
- Pending for the author: the task 3 manual check (Tab/Shift-Tab/J/K feel, clash message readability).

## TDD Gate Compliance

Not satisfied by design as separate RED/GREEN commits (see TDD note); each TDD task has tests and implementation in one commit.

## Self-Check: PASSED

- Created files present: src/input/actions.rs, src/config/keymap.rs.
- Commits `5ccbabc`, `53616b8`, `eb614e1` are ancestors of HEAD; `git rev-list --count 1ed6eb9..HEAD` = 3.
- `cargo test` 236 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, pty scenarios config_keymap, quit, insert, config_error and config_layout print SMOKE OK. Acceptance greps all met; Cargo.toml and Cargo.lock untouched.

---
*Phase: 02-configuration-system*
*Completed: 2026-11-16*
