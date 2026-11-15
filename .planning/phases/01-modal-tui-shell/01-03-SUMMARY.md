---
phase: 01-modal-tui-shell
plan: 03
subsystem: input
tags: [rust, ratatui, crossterm, tokio, vim-keys, trie, matcher, key-timeout]

requires:
  - phase: 01-modal-tui-shell
    provides: "TEA core, keymap rows, sidebar cursor, run loop and ui::view (plans 01-01, 01-02)"
provides:
  - "input::trie::Trie<A> with Lookup { None, Prefix, Exact, ExactAndPrefix }"
  - "input::matcher::Matcher (feed, timeout, is_pending, display, reset), Outcome, MAX_COUNT 9999, both with redacting Debug"
  - "input::key::format_keys, the inverse of parse_keys"
  - "Keymap backed by one Trie<Action> per mode (trie, lookup -> Lookup) with the complete Normal-mode rows"
  - "Action::KeyTimeout, Action::Counted(u32, Box<Action>), the Normal-mode vocabulary (MoveLeft, MoveRight, GotoTop, GotoBottom, HalfPageDown, HalfPageUp, Select, ToggleFold, FocusLeft, FocusDown, FocusRight) and Action::is_motion"
  - "App.matcher, App::keys_pending, App::pending_display, App::open_room"
  - "Private per-pane motion router in app::update (Sidebar wired; Messages and Input arms empty for plans 01-05 and 01-06)"
  - "Run loop select! over the channel and a key deadline that emits Action::KeyTimeout (KEY_TIMEOUT 1000 ms)"
  - "Status bar: mode badge, sanitized current room (or -), pending keys, status message"
affects: [01-04, 01-05, 01-06]

actuals:
  tokens: 9500
  tasks: 3
  commits: 6
plan_head_before: 3d16f718bfbebfb20afa6e6cfda83ee8d7ec1981
plan_head_after: 740f58a4e39081e5f3a80625a556ea7e29b56e05

tech-stack:
  added: []
  patterns:
    - "Matcher takes &Trie<Action> so it is unit-testable with hand-built tries; the reducer owns dispatch of its Outcome"
    - "Time reaches the reducer only as Action::KeyTimeout from a tokio select! deadline in src/lib.rs; no clock read under src/app or src/input"
    - "Every Normal-mode motion goes through one router that dispatches on app.focus"
    - "Counted motions are one clamped jump (Some(n)); counted non-motions repeat through the reducer up to MAX_COUNT"

key-files:
  created:
    - src/input/trie.rs
    - src/input/matcher.rs
  modified:
    - src/input/key.rs
    - src/input/keymap.rs
    - src/input/mod.rs
    - src/action.rs
    - src/app/mod.rs
    - src/app/update.rs
    - src/lib.rs
    - src/ui/status.rs
    - src/ui/mod.rs

key-decisions:
  - "Keymap holds three named Trie fields (normal, insert, command) instead of a HashMap<Mode, Trie>, so trie(mode) is total and cannot panic or return Option"
  - "Esc and Ctrl-c are the only chords the matcher names itself (reset before the trie lookup, D-17); what they fire still comes from keymap rows"
  - "The status bar passes the pending display through sanitize_line as well, although it only ever shows notation of keymap prefixes"
  - "Counted(n, a) stops repeating a non-motion as soon as should_quit is set"

patterns-established:
  - "RED commits carry only the minimum scaffolding needed to compile (new variants and stub methods); tests then fail on their planned assertions"
  - "Reducer dispatch of matcher results lives in one private fn dispatch(app, outcome)"

requirements-completed: [NAV-01, NAV-02]

coverage:
  - id: D1
    description: "Counts: 5j moves the sidebar exactly like jjjjj; 0-9 extend a count and a leading 0 stays an ordinary key; counts saturate at 9999 and never panic"
    requirement: "NAV-01"
    verification:
      - kind: unit
        ref: "cargo test --lib input::matcher::tests::counts_wrap_exact input::matcher::tests::count_extends_with_zero input::matcher::tests::count_saturates app::update::tests::motion_count_5j_equals_five_j app::update::tests::motion_huge_count_clamps"
        status: pass
    human_judgment: false
  - id: D2
    description: "Multi-key sequences resolve through a per-mode trie: Prefix waits, a wrong second key drops, an ExactAndPrefix binding fires on timeout and a prefix-only or count-only state is cancelled"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib input::trie::tests input::matcher::tests::multi_key_prefix input::matcher::tests::exact_and_prefix_timeout_fires input::matcher::tests::prefix_only_timeout_cancels"
        status: pass
    human_judgment: false
  - id: D3
    description: "Esc and Ctrl-c clear pending keys and count before acting as Cancel and never quit"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib input::matcher::tests::esc_and_ctrl_c_cancel_first app::update::tests::pending_count_tracked app::update::tests::mode_ctrl_c_and_esc_in_normal_do_not_quit"
        status: pass
    human_judgment: false
  - id: D4
    description: "No Debug output of Outcome or Matcher contains a typed character"
    verification:
      - kind: unit
        ref: "cargo test --lib input::matcher::tests::debug_redacts_typed_chars"
        status: pass
    human_judgment: false
  - id: D5
    description: "Ctrl-w h/j/l set absolute pane focus; the focused pane has a thick border; open_room focuses messages"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib app::update::tests::focus_ ui::tests::focused_pane_has_thick_border"
        status: pass
    human_judgment: false
  - id: D6
    description: "Complete Normal-mode keymap as Vim-notation rows that round-trip through parse_keys and format_keys; g is a prefix and gg is GotoTop"
    requirement: "NAV-02"
    verification:
      - kind: unit
        ref: "cargo test --lib input::keymap::tests input::key::tests::format_round_trip"
        status: pass
    human_judgment: false
  - id: D7
    description: "Status bar shows mode badge, sanitized room name (or -) and pending keys"
    verification:
      - kind: unit
        ref: "cargo test --lib ui::tests::status_shows_mode_room_and_pending"
        status: pass
    human_judgment: false
  - id: D8
    description: "Run loop key deadline: seed still drawn first and :q still exits (smoke quit and panic pass); the select!-driven timeout in a real terminal"
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui quit"
        status: pass
    human_judgment: true
    rationale: "Real terminals and multiplexers may intercept Ctrl-w (A7) and the ~1 s timeout feel is judged by eye; the plan's human-check was not run by this agent"

duration: 5min
completed: 2026-11-15
status: complete
---

# Phase 1 Plan 03: Key Sequence Matcher and Pane Focus Summary

**Vim key matcher (saturating counts, per-mode trie sequences, 1 s event-loop timeout), complete Normal-mode keymap, Ctrl-w pane focus with thick border, a per-pane motion router and a status bar with room and pending keys.**

## Performance

- **Duration:** about 5 min of tool time (started 2026-11-15T15:02:18Z, finished about 15:07:10Z)
- **Tasks:** 3 (all TDD, each RED then GREEN)
- **Files modified:** 2 created, 9 modified (src), plus this SUMMARY

## Accomplishments
- `input::trie::Trie<A>` distinguishes None, Prefix, Exact and ExactAndPrefix; `input::matcher::Matcher` implements counts (saturating at `MAX_COUNT` 9999), multi-key sequences, Esc/Ctrl-c reset-first, a timeout that fires an ExactAndPrefix binding or cancels, and the Insert/Command text fallback, with `format_keys` as the inverse of `parse_keys`.
- The reducer feeds every key through `app.matcher.feed(...)` and dispatches the `Outcome` in one private `dispatch`; `Counted(n, a)` applies a counted motion once as a clamped jump and repeats other actions up to `MAX_COUNT`; every motion goes through one router on `app.focus` (Sidebar wired, Messages and Input arms empty for 01-05 and 01-06).
- The run loop `select!`s over the channel and a deadline armed only after a key leaves something pending; the deadline yields `Action::KeyTimeout`. The reducer path stays clock-free (grep gate 0) and still has exactly one `update(&mut app` call site.
- Normal keymap rows `j k h l gg G <C-d> <C-u> <C-w>h <C-w>j <C-w>l <CR> za : <Esc> <C-c>`; `<C-w>h/j/l` are absolute focus targets, `App::open_room` focuses the messages pane, and the status bar shows mode, sanitized room (or `-`) and pending keys.

## Task Commits

Each task followed RED then GREEN:

1. **Task 1: Trie, matcher, format_keys** - RED `07b1d9c` (test), GREEN `6791311` (feat)
2. **Task 2: Matcher wired into the app, router, key timeout** - RED `5f9c009` (test), GREEN `30a2ca0` (feat)
3. **Task 3: Ctrl-w focus, full Normal keymap, open_room, status bar** - RED `fe2c3e4` (test), GREEN `740f58a` (feat)

**Plan metadata:** committed with this SUMMARY (docs).

## Files Created/Modified
- `src/input/trie.rs` - `Trie<A>`, `Lookup`
- `src/input/matcher.rs` - `Matcher`, `Outcome`, `MAX_COUNT`, redacting Debug impls
- `src/input/key.rs` - `format_keys` and its round-trip test
- `src/input/keymap.rs` - three-trie `Keymap`, `trie`, `lookup -> Lookup`, full Normal rows
- `src/input/mod.rs` - declares `matcher` and `trie`
- `src/action.rs` - new variants, `is_motion`, Debug arms
- `src/app/mod.rs` - `App.matcher`, `keys_pending`, `pending_display`, `open_room`
- `src/app/update.rs` - Key / KeyTimeout / Counted / Focus* arms, `dispatch`, motion router, tests
- `src/lib.rs` - `KEY_TIMEOUT`, `sleep_until_opt`, `select!` deadline
- `src/ui/status.rs` - room and pending keys in the status bar
- `src/ui/mod.rs` - `focused_pane_has_thick_border`, `status_shows_mode_room_and_pending` tests only

## Decisions Made
- Keymap uses three named trie fields so `trie(mode)` is total.
- The matcher's only hard-coded chords are Esc and Ctrl-c (D-17), reset before lookup; Phase 2 may move them into keymap data.
- Pending text in the status bar is run through `sanitize_line` anyway (defence in depth).
- Action variant names (MoveLeft, GotoTop, FocusLeft, ToggleFold, ...) are now the keymap.toml vocabulary for Phase 2 (task 3 reversibility note); they are unchanged from the plan.

## TDD Notes
- RED commits compile with the smallest scaffolding (stub `Trie::insert/lookup`, `Matcher::feed/timeout/display`, `format_keys`, then inert reducer arms, `open_room` stub). All target tests failed on their planned assertions: 12 of 12 new tests in task 1 (the 5 older `input::` tests still passed), 5 of 5 new reducer tests in task 2, and 6 of 6 new tests in task 3 (e.g. `focus_ctrl_w_wrong_key_drops` failed on the "dropped key is not replayed" assertion because the unbound `<C-w>` let `:` through).
- `workflow.tdd_mode` is not enabled and the plan is `type: execute`, so `gsd_run check tdd-red-evidence` was not run; RED evidence is the failing `cargo test` output on the named targets, inspected by hand.

## Deviations from Plan

None - plan executed exactly as written. (Task 1's matcher tests use `Action::EnterCommand` where the plan sketches generic actions A/B, because `Action::GotoTop` does not exist until task 3.)

## Issues Encountered
- Two Bash calls were refused by the worktree command guard (one combined `git rev-parse` plus file work, one inline Python heredoc followed by `cargo test`). They were split or moved to scratchpad scripts and re-run with no effect on results.
- The plan's `<human-check>` (press `5j`, Ctrl-w then wait, Ctrl-w l/j/h in `cargo run`) was not run by this agent. Per the `end-of-phase` human-verify mode it is left for phase verification; the unit tests and pty smoke scenarios cover the automatable part.

## Known Stubs
None that block the plan goal. By design: the Messages and Input arms of the motion router are empty until plans 01-05 and 01-06, and Select, ToggleFold, MoveLeft, MoveRight, GotoTop, GotoBottom and the half-page keys are inert in the sidebar until plan 01-04.

## Threat Flags
None. No new crates (Cargo.toml untouched), no network surface; the only clock reads remain `unix_now()` and the tokio deadline in `src/lib.rs`.

## Verification Results
- `cargo test`: 63 passed (40 before this plan, 23 new)
- `cargo test --lib input::`: 17 passed; the three task-targeted test commands report 6, 8 and 17 passed
- `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean
- `python3 -I scripts/smoke_pty.py target/debug/rockettui quit`: `SMOKE OK: quit`; `... panic`: `SMOKE OK: panic`
- Grep gates: `KEY_TIMEOUT` 2, `select!` 1, `matcher.feed(` 1, one `update(&mut app` before `#[cfg(test)]` in lib.rs, clock-read gate over src/app and src/input 0, `ExactAndPrefix` in trie.rs 3, `saturating_mul` 1, `pub fn format_keys` 1, `<C-w>h` 1, `"za"` 1, `pub fn open_room` 1, `fn is_motion` 1, `pending_display` in status.rs 1

## Next Phase Readiness
Plans 01-04 (sidebar sections, Enter opens a room via `App::open_room`) and 01-05 (message-pane motions through the router's empty Messages arm) can run in parallel; plan 01-06 fills the Input arm and Insert-mode rows. Outstanding for the human at phase verification: the task 3 real-terminal check (Ctrl-w interception by multiplexers, A7, and the timeout feel).

## Self-Check: PASSED

Files `src/input/trie.rs` and `src/input/matcher.rs` exist; commits `07b1d9c`, `6791311`, `5f9c009`, `30a2ca0`, `fe2c3e4` and `740f58a` are on this branch (6 commits measured from the plan base `3d16f71`).
