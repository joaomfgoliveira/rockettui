---
phase: 04-send-messages
plan: 03
subsystem: app
tags: [rocket-chat, chat.getMessage, send-recovery, retry, discard, keymap-vocabulary, pty-smoke, redaction]

requires:
  - phase: 04-send-messages
    provides: SendState, send::pump/on_failed/on_sent, Effect::SendMessage with a carried reconcile flag, mock POST chat.sendMessage (plans 04-01 and 04-02)
provides:
  - Action::RetrySend and Action::DiscardSend, keymap names retry_send and discard_send, default Normal rows r and dd
  - send::retry (requeue under the same client id, reconcile when Unconfirmed) and send::discard (remove, keep the stored cursor on the same message)
  - ApiError::Rejected and ApiError::ConnectionLost, ApiError::outcome_unknown, read_json(request, client_error)
  - Client::get_message (GET chat.getMessage) and the reconciling branch of Effect::SendMessage
  - the Failed / Unconfirmed split in on_failed and send_error_text(error, unconfirmed)
  - mock fail_next (status or store_then_500), GET chat.getMessage route, pty scenarios send_retry and send_unconfirmed
affects: [04-04, 04-05, 04-06, 04-07, 04-08]

actuals:
  tokens: 12000
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "retry and discard share one guard, unsent_under_cursor: messages pane, open room, cursor message Failed or Unconfirmed; anything else is a silent no-op, and every counted repeat re-reads the cursor message"
    - "read_json takes the ApiError a 4xx becomes, so the GET path keeps BadResponse and the send path reports Rejected without a second status table"
    - "A reconciling send asks chat.getMessage before the POST and once more after a failing POST; any non-200 means not stored"
    - "A scenario reads the status line before sending ESC, because any key press clears it"

key-files:
  created: []
  modified:
    - src/action.rs
    - src/input/actions.rs
    - src/input/keymap.rs
    - src/app/send.rs
    - src/app/update.rs
    - src/api/mod.rs
    - src/effect.rs
    - src/redaction.rs
    - scripts/smoke_pty.py

key-decisions:
  - "A 5xx is Unconfirmed (D-18): outcome_unknown is true for Timeout, Server(_), ConnectionLost, BadResponse and TooLarge; every other failure is Failed"
  - "read_json maps a status_error of BadResponse to the caller's client_error; a 200 with an undecodable body stays BadResponse on the send path"
  - "send_error's fallback branch is ConnectionLost (transport error after connecting); is_connect stays Unreachable, so TLS and refused connections remain Failed"
  - "discard removes through a private remove_at(room, index) that moves a stored cursor below the removed index up by one; the cursor guard means discard itself only ever hits the cursor index, so that branch is tested through remove_at"
  - "test helper answer_failed now fails with Forbidden (a definite failure), because Timeout became Unconfirmed"

patterns-established:
  - "Pattern: wiremock effect tests assert the exact (method, path) request sequence with received_requests, using up_to_n_times to script a first and a second answer for the same route"
  - "Pattern: connection loss is produced in a unit test by a one-shot TcpListener that reads the request and drops the stream"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "r on a Failed message re-sends it under the same client _id and swaps it in place; dd discards a Failed or Unconfirmed message; both are silent no-ops elsewhere"
    requirement: MSG-02
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send_retry"
        status: pass
      - kind: unit
        ref: "src/app/send.rs#retry_failed_requeues_same_id, retry_discard_noop_elsewhere, discard_removes_and_fixes_cursor, discard_counted, discard_unconfirmed, retry_waits_behind_a_message_in_flight"
        status: pass
    human_judgment: false
  - id: D2
    description: "retry_send and discard_send are keymap vocabulary bound to r and dd; d alone is a pure prefix; the Insert newline keys stay A-CR and C-j"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/input/actions.rs#names_round_trip, src/input/keymap.rs#keymap_d_is_prefix, vim_extras_adds_only_tab_rows"
        status: pass
    human_judgment: false
  - id: D3
    description: "Failures split into Failed and Unconfirmed (5xx included, D-18) with fixed status text for each"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/send.rs#fail_classification, fail_text_fixed, src/api/mod.rs#outcome_unknown_table, send_rejected, connection_reset_is_connection_lost"
        status: pass
    human_judgment: false
  - id: D4
    description: "Retrying an Unconfirmed message asks chat.getMessage first and never posts a delivered message twice"
    requirement: MSG-02
    verification:
      - kind: integration
        ref: "src/effect.rs#reconcile_found_skips_post, reconcile_absent_posts, reconcile_post_error_rechecks, no_reconcile_never_gets"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send_unconfirmed"
        status: pass
    human_judgment: false
  - id: D5
    description: "Nothing resends an unsent message on its own (timer, :reconnect, next message, another result)"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/send.rs#unsent_never_resent_on_its_own"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-11-19
status: complete
plan_head_before: dc160b511bcbf2a1a4630079a72ef8ec5a030094
plan_head_after: 4561c0dd44898c5892eb50c94fafd1fcd7dc1e12
commits: 3
---

# Phase 4 Plan 3: Send Recovery, Unconfirmed Outcome and Reconcile Summary

**A failed or unconfirmed message is recovered from the keyboard (`r` retries under the same client id, `dd` discards), a 5xx or lost response is Unconfirmed, and a retried Unconfirmed message asks chat.getMessage first, so a lost answer never becomes a duplicate.**

## Performance

- **Duration:** about 8 min
- **Started:** 2026-11-19T19:28:08Z
- **Completed:** 2026-11-19T19:36:30Z
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 9

## Accomplishments

- `Action::RetrySend` and `Action::DiscardSend` exist as keymap vocabulary (`retry_send`, `discard_send`), bound by default to Normal `r` and `dd` (`d` is a pure prefix), rebindable in keymap.toml. The Insert newline rows now carry a comment next to the Enter row documenting `<A-CR>` and `<C-j>` (D-09).
- `send::retry` puts the Failed or Unconfirmed message under the messages-pane cursor back in its room's queue under the same `_id`, at the same position, with `reconcile` set when it was Unconfirmed; `send::discard` removes it and keeps a stored cursor on the same message. Both share one guard and are silent no-ops on a Sent, Queued or Sending message, in the sidebar or input pane, and with no room or an empty room; a count repeats them and each repetition re-reads the cursor message.
- Failures are classified by `ApiError::outcome_unknown`: Timeout, any 5xx (D-18), ConnectionLost, BadResponse and TooLarge make an Unconfirmed row (`message may not have been sent: <reason>`); Unauthorized, Forbidden, NotFound, RateLimited, Redirected, Rejected, Unreachable and Client make a Failed row (`message not sent: <reason>`). Every reason is a fixed string with no hint at `:reconnect`.
- `ApiError::Rejected` carries a send-path 4xx other than 401/403/404/429 (D-12: too long, read-only, muted); a GET 400 is still `BadResponse` (the `status_mapping` and `error_mapping` tests are unchanged). `ApiError::ConnectionLost` is a transport error after connecting, on both paths; a connect failure stays `Unreachable`.
- `Client::get_message` (GET `chat.getMessage?msgId=`) and the reconciling branch of `Effect::SendMessage`: a 200 on the first ask is the answer and no POST is sent; after a failing POST the server is asked once more and a 200 still counts as sent.
- The pty mock gained `fail_next` (a status, or `store_then_500`) and a `chat.getMessage` route. `send_retry` proves one failed POST, `r`, a second POST with the same `_id` answered 200 (row becomes `smoke`), then `dd` on a failed `drop me` (three POSTs in all). `send_unconfirmed` proves a stored-then-500 answer, `r`, exactly one POST and one GET for that `_id`, and the row turning into the server's copy.

## Task Commits

1. **Task 1 (tracer): `r` -> RetrySend -> send::retry -> pump -> POST with the same _id -> swapped in place; `dd` discards** - `aef5eb0` (feat)
2. **Task 2 (TDD): unconfirmed outcome and reconcile-before-resend**
   - RED `c3498ed` (test)
   - GREEN `4561c0d` (feat)

**Plan metadata:** the docs commit that holds this file.

Tracer feedback gate: `workflow.auto_advance` is off and `human_verify_mode` is `end-of-phase`; the tracer's `<verify>` is automated only, so it was re-run end to end (`cargo test` 389 passed, clippy, fmt, pty scenarios `send_retry`, `send`, `insert`, `config_keymap`) and passed. "Tracer verified end-to-end - expanding".

## TDD notes (Task 2)

- **RED (`c3498ed`):** the new variants and compiling stubs (`outcome_unknown` returning false, `get_message` returning `BadResponse`, `send_error_text` ignoring its new flag) made the build compile; ten tests then failed on their planned assertions: `outcome_unknown_table` (Timeout), `send_timeout_is_unconfirmed` (`outcome_unknown` false), `send_rejected` (400 gave `BadResponse`, wanted `Rejected`), `connection_reset_is_connection_lost` (`Unreachable`), `get_message_found_and_absent` (`found: BadResponse`), `reconcile_found_skips_post`, `reconcile_absent_posts` and `reconcile_post_error_rechecks` (request sequence had no GET), `fail_classification` (Timeout left `Failed`) and `fail_text_fixed` (no unconfirmed prefix). Four tests passed already as regression pins of behavior that existed (`send_undecodable_answer_is_bad_response`, `no_reconcile_never_gets`, `unsent_never_resent_on_its_own`, `discard_unconfirmed`). Cargo's console report is not one of the formats `gsd_run check tdd-red-evidence` reads and `workflow.tdd_mode` is off, so the classifier was not run; the semantic assessment is that each target test executed and failed on the planned assertion on a compiling build.
- **GREEN (`4561c0d`):** `outcome_unknown`, `read_json`'s `client_error` parameter, `send_error`'s ConnectionLost fallback, `get_message`, the two-sided reconcile in `effect::run`, the Failed/Unconfirmed split in `on_failed`. 402 tests pass; the `api::tests effect::tests app::send redaction` target set reads 63 passed.
- **REFACTOR:** none needed.

## Verification

- `cargo test`: 402 passed; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean; `find src -name '*.snap.new'` prints nothing.
- Task 1 target set (`app::send::tests::retry`, `app::send::tests::discard`, `input::actions`, `input::keymap`, `config::keymap`): 33 passed. Task 2 target set: 63 passed.
- pty scenarios `send_retry`, `send_unconfirmed`, `send`, `insert`, `config_keymap`, `connect_read`, `unauthorized`: all `SMOKE OK`.
- Acceptance greps: `"retry_send"` 3, `"discard_send"` 3, `Action::RetrySend` in keymap.rs 2, `"dd", Action::DiscardSend` 1, `pub fn retry` 1, `pub fn discard` 1, `Rejected` in api/mod.rs 9, `ConnectionLost` 8, `pub fn outcome_unknown` 1, `chat.getMessage` 4, `ApiError; 13` 1, production `:reconnect` in send.rs 0.

## Files Created/Modified

- `src/action.rs` - `RetrySend`, `DiscardSend` and their Debug arms
- `src/input/actions.rs` - names `retry_send`, `discard_send`, module doc
- `src/input/keymap.rs` - default rows `r` and `dd`, Insert newline comment, test `keymap_d_is_prefix`
- `src/app/send.rs` - `retry`, `discard`, `remove_at`, `unsent_under_cursor`, the Failed/Unconfirmed `on_failed`, `send_error_text(error, unconfirmed)`, nine new tests
- `src/app/update.rs` - the two reducer arms
- `src/api/mod.rs` - `Rejected`, `ConnectionLost`, `outcome_unknown`, `read_json(request, client_error)`, `get_message`, seven new tests
- `src/effect.rs` - the reconciling `SendMessage` branch, doc comment, four tests
- `src/redaction.rs` - `ALL_ERRORS` is `[ApiError; 13]`
- `scripts/smoke_pty.py` - `fail_next`, `get_message_reply`, the GET route, `scenario_send_retry`, `scenario_send_unconfirmed`

## Decisions Made

- A 5xx is Unconfirmed (D-18) and any other 4xx is Failed; `outcome_unknown` is the one rule for both, and the status prefix follows it.
- `read_json` replaces only a `BadResponse` that came from a 4xx status, so a 200 with an undecodable body stays `BadResponse` on the send path (a test pins it).
- `discard` can only ever hit the cursor index, so the "stored cursor above the removed message moves up" rule lives in a private `remove_at` and is tested there directly; the plan's behavior text lists that case under `dd`, and it is covered, not skipped.
- The first reconcile check treats every error (not only 400) as "not stored" and proceeds to the POST, as the plan states.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The scenario reads the status line before ESC, not after**
- **Found during:** Task 1 (writing `scenario_send_retry`), applied again in `scenario_send_unconfirmed`
- **Issue:** the plan orders "lone ESC with the 0.3 s pause; read_until the status row shows `message not sent`". Any key press clears the status (`update.rs` sets `status_msg = None` on every `Action::Key`), so if the mock's answer arrives before the ESC is read, the status is cleared and the wait would hang.
- **Fix:** the scenarios wait for the status text first, then send ESC.
- **Files modified:** `scripts/smoke_pty.py`
- **Commit:** `aef5eb0`, `4561c0d`

**2. [Rule 1 - Bug] Existing test helper failed with an error that became Unconfirmed**
- **Found during:** Task 1 (planning Task 2)
- **Issue:** `answer_failed` in `app::send::tests` failed a send with `ApiError::Timeout`; once Timeout is Unconfirmed, `queue_one_in_flight_in_order` and `fail_marks_failed_and_keeps_row` (which expect `Failed`) would break.
- **Fix:** the helper fails with `ApiError::Forbidden`, a definite failure, from Task 1 on.
- **Files modified:** `src/app/send.rs`
- **Commit:** `aef5eb0`

**3. [Rule 1 - Bug] `fail_status_has_no_body` asserted the pre-split wording**
- **Found during:** Task 2 RED
- **Issue:** it asserted `message not sent: ` for every error kind on a message that was not in the Sending state; with the split it contradicts the plan's classification.
- **Fix:** replaced by `fail_classification` and `fail_text_fixed` over all 13 `ApiError` values (the plan's own test names).
- **Files modified:** `src/app/send.rs`
- **Commit:** `c3498ed`

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs). **Impact:** none on scope; all three are consequences of the plan's own split and of the key-clears-status rule.

## Issues Encountered

None. `read_json`'s new parameter and the exhaustive `match` in `send_error_text` made the compiler list every site that needed the two new variants.

## Known Stubs

None. `App.own_username` is still `None` until plan 04-06, so a pending row is authored `me` until the server's copy arrives (D-17, from plan 04-02). The failed and unconfirmed markers are drawn by plan 04-04 (parallel, wave 3); this plan's checks use the status line, the mock's request log and the swapped author.

## Threat Flags

None beyond the plan's register. T-04-09 (duplicate delivery) is mitigated by the reconcile branch and counted with `received_requests` in the four effect tests and by `send_unconfirmed` (one POST, one GET); T-04-10 (no automatic resend, one in flight) by `unsent_never_resent_on_its_own`, `retry_waits_behind_a_message_in_flight` and 429 being Failed; T-04-11 by the fixed texts, `fail_text_fixed` and `ALL_ERRORS` at 13 in `sentinel_sweep`; T-04-12 by `remove_at` and the discard tests. T-04-SC-03 holds: `Cargo.toml` is unchanged. The mock's new route lives only in the test harness on a loopback ephemeral port.

## Next Phase Readiness

Ready for plan 04-04 (merged with this one by the orchestrator): `SendState::Failed` and `Unconfirmed` now occur in real runs, the status line names which, and `r` / `dd` act on them. Rows to draw markers for exist; nothing in this plan changed the renderer.

## Self-Check: PASSED

All nine modified files exist and `git status` is clean; the commits `aef5eb0`, `c3498ed` and `4561c0d` are ancestors of HEAD. `cargo test` (402 passed), clippy and fmt pass; the seven pty scenarios print `SMOKE OK`.
