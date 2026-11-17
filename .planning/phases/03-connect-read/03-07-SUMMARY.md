---
phase: 03-connect-read
plan: 07
subsystem: testing
tags: [pty-smoke, docker, rocket.chat, fixtures, compatibility, python-stdlib]

requires:
  - phase: 03-connect-read
    provides: "plan 03-02 seeded local Rocket.Chat 8.9.1 and five captured fixtures; plans 03-01..03-06 the connect-and-read app"
provides:
  - "scripts/smoke_pty.py: docker_env, sidebar_rows, selectable_index, open_room_named and the docker_read scenario (honors RT_PROJECT)"
  - "src/api/map.rs: docker_fixtures_rooms, docker_fixtures_folders, docker_fixtures_history over the captured real responses"
  - "dev/README.md: Compatibility section with 8.9.1 and 9.0.0 results"
  - "Proof that the real binary authenticates with a PAT and reads rooms, folders, history pages, message kinds and the encrypted placeholder on 8.9.1 and 9.0.0"
affects: [phase-3-uat, phase-6-company-server]

actuals:
  tokens: 3735
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Pty harness reads the Docker state file in-process, passes values only through the child environment and prints pass or fail only"
    - "Sidebar driven by selectable index computed from the replayed screen, not hard-coded key counts"

key-files:
  created: []
  modified:
    - scripts/smoke_pty.py
    - src/api/map.rs
    - dev/README.md

key-decisions:
  - "docker_env refuses a non-loopback server URL, so docker_read can never reach a real server (T-03-30)"
  - "The attachment assertion checks the two wrapped halves of [attachment: report.pdf], because the 50-column message pane wraps the tag"
  - "No mapping change was needed: the RESEARCH-derived mapping already matches real 8.9.1 output"

patterns-established:
  - "Docker-backed pty scenarios read their credentials from dev/state/<RT_PROJECT>/.env.local via docker_env"

requirements-completed: [DLVR-01, AUTH-01, ROOM-01, ROOM-02, ROOM-03, MSG-01]

coverage:
  - id: D1
    description: "The real binary, started with the seeded PAT, lists the seeded rooms and the Favorites and Work folders, never lists the closed bob DM, opens rt-public with its newest page, pages back to Beginning of history, shows the join and attachment lines, shows encrypted, not supported for secrets, opens the alice DM and quits cleanly, on Rocket.Chat 8.9.1"
    requirement: DLVR-01
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui docker_read (SMOKE OK, run 4 times)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The api mapping is locked to JSON captured from the real server: kinds, DM naming by username (A9), encrypted flag, ISO dates (A2), Favorites and Work folders, system messages"
    requirement: ROOM-01
    verification:
      - kind: unit
        ref: "src/api/map.rs#docker_fixtures_rooms, docker_fixtures_folders, docker_fixtures_history"
        status: pass
    human_judgment: false
  - id: D3
    description: "Rocket.Chat 9.0.0 compatibility: docker_read passes against a separately seeded 9.0.0 stack"
    requirement: DLVR-01
    verification:
      - kind: e2e
        ref: "RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_read (SMOKE OK, run twice)"
        status: pass
    human_judgment: false
  - id: D4
    description: "The token from the Docker state file never appears in the pty output of docker_read"
    requirement: AUTH-01
    verification:
      - kind: e2e
        ref: "scenario_docker_read fails if the token bytes occur anywhere in the pty output"
        status: pass
    human_judgment: false
  - id: D5
    description: "The author walks the five ROADMAP Phase 3 criteria by hand against the 8.9.1 stack and judges the folder layout (A6), D-15 wording and status messages"
    requirement: DLVR-01
    verification: []
    human_judgment: true
    rationale: "DLVR-01 is defined by the author's own use against a real server; folder layout (A6) and D-15 wording are product judgments (03-VALIDATION.md Manual-Only Verifications). Deferred to the end-of-phase UAT."

duration: 6min
completed: 2026-11-17
status: complete
plan_head_before: 0a410763b26f466d403547ebfa64889827786f23
plan_head_after: 1ee6763240acbd2f60e2d46a7cb5982925aaacb8
commits: 3
---

# Phase 3 Plan 07: Real-Server Proof, Docker Fixtures and 9.0.0 Compatibility Summary

**The real rockettui binary reads rooms, folders, paged history, system and attachment lines and the encrypted placeholder from local Rocket.Chat 8.9.1 and 9.0.0 through a pty harness that never exposes the PAT, and the api mapping is pinned to the captured real responses.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-11-17T11:19:21Z
- **Completed:** 2026-11-17T11:25:37Z
- **Tasks:** 2 (1 tracer, 1 auto/tdd)
- **Files modified:** 3

## Accomplishments

- `docker_read` drives the real binary against the seeded server: waits up to 20 s for rooms, requires rt-public, rt-private, alice, secrets, rt-team, rt-discussion, Favorites and Work, rejects a listed bob DM, opens rt-public and sees `seed message 129` and the attachment tag, pages with `gg` until `* joined the room` and then `Beginning of history`, opens `secrets` (`encrypted, not supported`) and the alice DM, quits with `:q`, and fails if the token appears in the pty output. Stable over four runs on 8.9.1.
- Three Docker-fixture mapping tests lock `map::rooms`, `map::folders` and `map::history` to the five captured real responses. They pass without any mapping change, and mutation checks (DM naming swapped to `fname`, system kind turned to Normal) make them fail, so they are sensitive.
- Rocket.Chat 9.0.0 pass: a separate stack (`rockettui-dev9`, port 3101) was brought up, seeded, checked, `docker_read` passed twice with no client change, and the stack was removed. The 8.9.1 stack (`rockettui-dev`, port 3100) is left running for the UAT; `python3 -I dev/seed.py check` prints `ok:`.
- `dev/README.md` has a `## Compatibility` section with the results and known limits.

## Task Commits

1. **Task 1 (tracer): docker_read against the seeded 8.9.1 stack** - `b38b962` (feat)
2. **Task 2 (TDD) RED/test: docker_fixtures mapping tests** - `c7dae6a` (test)
3. **Task 2: Compatibility section in dev/README.md** - `1ee6763` (docs)

**Plan metadata:** committed with this SUMMARY (docs: complete plan).

## Files Created/Modified

- `scripts/smoke_pty.py` - `docker_env`, `sidebar_rows`, `room_name_of`, `selectable_index`, `open_room_named`, `scenario_docker_read`, registered as `docker_read`; `wait_ready` gained an optional `timeout` (default 10, mock scenarios unchanged)
- `src/api/map.rs` - `docker_fixtures_rooms`, `docker_fixtures_folders`, `docker_fixtures_history` with `include_str!` of `fixtures/docker/*.json`
- `dev/README.md` - Compatibility section

## Assumptions Resolved

- **A2 confirmed.** Real REST dates are ISO-8601 strings (`2026-11-17T10:49:25.248Z`); every mapped room has `last_activity > 0`, including `general`, which has no `lm` and falls back to the subscription `ts`.
- **A9 confirmed.** The DM subscription `name` is the other user's username (`alice`), `fname` is the full name (`Alice`); the rooms.get record of the DM has no `name`. The mapping names the DM `alice`.
- **A5 partially confirmed.** The same endpoints and fields work on 8.9.1 and 9.0.0 (`docker_read` passes on both). Older servers and the company server remain unknown (Phase 6, OQ-4).
- **A6 still awaiting the author.** The folder layout seen on the real servers: Unread (alice, general), Folders (Favorites with rt-private, Work with rt-team), Recent (rt-discussion, rt-public, secrets). Needs the author's judgment in the UAT.
- Real-server facts surfaced by the harness: the `discussion-created` message renders as `* system message` through the generic fallback; the attachment tag wraps over two rows in the 50-column pane.

## Decisions Made

- `docker_env` rejects a server URL whose host is not `127.0.0.1` or `localhost`, so a state file can never point `docker_read` at a real server (T-03-30).
- Because the message pane wraps `[attachment: report.pdf]`, `docker_read` asserts `[attachment:` and `report.pdf]`; the unit test still asserts the full tag on the mapped body.
- `room_name_of` compares the text after the kind glyph with the name after dropping the ` [e2e]` tag, rather than taking the last whitespace token (which would be `[e2e]` for secrets).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Attachment tag assertion broke on a wrapped row**
- **Found during:** Task 1 (first docker_read run)
- **Issue:** The plan asks for the Screen to contain `[attachment: report.pdf]`, but the real message row is wrapped at 50 columns, so the tag spans two rows (`[attachment:` / `report.pdf]`) and the literal never matches. The app is correct; the harness assumption was wrong.
- **Fix:** The scenario checks the two halves. No app change.
- **Files modified:** scripts/smoke_pty.py
- **Verification:** `docker_read` prints `SMOKE OK`
- **Committed in:** b38b962

**2. [Rule 2 - Missing critical] Loopback guard in docker_env**
- **Found during:** Task 1
- **Issue:** The plan's T-03-30 mitigation relies on the state file being written by seed.py; nothing stopped a hand-edited state file from pointing the scenario at another host.
- **Fix:** `docker_env` raises `SmokeFail` unless the URL host is `127.0.0.1` or `localhost`.
- **Files modified:** scripts/smoke_pty.py
- **Committed in:** b38b962

---

**Total deviations:** 2 auto-fixed (1 bug in a harness assumption, 1 missing safeguard)
**Impact on plan:** Both inside smoke_pty.py; no scope creep.

## TDD Gate Compliance

Task 2 carries `tdd="true"`, but the behavior (mapping over captured fixtures) already existed from plans 03-01 and 03-04, and the plan itself expects the tests to pass unless real output differs.

- **RED:** `cargo test --lib -- api::map::tests::docker_fixtures` passed 3 of 3 on the first run: an unexpected green, investigated per the TDD error-handling rule. The mapping already matches the real 8.9.1 output, so there was no failing assertion to commit. No `RED_EVIDENCE_OK` record exists and none was fabricated. To prove the tests are not vacuous I mutated `display_name` (DM naming) and `kind_and_body` (system kind): `docker_fixtures_rooms` and `docker_fixtures_history` failed, then I restored the code. The `docker_fixtures_folders` test did not fail when Favorites stopped excluding categorized rooms, because no captured favorited room carries a category; that exclusion is covered by the earlier hand-written folder tests.
- **GREEN:** no implementation commit; no code change was required. The `test(03-07)` commit `c7dae6a` stands alone with no `feat(03-07)` in the Rust path (the only `feat(03-07)` commit is the Task 1 harness).
- **REFACTOR:** none.

This is a flagged gate deviation for the end-of-phase TDD review, not a hidden one.

## Issues Encountered

- The 9.0.0 image pull and first start took a few minutes but `up`, `seed` and `check` needed no retries.
- None of the plan's `<verify>` commands failed after the attachment fix.

## Authentication Gates

None. The Docker PAT was read only by the harness; no secret value was printed, logged or placed on a command line.

## Known Stubs

None.

## Threat Flags

None. No new network endpoint or trust boundary; T-03-29 (token only in the child environment, pass/fail output, fail if the token reaches the pty), T-03-30 (loopback-only state file, now enforced in code) and T-03-31 (captured fixtures untouched, no mapping change) are all mitigated.

## User Setup Required

None - no external service configuration required. The 8.9.1 stack is running on 127.0.0.1:3100 for the author's UAT.

## Pending Human Verification (end-of-phase UAT)

Task 2's `<human-check>` is deferred to the author, as planned: in a real terminal run `set -a; . dev/state/rockettui-dev/.env.local; set +a; cargo run`, then walk the five ROADMAP Phase 3 criteria (env-var auth and config.toml auth at chmod 600; sidebar lists channels, private groups and DMs and Enter opens one; `k`/`gg` page back to `Beginning of history` in rt-public; `secrets` shows `encrypted, not supported`; chmod 644 on config.toml warns in the status bar and on exit, and the token never shows in scrollback after `:q`). Also judge the Folders layout (A6), the dim system lines and attachment tags (D-15) and the status wording.

## Next Phase Readiness

- Phase 3 plans are complete; ready for phase verification and the author's UAT against the running 8.9.1 stack.
- Open for Phase 6: company server version, license and policy (OQ-4); servers older than 8.x are untested.

## Self-Check: PASSED

- FOUND: scripts/smoke_pty.py, src/api/map.rs, dev/README.md
- FOUND commits: b38b962, c7dae6a, 1ee6763 (ancestors of HEAD)
- `cargo test` 363 passed, clippy and fmt clean, all twelve pty scenarios print `SMOKE OK`, `docker_read` also passes with `RT_PROJECT=rockettui-dev9` on 9.0.0
- `grep -c "^## Compatibility" dev/README.md` is 1; `docker_fixtures` appears in map.rs 3 or more times

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*
