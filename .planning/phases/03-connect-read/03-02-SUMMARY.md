---
phase: 03-connect-read
plan: 02
subsystem: infra
tags: [docker, rocket.chat, mongodb, python-stdlib, fixtures, pat]

requires:
  - phase: 03-connect-read
    provides: "plan 03-01 tracer (env PAT to real rooms), runs in parallel and shares no file with this plan"
provides:
  - "dev/compose.yml: Rocket.Chat + single-node MongoDB 8.0 replica set, loopback only, RELEASE and HOST_PORT variables"
  - "dev/seed.py: stdlib-only up / seed / check / capture / down with secrets in 0600 files under git-ignored /dev/state/"
  - "A seeded local server (8.9.1 on 127.0.0.1:3100) covering every Phase 3 case, left running"
  - "Five scrubbed, secret-free REST fixtures under src/api/fixtures/docker/"
affects: [03-07, phase-3-uat, rocket.chat-9.0.0-compat-pass]

actuals:
  tokens: 14700
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Throwaway secrets only in dev/state/<project>/ (0700 dir, 0600 files, git-ignored), never printed or on argv"
    - "api() helper raises with method, path and status only, never body or headers"
    - "assert_no_secrets gate before any fixture is written"

key-files:
  created:
    - dev/compose.yml
    - dev/seed.py
    - dev/README.md
    - src/api/fixtures/docker/subscriptions.json
    - src/api/fixtures/docker/rooms.json
    - src/api/fixtures/docker/preferences.json
    - src/api/fixtures/docker/history_rt_public.json
    - src/api/fixtures/docker/history_dm_alice.json
  modified:
    - .gitignore

key-decisions:
  - "Secrets live in dev/state/<project>/ (not dev/.env.local) so the 8.9.1 and 9.0.0 stacks keep separate credentials under one ignore rule"
  - "rooms.get, subscriptions.get and history are scrubbed recursively for lastMessage and md, a superset of the plan's wording"
  - "The discussion is created inside the rt-public loop (before message 127) so its discussion-created system message sits in the newest history page while alice still posts last"

patterns-established:
  - "Dev fixture scripts: python3 -I, stdlib only, print names and counts only"

requirements-completed: [DLVR-01]

coverage:
  - id: D1
    description: "A local Rocket.Chat (8.9.1, MongoDB 8.0 replica set) starts from one command, bound to 127.0.0.1:3100 only, and the dev user's PAT reads subscriptions.get"
    requirement: DLVR-01
    verification:
      - kind: integration
        ref: "python3 -I dev/seed.py up && python3 -I dev/seed.py seed && python3 -I dev/seed.py check"
        status: pass
    human_judgment: false
  - id: D2
    description: "The seeded dataset holds every Phase 3 case: paging (130 messages), join and discussion-created system messages, attachment, favorite, unread DM, closed DM, encrypted group, discussion, team and the Work category"
    requirement: DLVR-01
    verification:
      - kind: integration
        ref: "python3 -I dev/seed.py down && up && seed && check && capture (check lists alice, bob, general, rt-discussion, rt-private, rt-public, rt-team, secrets)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Secrets stay local: state files are 0600 in a 0700 directory, git-ignored, and check verifies ignore rule and modes"
    requirement: DLVR-01
    verification:
      - kind: integration
        ref: "python3 -I dev/seed.py check; git status --porcelain --untracked-files=all dev/ shows no dev/state entry"
        status: pass
    human_judgment: false
  - id: D4
    description: "Five fixtures captured with no lastMessage or md key and no credential text"
    requirement: DLVR-01
    verification:
      - kind: other
        ref: "python3 -I -c rooms.json has no lastMessage and one encrypted room; grep -c '\"md\"' history_rt_public.json == 0; assert_no_secrets passed in capture"
        status: pass
    human_judgment: false
  - id: D5
    description: "Prohibition: this setup is never pointed at the company server (CLAUDE.md Policy)"
    requirement: DLVR-01
    verification: []
    human_judgment: true
    rationale: "A policy statement about future use; automation cannot assert it. seed.py and the README only reference 127.0.0.1, but a human must confirm intent in Phase 6."

duration: 7min
completed: 2026-11-17
status: complete
plan_head_before: 4721005598cc226860c1c5eb936f13aec318adae
plan_head_after: 982d5b208ea473543ed1ba3c01006f678267cd93
commits: 2
---

# Phase 3 Plan 02: Local Docker Rocket.Chat Fixture Summary

**One-command local Rocket.Chat 8.9.1 + MongoDB 8.0 replica set on 127.0.0.1:3100, seeded with every Phase 3 case and a PAT kept only in 0600 git-ignored files, plus five scrubbed real-response fixtures.**

## Performance

- **Duration:** about 7 min (image pull included)
- **Completed:** 2026-11-17T10:50Z
- **Tasks:** 2 (1 tracer, 1 auto)
- **Files modified:** 9 (8 created, `.gitignore` modified)

## Accomplishments

- `dev/compose.yml` and `dev/seed.py up` bring up Rocket.Chat 8.9.1 with a single-node replica set; `seed` creates users dev, alice, bob and a PAT; `check` proves the PAT reads `subscriptions.get` and that the state files are ignored and 0600.
- The full seed produces: `rt-public` (130 messages, alice's `uj` join message, a `discussion-created` message, message 125 with a `report.pdf` attachment), `rt-private` (favorited), alice DM (2 unread), bob DM (closed, `open=false`), `secrets` (encrypted in MongoDB), `rt-discussion`, `rt-team` (`teamMain`) held by the custom category `Work`.
- `capture` wrote `subscriptions`, `rooms`, `preferences`, `history_rt_public` (50 messages), `history_dm_alice` (3 messages) with `lastMessage` and `md` removed and a pre-write secret check over the PAT, all passwords and fresh login tokens.
- The stack is left running for plan 03-07 and the end-of-phase UAT. The 9.0.0 stack is one command away (documented in `dev/README.md`) but was not started.

## Task Commits

1. **Task 1 (tracer): compose up, minimal seed, PAT, check** - `2342597` (feat)
2. **Task 2: full seed dataset and scrubbed fixture capture** - `982d5b2` (feat)

**Plan metadata:** committed with this SUMMARY (docs: complete plan).

## Files Created/Modified

- `dev/compose.yml` - mongodb + rocketchat services, `127.0.0.1:${HOST_PORT:-3100}:3000`, OVERWRITE_SETTING_* for wizard, 2FA and rate limiter
- `dev/seed.py` - up / seed / check / capture / down; helpers `api`, `assert_no_secrets`, `write_secret`, `mongosh`
- `dev/README.md` - purpose, prerequisites, commands, dataset table, running the app, 9.0.0 stack, security note
- `.gitignore` - `/dev/state/`
- `src/api/fixtures/docker/*.json` - five captured REST responses

## Assumptions Resolved (RESEARCH)

- **A3 held.** Every request body worked as written on the first run: `channels.create`, `groups.create`, `chat.postMessage` (including `attachments`), `rooms.favorite`, `im.create`, `im.close`, `rooms.createDiscussion`, `teams.create`, `users.setPreferences`, `channels.join`.
- **A4 held.** The compose skeleton started as written (replica-set healthcheck initiated rs0, Rocket.Chat reached `/api/info` without retries beyond the poll).
- **A7 held (as far as observable).** The REST flow worked with `Show_Setup_Wizard=completed`; no wizard interference.
- **A8 held.** `encrypted: true` set through `mongosh` is visible in `rooms.get` immediately (the rooms fixture carries exactly one encrypted room, `secrets`).
- **Categories were not skipped.** `users.setPreferences` with `sidebarCategories` and the MongoDB `category` update both succeeded without a license; the preferences fixture contains `[{_id: "cat-work", name: "Work"}]` and the `rt-team` subscription has `category: "cat-work"`.

## Decisions Made

- Secrets in `dev/state/<project>/` rather than `dev/.env.local` (plan's A-STATE, followed).
- Scrub applied recursively to every captured document (`lastMessage`, `md`), not only to `rooms.get` items and messages.
- Discussion creation moved into the `rt-public` loop (before message 127), see Deviations.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Discussion-created message made rt-public's last message dev's own**
- **Found during:** Task 2 (first full run)
- **Issue:** Creating `rt-discussion` after the 130 messages added a dev-authored `discussion-created` message at the top of `rt-public`, so alice no longer posted last, contrary to the plan's "alice's posts after dev's own posts".
- **Fix:** Create the discussion at message 127 inside the loop, so alice's 127 and 129 are still the newest regular messages and the system message sits in the newest history page.
- **Files modified:** dev/seed.py
- **Verification:** full `down, up, seed, check, capture` re-run passes; history fixture top is 129, 128, 127, `discussion-created`, 126.
- **Committed in:** 982d5b2

**2. [Rule 1 - Bug] `check` listed the discussion by its room id**
- **Found during:** Task 2
- **Issue:** A discussion subscription's `name` is the room id, so the `ok:` line printed a random id instead of `rt-discussion`.
- **Fix:** `check` uses `fname` for subscriptions with `prid`.
- **Files modified:** dev/seed.py
- **Verification:** `check` prints `alice, bob, general, rt-discussion, rt-private, rt-public, rt-team, secrets`.
- **Committed in:** 982d5b2

---

**Total deviations:** 2 auto-fixed (2 bugs in the plan's seed ordering and check output)
**Impact on plan:** Both are local to dev/seed.py; no scope creep.

## Issues Encountered

- `rt-public` shows `unread: 0` (with `alert: true`) for dev even though alice posted last: this server counts only mentions as unread in channels by default. The unread case is covered by the alice DM (`unread: 2`) and `general` (`unread: 1`, a mention in the auto welcome message). Plan 03-07 and the UI work should not assume channel `unread` counts all messages. `/api/info` reports `version: "8.9"` (major.minor), not 8.9.1.
- Server-side facts for later plans: the `general` channel exists automatically and appears in the sidebar; a discussion subscription's `name` is its room id and the readable title is `fname`; the newest `rt-public` page contains a `discussion-created` system message (`t` set, no `uj`/`ul`) that the mapper must render through its generic fallback.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required. Docker with compose is the only prerequisite and was already present. The stack is running now on 127.0.0.1:3100 (project `rockettui-dev`).

## Known Stubs

None.

## Threat Flags

None. The only network surface added is the loopback-bound dev server, already covered by T-03-08, T-03-09 and T-03-10 in the plan's threat model. Mitigations verified: state files 0600 in a 0700 directory and ignored (`check`), compose binds `127.0.0.1` only, secrets pass through the subprocess environment and never argv, `assert_no_secrets` guards `capture`, and no credential text appears in the committed fixtures.

## Next Phase Readiness

- Plan 03-07 can run the real binary against 127.0.0.1:3100 (`set -a; . dev/state/rockettui-dev/.env.local; set +a`) and turn the five fixtures into mapping tests.
- The Rocket.Chat 9.0.0 compatibility pass needs its stack started (`RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101`, then `up` and `seed`); not done here.
- Edge probe E-DLVR-01 (what "works against a local Docker server" covers beyond the five ROADMAP criteria) stays flagged for plan 03-07.

## Self-Check: PASSED

- FOUND: dev/compose.yml, dev/seed.py, dev/README.md, five fixtures under src/api/fixtures/docker/
- FOUND commits: 2342597, 982d5b2 (ancestors of HEAD)
- `python3 -I dev/seed.py check` prints `ok: 8 subscriptions: alice, bob, general, rt-discussion, rt-private, rt-public, rt-team, secrets`
- `git status --porcelain --untracked-files=all dev/` shows no `dev/state` entry

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*
