---
phase: 03-connect-read
plan: 06
subsystem: auth
tags: [reconnect, redaction, wiremock, pty-smoke, error-mapping, tls, rust]

requires:
  - phase: 03-connect-read
    provides: "api::Client with typed ApiError, Effect::LoadRooms outbox, HistoryState and on_open (plans 03-01, 03-03)"
provides:
  - ":reconnect and :retry command (one table row) reloading rooms and retrying failed history"
  - "history::retry_failed(app)"
  - "Client::with_timeouts for short-deadline tests"
  - "api tests: error_mapping (12 failure classes), chunked oversize, no_redirect_follow, only_get_requests, client_debug_redacts"
  - "src/redaction.rs cfg(test) D-07 sentinel sweep and hostile-text render check"
  - "pty scenario unauthorized with a settable mock mode"
affects: [03-07, phase-04-send, phase-05-realtime, phase-06-company-server]

plan_head_before: 4ef28821393285e03117d7e557f981fd2036a06a
plan_head_after: bcd07619ede5c16f5a77f6878defb7720557d8f7

actuals:
  tokens: 8425
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Failure recovery is user-driven only: the reducer records no effect on LoadFailed or HistoryFailed, so retries are :reconnect or reopening a room"
    - "One redaction module (src/redaction.rs) lists every type holding a secret or message text; new such types must be added there"
    - "Characterization tests proved to bite by a one-line mutation (redirect policy) instead of a RED run"

key-files:
  created:
    - src/redaction.rs
  modified:
    - src/app/command.rs
    - src/app/history.rs
    - src/api/mod.rs
    - src/lib.rs
    - scripts/smoke_pty.py

key-decisions:
  - "retry_failed forgets every Failed entry (closed rooms are fetched when next opened) and reopens only the current room if it had failed; Loading and Loaded rooms are never refetched (D-13, A-OQ5)"
  - "with_timeouts is an associated crate-visible fn on Client and is the only place deadlines vary; TLS verification and the redirect policy are not parameters (D-08)"
  - "The oversize-without-Content-Length case is a separate raw-TCP chunked test using std::net in a thread, so no new dependency or tokio feature was added"

patterns-established:
  - "Sentinel strings: TOKEN SENTINEL-TOKEN-9c41, user SENTINEL-USER, password SENTINEL-PASS, body SENTINEL-BODY-7f3a, request path SENTINEL-ROOM-5d1e"
  - "Pty mock mode switch: start_mock(mode) exposes server.mode, read by the handler on every request"

requirements-completed: [AUTH-03, AUTH-01, ROOM-02]

coverage:
  - id: D1
    description: ":reconnect and its alias :retry reload rooms through Effect::LoadRooms, drop Failed history entries and re-request the open room's failed first page; nothing retries on its own"
    requirement: "AUTH-01"
    verification:
      - kind: unit
        ref: "src/app/command.rs#reconnect_reloads, retry_alias, reconnect_retries_failed_history, reconnect_leaves_loading_and_loaded_rooms_alone, quit_after_failure, no_auto_retry"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui unauthorized"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every failure class maps to its own fixed-text ApiError (401, 403, 404, 429, 3xx, other 4xx, 5xx, non-JSON, oversize with and without Content-Length, timeout, unreachable) with no request path or token in Display or Debug"
    requirement: "AUTH-03"
    verification:
      - kind: integration
        ref: "src/api/mod.rs#error_mapping, chunked_oversized_body_is_too_large"
        status: pass
    human_judgment: false
  - id: D3
    description: "A cross-origin redirect is never followed (second origin receives zero requests) and only GET requests are sent"
    requirement: "AUTH-01"
    verification:
      - kind: integration
        ref: "src/api/mod.rs#no_redirect_follow, only_get_requests"
        status: pass
    human_judgment: false
  - id: D4
    description: "No Phase 3 type, error, report or action prints the token, user id, URL credentials or message text; hostile server text leaves no control or bidi cell; no logging crate or file writer exists"
    requirement: "AUTH-03"
    verification:
      - kind: unit
        ref: "src/redaction.rs#sentinel_sweep, userinfo_url_not_echoed, hostile_server_text"
        status: pass
      - kind: other
        ref: "cargo tree -e normal --depth 1 logging-crate guard returns 0; grep for danger_accept_invalid|add_root_certificate|tls_built_in_root_certs returns 0; grep -rlE 'File::create|OpenOptions::new' src returns 0"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-11-17
status: complete
---

# Phase 3 Plan 06: Reconnect and Redaction Summary

**`:reconnect`/`:retry` recovery (rooms via Effect::LoadRooms plus retry of Failed history, never automatic), a 12-class error-mapping and no-redirect transport test set, and a cfg(test) sentinel sweep proving no Phase 3 type or terminal cell leaks the token, URL credentials or message text.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-11-17T11:12:26Z
- **Completed:** 2026-11-17T11:17:29Z
- **Tasks:** 2
- **Files modified:** 6 (1 created, 5 modified)

## Accomplishments

- Tracer: with the pty mock answering 401, the real binary shows `unauthorized: check the user id and token`, lists no room, sends exactly one `subscriptions.get` (no automatic retry), and after the mock accepts again `:reconnect` clears the line and fills the sidebar. `connect_read` still passes.
- `:reconnect` and `:retry` are one `COMMANDS` row. `cmd_reconnect` sets `ConnState::Connecting`, pushes `Effect::LoadRooms` and calls `history::retry_failed`, which drops every Failed entry and reopens the current room only if it had failed.
- Transport hardening: `Client::with_timeouts` plus `error_mapping` (401, 403, 404, 429, 307, 500, 502, 418, non-JSON, 9 MiB body, delayed past a 200 ms deadline, closed port), a chunked (no Content-Length) oversize test, `no_redirect_follow` with a second origin declared `.expect(0)`, `only_get_requests`, and `client_debug_redacts`.
- `src/redaction.rs`: one sweep over Secret, EnvValue, ConnEnv, Connection, FileConn, Sources, Reports (including wrong-type and userinfo-URL configs), every ApiError, Client, HistoryPage, Message, Effect and the Phase 3 Actions, plus the userinfo-URL and hostile-text tests.

## Task Commits

1. **Task 1 (tracer): rejected token, :reconnect, :retry, failed history retried** - `4dc7718` (feat)
2. **Task 2: transport hardening tests and the D-07 redaction sweep** - `bcd0761` (test)

**Plan metadata:** recorded by the docs commit that follows this file.

## Files Created/Modified

- `src/app/command.rs` - reconnect/retry row, `cmd_reconnect`, six new tests (10 total in the module)
- `src/app/history.rs` - `retry_failed`
- `src/api/mod.rs` - `Client::with_timeouts`, `new` delegates; five new test functions plus helpers
- `src/redaction.rs` - new cfg(test) sentinel sweep, userinfo URL and hostile-text tests
- `src/lib.rs` - `#[cfg(test)] mod redaction;`
- `scripts/smoke_pty.py` - mock mode `unauthorized` (settable `server.mode`), `scenario_unauthorized`, registered

## Decisions Made

- `retry_failed` forgets Failed entries of closed rooms rather than refetching them: they load when next opened (D-16, D-13). Loading and Loaded rooms are untouched (A-OQ5).
- Deadlines are the only thing `with_timeouts` varies; redirect policy and TLS stay fixed (D-08).
- The chunked oversize test uses `std::net` in a thread, so no dependency or tokio feature was added.

## Deviations from Plan

### Auto-fixed Issues

None - no auto-fix was needed. Two small additions beyond the listed behavior, both inside the plan's intent:

- Added a sixth command test `reconnect_leaves_loading_and_loaded_rooms_alone` (module now reports 10 passed, the plan's floor was 9) to pin D-13 for a Loading open room.
- Added `chunked_oversized_body_is_too_large`, because the must-have says TooLarge applies with or without Content-Length and a wiremock body always carries one. This brings the Task 2 verify count to exactly 7 passed (4 api plus 3 redaction), as required.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** none.

## TDD Gate Compliance

Task 1 (tracer): RED was real. Before implementation, four of the new command tests failed on their planned assertions (`reconnect_reloads`, `retry_alias`, `reconnect_retries_failed_history`, `reconnect_leaves_loading_and_loaded_rooms_alone`); `quit_after_failure` and `no_auto_retry` passed because the reducer already never retried. The plan makes the tracer one commit, so RED and GREEN share `4dc7718`.

Task 2 (`tdd="true"`): the behaviors it asserts (error mapping, `Policy::none()`, redacting `Debug`, `sanitize_line`) were implemented by plans 03-01 to 03-05, so the new tests passed on first run: they are characterization tests, not new-behavior tests, and there is no RED run or `feat` commit for them. Instead, a one-line mutation proved a test bites: replacing `Policy::none()` with `Policy::limited(5)` made `no_redirect_follow` fail (second origin received a request); the change was reverted before the commit. The only production change is the behavior-preserving `with_timeouts` extraction, committed together with its tests as `test(03-06)` because the tests do not compile without it. No separate `feat(03-06)` commit exists for Task 2, by design.

Known limit of `hostile_server_text`: ratatui already drops control and zero-width graphemes when writing cells, so the cell-level check proves the end result and would still pass if one widget skipped `sanitize_line` (confirmed by a throwaway mutation of the status-bar room name, reverted). The `sanitize_line` behavior itself is covered by the unit tests in `src/ui/sanitize.rs`.

## Issues Encountered

None. `with_timeouts` was added before the tests so they compile; all existing tests stayed green throughout (360 passed at the end).

## Known Stubs

None.

## Threat Flags

None. No new network endpoint, auth path, file access or schema surface; the plan only adds a user command that issues the existing `LoadRooms` effect, plus tests. Threat register items T-03-23 (redirect), T-03-24 (reqwest error text), T-03-25 (Debug of Phase 3 types), T-03-26 (hostile text) and T-03-27 (no automatic retry) are covered by tests above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 3 failure UX is complete: rejected tokens, unreachable servers, rate limits and redirects each show one actionable line and `:reconnect` recovers; only the final plan (03-07) of this phase remains.
- Any later type holding a secret or message text (Phase 4 send, Phase 5 realtime) must be added to `src/redaction.rs::sentinel_sweep`.
- Realtime reconnect remains MSG-04 (Phase 5); private CA handling remains Phase 6 (D-08).

## Self-Check: PASSED

- FOUND: src/redaction.rs, src/app/command.rs, src/app/history.rs, src/api/mod.rs, src/lib.rs, scripts/smoke_pty.py
- FOUND commits: 4dc7718, bcd0761 (ancestors of HEAD)
- `cargo test` 360 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean
- `SMOKE OK` for unauthorized, perm_warning and connect_read; logging-crate guard 0 (positive control 2)

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*
