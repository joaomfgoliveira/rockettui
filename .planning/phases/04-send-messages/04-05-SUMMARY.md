---
phase: 04-send-messages
plan: 05
subsystem: messaging
tags: [rust, ratatui, config, drafts, dedupe, send-queue]

requires:
  - phase: 04-send-messages
    provides: "send::submit/pump/on_sent/on_failed, SendState, Effect::SendMessage (plans 04-01 to 04-04)"
provides:
  - "config.toml follow_on_send (always | if_at_bottom) and messages::is_at_bottom"
  - "per-room in-memory drafts swapped in App::open_room"
  - "messages::confirm_held and merge_incoming: the state-aware _id dedupe (D-13 as amended by D-21), applied to realtime-style arrivals and to history pages"
  - "App.awaiting_answer: the in-flight slot of a Sending row confirmed early (D-03)"
affects: [05-realtime, send-recovery, history]

plan_head_before: ea5caf2cc9c1ad3b9af36184ee93b0080bfdc2f4
plan_head_after: 23258694f2741a3878f1bd5dc835ebe18aece573

actuals:
  tokens: 11500
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "one place per rule: confirm_held is the only home of the D-21 held-id rule, used by merge_incoming and both history merges"
    - "side map App.awaiting_answer consulted by pump next to the Sending scan, released only by on_sent and on_failed"

key-files:
  created: []
  modified:
    - src/config/settings.rs
    - src/app/messages.rs
    - src/app/send.rs
    - src/app/mod.rs
    - src/app/history.rs

key-decisions:
  - "follow_on_send is measured with is_at_bottom BEFORE the new row is appended; when not following, msg_views is left untouched (an append shifts no index)"
  - "open_room is the single place drafts move; with no room open yet a non-empty live buffer is kept so early typing is not lost"
  - "confirm_held replaces a held pending copy in place and never pumps, touches msg_views or records an effect; only the POST's own answer releases awaiting_answer"
  - "A SendFailed for a row already confirmed Sent is silent (no status error) but still frees the slot and pumps the queue"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "follow_on_send config key: always (default) jumps to the newest message after a send; if_at_bottom keeps a scrolled-up pane in place and follows only when the last row was visible; a bad value is one clear config error"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/config/settings.rs#follow_on_send_values"
        status: pass
      - kind: unit
        ref: "src/app/messages.rs#is_at_bottom_cases"
        status: pass
      - kind: unit
        ref: "src/app/send.rs#follow_always_jumps, follow_if_at_bottom_stays, follow_if_at_bottom_follows"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui send; ... config_error"
        status: pass
    human_judgment: false
  - id: D2
    description: "Drafts are per room, in memory only: switching rooms stores the old room's non-empty draft and restores the new room's; the input bar height follows the restored draft; Enter after a switch sends to the open room only"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/mod.rs#drafts_switch_and_restore, drafts_same_room_keeps, drafts_empty_not_stored, drafts_before_any_room, drafts_send_goes_to_own_room, drafts_resettle_height, drafts_stay_in_memory_and_redacted"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui insert; ... connect_read"
        status: pass
    human_judgment: false
  - id: D3
    description: "State-aware _id dedupe (D-13 amended by D-21) for MessageSent with a discarded row, realtime-style arrivals and history pages: absent appends, held Sent is ignored, held pending is replaced in place and marked Sent"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/messages.rs#merge_incoming_absent_appends, merge_incoming_sent_ignored, merge_incoming_pending_confirms"
        status: pass
      - kind: unit
        ref: "src/app/send.rs#sent_after_discard_appends"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#first_page_confirms_held_pending, first_page_ignores_held_sent, older_page_confirms_held_pending, older_page_ignores_held_sent"
        status: pass
    human_judgment: false
  - id: D4
    description: "D-03 survives a confirmation: a confirmed Sending copy keeps the room's one in-flight slot until its own MessageSent or SendFailed; a late failure for a confirmed row is silent"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/app/send.rs#confirm_sending_keeps_slot, confirmed_row_failure_is_silent"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-11-19
status: complete
---

# Phase 4 Plan 05: Follow-on-send, per-room drafts and state-aware id dedupe Summary

**follow_on_send config (always / if_at_bottom), per-room in-memory drafts swapped in open_room, and a state-aware `_id` dedupe (confirm_held / merge_incoming, D-21) used by realtime-style arrivals and history pages, with an awaiting_answer slot that keeps D-03 true after an early confirmation.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-11-19T19:23:00Z
- **Completed:** 2026-11-19T19:48:02Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- `follow_on_send = "always" | "if_at_bottom"` in config.toml: a user reading older messages is no longer yanked to the bottom by their own send under `if_at_bottom`; the default (`always`) keeps the Phase 1 behavior. A bad value is one config error naming the file, line and key and listing both values.
- Drafts are per room and in memory only: opening room B from room A stores A's non-empty draft and shows B's own; reopening the current room keeps its draft; a draft typed before any room was open stays in the bar; the input buffer is re-settled so the bar height follows the restored draft.
- `messages::confirm_held` is the single home of D-21 (absent: not handled, held Sent: ignored, held pending: replaced in place and marked Sent). `merge_incoming` is the append path Phase 5's realtime echo reuses, `on_sent` falls back to it when the row was discarded, and `history::on_loaded` / `on_older_page` send every page message through `confirm_held` instead of skipping held ids.
- `App.awaiting_answer` keeps a room's single in-flight slot occupied after a `Sending` row is confirmed early; `pump` honors it, `on_sent` and `on_failed` release it, and a failure for an already confirmed row is silent.

## Task Commits

1. **Task 1 (tracer): follow_on_send -> Settings -> submit -> is_at_bottom** - `93c8da6` (feat). Tests were written first (RED was a compile failure on the missing `FollowOnSend`/`is_at_bottom`), single commit because the task is `type="tracer"`, not `tdd="true"`. Tracer gate: end-of-phase mode and automated-only verify, re-run end-to-end (unit tests, smoke `send` and `config_error`, full suite, clippy, fmt) all passed, so expansion continued.
2. **Task 2: per-room drafts (D-10)**
   - RED `c8950f5` (test): `App.drafts` added empty; 4 of 7 tests fail on assertions (switch and restore, send to own room, resettle height, redacted store)
   - GREEN `9eac27a` (feat): `open_room` swap
3. **Task 3: D-21 dedupe and in-flight slot**
   - RED `90a5ab5` (test): `Merge`, `confirm_held`/`merge_incoming` stubs and `App.awaiting_answer` field; 7 new tests fail on assertions
   - GREEN `2325869` (feat): confirm_held, merge_incoming, awaiting_answer in pump/on_sent/on_failed, history merges

**Plan metadata:** committed with this SUMMARY (docs: complete plan)

## TDD notes

- Task 2 RED: `cargo test --lib -- app::tests::drafts` failed on `drafts_switch_and_restore` ("random has no draft yet"), `drafts_send_goes_to_own_room`, `drafts_resettle_height` (left 3, right 1) and `drafts_stay_in_memory_and_redacted` (left 0, right 1); semantic assessment: each failed on the planned assertion for the intended reason (the live buffer was shared across rooms), not on a compile, fixture or setup fault. The other three tests pass before the change by design (no behavior to change) and guard the same-room, empty and before-any-room rules.
- Task 3 RED: `merge_incoming_pending_confirms`, `merge_incoming_sent_ignored`, `confirm_sending_keeps_slot`, `confirmed_row_failure_is_silent`, `sent_after_discard_appends`, `first_page_confirms_held_pending`, `older_page_confirms_held_pending` failed on assertions against the stubs (left/right mismatches). `first_page_ignores_held_sent` and `older_page_ignores_held_sent` already passed before GREEN because the old skip-held-ids behavior already ignores a held Sent row; they stay as regression guards for the already-Sent case. `gsd_run check tdd-red-evidence` was not run (cargo test console output is not one of its supported report formats); the semantic assessment above is the RED evidence.
- No REFACTOR commit was needed.

## Files Created/Modified

- `src/config/settings.rs` - `FollowOnSend`, `Settings.follow_on_send`, KNOWN_KEYS entry, `one_of` parse, tests
- `src/app/messages.rs` - `is_at_bottom`, `Merge`, `confirm_held`, `merge_incoming`, tests
- `src/app/send.rs` - follow policy in `submit`; `awaiting_answer` in `pump`/`on_sent`/`on_failed`; `on_sent` merge fallback; tests
- `src/app/mod.rs` - `App.drafts`, `App.awaiting_answer`, draft swap in `open_room`, draft tests
- `src/app/history.rs` - `on_loaded` and `on_older_page` use `confirm_held`; four new tests

## Decisions Made

- Followed the plan as specified. `confirm_held` never pumps, touches `msg_views` or records an effect (indices are preserved by an in-place replace), so a confirmed row keeps its position and cursor and only the POST's own answer frees the slot.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug/Consistency] Updated an existing test that asserted the superseded behavior**
- **Found during:** Task 3 (GREEN)
- **Issue:** `send::tests::on_sent_other_room_and_unknown_id` asserted that `on_sent` for an id the room does not hold changes nothing and creates no vector. The plan's new rule (`on_sent` falls back to `merge_incoming`, `sent_after_discard_appends`) makes that behavior intentionally different.
- **Fix:** The test now asserts the stray server message is appended once (existing rows untouched) in a room with messages and in an empty room.
- **Files modified:** src/app/send.rs
- **Verification:** `cargo test --lib -- app::send` passes
- **Committed in:** `90a5ab5` (test) and `2325869`

---

**Total deviations:** 1 auto-fixed (1 test updated to the plan's new rule)
**Impact on plan:** None on scope; the change is the direct consequence of Task 3's specified `on_sent` behavior.

## Issues Encountered

None. One shell-guard refusal on a compound python-heredoc plus cargo command; resubmitted as separate commands and the Edit tool.

## User Setup Required

None - no external service configuration required. New user-facing vocabulary: config.toml `follow_on_send` with values `always` (default) and `if_at_bottom`.

## Known Stubs

None.

## Threat Flags

None. The register's T-04-15 (drafts in memory only, redacting Debug, no drafts under `src/config/`), T-04-16 (swap only in `open_room`, `drafts_send_goes_to_own_room`), T-04-17 (`Reader::one_of`), T-04-18 (`confirm_held` matches by `_id` within the same room) and T-04-27 (`awaiting_answer.remove` appears only in `src/app/send.rs`) are implemented as planned.

## Verification Results

- `cargo test`: 435 passed, 0 failed; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- `cargo test --lib -- app::messages app::send app::history`: 65 passed (plan floor 45); the three named acceptance tests: 3 passed.
- `scripts/smoke_pty.py` scenarios `send`, `config_error`, `insert`, `connect_read`: SMOKE OK each.
- Acceptance greps: `"follow_on_send"` x4 in settings.rs, `pub enum FollowOnSend` 1, `pub fn is_at_bottom` 1, `follow_on_send` in send.rs 3, `pub drafts` 1, `drafts.remove` 2, no `drafts` under `src/config/`, `pub fn merge_incoming` 1, `pub enum Merge` 1, `pub fn confirm_held` 1, `confirm_held` in history.rs 4, no `have.contains/insert` outside comments, `pub awaiting_answer` 1, `awaiting_answer` in send.rs 11, `awaiting_answer.remove` only in send.rs.

## Next Phase Readiness

- Phase 5 can append realtime messages with `messages::merge_incoming(app, &room, message)`; the echo of the user's own message lands on its client id as a no-op or an in-place confirmation.
- Wave 4 sibling plan 04-06 touches disjoint files (update.rs, command.rs) and is unaffected.

## Self-Check: PASSED

All five modified files exist; commits `93c8da6`, `c8950f5`, `9eac27a`, `90a5ab5` and `2325869` are ancestors of HEAD; the full suite, clippy, fmt and the four pty smoke scenarios pass.

---
*Phase: 04-send-messages*
*Completed: 2026-11-19*
