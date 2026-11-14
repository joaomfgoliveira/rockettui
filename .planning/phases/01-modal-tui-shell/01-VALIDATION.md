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

Task IDs are assigned by the planner; rows below are seeded from `01-RESEARCH.md` § Validation Architecture and re-keyed to tasks once PLAN.md files exist.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | NAV-01 | — | `i`/`a`/`o` → Insert, `:` → Command, `Esc`/`Ctrl-c` → Normal, never quits | unit (reducer) | `cargo test --lib app::update::tests::mode_` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-01 | — | Mode name shown in status bar | snapshot | `cargo test --lib ui::status::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-02 | — | `hjkl`, `gg`, `G`, `<C-d>`/`<C-u>`, counts (`5j`, `3<C-d>`) move cursor/scroll | unit (reducer + matcher) | `cargo test --lib app::update::tests::motion_` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-02 | — | Notation parse/format round trip, normalize table (`G`+SHIFT, Ctrl letters, BackTab), trie Exact/Prefix/ExactAndPrefix/None, timeout fire vs cancel | unit | `cargo test --lib input::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-02 | — | Wrap with hanging indent, wide chars, one very long message, scroll-into-view math | unit | `cargo test --lib ui::wrap::tests app::messages::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-03 | — | `:q`/`:quit` set `should_quit`; `:foo` sets error status, does not quit | unit | `cargo test --lib app::command::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-04 | — | Sidebar (sections, fold, unread flag), messages pane, input bar, status + cmdline rows at 80x24 | snapshot | `cargo test --lib ui::tests::layout_80x24` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-04 | — | No panic at 0x0, 1x1, 10x3, 200x60 | unit (render smoke) | `cargo test --lib ui::tests::tiny_sizes` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | security | T-01-xx | Sanitizer strips ESC/CSI/OSC/C1/bidi/NUL, expands tab, normalizes CRLF | unit | `cargo test --lib ui::sanitize::tests` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | security | T-01-xx | `Debug` of `Message`/`Action::Paste` does not contain body text | unit | `cargo test --lib data::tests::debug_redacts` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | NAV-01..04 | — | Launches, shows layout, `:q` exits 0, terminal restored; panic restores terminal | optional pty script | `python3 -I scripts/smoke_pty.py target/debug/rockettui` | ❌ W0 (optional) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` + `src/lib.rs` skeleton with the module tree (nothing exists yet)
- [ ] dev-dependency `insta`; first snapshot generation via `INSTA_UPDATE=always cargo test`, review and commit the `.snap` files (a missing snapshot fails by default; `cargo-insta` is not installed)
- [ ] test helpers: `key('g')`, `ctrl('d')` constructors producing `Action::Key`; `App::with_fake_data(size)`
- [ ] `.gitignore` containing `/target` (written by `cargo init`)
- [ ] optional `scripts/smoke_pty.py` (spawns the binary under `pty.fork`, sends keys, asserts `\x1b[?1049l` and exit code 0)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Ctrl-c in every mode returns to Normal and never quits | NAV-01 | Real-terminal key delivery differs from synthetic events | Launch, press Ctrl-c in Normal/Insert/Command, confirm app stays open |
| Resize while running re-lays out cleanly | NAV-04 | Needs a live terminal emulator | Resize the window; layout reflows, no artifacts or panic |
| Multi-line paste in Insert mode is a single insert, not N sends | NAV-01 | Bracketed paste depends on the emulator | Paste 3 lines in Insert mode; input holds 3 lines, no fake messages appended |
| Panic restores the terminal | NAV-01 | Needs a real tty | Trigger a debug panic path; shell prompt returns usable (cooked mode, main screen) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
