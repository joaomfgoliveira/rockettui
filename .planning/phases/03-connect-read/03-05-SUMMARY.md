---
phase: 03-connect-read
plan: 05
subsystem: config
tags: [config, toml, credentials, pat, permissions, security, ratatui, pty-smoke]

requires:
  - phase: 03-connect-read
    provides: "ConnEnv/EnvValue/Secret/Connection, parse_server_url and the env-only resolve (plan 03-01); the pty mock and smoke scenarios (03-01..03-04)"
provides:
  - "FileConn and connection::parse_file: server_url, user_id and token read from config.toml through Reader::string, type-only messages, empty string = unset"
  - "connection::resolve(env, file, require, out): per-key env-over-file precedence, CredSource::Env or File for the token, missing errors naming config.toml and the variable"
  - "server_url, user_id and token are known config.toml keys (Phase 2 D-12 deferral closed)"
  - "Sources.config_mode (read once with the file, cfg(unix), read-only) and a hand-written Debug for Sources that prints byte counts only"
  - "connection::warnings, Config.warnings, App.warnings: chmod 600 warning (token from file, mode & 0o077 != 0) and cleartext-http warning (non-loopback host)"
  - "Persistent ' warning: ' spans on the status row and 'rockettui: warning: <text>' lines on stderr after the terminal is restored"
  - "pty scenarios config_creds and perm_warning; make_config_dir/spawn accept a modes dict"
affects: [03-06 redaction and hostile-text sweep, 03-07, 06 company-server first use]

actuals:
  tokens: 10629
  tasks: 2
  commits: 3
plan_head_before: df69d276985238d6441bdabf64fbdfe16859c231
plan_head_after: bf010f09c65452e02971a0dd985f9b1dcd48c2df

tech-stack:
  added: []
  patterns:
    - "Per-key precedence: a small pick() decides env vs file for each of the three values, so the origin of the token is known without a second pass"
    - "Runtime notices (Config.warnings -> App.warnings) are separate from config Report warnings; the renderer draws them every frame so a key press never clears them"
    - "File metadata is read in the IO shell at the same moment as the text; the pure core receives the mode as data (Sources.config_mode)"

key-files:
  created: []
  modified:
    - src/config/connection.rs
    - src/config/mod.rs
    - src/config/paths.rs
    - src/config/settings.rs
    - src/app/mod.rs
    - src/ui/status.rs
    - src/main.rs
    - scripts/smoke_pty.py

key-decisions:
  - "FileConn carries an extra invalid list (keys present with a non-string value) so a wrong type is reported once and not followed by a second 'missing' error for the same key"
  - "A value that is set anywhere (env, file, or present with a wrong type) counts as 'given', so a partial setting is an error for the other keys even when the connection is not required"
  - "A bad file URL is reported only when the file URL is the one in use; a set env URL silently overrides it"
  - "Warnings follow the connection span on the status row and precede pending keys and status_msg, so on a very narrow terminal the later spans are the ones clipped"

patterns-established:
  - "No config value (token, user id) is ever placed in a Problem, a warning, Sources Debug or stderr; token_never_echoed covers the Report Display, the Report Debug and the Sources Debug, including an unterminated string"
  - "Loading stays read-only: the only filesystem calls in non-test paths.rs code are metadata and read_to_string"

requirements-completed: [AUTH-01, AUTH-02, AUTH-03]

coverage:
  - id: D1
    description: "The real binary authenticates and lists rooms with server_url, user_id and token in a 0600 config.toml and no ROCKETTUI_* variable"
    requirement: AUTH-01
    verification:
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui config_creds"
        status: pass
      - kind: unit
        ref: "src/config/connection.rs#file_values"
        status: pass
    human_judgment: false
  - id: D2
    description: "Each value is resolved on its own, env over file, with the token origin recorded; missing and invalid values are config errors naming the key and both places and never echoing a value"
    requirement: AUTH-01
    verification:
      - kind: unit
        ref: "src/config/connection.rs#env_overrides_per_key, file_url_rules, type_and_empty, missing_names_both_places"
        status: pass
      - kind: unit
        ref: "src/config/mod.rs#server_url_is_known_key"
        status: pass
    human_judgment: false
  - id: D3
    description: "Neither Sources Debug nor the Report built from a config.toml holding a sentinel token contains the token"
    requirement: AUTH-03
    verification:
      - kind: unit
        ref: "src/config/mod.rs#token_never_echoed"
        status: pass
    human_judgment: false
  - id: D4
    description: "A config.toml holding the token with any group or other bit starts the app with a persistent status-bar warning repeated on stderr after exit; 0600, env tokens and no connection give none; config.toml is never chmodded"
    requirement: AUTH-02
    verification:
      - kind: unit
        ref: "src/config/mod.rs#perm_warning_rules, src/config/paths.rs#perm_mode_recorded, src/ui/status.rs#warning_persists"
        status: pass
      - kind: e2e
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui perm_warning"
        status: pass
    human_judgment: false
  - id: D5
    description: "http:// to a non-loopback host adds a persistent cleartext warning; http to localhost, 127.0.0.0/8 and ::1 and https give none"
    requirement: AUTH-02
    verification:
      - kind: unit
        ref: "src/config/mod.rs#cleartext_warning"
        status: pass
    human_judgment: false
  - id: D6
    description: "Wording and placement of the warnings on a real 80-column terminal with a room open (the status row is shared with the room name, connection state and pending keys)"
    verification: []
    human_judgment: true
    rationale: "Whether the warning stays readable next to the other status spans is a usability judgment no test asserts"

duration: 6min
completed: 2026-11-17
status: complete
---

# Phase 3 Plan 05: Config credentials and warnings Summary

**server_url, user_id and token now load from config.toml with per-key environment override, and a group-readable token file or a cleartext http URL to a remote host raises a persistent status-bar warning repeated on stderr after exit.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-11-17T11:05:44Z
- **Completed:** 2026-11-17T11:10:32Z
- **Tasks:** 2 (3 commits: tracer, TDD RED, TDD GREEN)
- **Files modified:** 8

## Accomplishments

- Tracer: the real binary authenticates and lists the mock's rooms from a 0600 config.toml with no ROCKETTUI_* variable (`config_creds`). Per key, a set non-empty variable beats the file value (D-01); the token's origin is `CredSource::Env` or `File`.
- config.toml `server_url` is validated by the same rules as the env value and a bad one is reported at its line:col with key `server_url`; wrong types name only the type; an empty string counts as unset; a missing value names both places (`missing: set token in config.toml or ROCKETTUI_TOKEN`).
- `Sources` has a hand-written `Debug` (byte counts, env presence, file mode). `token_never_echoed` shows the sentinel token is absent from the Report Display, the Report Debug and the Sources Debug, including for an unterminated string.
- D-04: the file mode is read once with the file; when the token came from the file and `mode & 0o077 != 0` the app starts and shows `config.toml holds the token but is mode 0644: chmod 600 it` on every frame and again on stderr after the terminal is restored. Env tokens, 0600 files and no connection give no warning; the cleartext-http warning fires only for non-loopback hosts.

## Task Commits

1. **Task 1 (tracer): config.toml credentials with per-key env override** - `33ba4b9` (feat)
2. **Task 2 RED: failing tests for the permission and cleartext warnings** - `193ac69` (test)
3. **Task 2 GREEN: permission and cleartext warnings, status bar and stderr** - `bf010f0` (feat)

**Plan metadata:** the docs commit that carries this SUMMARY, STATE.md, ROADMAP.md and REQUIREMENTS.md.

## Files Created/Modified

- `src/config/connection.rs` - `FileConn`, `parse_file`, `pick`, the new `resolve` signature, `warnings`, loopback check, tests
- `src/config/mod.rs` - `Sources.config_mode`, hand-written `Sources` Debug, `Config.warnings`, `server_url_is_known_key`, `token_never_echoed`, `perm_warning_rules`, `cleartext_warning`
- `src/config/paths.rs` - `file_mode` (cfg(unix)), `read_sources` fills `config_mode`, `perm_mode_recorded`
- `src/config/settings.rs` - `server_url`, `user_id`, `token` in `KNOWN_KEYS`
- `src/app/mod.rs` - `App.warnings` copied in `with_config`
- `src/ui/status.rs` - persistent warning spans with `status.error`, `warning_persists`
- `src/main.rs` - warnings cloned before `run`, printed after `tui::restore()`
- `scripts/smoke_pty.py` - `make_config_dir(files, modes)`, `spawn(config_modes=)`, scenarios `config_creds` and `perm_warning`

## TDD Gate Compliance

Task 2 (tdd="true"):

- **RED** (`193ac69`): `perm_mode_recorded`, `perm_warning_rules`, `cleartext_warning` and `warning_persists` were committed with compile-only stubs (empty `warnings()`, the new fields, no mode read, no status spans). All four ran and failed on their planned assertions: `config_mode` was `None` instead of `Some(0o644)`, the warning list was empty instead of one entry, the cleartext list was empty instead of the fixed text, and the status row showed ` NORMAL  -` with no warning. Semantic assessment: each target executed and failed for the intended reason (missing behavior, not a syntax, import or fixture fault). `gsd_run check tdd-red-evidence` was not run: the classifier does not parse the `cargo test` report format, so the assessment above is the record.
- **GREEN** (`bf010f0`): all four pass; the full suite is 346 tests.
- **REFACTOR:** none needed.

Task 1 is `type="tracer"`; its tests were written together with the implementation and committed once as the plan prescribes for a tracer. Tracer feedback gate (end-of-phase mode, automated-only verify): the verify commands passed and were re-run at the end of the plan, "Tracer verified end-to-end - expanding".

## Decisions Made

- `FileConn` gained an `invalid: Vec<&'static str>` field beyond the plan's three (see Deviations).
- A partial setting is an error for the remaining keys even with `require` false, whether the given value lives in the environment, in config.toml or is present with the wrong type.
- A bad config.toml URL is only an error when the file URL is the one in use.
- Warning spans sit after the connection span and before pending keys and `status_msg`. At 80 columns with a room open (room name plus the 58-character chmod text) the tail of the status row is clipped, so the end of the warning or the status message can be cut; wider terminals show everything. No width-aware shortening was added (plan left layout to discretion).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Duplicate error for a wrongly typed value**
- **Found during:** Task 1 (resolve design)
- **Issue:** With `token = 5` and the connection required, `parse_file` reports the type error and `resolve` would then also report `missing: set token ...` for the same key, which is wrong and noisy.
- **Fix:** `FileConn` records the keys it rejected in a new `invalid` list; `resolve` neither reports them missing nor treats them as unset, but still counts them as "given" for the partial-setting rule. Covered by `type_and_empty`.
- **Files modified:** src/config/connection.rs
- **Committed in:** 33ba4b9

---

**Total deviations:** 1 auto-fixed (1 bug). **Impact on plan:** one extra field on `FileConn`; no behavior outside the planned error messages changed.

## Issues Encountered

None. No `ROCKETTUI_*` variable or real config was touched; all smoke scenarios use a temp XDG_CONFIG_HOME and the stdlib mock.

## Known Stubs

None.

## Threat Flags

None. The new surface (token read from config.toml, config.toml mode read, http warning) is exactly the plan's T-03-19..T-03-22.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- ROADMAP criteria 1 (credentials from env or config file) and 5 (first half: chmod warning) are met for Phase 3; AUTH-03 for config values is covered, and the hostile-text and log-redaction sweep is plan 03-06.
- Assumption A-D04 stands as decided: an env token that overrides a token also present in config.toml produces no permission warning (the author may want one whenever the file holds a token).
- Smoke scenarios `config_creds`, `perm_warning`, `missing_creds`, `config_error` and `connect_read` all print SMOKE OK.

## Self-Check: PASSED

- Files present: src/config/connection.rs, src/config/paths.rs, src/config/mod.rs, src/ui/status.rs, scripts/smoke_pty.py
- Commits present on HEAD: 33ba4b9, 193ac69, bf010f0
- `cargo test` (346 passed), `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean; five smoke scenarios SMOKE OK

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*
