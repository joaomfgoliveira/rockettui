---
phase: 02-configuration-system
verified: 2026-11-16T12:00:00Z
status: passed
score: 5/5 roadmap success criteria verified automatically; 1 plan truth present-but-behavior-unverified (key_timeout_ms wall-clock); 7/7 requirements satisfied
covered_files:
  - .planning/phases/02-configuration-system/02-01-PLAN.md
  - .planning/phases/02-configuration-system/02-01-SUMMARY.md
  - .planning/phases/02-configuration-system/02-02-PLAN.md
  - .planning/phases/02-configuration-system/02-02-SUMMARY.md
  - .planning/phases/02-configuration-system/02-03-PLAN.md
  - .planning/phases/02-configuration-system/02-03-SUMMARY.md
  - .planning/phases/02-configuration-system/02-04-PLAN.md
  - .planning/phases/02-configuration-system/02-04-SUMMARY.md
  - .planning/phases/02-configuration-system/02-05-PLAN.md
  - .planning/phases/02-configuration-system/02-05-SUMMARY.md
  - Cargo.toml
  - scripts/smoke_pty.py
  - src/action.rs
  - src/app/messages.rs
  - src/app/mod.rs
  - src/app/sidebar.rs
  - src/app/update.rs
  - src/config/diag.rs
  - src/config/keymap.rs
  - src/config/mod.rs
  - src/config/paths.rs
  - src/config/settings.rs
  - src/config/theme.rs
  - src/input/actions.rs
  - src/input/keymap.rs
  - src/input/mod.rs
  - src/input/trie.rs
  - src/lib.rs
  - src/main.rs
  - src/test_util.rs
  - src/ui/error_screen.rs
  - src/ui/input.rs
  - src/ui/layout.rs
  - src/ui/messages.rs
  - src/ui/mod.rs
  - src/ui/palette.rs
  - src/ui/sidebar.rs
  - src/ui/status.rs
  - src/ui/timefmt.rs
  - src/ui/wrap.rs
covered_digest: "v4:sha256:f8debfd9f5947d661da8cc86ec6e44e95005a3dca77b88e3352dd640eba148db"
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "key_timeout_ms from config.toml replaces the fixed 1 s pending-key timeout of the run loop through App::key_timeout()"
    test: "Set key_timeout_ms = 3000 in config.toml, run the binary, press g once and watch the status bar"
    expected: "The pending g stays in the status bar for about 3 seconds instead of 1, then clears"
    why_human: "App::key_timeout() is unit-tested (250 ms -> Duration) and is wired at src/lib.rs:108, but no test exercises the wall-clock deadline in the run loop"
human_verification:
  - test: "Plan 02-01 human-check: with config.toml `sidebar_width = 99`, `timestamp_format = \"%Q\"`, `bogus = 1` and keymap.toml `[normal`, run `XDG_CONFIG_HOME=/tmp/rt-uat cargo run`, press any key, then `echo $?`"
    expected: "The screen lists the two config.toml errors and the keymap.toml syntax error with line:col, plus bogus as a warning; any key quits; the shell shows the same list on stderr and exit status 1; the terminal is fully restored"
    why_human: "Readability of the error screen is a human judgment (the pty smoke scenario config_error proves the behavior, not the readability)"
  - test: "Plan 02-02 human-check: config.toml with `sidebar_width = 40`, `message_layout = \"two_line\"`, `timestamp_format = \"%Y-%m-%d %H:%M\"`, `key_timeout_ms = 3000`; run, open general with Enter, press g once and watch the status bar"
    expected: "The sidebar is visibly wider; messages show two-line headers with a full local date and time; the pending g stays about 3 seconds instead of 1; :q quits cleanly"
    why_human: "Look and feel and wall-clock timing in a real terminal"
  - test: "Plan 02-03 human-check: `keymap_preset = \"vim-extras\"` in config.toml, `[normal]` with `J = \"move_down\"` and `K = \"move_up\"` in keymap.toml; press Tab, Shift-Tab, J and K, quit with :q. Then add `g = \"goto_top\"` to [normal] and relaunch"
    expected: "Tab and Shift-Tab move the thick border between panes; J and K move the sidebar cursor; with g added the app refuses to start and the error screen says g is a prefix of gg with keymap.toml line:col"
    why_human: "Key feel in a real terminal and readability of the clash message"
  - test: "Plan 02-04 human-check: theme.toml with [sidebar.cursor] bg = \"#3a3a3a\", [status.mode] fg = 208 and modifiers = [\"bold\"], [border.focused] fg = \"cyan\" and type = \"rounded\", [markers] unread = \"● \"; move with j/k and Ctrl-w l; then set fg = 300 under [status.mode]"
    expected: "The cursor row, mode badge, focused border and unread markers change as written and read rooms stay aligned; with fg = 300 the app refuses to start and the error screen names theme.toml, the line and status.mode.fg"
    why_human: "Colors and look in a real terminal theme cannot be asserted by buffer tests"
  - test: "Plan 02-05 human-check: theme.toml with `author_palette = [\"#ff8800\", \"magenta\"]`, [messages.cursor] bg = \"#303030\" and modifiers = [], [messages.time] fg = \"light-blue\"; open general and move with j/k; then set `author_palette = []` and relaunch"
    expected: "Authors alternate between orange and magenta, the cursor row is a grey bar instead of reversed video, times are light blue; with an empty palette the app refuses to start and names theme.toml author_palette"
    why_human: "Colors in a real terminal theme cannot be asserted by buffer tests"
---

# Phase 2: Configuration System Verification Report

**Phase Goal:** The TUI is customizable through three TOML files in `~/.config/rockettui/`, with built-in defaults
**Verified:** 2026-11-16
**Status:** human_needed (no gaps; automated evidence supports every roadmap success criterion; visual and wall-clock checks the plans reserved for a human could not be run here)
**Re-verification:** No, initial verification

## Goal Achievement

### Roadmap Success Criteria

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | App starts normally with no config files present | VERIFIED | `src/main.rs` calls `config::load()`; `paths::config_dir` returning `None` gives `Config::default()` (`src/config/mod.rs:77`). Tests `config::tests::absent_files_default`, `empty_files_are_absent`, `config::keymap::tests::absent_and_empty_give_preset` pass. pty smoke `quit`, `insert`, `panic` pass against the built binary with the isolated (empty) XDG dir; the 7 Phase 1 snapshots are unchanged (268 tests pass) |
| 2 | User changes the keymap preset and rebinds a key in `keymap.toml`, and behavior changes | VERIFIED | `keymap::build(settings.keymap_preset, keymap_toml)` is called in `load_from_sources`; result stored in `App.keymap` (`src/app/mod.rs:119`). Pty smoke `config_keymap` passes: `;` rebound to `enter_command` enters COMMAND and `;q<CR>` exits 0. `vim_extras_tab_moves_focus_rendered`, `preset_vim_extras_adds_tab`, `tracer_rebind_semicolon` pass |
| 3 | User edits `theme.toml` colors and the UI look changes | VERIFIED | `theme::parse` -> `Config.theme` -> `App.theme` (`src/app/mod.rs:120`); sidebar/status/input/borders/messages draw from `app.theme`. `tracer_status_mode_from_dir` (Indexed(208) on Rgb(16,32,48), italic), `tracer_author_palette_from_dir`, `every_element_is_themeable` (22 paths) pass. Real-terminal appearance is a human item |
| 4 | User sets sidebar width, timestamp format and history size in `config.toml` | VERIFIED (history_size is stored only) | `sidebar_width` drives `App::areas()` for renderer and reducer (`layout_80x24_sidebar_40` snapshot, `viewport_follows_sidebar_width`); `timestamp_format` drives `format_ts` through the shared `App::row_ctx` (`layout_80x24_custom_config` snapshot); `history_size` is parsed, range-checked (1-1000, `bounds_inclusive`) and stored in `Settings`, but no code reads it yet. That is locked decision D-13 (Phase 3 consumes it; there is no history to bound until then). Setting it is possible and validated, so the criterion as worded holds |
| 5 | A malformed config entry shows a clear error instead of a crash | VERIFIED | Pty smoke `config_error` passes against the binary: bad `sidebar_width`, bad `timestamp_format` and a keymap.toml syntax error all appear on the error screen with file/key names, a key press exits with code 1, alt screen is left, and the report is printed to stderr; app never reaches NORMAL. `error_screen_80x24`, `hostile_text_sanitized`, `truncates_with_more_line`, `tiny_sizes_no_panic` pass |

**Score:** 5/5 roadmap criteria verified. 1 plan truth (key_timeout_ms wall-clock) is present and wired but behavior-unverified.

### Plan must-have truths (spot-checked against code and tests)

| Truth | Status | Evidence |
|-------|--------|----------|
| XDG resolution (absolute `$XDG_CONFIG_HOME`, else `$HOME/.config`; relative/empty ignored) | VERIFIED | `src/config/paths.rs`; `paths::config_dir(env XDG_CONFIG_HOME, HOME)` called from `load()`; path tests pass |
| Every problem collected in one pass across the three files, sorted; unknown keys warn only next to an error | VERIFIED | `load_from_sources` accumulates into one `Vec<Problem>` -> `Report::new`; smoke `config_error` shows several problems at once |
| Bad file -> refuse to start, exit 1, no continue-with-defaults | VERIFIED | `src/main.rs` returns `ExitCode::from(1)` after `show_config_errors` and `eprint!` |
| keymap.toml: curated actions, `none`, `cmd:`, duplicate/prefix/lockout checks | VERIFIED | `src/config/keymap.rs`, `src/input/actions.rs` (`ACTION_NAMES`); plan 03 artifacts 5/5 and key links 5/5 pass; unit tests pass |
| theme.toml: D-08 colors, D-09 modifiers, border types, markers, author_palette 1-64 | VERIFIED | `src/config/theme.rs` (1135 lines, tests pass); `golden_author_colors` and `palette_avoids_clashes` still pass against defaults |
| Binary renders local time; tests stay UTC | VERIFIED (code) | `App::with_config` sets `Zone::Utc`; `Zone::Local` is set once in `run()` per plan; tests use UTC |
| `key_timeout_ms` replaces the 1 s timeout in the run loop | PRESENT_BEHAVIOR_UNVERIFIED | `src/lib.rs:108` uses `app.key_timeout()`; accessor unit-tested; no test exercises the run-loop deadline. Routed to human verification |

### Required Artifacts and Key Links

`verify.artifacts` and `verify.key-links` per plan: 02-01 7/7 and 7/7, 02-02 6/6 and 5/5, 02-03 5/5 and 5/5, 02-04 3/3 and 5/5, 02-05 3/3 and 3/3. All files exist, are substantive (config modules 154 to 1135 lines, no stubs) and are wired (`main.rs` -> `config::load` -> `Config` -> `App::with_config` -> reducer/renderer).

### Data-Flow Trace (Level 4)

| Value | Source | Flows to | Status |
|-------|--------|----------|--------|
| `sidebar_width` | `settings::parse` from config.toml | `App::areas()` -> `layout::areas` (view and reducer) | FLOWING |
| `timestamp_format` | `settings::parse` | `App::row_ctx` -> `format_ts` (renderer and row model) | FLOWING |
| `keymap` | `keymap::build(preset, keymap.toml)` | `App.keymap` -> matcher | FLOWING |
| `theme` and `author_palette` | `theme::parse` | `App.theme` -> `RowCtx.theme`, sidebar, status, input, borders | FLOWING |
| `history_size` | `settings::parse` | `Settings.history_size`, no reader | STATIC by design (D-13, consumed in Phase 3) |
| `key_timeout_ms` | `settings::parse` | `App::key_timeout()` -> run loop at `src/lib.rs:108` | FLOWING (timing unverified) |

### Behavioral Spot-Checks and Probes

| Check | Command | Result | Status |
|-------|---------|--------|--------|
| Full test suite | `cargo test` | 268 passed, 0 failed | PASS |
| Lints and format | `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check` | clean | PASS |
| Pty smoke, six scenarios | `python3 -I scripts/smoke_pty.py target/debug/rockettui <quit, panic, insert, config_layout, config_keymap, config_error>` | all `SMOKE OK` | PASS |

### Requirements Coverage

Every ID in the plans' `requirements:` frontmatter is accounted for in REQUIREMENTS.md; no ORPHANED requirements map to Phase 2.

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| CONF-01 | 02-01 | Load the three files from `~/.config/rockettui/`, honoring `$XDG_CONFIG_HOME` | SATISFIED | `config::paths`, `config::load` |
| CONF-02 | 02-01 | Run with defaults when any or all files are absent | SATISFIED | `absent_files_default`, `empty_files_are_absent`, Phase 1 snapshots unchanged |
| CONF-03 | 02-01, 02-03, 02-04 | Invalid entries show a clear error, not a crash | SATISFIED | smoke `config_error`, error screen tests |
| CONF-04 | 02-03 | Pick `vim` or `vim-extras` preset | SATISFIED | `preset_vim_extras_adds_tab`, `vim_extras_tab_moves_focus_rendered` |
| CONF-05 | 02-03 | Rebind keys per mode in `keymap.toml` | SATISFIED | smoke `config_keymap`, `tracer_rebind_semicolon` |
| CONF-06 | 02-04, 02-05 | Colors and styles for every UI element in `theme.toml` | SATISFIED | `every_element_is_themeable` (22 paths), tracer tests |
| CONF-07 | 02-01, 02-02 | History size, timestamp format, sidebar width in `config.toml` (server URL deferred to Phase 3 by D-12) | SATISFIED | settings tests, snapshots; history_size stored for Phase 3 |

### Anti-Patterns Found

None: no TBD/FIXME/XXX/TODO/HACK/`todo!`/`unimplemented!` in `src/` or `scripts/smoke_pty.py`.

### Code review findings (02-REVIEW.md, all dispositions `open`)

None breaks a roadmap success criterion. Recorded as non-blocking observations. WR-01 was reproduced here; WR-02 to WR-04 are taken from the review's own probes (not re-run).

- **WR-01 (reproduced):** `diag::line_col` is O(file size) per problem. A 200 KB `config.toml` (50,000 `a=1` lines plus one bad key) kept the debug binary busy for about 55 s before the terminal started. Pathological input only; ordinary malformed entries report instantly. It does weaken the 1 MiB hardening claim (T-02-03).
- **WR-02:** `sidebar_width = 60` (legal) leaves zero-width messages and input panes on terminals 40 to 60 columns wide, with no explanation.
- **WR-03:** `[command] q = "cancel"` passes the lockout guard but makes `:q` untypeable; a user must have another quit binding or kill the process.
- **WR-04:** the dead-chord warning misses sequences containing `<Esc>` or `<C-c>` after the first chord.
- IN-01 to IN-05: info-level (dropped warnings when otherwise valid, `%.f` renders empty, uncalled `check_syntax`, duplicated `message_layout` field, unreachable fallbacks).

### Process observation

ROADMAP marks Phase 2 `Mode: mvp`, but its goal is not in "As a ..., I want to ..., so that ..." form (`user-story.validate` returns `valid: false`; Phase 1 had the same shape). The MVP User Flow Coverage table was therefore not produced; verification used the standard goal-backward method against the five roadmap success criteria. If strict MVP verification is wanted, run `/gsd-mvp-phase 2` to restate the goal first.

### Human Verification Required

See `human_verification` in the frontmatter: five items from the plans' `<human-check>` blocks (error screen readability, wide sidebar plus two-line plus full timestamp plus 3 s timeout, vim-extras/rebind/prefix-clash, themed chrome colors, author palette and empty-palette refusal). The `key_timeout_ms` wall-clock behavior is also listed as the one behavior-unverified truth.

### Gaps Summary

No gaps. All five roadmap success criteria and all seven CONF requirements have code, wiring and passing automated evidence. The phase remains `human_needed` only because look, feel and timing checks the plans explicitly reserved for a person have not been run.

---

_Verified: 2026-11-16_
_Verifier: Claude (gsd-verifier)_
