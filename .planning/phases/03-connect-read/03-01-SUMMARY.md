---
phase: 03-connect-read
plan: 01
subsystem: api
tags: [reqwest, rustls, serde, wiremock, rocket-chat, pat, effect-outbox, pty-smoke]

requires:
  - phase: 02-configuration-system
    provides: config error screen (Problem/Report), Sources/load_from_sources pure core, redacting-Debug convention, status.error style
provides:
  - config::connection (Secret, EnvValue, ConnEnv, Connection, parse_server_url, resolve) with the ROCKETTUI_* env names
  - api::Client (sensitive auth headers, no redirects, 5 s/15 s timeouts, 8 MiB body cap), ApiError with fixed actionable Display
  - api::wire, api::map, three JSON fixtures; load_rooms joining subscriptions.get, rooms.get and users.getPreferences
  - effect::Effect outbox on App drained by run(), Action::LoadFailed, ConnState in the status bar
  - run() without fake data (empty sidebar and Connecting… first frame)
  - scripts/smoke_pty.py mock Rocket.Chat harness and the connect_read and missing_creds scenarios
affects: [03-03 open a room, 03-04 older history, 03-05 config.toml credentials, 03-06 failures and leak hardening, 03-07 Docker proof]

plan_head_before: 62d2162f5671dec1c6db6e5cdd1e639d6e38576e
plan_head_after: 57aa5722a63411403fcead743d2d46b48f088feb

actuals:
  tokens: 22300
  tasks: 2
  commits: 3

tech-stack:
  added: [reqwest 0.13 (json, query, default rustls), serde 1 (derive), serde_json 1, wiremock 0.6 (dev)]
  patterns:
    - "Effect outbox: the reducer pushes Effect onto App.effects, only run() drains and spawns tasks that send Actions on the one channel"
    - "Secret wrapper: credentials wrapped at read time, redacting Debug, no Display"
    - "One mapping point for reqwest errors to a payload-free ApiError; reqwest::Error is never formatted"
    - "All-or-nothing room load with a best-effort side call (users.getPreferences) that degrades to Favorites"
    - "Pure wire to domain mapping in api::map tested over shared JSON fixtures that the pty mock also serves"

key-files:
  created:
    - src/config/connection.rs
    - src/api/mod.rs
    - src/api/wire.rs
    - src/api/map.rs
    - src/api/fixtures/subscriptions.json
    - src/api/fixtures/rooms.json
    - src/api/fixtures/preferences.json
    - src/effect.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - src/config/mod.rs
    - src/config/paths.rs
    - src/action.rs
    - src/input/actions.rs
    - src/app/mod.rs
    - src/app/update.rs
    - src/ui/status.rs
    - src/lib.rs
    - scripts/smoke_pty.py

key-decisions:
  - "api::map::rooms takes slices (&[WireSub], &[WireRoom]) from the start, so folders can read the same subscriptions without a signature change in Task 2"
  - "Folders = Favorites (favorited rooms without a category, when non-empty) then custom categories; one condition in map::folders flips to the literal D-10 reading (A6, needs author confirmation)"
  - "wire structs derive Deserialize only; lastMessage is never declared so room message text cannot reach the sidebar load path"
  - "A subscription with no rooms.get record, open=false, a status, or t=l is skipped; duplicate rids keep the first"
  - "Pty harness strips ROCKETTUI_* from the child environment and sets NO_PROXY for 127.0.0.1 so a developer's credentials or proxy never affect a scenario"

patterns-established:
  - "Every request path goes through Client::get_json: status mapped first, body size capped, decode failure is BadResponse"
  - "Server strings are drawn through sanitize_line; the connection status line goes through it too"
  - "Pty scenarios start start_mock() and spawn with server_url=mock.base_url; wait_ready waits for a room, wait_drawn for the first frame"

requirements-completed: [AUTH-01, ROOM-01, AUTH-03]

coverage:
  - id: D1
    description: "ROCKETTUI_SERVER_URL, ROCKETTUI_USER_ID and ROCKETTUI_TOKEN resolve to a validated Connection; empty counts as unset; a partial setting is an error; the URL rules and fixed error texts hold"
    requirement: AUTH-01
    verification:
      - kind: unit
        ref: "src/config/connection.rs#env_resolution, missing_reported, server_url_rules, empty_is_unset_and_not_unicode_is_reported"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#connection_env_through_sources"
        status: pass
    human_judgment: false
  - id: D2
    description: "api::Client sends X-User-Id and X-Auth-Token as sensitive headers, never follows redirects, caps the body at 8 MiB, and maps every failure to a fixed ApiError"
    requirement: AUTH-01
    verification:
      - kind: unit
        ref: "src/api/mod.rs#tracer_rooms_end_to_end, redirect_is_not_followed, oversized_body_is_too_large, undecodable_body_is_bad_response, unreachable_server_is_unreachable, status_mapping, rooms_load_all_or_nothing"
        status: pass
    human_judgment: false
  - id: D3
    description: "The token and user id never appear in any Debug or Display of Secret, EnvValue, ConnEnv, Connection, Sources, Client, ApiError, Action::LoadFailed or the pty output"
    requirement: AUTH-03
    verification:
      - kind: unit
        ref: "src/config/connection.rs#secret_redacts; src/api/mod.rs#token_control_chars, debug_and_display_never_hold_the_token; src/action.rs#debug_redacts"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D4
    description: "subscriptions.get and rooms.get map to the sidebar rooms with kinds, unread, encrypted flag and last-activity order; skipped subscriptions never appear"
    requirement: ROOM-01
    verification:
      - kind: unit
        ref: "src/api/map.rs#rooms_from_fixtures, rooms_edges, same_name_rooms_stay_separate, last_message_text_is_ignored"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D5
    description: "Favorites and the user's custom categories become sidebar folders; any users.getPreferences failure degrades to Favorites only and never fails the room load"
    requirement: ROOM-01
    verification:
      - kind: unit
        ref: "src/api/map.rs#folders_union, folders_no_categories; src/api/mod.rs#preferences_failure_falls_back, preferences_requested"
        status: pass
    human_judgment: false
  - id: D6
    description: "The real binary starts at once with an empty sidebar and Connecting…, loads rooms in the background through the effect outbox, and shows one fixed status line on failure while :q still quits"
    requirement: ROOM-01
    verification:
      - kind: unit
        ref: "src/app/mod.rs#conn_state"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D7
    description: "A missing credential shows the Phase 2 error screen with one error per missing value, exits 1, prints the report to stderr after the terminal is restored, and never starts the app"
    requirement: AUTH-01
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui missing_creds"
        status: pass
    human_judgment: false
  - id: D8
    description: "The folder rule is the union of Favorites and custom categories (A6), a slight extension of the literal D-10 wording"
    verification: []
    human_judgment: true
    rationale: "A6 is a product decision the author has not yet confirmed; tests prove the union behaves as designed, not that it is the wanted behavior"

duration: 8min
completed: 2026-11-17
status: complete
---

# Phase 3 Plan 01: Connect & Read tracer Summary

**Environment PAT to real rooms in the sidebar: a reqwest 0.13 client with sensitive headers and no redirects, an Effect outbox drained by run(), Favorites and category folders, and a mock Rocket.Chat pty harness that proves the real binary**

## Performance

- **Duration:** 8 min
- **Started:** 2026-11-17T10:33:39Z
- **Completed:** 2026-11-17T10:42:05Z (code and SUMMARY work; the plan-metadata commit follows)
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 19 (8 created, 11 modified)

## Accomplishments

- The binary no longer seeds fake data. It starts at once with an empty sidebar and `Connecting…`, builds `api::Client` from the environment before `App` consumes the config (the token never enters `App`), and loads rooms through `Effect::LoadRooms` spawned from the run loop.
- `api::Client` carries `X-User-Id` and `X-Auth-Token` as sensitive header values, uses `redirect::Policy::none()`, 5 s connect and 15 s total timeouts, an 8 MiB body cap and the default rustls verification; every failure becomes a payload-free `ApiError` with one fixed status-bar line. The room load is all-or-nothing across `subscriptions.get` and `rooms.get`.
- `api::map` joins the two responses on room id (kinds, DM naming by username, encrypted from `rooms.get`, unread as a boolean, last activity from `lm` then the subscription `ts`, stable order) and `map::folders` builds Favorites plus the user's custom categories from a best-effort `users.getPreferences`.
- A missing, empty, non-UTF-8 or malformed credential shows the existing Phase 2 error screen (one error per missing value, naming `ROCKETTUI_*`), exit code 1, report on stderr after restore.
- `scripts/smoke_pty.py` now runs every scenario against a stdlib mock Rocket.Chat serving the same fixtures the Rust tests include, answering 401 to wrong credentials; all eight scenarios pass (the six Phase 1 and 2 ones plus `connect_read` and `missing_creds`).

## Task Commits

1. **Task 1: environment PAT to real rooms in the sidebar (tracer)** - `430845a` (feat)
2. **Task 2: sidebar folders from Favorites and custom categories**
   - RED: `da44252` (test)
   - GREEN: `57aa572` (feat)

**Plan metadata:** recorded in the docs(03-01) commit that follows this file.

## TDD Notes (Task 2)

- **RED:** four tests (`folders_union`, `folders_no_categories`, `preferences_failure_falls_back`, `preferences_requested`) over the preferences fixture, with `map::folders` stubbed to return no folders. All four compiled, executed and failed on the planned assertions (left `[]` versus the expected Favorites and Work folders; zero `users.getPreferences` requests). Semantic assessment: valid RED, no build, load or fixture fault, each target test ran and failed for the intended reason. `gsd_run check tdd-red-evidence` was not used: the plan is `type: execute` (not `type: tdd`) and cargo's console format is not one of its supported report formats.
- **GREEN:** `map::folders` and the third concurrent call in `load_rooms`; all four tests pass.
- **REFACTOR:** none needed.

## Files Created/Modified

- `src/config/connection.rs` - Secret, EnvValue, ConnEnv, CredSource, Connection, parse_server_url, resolve, env names
- `src/config/mod.rs` - `Config.connection`, `Sources.env`/`require_connection`, `load_from_dir_env`, `load()` requires the connection
- `src/api/mod.rs` - Client, ApiError, status and transport mapping, bounded body read, load_rooms, wiremock tests
- `src/api/wire.rs` - lenient Deserialize-only wire types and ApiDate
- `src/api/map.rs` - pure rooms and folders mapping with fixture tests
- `src/api/fixtures/{subscriptions,rooms,preferences}.json` - hand-written fixtures shared by Rust tests and the pty mock
- `src/effect.rs` - Effect, run, spawn
- `src/action.rs`, `src/input/actions.rs` - `LoadFailed(ApiError)` with redacting Debug and a non-bindable arm
- `src/app/mod.rs`, `src/app/update.rs` - `ConnState`, `App.effects`, `App.conn`, reducer arms
- `src/ui/status.rs` - `Connecting…` and the failure line in the status bar
- `src/lib.rs` - client build, initial LoadRooms, effect drain, kept sender, no fake seed
- `scripts/smoke_pty.py` - mock Rocket.Chat, credential env per spawn, new scenarios
- `Cargo.toml`, `Cargo.lock` - reqwest, serde, serde_json, wiremock (dev)

## Decisions Made

- `map::rooms` takes slices from the start (the plan's Task 1 text said `Vec`, Task 2 said it must change); this avoids a signature change mid-plan.
- Union folder rule (Favorites then custom categories) per RESEARCH A6, still needing the author's confirmation; the literal D-10 reading is one condition in `map::folders`.
- Duplicate subscription ids keep the first record so room ids stay unique for the sidebar.
- The pty mock tolerates a broken pipe so a deliberate panic scenario does not print server tracebacks.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test asserted the fixed URL error text never contains the input**
- **Found during:** Task 1 (`server_url_rules`)
- **Issue:** the specified fixed message "must be an http:// or https:// URL" itself contains the rejected input `http://`, so a "message does not contain the input" check is unsatisfiable for that case.
- **Fix:** the test asserts the error is exactly one of the three fixed texts, which proves no input can be echoed more strongly.
- **Files modified:** src/config/connection.rs
- **Committed in:** 430845a

**2. [Rule 1 - Bug] "Unreachable" test reused a live wiremock server**
- **Found during:** Task 1 (`unreachable_server_is_unreachable`)
- **Issue:** wiremock pools and reuses dropped servers, so a dropped `MockServer` port still answered.
- **Fix:** bind an ephemeral `TcpListener`, release it and connect to that closed port.
- **Files modified:** src/api/mod.rs
- **Committed in:** 430845a

**3. [Rule 2 - Missing critical] Extra tests beyond the plan's list**
- **Found during:** Task 1
- **Issue:** the plan names five api tests; the threat model (T-03-02, T-03-05) and Pitfalls 1, 2 and 13 also warrant 3xx, oversized body, undecodable body, status mapping and unreachable coverage.
- **Fix:** added `redirect_is_not_followed`, `oversized_body_is_too_large`, `undecodable_body_is_bad_response`, `status_mapping`, `unreachable_server_is_unreachable`, `wrong_credentials_are_unauthorized`, `effect_run_without_client_fails_closed`, wire and mapping edge tests and a `connection_env_through_sources` config test.
- **Files modified:** src/api/mod.rs, src/api/wire.rs, src/api/map.rs, src/config/mod.rs
- **Committed in:** 430845a, da44252

**4. [Rule 3 - Blocking] Pty harness hardening**
- **Found during:** Task 1 and Task 2 pty runs
- **Issue:** a proxy environment variable could route 127.0.0.1 traffic away from the mock; the abrupt exit of the panic scenario made the mock print BrokenPipe tracebacks; `wait_ready` on raw bytes is fragile now that rows arrive after the first frame.
- **Fix:** the child gets `NO_PROXY=127.0.0.1,localhost`; `reply` ignores BrokenPipe and ConnectionReset; `wait_ready` and `connect_read` read the screen model.
- **Files modified:** scripts/smoke_pty.py
- **Committed in:** 430845a, 57aa572

---

**Total deviations:** 4 auto-fixed (2 Rule 1, 1 Rule 2, 1 Rule 3)
**Impact on plan:** All necessary for correctness of tests or harness stability. No scope creep, no architectural change.

## Issues Encountered

None beyond the deviations above. The plan's `estimate` projected 98000 tokens; the realized diff is about 22300 on the chars/4 scale (Cargo.lock excluded).

## Authentication Gates

None. All HTTP in this plan is against wiremock or the stdlib pty mock; no real server was contacted.

## Known Stubs

None. `CredSource::File` is declared but unused until plan 03-05 adds config.toml credentials (documented in the type); `Effect` has one variant until plan 03-03 adds `LoadHistory`. Neither stub prevents this plan's goal.

## Threat Flags

None. The new network surface (REST GETs with the PAT headers) and trust boundaries are the ones in the plan's threat model (T-03-01 to T-03-07, T-03-SC); mitigations for T-03-01, T-03-02, T-03-03, T-03-04, T-03-05 and T-03-06 are in place as described. T-03-02 and T-03-04 get their hardening tests (two-server no-follow, negative source gate) in plan 03-06.

## User Setup Required

None - no external service configuration required for the tests. To run the binary against a real server set `ROCKETTUI_SERVER_URL`, `ROCKETTUI_USER_ID` and `ROCKETTUI_TOKEN` (plan 03-02 provides a local Docker server).

## Verification

- `cargo test`: 296 passed (all pre-phase tests and the 11 insta snapshots unchanged)
- `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean
- `python3 -I scripts/smoke_pty.py target/debug/rockettui <scenario>`: SMOKE OK for connect_read, missing_creds, quit, panic, insert, config_layout, config_keymap, config_error (run three times, stable)
- All Task 1 and Task 2 acceptance greps hold; `Cargo.lock` is committed and clean against HEAD

## Next Phase Readiness

- Plan 03-03 can add `Effect::LoadHistory` and the first-open logic on the same outbox; `effect::run` and `effect::spawn` are the extension points.
- Plan 03-05 extends `connection::resolve` with config.toml credentials and the missing-credential wording; `Sources` Debug is still derived (safe now, must be hand-written once the file can hold a token).
- Open item for the author: confirm A6 (Favorites plus custom categories) against the literal D-10 wording.

## Self-Check: PASSED

All created files exist, all three task commits are ancestors of HEAD, and the verification commands above were run after the last code commit.

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*
