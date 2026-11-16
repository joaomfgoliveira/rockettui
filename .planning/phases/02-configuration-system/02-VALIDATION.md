---
phase: "02"
slug: "configuration-system"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-16"
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | built-in `cargo test` (libtest) + `insta` 1 snapshots; pty e2e via `scripts/smoke_pty.py` |
| **Config file** | none (Cargo.toml dev-dependency `insta = "1"`) |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds (baseline: 169 tests pass before this phase) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite must be green, plus smoke scenarios `quit`, `panic`, `insert` (existing) and `config_error`, `config_keymap` (new)
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

Task IDs and plan numbers are filled in by the planner; rows below are keyed by requirement.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | CONF-01 | — | `config_dir` honors absolute XDG, ignores relative, falls back to `$HOME/.config`; non-regular / oversize / non-UTF-8 file becomes a Problem | unit | `cargo test --lib config::paths::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-02 | — | all sources absent => `Config::default()`; default renders identical to `App::new`; 169 existing tests unchanged | unit + regression | `cargo test --lib config::tests::absent_files_default && cargo test` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-03 | T-02 (no source-line echo in errors) | all errors across 3 files reported, sorted, with line:col and key; unknown key = warning only | unit | `cargo test --lib config::problem config::reader` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-03 | — | error screen snapshot; any key press quits non-zero; Release/Resize do not | snapshot + unit | `cargo test --lib ui::error_screen::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-03 | — | invalid config => error screen, any key exits non-zero, stderr has report, terminal restored | e2e pty | `cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui config_error` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-04 | — | presets `vim` / `vim-extras` / unknown (error); extras add Tab/S-Tab focus cycling | unit | `cargo test --lib config::keymap::tests::preset_` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-04 | — | `focus_next`/`focus_prev` cycle Sidebar→Messages→Input→Sidebar | unit (reducer) | `cargo test --lib app::update::tests::focus_cycle` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-05 | — | override replaces; `none` removes and prunes; unknown action / bad notation / prefix clash named; `cmd:quit` fires; lockout guard | unit | `cargo test --lib config::keymap input::trie input::actions` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-05 | — | rebinding a key changes behavior in the binary | e2e pty | `python3 -I scripts/smoke_pty.py target/debug/rockettui config_keymap` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-06 | T-01 (markers sanitized + width-checked) | theme default == today's styles; per-field inheritance; colors named/index/hex; borders; palette validated; renderers use theme | unit + render | `cargo test --lib config::theme ui::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CONF-07 | — | `sidebar_width` 10-60 drives layout; `history_size` 1-1000 stored; `timestamp_format` valid/invalid/empty/`%n`; `message_layout`; key timeout read by run loop | unit | `cargo test --lib config::settings ui::layout::tests app::messages::tests ui::messages::tests` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/config/*` modules with their `#[cfg(test)]` tests (none exist)
- [ ] `src/input/actions.rs` name table + exhaustive coverage test
- [ ] `Trie::remove` / `Trie::clashes` tests in `src/input/trie.rs`
- [ ] `src/ui/error_screen.rs` + insta snapshot
- [ ] `scripts/smoke_pty.py`: default isolated `XDG_CONFIG_HOME`, scenarios `config_error`, `config_keymap`
- [ ] Snapshots for a non-default config (e.g. `sidebar_width = 40`, `message_layout = "two_line"`)
- [ ] Framework install: none needed

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Editing each of the three files visibly changes the running UI | CONF-04..07 | Look and feel in a real terminal | Edit `config.toml`, `keymap.toml`, `theme.toml` under `~/.config/rockettui/`, relaunch, observe each change |
| Breaking each file shows a clear error screen, not a crash | CONF-03 | Readability of the screen is a human judgment | Introduce a syntax error, a bad color, and a prefix clash; relaunch; check the screen lists all problems and any key exits non-zero |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
