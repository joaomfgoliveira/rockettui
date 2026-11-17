---
phase: 03-connect-read
verified: 2026-11-17T00:00:00Z
status: human_needed
score: 5/5 must-haves verified
covered_files:
  - ".planning/phases/03-connect-read/03-01-PLAN.md"
  - ".planning/phases/03-connect-read/03-01-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-02-PLAN.md"
  - ".planning/phases/03-connect-read/03-02-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-03-PLAN.md"
  - ".planning/phases/03-connect-read/03-03-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-04-PLAN.md"
  - ".planning/phases/03-connect-read/03-04-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-05-PLAN.md"
  - ".planning/phases/03-connect-read/03-05-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-06-PLAN.md"
  - ".planning/phases/03-connect-read/03-06-SUMMARY.md"
  - ".planning/phases/03-connect-read/03-07-PLAN.md"
  - ".planning/phases/03-connect-read/03-07-SUMMARY.md"
  - "dev/compose.yml"
  - "dev/seed.py"
  - "scripts/smoke_pty.py"
  - "src/api/map.rs"
  - "src/api/mod.rs"
  - "src/api/wire.rs"
  - "src/app/command.rs"
  - "src/app/history.rs"
  - "src/app/sidebar.rs"
  - "src/app/update.rs"
  - "src/config/connection.rs"
  - "src/config/mod.rs"
  - "src/config/paths.rs"
  - "src/effect.rs"
  - "src/lib.rs"
  - "src/redaction.rs"
  - "src/ui/messages.rs"
  - "src/ui/status.rs"
covered_digest: "v4:sha256:ae3250c75a79c7db886f800016fb60126de2e6a1bdff7c9e5bbfaf9502c59dcb"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "A6: judge the sidebar folder layout on the real servers (Unread, Folders with Favorites and Work, Recent)"
    expected: "The author finds the grouping and ordering usable for daily use"
    why_human: "Product/layout judgment; automated checks only prove the rooms are listed and grouped as designed"
  - test: "A10 / D-15: judge the wording of system lines ('* joined the room'), attachment tags ('[attachment: name]') and '[encrypted message]'"
    expected: "Wording is acceptable and the dim system lines are readable"
    why_human: "Wording and styling are subjective"
  - test: "D6: with the real binary in a real 80-column terminal, check the status bar when a failure text and a config warning are shown together (and the http-to-remote warning)"
    expected: "The chmod 600 warning and the failure reason are not clipped to the point of being unreadable"
    why_human: "Visual clipping at a real terminal width; also the subject of review item IN-04"
  - test: "End-of-phase UAT (plan 03-07 Task 2 human-check): set -a; . dev/state/rockettui-dev/.env.local; set +a; cargo run, then walk the five ROADMAP criteria: env-var auth and config.toml auth at chmod 600; sidebar lists channels, private groups and DMs and Enter opens one; k/gg pages back to 'Beginning of history' in rt-public; 'secrets' shows 'encrypted, not supported'; chmod 644 on config.toml warns in the status bar and on exit; the token never shows in scrollback after :q"
    expected: "All five criteria hold when driven by a human in a real terminal against the running 8.9.1 stack"
    why_human: "DLVR-01 is defined by the author's own use against a real server; deferred by the plans to the end-of-phase UAT"
---

# Phase 3: Connect & Read Verification Report

**Phase Goal:** User authenticates with a PAT and reads real rooms and history from a local Docker Rocket.Chat
**Verified:** 2026-11-17
**Status:** human_needed
**Re-verification:** No, initial verification

Automated evidence supports every ROADMAP success criterion. No gap was found. The status is `human_needed` only because the author's own judgment items (A6, A10/D-15, D6) and the end-of-phase UAT the plans deferred are still open, and these must not be recorded as verified.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User supplies user id + token (env var or config) and the app authenticates | VERIFIED | `config/connection.rs`: env vars `ROCKETTUI_SERVER_URL/USER_ID/TOKEN`, `config.toml` keys, env wins per key (`pick`), `Secret` wrapper with redacting Debug and no Display. `lib.rs::run` builds `api::Client::new(connection)` and pushes `Effect::LoadRooms` before the first draw. `api/mod.rs::Client::with_timeouts` sets `X-User-Id` and `X-Auth-Token` as sensitive headers, `redirect::Policy::none()`, 5 s connect and 15 s total timeouts. `status_error` maps 401/403/404/429/3xx/5xx first. Tests `tracer_rooms_end_to_end`, `wrong_credentials_are_unauthorized`, `rooms_load_all_or_nothing`, `redirect_is_not_followed` pass. Smoke `connect_read`, `config_creds`, `unauthorized`, `missing_creds` and `docker_read` print SMOKE OK. `docker_read` runs against the live 8.9.1 server (127.0.0.1:3100, version "8.9") and lists the seeded rooms. |
| 2 | Sidebar lists real channels, private groups and DMs, and user can open one | VERIFIED | `Client::load_rooms` joins `subscriptions.get` and `rooms.get` on the room id (+ best-effort `users.getPreferences`), `map::rooms` and `map::folders` produce rooms and Favorites/category folders; `RoomsLoaded` and `FoldersLoaded` reach `app.rooms` and `app.folders` through the reducer. `sidebar.rs` Select on a room row calls `app.open_room` then `history::on_open`, which pushes `Effect::LoadHistory` (groups, channels or im routing by `history_method`). `docker_read` lists rt-public, rt-private, alice, secrets, rt-team, rt-discussion, Favorites and the custom categories, confirms the closed `bob` DM is not listed, and opens rooms with Enter. `docker_fixtures_rooms` and `docker_fixtures_folders` pass against fixtures captured from the live server. |
| 3 | User scrolls the message history of the opened room | VERIFIED | `Client::load_history` (roomId + count, `latest` sent verbatim) feeds `history::on_loaded`; `maybe_load_older` fires on the oldest loaded message, one request in flight, de-duplicated by id, cursor and screen row preserved (`older_prepend_keeps_cursor`, `older_dedupe`, `older_page_trigger`, `older_failure_allows_retry`), `Beginning of history` shown when exhausted. `docker_read` draws the newest page (message 129 and the attachment tag), pages back with `gg` until the join system line appears, then sees `Beginning of history`. Alice DM shows messages. |
| 4 | Encrypted rooms show "encrypted, not supported" | VERIFIED | `history::on_open` returns early for `room.encrypted` (never fetched; test `encrypted_never_loads`). `ui/messages.rs` renders the fixed placeholder with precedence over Loading and Failed (`placeholder_encrypted_52x20`, `placeholder_precedence`). The encrypted flag comes from `rooms.get` only and a subscription without a `rooms.get` record is skipped, so the list is never applied without it. `docker_read` opens the real `secrets` room and sees the text. |
| 5 | App warns about a config file with credentials that is not chmod 600, and logs contain no token or message content | VERIFIED (automated); human UAT pending | `paths::file_mode` reads the mode only when config.toml was read; `connection::warnings` warns only when the token came from the file and `mode & 0o077 != 0`, with fixed text, and an `http://` non-loopback warning. Warnings render on every frame in `ui/status.rs` and are printed to stderr after terminal restore in `main.rs`. Tests `perm_warning_rules`, `perm_mode_recorded` and smoke `perm_warning` pass. Logging: the crate has no logging framework, no `tracing` or `log` dependency, no file writes, and the only `eprintln!` calls are the fixed-text warnings and the `io::Error` of the run loop. `redaction::sentinel_sweep` (Debug and Display of every secret or message-holding type, hostile-text rendering) passes. `reqwest::Error` is never formatted. `docker_read` asserts the token never appears in terminal output. |

**Score:** 5/5 truths verified (0 present, behavior-unverified). Truth 5 and the DLVR-01 "own use" half stay open for the human UAT below.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/api/mod.rs` | REST client, error mapping | VERIFIED | 971 lines, substantive, used by `lib.rs` and `effect.rs` |
| `src/api/map.rs`, `src/api/wire.rs` | Wire to domain mapping | VERIFIED | fixture-driven tests including docker-captured fixtures |
| `src/effect.rs` | Effect outbox and spawn | VERIFIED | `LoadRooms` and `LoadHistory`, fail-closed without a client; drained in `lib.rs` |
| `src/app/history.rs` | Per-room history state and paging | VERIFIED | wired from `sidebar.rs` (open), `update.rs` (motion, results) and `command.rs` (`:reconnect`) |
| `src/config/connection.rs`, `config/paths.rs`, `config/mod.rs` | Credentials, warnings | VERIFIED | `config::load()` requires the connection; warnings flow into `App.warnings` |
| `src/ui/messages.rs`, `src/ui/status.rs` | Placeholders, warnings | VERIFIED | rendered every frame |
| `dev/compose.yml`, `dev/seed.py`, `dev/README.md` | Local Docker server and seed | VERIFIED | tracked; `dev/state/` git-ignored and holds no tracked file; `seed.py check` reports 8 subscriptions (alice, bob, general, rt-discussion, rt-private, rt-public, rt-team, secrets) |
| `scripts/smoke_pty.py` | pty smoke harness | VERIFIED | 12 scenarios, all re-run by me |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `lib.rs::run` | `api::Client` | built from `config.connection` before `App::with_config` consumes the config (token never reaches `App`) | WIRED |
| `App.effects` | `effect::spawn` | drained once per loop iteration | WIRED |
| `effect::run` | `Action::RoomsLoaded/FoldersLoaded/HistoryLoaded/Failed` | channel to `update()` | WIRED |
| sidebar Select | `history::on_open` | `sidebar.rs` | WIRED |
| messages motion | `history::maybe_load_older` | `update.rs` `apply_motion` | WIRED |
| `connection::warnings` | `ui::status` | `Config.warnings` to `App.warnings` | WIRED |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|----------|------|--------|-----------|--------|
| Sidebar | `app.rooms`, `app.folders` | `subscriptions.get` + `rooms.get` + `users.getPreferences` over HTTP | Yes (live 8.9.1 in `docker_read`) | FLOWING |
| Messages pane | `app.messages[room]` | `*.history` over HTTP | Yes (message 129 and join line seen in `docker_read`) | FLOWING |
| Status bar warnings | `app.warnings` | file mode of config.toml | Yes (`perm_warning` smoke) | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole suite | `cargo test` | 363 passed, 0 failed | PASS |
| Lints and format | `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` | clean | PASS |
| 12 pty scenarios | `python3 -I scripts/smoke_pty.py target/debug/rockettui <scenario>` | all 12 print `SMOKE OK`, including `docker_read` | PASS |
| Live server reachable | `curl http://127.0.0.1:3100/api/info` and `dev/seed.py check` | version 8.9, `ok: 8 subscriptions` | PASS |

The 9.0.0 compatibility pass (stack `rockettui-dev9`) was removed after it ran, so I could not re-run it. It rests on 03-07-SUMMARY.md only and is not part of the roadmap criteria.

### Probe Execution

Step 7c: SKIPPED. No `probe-*.sh` files exist and the plans declare none; the pty smoke harness above is the runnable proof.

### Requirements Coverage

All eight IDs from the phase are claimed by at least one PLAN frontmatter `requirements` field. No orphaned requirement: REQUIREMENTS.md maps exactly these eight to Phase 3.

| Requirement | Source plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| AUTH-01 | 03-01, 03-05, 03-06, 03-07 | PAT (user id + token) from env var or config file | SATISFIED | truth 1; `config_creds` and `connect_read` smoke |
| AUTH-02 | 03-05 | Warn if a config holding credentials is not chmod 600 | SATISFIED | truth 5; `perm_warning_rules`, `perm_warning` smoke. Real-terminal check in human UAT |
| AUTH-03 | 03-01, 03-05, 03-06 | Token and message contents never logged | SATISFIED | truth 5; no logging code, `sentinel_sweep`, token absent from pty output |
| ROOM-01 | 03-01, 03-07 | Channels, private groups, DMs in the sidebar | SATISFIED | truth 2 |
| ROOM-02 | 03-03, 03-06, 03-07 | Select a room to open it | SATISFIED | truth 2 |
| ROOM-03 | 03-03, 03-07 | E2EE rooms show "encrypted, not supported" | SATISFIED | truth 4 |
| MSG-01 | 03-03, 03-04, 03-07 | View and scroll message history | SATISFIED | truth 3 |
| DLVR-01 | 03-02, 03-07 | Works against a local Docker Rocket.Chat | SATISFIED (automated); author UAT pending | `docker_read` against live 8.9.1; REQUIREMENTS.md already ticks it, but 03-07 defines it by the author's own use, which is the open UAT item |

### Anti-Patterns Found

None blocking. No `TBD`, `FIXME`, `XXX`, `TODO`, `todo!` or `unimplemented!` in any phase file. The token handling is confined to `Secret` and `Client`.

### Code Review Cross-Reference (03-REVIEW.md, advisory, 0 critical / 5 warning / 6 info)

I do not contradict the review. All 11 findings are `open` in 03-REVIEW-DISPOSITION.md. I confirmed WR-04 against the code (`src/api/wire.rs` has no `success` check). None of them defeats a ROADMAP criterion, but several bear on the daily-use goal and should be triaged before Phase 6:

- WR-03: Enter in Insert mode appends a local "me" message that looks sent but is never sent. It is Phase 4's feature, but it is visible against a real server today.
- WR-04: a wrong HTTP 200 JSON body becomes Ready with no rooms, or a permanently empty room.
- WR-05: `:reconnect` has no in-flight guard, so a stale load can overwrite a newer one.
- WR-01 and WR-02 concern dev tooling only (`dev/seed.py`, `smoke_pty.py`). IN-04 overlaps the human D6 item.

### Gaps Summary

No gaps. The phase goal is achieved as far as automation can show: authentication, room list, history paging, the encrypted placeholder, the permission warning and the no-logging property are implemented, wired and exercised end to end against a live local Rocket.Chat 8.9.1. Remaining items are the author's judgments (A6, A10/D-15, D6) and the deferred end-of-phase UAT, listed under `human_verification`.

---

_Verified: 2026-11-17_
_Verifier: Claude (gsd-verifier)_
