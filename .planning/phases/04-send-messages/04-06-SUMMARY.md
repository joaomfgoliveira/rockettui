---
phase: 04-send-messages
plan: 06
subsystem: api
tags: [rocket-chat, me-endpoint, own-username, tea-reducer, redaction, pty-smoke]

requires:
  - phase: 04-send-messages
    provides: App.own_username slot and LOCAL_AUTHOR fallback (04-01), send::submit authoring pending rows (04-02), the send pty scenarios (04-02, 04-03)
provides:
  - Client::me (GET /api/v1/me, username only) and wire::MeResp
  - Effect::LoadMe, pushed once by run() and once by :reconnect, never per send
  - Action::MeLoaded(String) with a Debug that prints `MeLoaded(..)`
  - own_username_loaded: sets App.own_username and renames every unsent LOCAL_AUTHOR row in every room
  - mock GET /api/v1/me route and the send_author pty scenario
affects: [04-07, 04-08]

actuals:
  tokens: 14000
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Best-effort load: an effect that reports nothing on any failure, so the previous or fallback value stays"
    - "The own username lives only in App memory; rows authored before it arrives are renamed in one reducer pass"

key-files:
  created: []
  modified:
    - src/api/wire.rs
    - src/api/mod.rs
    - src/effect.rs
    - src/action.rs
    - src/input/actions.rs
    - src/app/update.rs
    - src/app/command.rs
    - src/lib.rs
    - src/redaction.rs
    - scripts/smoke_pty.py

key-decisions:
  - "Effect::LoadMe is a separate best-effort load, not folded into load_rooms; Ok(None), any Err and a missing client all return no actions"
  - "A failed /me on :reconnect keeps the username already known (A-KEEP): no action is produced, so own_username is never cleared"
  - "The rename touches only rows that are unsent AND authored LOCAL_AUTHOR, so Sent rows and rows by other authors are never changed, and a later different name does not rewrite rows that already carry the first one"

patterns-established:
  - "Pattern: a wiremock-backed effect test with a local serve(status, body) helper"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "The own username is learned once with GET /api/v1/me at startup and every pending row is authored by it, in the real binary"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/api/mod.rs#me_username, src/effect.rs#load_me_actions, src/app/update.rs#me_loaded_sets_author"
        status: pass
      - kind: other
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send_author"
        status: pass
    human_judgment: false
  - id: D2
    description: "A username that arrives after the first send renames the earlier unsent rows in every room and leaves Sent rows and other authors alone"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/update.rs#me_loaded_renames_pending, me_loaded_keeps_other_authors"
        status: pass
    human_judgment: false
  - id: D3
    description: ":reconnect re-runs /me after LoadRooms and never changes a send state or sends anything"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/command.rs#reconnect_loads_me, reconnect_leaves_sends"
        status: pass
      - kind: other
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send_author (two /me, one POST in all)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Neither the username nor any send type reaches a Debug or Display output; /me is never fetched per send"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/action.rs#debug_redacts, src/redaction.rs#sentinel_sweep"
        status: pass
      - kind: other
        ref: "grep -c LoadMe src/app/send.rs == 0"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-11-19
status: complete
plan_head_before: ea5caf2cc9c1ad3b9af36184ee93b0080bfdc2f4
plan_head_after: f8572ba9947cf10bc1a79e4d62bb329613436e47
commits: 3
---

# Phase 4 Plan 6: Own Username from /me Summary

**The user's own username is fetched once per connection with GET /api/v1/me (startup and `:reconnect`), kept in memory, used as the author of every pending row and applied retroactively to rows composed before it arrived.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-11-19T19:33:00Z
- **Completed:** 2026-11-19T19:45:00Z
- **Tasks:** 2 (one tracer, one TDD)
- **Files modified:** 10

## Accomplishments

- Tracer: `run()` pushes `Effect::LoadMe` next to `LoadRooms`; `effect::run` maps it to `Client::me()` and returns `[MeLoaded(name)]`, or nothing for a 401, an empty or missing username, an undecodable body and a missing client. In the real binary a Failed row the server never echoed is drawn under `smoke`, with exactly one /me request since launch.
- `own_username_loaded` sets `App.own_username` and renames every unsent row authored `me` in every room (Sending, Queued, Failed, Unconfirmed); a Sent row and a row by another author are left alone.
- `:reconnect` pushes `LoadMe` right after `LoadRooms` and nothing else changes: the pty scenario counts two /me and still one chat.sendMessage POST after it, and the Failed row keeps its place and author.
- `MeLoaded`'s Debug prints `MeLoaded(..)`; `debug_redacts` and `sentinel_sweep` prove the username (and `LoadMe`) never reach a Debug output. The username reaches the screen only through the author cells that `wrap::render_rows` already passes through `sanitize_line` (T-04-19).

## Task Commits

1. **Task 1 (tracer): startup LoadMe, GET /api/v1/me, MeLoaded, own_username, mock route and send_author scenario** - `08c441a` (feat)
2. **Task 2 (TDD): late rename, :reconnect re-runs /me, redaction sweep**
   - RED `021c065` (test)
   - GREEN `f8572ba` (feat)

**Plan metadata:** the docs commit that holds this file.

## TDD notes (Task 2)

- **RED:** `cargo test --lib -- app::update::tests::me app::command redaction` ran 18 tests, 7 failed on their planned assertions: `me_loaded_renames_pending` (`["me","me","me","me"]` against `["smoke","smoke","smoke","me"]`), `me_loaded_keeps_other_authors` (`["alice","me"]` against `["alice","smoke"]`), `reconnect_loads_me` and the four existing `:reconnect` effect tests, updated to expect `[LoadRooms, LoadMe, ..]` (`[LoadRooms]` against `[LoadRooms, LoadMe]`). `reconnect_leaves_sends`, `me_loaded_sets_author` and the extended `sentinel_sweep` passed already: they pin behavior that holds today (D-08, Task 1, the redacting Debug) and are regression guards, not new behavior. Semantic assessment: every target test executed and failed for the intended reason (no rename, no LoadMe); no compile, import or fixture fault. Cargo's console report is not one of the formats `gsd_run check tdd-red-evidence` reads, so it was not run through the classifier (`workflow.tdd_mode` is off and this is not a `type: tdd` plan).
- **GREEN:** `own_username_loaded` renames unsent `LOCAL_AUTHOR` rows; `cmd_reconnect` pushes `LoadMe`. All 420 tests pass.
- **REFACTOR:** none needed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Existing `:reconnect` tests asserted the old effect list**
- **Found during:** Task 2 (RED)
- **Issue:** `reconnect_reloads`, `retry_alias`, `reconnect_retries_failed_history` and `reconnect_leaves_loading_and_loaded_rooms_alone` asserted `app.effects == [LoadRooms ..]`; pushing `LoadMe` after `LoadRooms` is the planned behavior and they would fail by design.
- **Fix:** updated their expectations to `[LoadRooms, LoadMe ..]` in the RED commit, so they fail first and pass with the GREEN change.
- **Files modified:** src/app/command.rs
- **Commit:** 021c065

### Acceptance criterion read as production code

`grep -c "Effect::LoadMe" src/app/command.rs == 1` counts 6 because the required `:reconnect` tests name `Effect::LoadMe`. The production code in `cmd_reconnect` holds exactly one push (the `sed -n '/fn cmd_reconnect/,/^}/p'` range holds one `Effect::LoadMe` and no `send::` or `SendState`). Every other criterion holds as written.

### Process note

The plan-head ledger file (protocol 0c) could not be written because the sandbox refused the compound command; `plan_head_before` is the dispatch base `ea5caf2cc9c1ad3b9af36184ee93b0080bfdc2f4` (the SHA the worktree HEAD assertion checked), and `commits: 3` is `git rev-list --count` over that base.

**Total deviations:** 1 auto-fixed (Rule 3), 0 behavioral. **Impact:** none on behavior or scope.

## Issues Encountered

None.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-04-19 (hostile username drawn only through `sanitize_line` in `wrap::render_rows`), T-04-20 (redacting Debug, no persistence, sentinel sweep) and T-04-21 (`LoadMe` only in `run()` and `cmd_reconnect`; `grep -c LoadMe src/app/send.rs` is 0; the smoke counts one /me before `:reconnect` and two after) are mitigated. T-04-SC-06 holds (Cargo.toml unchanged).

## Next Phase Readiness

Ready for plan 04-07: the `send_author` scenario still ends with a plain `:q`; D-20 later changes that final quit to `:q!` because a plain `:q` will then warn while an unsent row is on screen. Plan 04-08 and the realtime phase can rely on `App.own_username` being set once per connection.

## Self-Check: PASSED

All ten modified source files exist and the three commits (`08c441a`, `021c065`, `f8572ba`) are ancestors of HEAD. `cargo test` (420 passed), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and the pty scenarios `send_author`, `send`, `send_retry`, `connect_read`, `missing_creds` and `unauthorized` (all `SMOKE OK`) pass.
