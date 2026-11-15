---
phase: "01"
slug: "modal-tui-shell"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-14"
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | built-in `cargo test` (libtest) + `insta` 1.49.0 for render snapshots |
| **Config file** | none — Wave 0 creates `Cargo.toml` (dev-dependency `insta`; snapshots in `src/**/snapshots/`) |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~30 seconds (quick ~5s after first build) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite must be green, plus manual UAT in a real terminal (Ctrl-c in each mode, resize while running, multi-line paste, panic restore)
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

Re-keyed by the planner to the task IDs in `01-0N-PLAN.md` (`{phase}-{plan}-{task}`) after the plan-check iteration 2 restructure (old plan 01-01 split into 01-01 headless core + 01-02 terminal; old 01-02..01-05 renumbered to 01-03..01-06). Each Automated Command is copied verbatim from that task's verify block: the first `<automated>` command, plus a second row for 01-02-03 and 01-06-03 whose pty e2e command is their second `<automated>`. Multiple filters are passed after `--` because `cargo test` accepts only one positional filter before it.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | NAV-01, NAV-02, NAV-03 | T-01-07, T-01-SC1 | Headless tracer: seed Actions through the real reducer select `general`, `j`/`k` move the sidebar selection, `:q<CR>` sets `should_quit`; only audited crates, no networking crate, resolved tokio has no net feature; Cargo.lock listed and committed (`git ls-files --error-unmatch` / `git cat-file -e HEAD:Cargo.lock` gate); no clock read under src/app or src/input | unit (reducer tracer) | `cargo build && cargo test --lib -- app::update::tests::tracer_seed_keys_quit app::update::tests::motion_j_k_sidebar app::command::tests::colon_q_quits` | ❌ W0 (created by 01-01-01) | ⬜ pending |
| 01-01-02 | 01 | 1 | NAV-01 (D-08) | T-01-03 | Full Vim-notation grammar behind every mode key; `Chord`, `Action` and `Message` Debug never contain typed, pasted or message text | unit | `cargo test --lib -- input:: action::tests::debug_redacts data::tests::debug_redacts` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | NAV-01, NAV-03 | T-01-25 | Full D-13 dataset (exactly one empty room, encrypted room has messages); `:` → Command, `Esc`/`Ctrl-c` → Normal, never quits; `:q`/`:quit` quit; `:foo` errors without quitting; empty command no-op; BS on empty returns Normal; command line capped; Resize/Clock only store | unit (reducer) | `cargo test --lib -- data::fake::tests::seed_matches_d13 data::tests::with_fake_data_loads_everything app::update::tests::mode_ app::update::tests::command_ app::update::tests::resize_ app::update::tests::clock_ app::command::tests` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 2 | NAV-01, NAV-03, NAV-04 | T-01-04, T-01-05 | Binary tracer: layout + command row render tests, then the binary under a pty shows `general` before `NORMAL` in its first frame (seed applied before the first draw), accepts `j`/`k`, `:q` exits 0, alternate screen and bracketed paste entered and left | unit + e2e (pty) | `cargo build && cargo test --lib -- ui::layout::tests::areas_80x24 ui::tests::layout_80x24_has_all_panes ui::tests::command_mode_renders_cmdline && python3 -I scripts/smoke_pty.py target/debug/rockettui quit` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 2 | NAV-04 (security) | T-01-01, T-01-02 | Sanitizer strips ESC/CSI/OSC/C1/bidi/NUL and Unicode tag characters (U+E0000..U+E007F), keeps ZWJ / variation-selector / flag emoji, expands tab, normalizes CRLF; hostile names/bodies/status/command text never reach the buffer | unit + render | `cargo test --lib -- ui::sanitize::tests ui::tests::hostile_text_is_sanitized_in_render` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 2 | NAV-04 | T-01-04, T-01-06 | No panic at 0x0, 1x1, 10x3, 39x7, 40x8, 80x24, 120x40, 200x60; `terminal too small` below 40x8; panic restores the terminal before the message | unit | `cargo test --lib -- ui::tests::tiny_sizes ui::layout::tests::areas_tiny` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 2 | NAV-04 | T-01-04 | Deliberate debug panic exits 101 with CSI ?1049l before "panicked at"; `quit` still passes | e2e (pty) | `cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui panic && python3 -I scripts/smoke_pty.py target/debug/rockettui quit` | ❌ W0 | ⬜ pending |
| 01-03-01 | 03 | 3 | NAV-02 | T-01-09, T-01-10 | Trie Exact/Prefix/ExactAndPrefix/None, counts saturate at 9999, timeout fire vs cancel, notation round trip; `Outcome`/`Matcher` Debug never shows a typed char | unit | `cargo test --lib input::` | ❌ W0 | ⬜ pending |
| 01-03-02 | 03 | 3 | NAV-01, NAV-02 | T-01-09, T-01-11 | `5j` == `jjjjj`, huge count clamps, pending state tracked, KeyTimeout clears; one reducer call site; no clock read under src/app or src/input (grep gate in the plan) | unit (reducer) | `cargo test --lib -- app::update::tests::motion_ app::update::tests::pending_ app::update::tests::counted_` | ❌ W0 | ⬜ pending |
| 01-03-03 | 03 | 3 | NAV-02 | T-01-12 | `<C-w>h/j/l` focus with thick border, `open_room` focuses messages, status bar shows mode/room/pending, full Normal keymap parses and round-trips | unit + render | `cargo test --lib -- app::update::tests::focus_ ui::tests::focused_pane_has_thick_border ui::tests::status_shows_mode_room_and_pending input::keymap::tests input::key::tests::format_round_trip` | ❌ W0 | ⬜ pending |
| 01-04-01 | 04 | 4 | NAV-02 | T-01-15 | Sidebar sections (each room has one home section, Unread wins: author decision; a collapsed folder's rooms are hidden, never moved to Recent), folds, h/l fold/unfold folder rows only and are no-ops on room rows (author decision D-19), cursor never on headers, motions/counts clamp, Enter opens the room and focuses messages (D-02) | unit | `cargo test --lib app::sidebar::tests` | ❌ W0 | ⬜ pending |
| 01-04-02 | 04 | 4 | NAV-04 | T-01-13, T-01-14 | Sectioned sidebar render, glyphs, unread/encrypted markers, hostile names sanitized | snapshot + render | `cargo test --lib ui::sidebar::tests` | ❌ W0 | ⬜ pending |
| 01-05-01 | 05 | 4 | NAV-04 | T-01-16, T-01-17 | Wrap with hanging indent, wide chars, long token, TwoLine layout, sanitize before wrap | unit | `cargo test --lib ui::wrap::tests` | ❌ W0 | ⬜ pending |
| 01-05-02 | 05 | 4 | NAV-02 | T-01-18 | Message cursor j/k, gg/G, Ctrl-d/Ctrl-u, counts, opens at newest, empty room; h/l intentional no-ops (author decision D-19); growing and shrinking resizes keep the cursor message in view | unit (reducer) | `cargo test --lib app::messages::tests` | ❌ W0 | ⬜ pending |
| 01-05-03 | 05 | 4 | NAV-04 | T-01-16, T-01-17 | Windowed messages render, cursor highlight, empty states, hostile text sanitized; every render test draws at the app's own `layout::areas` messages rect (snapshots: 80x25 app, pane asserted 52x20), so `view()` and the drawn window agree | snapshot + render | `cargo test --lib ui::messages::tests` | ❌ W0 | ⬜ pending |
| 01-06-01 | 06 | 5 | NAV-01, NAV-02 (D-18) | T-01-19, T-01-20, T-01-21, T-01-24 | InputBuffer grapheme editing, visual rows, view window, cursor math, 64 KiB cap, typed control/bidi chars dropped, insert_char refuses line breaks so no stored line holds LF, redacting Debug; with the input bar focused in Normal mode h/l/j/k/gg/G/Ctrl-d/Ctrl-u act on the draft (D-18) and counts clamp | unit + unit (reducer) | `cargo test --lib -- app::input_buf::tests app::update::tests::input_normal_` | ❌ W0 | ⬜ pending |
| 01-06-02 | 06 | 5 | NAV-01 | T-01-20, T-01-22 | `i`/`a`/`o` → Insert, Esc/Ctrl-c → Normal + previous pane, Enter appends local message stamped from `Action::Clock` (no clock read in the reducer), paste never submits; hardware cursor in Insert and in Normal with the input bar focused | unit (reducer) + render | `cargo test --lib -- app::update::tests::mode_ app::update::tests::insert_ app::update::tests::submit_ app::update::tests::paste_ ui::input::tests::hardware_cursor_rules` | ❌ W0 | ⬜ pending |
| 01-06-03 | 06 | 5 | NAV-01..04 | — | Full 80x24 snapshots (Normal/Insert/Command), input grows to 5 rows (and survives 40x8/39x7) | snapshot | `cargo test --lib -- ui::tests::layout_80x24 ui::tests::input_grows_to_five_rows` | ❌ W0 | ⬜ pending |
| 01-06-03 | 06 | 5 | NAV-01..04 | T-01-04 | Phase gate: full suite plus `quit`, `panic` and `insert` pty scenarios (all reuse `wait_ready`) | e2e (pty) | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui quit && python3 -I scripts/smoke_pty.py target/debug/rockettui panic && python3 -I scripts/smoke_pty.py target/debug/rockettui insert` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` (every audited Phase 1 crate, including dev-dependency `insta`) + `src/lib.rs` with the headless module tree (nothing exists yet) → task 01-01-01; binary target `src/main.rs` and the `ui`/`tui`/`event` modules → task 01-02-01
- [ ] first snapshot generation via `INSTA_UPDATE=always cargo test`, review and commit the `.snap` files (a missing snapshot fails by default; `cargo-insta` is not installed) → 01-04-02, 01-05-03, 01-06-03
- [ ] test helpers in the cfg(test) module `crate::test_util`: `press(app, "<Vim keys>")` (feeds `Action::Key` through `update`) and `app_80x24()`, plus `App::with_fake_data(size)` → 01-01-01 (inline as the last item of `src/lib.rs`); moved unchanged to `src/test_util.rs`, same crate path, plus `render_to_string()` → 01-02-01
- [ ] `.gitignore` containing `/target` (written by `cargo init`), `Cargo.lock` listed in `files_modified` and committed (gate `git ls-files --error-unmatch Cargo.lock && git cat-file -e HEAD:Cargo.lock`) → 01-01-01
- [ ] `scripts/smoke_pty.py` (spawns the binary under `pty.fork`; shared `wait_ready` requires `general` and `NORMAL` with `general` first, i.e. the seed applied before the first draw; asserts exit code and `\x1b[?1049l`) → 01-02-01 (`quit`), 01-02-03 (`panic`), 01-06-03 (`insert` + screen model)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Ctrl-c in every mode returns to Normal and never quits | NAV-01 | Real-terminal key delivery differs from synthetic events | Launch, press Ctrl-c in Normal/Insert/Command, confirm app stays open |
| Resize while running re-lays out cleanly | NAV-04 | Needs a live terminal emulator | Resize the window; layout reflows, no artifacts or panic |
| Multi-line paste in Insert mode is a single insert, not N sends | NAV-01 | Bracketed paste depends on the emulator | Paste 3 lines in Insert mode; input holds 3 lines, no fake messages appended |
| Panic restores the terminal | NAV-01 | Needs a real tty | Trigger a debug panic path; shell prompt returns usable (cooked mode, main screen) |
| Input-bar Normal-mode motions are visible and feel right (D-18; planner details A25) and the discretionary sidebar fold keys za / Enter on folders (A2) suit the author | NAV-02 | Author confirmation of the remaining planner assumptions (A25, A2); cursor visibility is judged by eye. The h/l no-ops on sidebar room rows and in the messages pane are author decision D-19 and are already pinned by `folds` and `motion_h_l_noop`, so they are only observed here, not re-decided | Type a multi-line draft, press Esc, use h/l/j/k/gg/G/Ctrl-d/Ctrl-u in the input bar; try h/l/za on sidebar folders and room rows and h/l in the messages pane |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
