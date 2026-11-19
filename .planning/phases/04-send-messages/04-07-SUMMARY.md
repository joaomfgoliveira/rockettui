---
phase: 04-send-messages
plan: 07
subsystem: app
tags: [quit-guard, undelivered-messages, command-line, tea-reducer, pty-smoke]

requires:
  - phase: 04-send-messages
    provides: SendState::is_unsent (04-02), failed and unconfirmed rows (04-03), the send pty scenarios and the mock's fail_next (04-02, 04-03, 04-06)
provides:
  - App.quit_warned and App::undelivered_count (every non-Sent message of every room)
  - Guarded cmd_quit (warns once with a count-only status line) and cmd_force_quit for `q!` and `quit!`
  - The disarm rule in command::execute (any other non-empty command line clears the warning)
  - smoke helper wait_confirmed and scenario quit_undelivered; send_author quits with `:q!`
affects: [04-08]

status: complete
plan_head_before: 603becfe5e9aa13304056ff1024120488d547d53
plan_head_after: 04e59f3e2c3dd609bfa3ed3421d1107a58a25ab6
commits: 4

actuals:
  tokens: 22000
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "A one-shot warning arm on App that the command dispatcher clears for every command but the guarded one"
    - "Warning text built from a count alone, so it cannot hold message text, an author or a room name"

key-files:
  created: []
  modified:
    - src/app/mod.rs
    - src/app/command.rs
    - scripts/smoke_pty.py

key-decisions:
  - "Action::Quit (the keymap action and the end-of-input signal of src/event.rs) stays unconditional; only the `:q`/`:quit` commands are guarded (A-QUITKEY)"
  - "Only `:q` and `:quit` keep the arm; every other non-empty line, an unknown one included, disarms it; an empty line leaves it as it is (A-ARM)"

patterns-established:
  - "Pattern: pty scenarios that send wait for the server's copy (its time instead of a marker) before `:q`, so the quit is deterministic"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "`:q` with an undelivered message warns with a count only and keeps running; a second `:q` or `:q!` quits (D-20), in the real binary"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/command.rs#quit_warns_then_quits, src/app/command.rs#force_quit_skips_warning, src/app/mod.rs#undelivered_count_states"
        status: pass
      - kind: e2e
        ref: "scripts/smoke_pty.py quit_undelivered"
        status: pass
  - id: D2
    description: "The warning counts every state and room, never holds text, is disarmed by other commands and leaves sends untouched (D-14, D-20)"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/command.rs#warning_counts_states_and_rooms, #other_command_disarms, #warning_holds_no_text, #quit_warning_leaves_sends, #empty_line_keeps_warning_armed"
        status: pass
  - id: D3
    description: "Every pty scenario keeps a deterministic exit now that `:q` can refuse"
    requirement: MSG-02
    verification:
      - kind: e2e
        ref: "scripts/smoke_pty.py quit, panic, insert, send, send_retry, send_author, send_unconfirmed, config_layout, config_keymap, config_error, connect_read, missing_creds, config_creds, perm_warning, unauthorized"
        status: pass
---

# Phase 4 Plan 07: Quit warning for undelivered messages Summary

**`:q` now refuses once while any message is Queued, Sending, Failed or Unconfirmed, showing a count-only status line; a repeated `:q` or `:q!` quits (D-20).**

## What was built

- `App::undelivered_count` counts messages whose `send.is_unsent()` is true over every room. `App.quit_warned` records that `:q` warned.
- `cmd_quit` quits at once when the count is 0 or the arm is set; otherwise it sets the arm and returns `1 message not delivered: :q again or :q! to quit` (or `<n> messages ...`) as a status error. The text comes from the count alone (D-14).
- `cmd_force_quit` backs the new `q!` and `quit!` rows and quits unconditionally.
- `execute` clears the arm for every non-empty command line whose name is not `q` or `quit`.
- smoke_pty.py: `wait_confirmed` waits for the server's copy (its `HH:MM` instead of a marker; two-line mode checks the header row above). `scenario_insert`, `scenario_config_layout`, `scenario_send`, `scenario_send_retry` and `scenario_send_unconfirmed` call it before their final `:q`; `scenario_send_author` ends its intentional Failed row with `:q!`. New `quit_undelivered` scenario runs the warn-then-quit path and the `:q!` path against a Failed row.

## TDD record

Two RED/GREEN cycles, Rust `cargo test`. The classifier `gsd_run check tdd-red-evidence` has no cargo adapter, so RED was assessed semantically from the unmodified `cargo test` output.

- Task 1 RED (`45d6643`): `quit_warns_then_quits` (assertion `!app.should_quit`), `force_quit_skips_warning` (assertion `app.should_quit` on `:q!`, an unknown command at that point) and `undelivered_count_states` (left 0, right 4) each failed on their planned assertion. To compile, the RED commit carries the `quit_warned` field and an `undelivered_count` stub returning 0. semanticAssessment: the target tests executed and failed for the intended reason, not for build or fixture faults. GREEN: `6797a8e`.
- Task 2 RED (`60657f3`): `other_command_disarms` failed at its first post-`:foo` assertion; the other four new tests pass at once because Task 1 already implemented those rules, and they pin them. semanticAssessment: the target failed on the planned disarm assertion. GREEN: `04e59f3`.
- No REFACTOR commit.

## Commits

- `45d6643` test(04-07): add failing tests for the undelivered-quit warning
- `6797a8e` feat(04-07): warn on :q while messages are undelivered, add :q! (D-20)
- `60657f3` test(04-07): add failing test for disarming the quit warning
- `04e59f3` feat(04-07): disarm the quit warning on any other command (D-20)
- the docs commit of this SUMMARY

## Verification

- `cargo test`: 450 passed; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.
- All 16 non-docker pty scenarios print SMOKE OK (quit, panic, insert, send, send_retry, send_author, send_unconfirmed, config_layout, config_keymap, config_error, connect_read, missing_creds, config_creds, perm_warning, unauthorized, quit_undelivered).
- Plan grep checks: `"q!"` 1, `fn cmd_force_quit` 1, `pub quit_warned` 1, `pub fn undelivered_count` 1, privacy guard grep over `cmd_quit` 0, `quit_warned = false` 1.

## Deviations from Plan

None - plan executed exactly as written. Two small additions beyond the listed tests: `empty_line_keeps_warning_armed` (pins the plan's "an empty line leaves the arm as it is") and the `1 message` / `n messages` singular-plural wording.

## Known Stubs

None. The `undelivered_count` stub existed only inside the RED commit and is replaced in GREEN.

## Threat Flags

None. The only new surface is a status line built from a count (T-04-25 mitigated by `warning_holds_no_text`, the cmd_quit guard grep and the smoke check that the status row never holds the typed text; T-04-26 mitigated by the warn-once rule).

## Self-Check

PASSED: SUMMARY, source and script files exist; commits 45d6643, 6797a8e, 60657f3, 04e59f3 are ancestors of HEAD.
