---
phase: 04-send-messages
plan: 02
subsystem: app
tags: [rocket-chat, chat.sendMessage, tea-reducer, send-queue, send-state, pty-smoke, redaction]

requires:
  - phase: 04-send-messages
    provides: Effect::SendMessage, MessageSent/SendFailed, send::on_sent/on_failed, App.id_prefix and App.own_username (plan 04-01)
provides:
  - SendState { Sent, Queued { reconcile }, Sending, Failed, Unconfirmed } with is_pending and is_unsent, and the field Message.send
  - send::submit (guards, pending row with a client id, always-follow) and send::pump (one request in flight per room, Failed and Unconfirmed skipped)
  - on_sent marks the row Sent in place and pumps; on_failed marks it Failed in place and pumps
  - the SubmitInput arm calls send::submit; the Phase 1 local append is gone
  - mock POST chat.sendMessage in scripts/smoke_pty.py (records bodies, enforces the exact key set, rejects a repeated _id) and the pty scenario `send`
affects: [04-03, 04-04, 04-05, 04-06, 04-07, 04-08]

actuals:
  tokens: 11000
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "One request in flight per room: pump returns while any message of the room is Sending, else starts the first Queued one in vector order"
    - "A message's send state travels on Message; Debug prints the state and still no body or author"
    - "The encrypted-room guard sits before input.take(), so a refused draft is kept and nothing leaves the process"

key-files:
  created: []
  modified:
    - src/data/mod.rs
    - src/data/fake.rs
    - src/action.rs
    - src/effect.rs
    - src/redaction.rs
    - src/api/map.rs
    - src/api/mod.rs
    - src/app/history.rs
    - src/app/messages.rs
    - src/app/send.rs
    - src/app/update.rs
    - src/ui/mod.rs
    - src/ui/messages.rs
    - src/ui/wrap.rs
    - scripts/smoke_pty.py

key-decisions:
  - "pump finds the first Queued message in vector order and ignores Failed and Unconfirmed ones, so a failure never blocks the queue (D-16, overriding RESEARCH Pattern 3's head-of-line blocking)"
  - "on_failed only marks a message that is Sending; a failure for a Sent or unknown id changes the status line only"
  - "on_sent forces the replacing message to Sent whatever state the answer carried"

patterns-established:
  - "Pattern: send tests drive the reducer through press() and answer a send with Action::MessageSent / Action::SendFailed, asserting on app.effects"
  - "Pattern: the pty mock applies the 401 check to every method before any route"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "Insert-mode Enter shows the draft at once, POSTs it once with an rt-prefixed client _id, and the row turns into the server's copy at the same position with no history request"
    requirement: MSG-02
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send"
        status: pass
      - kind: unit
        ref: "src/app/send.rs#submit_queues_and_sends, sent_marks_in_place_and_pumps"
        status: pass
    human_judgment: false
  - id: D2
    description: "Sends are serialized per room and a failed message never holds back later queued ones"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/send.rs#queue_one_in_flight_in_order, fail_marks_failed_and_keeps_row"
        status: pass
    human_judgment: false
  - id: D3
    description: "An encrypted room refuses to send (draft kept, nothing recorded); blank drafts, no room and loading history behave as specified"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/send.rs#guards_blank_no_room_encrypted, edge_empty_and_text_as_is, send_while_history_loading"
        status: pass
    human_judgment: false
  - id: D4
    description: "No message text reaches Debug output in any of the five send states"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/data/mod.rs#debug_redacts, src/redaction.rs#sentinel_sweep"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-11-19
status: complete
plan_head_before: 8de0b65346cd5549b82e5f738d4649024cea8a08
plan_head_after: bd8363e24e31c66587c1016f44dcb58cc471c109
commits: 3
---

# Phase 4 Plan 2: Send Wiring, Queue and SendState Summary

**Insert-mode Enter now sends: a pending row with an `rt`-prefixed client id appears at once, sends are serialized one-in-flight per room by `send::pump`, and the server's answer turns the row Sent in place or Failed without ever stalling the queue; proven in the real binary against the mock.**

## Performance

- **Duration:** about 10 min
- **Completed:** 2026-11-19
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 15

## Accomplishments

- `SendState` and `Message.send` exist; every server message is `Sent` (`map::message`), and the Debug of a message adds the send state while still printing no body or author.
- `send::submit` builds the pending row (id = `id_prefix` + `local_seq`, author = own username or `me`, ts from the clock, state `Queued`), keeps the view following the newest row, and calls `pump`. The Phase 1 local append in `update.rs` is deleted; the `SubmitInput` arm calls `send::submit`.
- `send::pump` keeps one request in flight per room and skips Failed and Unconfirmed messages (D-16); `on_sent` swaps the row in place marked Sent and pumps; `on_failed` marks the Sending row Failed in place, sets the fixed `message not sent: <reason>` status and pumps.
- An encrypted room shows `encrypted, not supported`, keeps the draft and sends nothing; the guard is before `input.take()` (D-11, T-04-03).
- The pty mock serves POST `chat.sendMessage` (records bodies, requires exactly `{message: {_id, rid, msg}}`, rejects a repeated `_id`, answers with the stored message authored `smoke`) and the new `send` scenario passes in the real binary: one authed POST, `_id` starts with `rt` and has at least 35 chars, the row turns into the `smoke` copy.

## Task Commits

1. **Task 1 (tracer): Enter -> submit -> SendState -> pump -> effect -> POST -> Sent in place** - `633b061` (feat)
2. **Task 2 (TDD): guards and queue rules**
   - RED `a169482` (test)
   - GREEN `bd8363e` (feat)

**Plan metadata:** the docs commit that holds this file.

Tracer feedback gate: `workflow.auto_advance` is off and `human_verify_mode` is `end-of-phase`; the tracer's `<verify>` is automated only, so it was re-run end to end (cargo test, clippy, fmt, the six pty scenarios) and passed. "Tracer verified end-to-end - expanding".

## TDD notes (Task 2)

- **RED (`a169482`):** `guards_blank_no_room_encrypted` failed with `expected the encrypted-room error` (the encrypted room still took the draft), `queue_one_in_flight_in_order` failed with `left: [Sent, Sending, Queued], right: [Sent, Failed, Sending]` and `fail_marks_failed_and_keeps_row` with `left: Sending, right: Failed`. Each is the planned assertion on a compiling build. `edge_empty_and_text_as_is` and `send_while_history_loading` passed already: they pin behavior Task 1's `submit` has (byte-for-byte text, no history guard), so they are regression pins, not new behavior. Cargo's console report is not one of the formats `gsd_run check tdd-red-evidence` reads, and `workflow.tdd_mode` is off, so the classifier was not run; the semantic assessment is that the target tests executed and failed on the intended assertions.
- **GREEN (`bd8363e`):** the encrypted guard in `submit`, and `on_failed` marking Failed and pumping. 10 `app::send` tests and the full suite pass.
- **REFACTOR:** none needed.

## Verification

- `cargo test`: 383 passed; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean; `find src -name '*.snap.new'` prints nothing (no Phase 1-3 snapshot changed).
- Task 1 target set (`app::send`, `app::update::tests::submit`, `api::map::tests::message`, `data::tests`, `redaction`): 15 passed at Task 1; `cargo test --lib -- app::send` reads 10 passed at Task 2.
- pty scenarios `send`, `insert`, `config_layout`, `connect_read`, `unauthorized`, `quit`: six `SMOKE OK`.

## Files Created/Modified

- `src/data/mod.rs` - `SendState`, `is_pending`, `is_unsent`, `Message.send`, Debug with the state
- `src/app/send.rs` - `submit`, `pump`, `on_sent` (Sent in place + pump), `on_failed` (Failed + pump), ten tests
- `src/app/update.rs` - the SubmitInput arm; `submit_input` removed; `submit_appends_pending_and_sends`
- `src/redaction.rs` - sentinel sweep over a message in each of the five SendState values
- `src/api/map.rs` - `map::message` marks Sent; `message_single` asserts it
- `src/action.rs` - SubmitInput doc comment
- `src/data/fake.rs`, `src/api/mod.rs`, `src/app/history.rs`, `src/app/messages.rs`, `src/ui/mod.rs`, `src/ui/messages.rs`, `src/ui/wrap.rs` - `send: SendState::Sent` on every `Message` literal
- `src/effect.rs` - one test literal (see deviations)
- `scripts/smoke_pty.py` - `MOCK_USERNAME`, 401 before any route for every method, `bodies`, `stored`, POST `chat.sendMessage`, `scenario_send`

## Decisions Made

- `pump` scans in vector order and returns while any message of the room is `Sending`; Failed and Unconfirmed rows are never picked, which is what lets a failure leave the queue moving (D-16).
- `on_failed` marks only a `Sending` message; a failure for a row that is not Sending (or not held) changes the status line only, so a stale result can never regress a Sent row.
- Task 1 left `on_failed` as status-only, as the plan splits it, so between the two commits a failed send would leave a Sending row; Task 2 closes that.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] A Message literal the plan's sweep list missed**
- **Found during:** Task 1
- **Issue:** `src/effect.rs` (the `send_round_trip_swaps_in_place` test, added by plan 04-01) builds a `Message { .. }`; the plan's A-SWEEP list named 18 sites and not this one, so the test build failed on the new field (the plan flags this as A-SWEEP: `cargo build` names the missed literal).
- **Fix:** added `send: SendState::Sending` there (the row is in flight when the answer arrives); `src/effect.rs` is a 15th file in the Task 1 commit.
- **Files modified:** `src/effect.rs`
- **Commit:** `633b061`

**2. [Rule 1 - Bug] The 04-01 round-trip test asserted the old failure behavior**
- **Found during:** Task 2 (after GREEN)
- **Issue:** `effect::tests::send_round_trip_swaps_in_place` expected the messages unchanged after a 401, which was true while `on_failed` only set the status; D-06 and D-16 now mark the row Failed.
- **Fix:** the 401 branch now expects the same list with the row's `send` set to `Failed` (still in place, author `me`, fixed status, no body in the status).
- **Files modified:** `src/effect.rs`
- **Commit:** `bd8363e`

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug). **Impact:** none on scope; both are consequences the plan anticipated or implied.

## Issues Encountered

None. The first helper script that wrapped `Message { .. }` literals also matched `-> Message {` function signatures; it was reverted and rerun with a stricter pattern before anything was committed.

## Known Stubs

None. `Effect::SendMessage.reconcile` is still carried through unchanged and the run loop still always POSTs; plan 04-03 gives it the `chat.getMessage`-first branch. `App.own_username` stays `None` until plan 04-06, so a pending row is authored `me` until the server's copy arrives (D-17 fallback, documented). `SendState::Unconfirmed` exists and is skipped by `pump`, but nothing sets it until plan 04-03 (D-07, D-18).

## Threat Flags

None beyond the plan's register. T-04-03 (encrypted guard before `take()`) is covered by `guards_blank_no_room_encrypted`; T-04-06 (room captured at Enter, one in flight, order) by `queue_one_in_flight_in_order` and `sent_marks_in_place_and_pumps`; T-04-24 (Debug in every state) by `data::tests::debug_redacts` and `redaction::sentinel_sweep`. T-04-SC-02 holds: `Cargo.toml` is unchanged. The mock POST route lives only in the test harness on a loopback ephemeral port.

## Next Phase Readiness

Ready for plan 04-03: `SendState::Queued { reconcile }`, `Unconfirmed` and `Effect::SendMessage.reconcile` exist and are carried; `on_failed` is the place that splits Unconfirmed from Failed.

## Self-Check: PASSED

All fifteen modified files exist and `git status` is clean; the commits `633b061`, `a169482` and `bd8363e` are ancestors of HEAD. `cargo test` (383 passed), clippy and fmt pass; six pty scenarios print `SMOKE OK`.
