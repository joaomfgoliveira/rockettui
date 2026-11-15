---
phase: 01-modal-tui-shell
verified: 2026-11-15T16:30:00Z
status: human_needed
score: 4/4 roadmap success criteria verified (plan must-haves all verified automatically); 0 behavior-unverified
covered_files:
  - .planning/phases/01-modal-tui-shell/01-01-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-01-SUMMARY.md
  - .planning/phases/01-modal-tui-shell/01-02-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-02-SUMMARY.md
  - .planning/phases/01-modal-tui-shell/01-03-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-03-SUMMARY.md
  - .planning/phases/01-modal-tui-shell/01-04-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-04-SUMMARY.md
  - .planning/phases/01-modal-tui-shell/01-05-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-05-SUMMARY.md
  - .planning/phases/01-modal-tui-shell/01-06-PLAN.md
  - .planning/phases/01-modal-tui-shell/01-06-SUMMARY.md
  - Cargo.toml
  - scripts/smoke_pty.py
  - src/action.rs
  - src/app/command.rs
  - src/app/input_buf.rs
  - src/app/messages.rs
  - src/app/mod.rs
  - src/app/sidebar.rs
  - src/app/update.rs
  - src/data/fake.rs
  - src/data/mod.rs
  - src/event.rs
  - src/input/key.rs
  - src/input/keymap.rs
  - src/input/matcher.rs
  - src/input/mod.rs
  - src/input/trie.rs
  - src/lib.rs
  - src/main.rs
  - src/tui.rs
  - src/ui/input.rs
  - src/ui/layout.rs
  - src/ui/messages.rs
  - src/ui/mod.rs
  - src/ui/sanitize.rs
  - src/ui/sidebar.rs
  - src/ui/status.rs
  - src/ui/wrap.rs
covered_digest: "v4:sha256:4c1cba2d9c1678eba39c3407479aaa6ccdb0a18bbe4cbe9a3e804f386c952168"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Resize behavior in a real terminal"
    expected: "Dragging the terminal smaller and larger re-lays out the sidebar, messages pane and input bar on the next frame with no garbage or panic. Below 40x8 a 'terminal too small' notice replaces the panes. The message cursor stays visible after a resize. Note WR-03: between 40 and about 60 columns the messages body is clipped, so judge usability, not just absence of a panic."
    why_human: "Real terminal resize events and visual correctness cannot be exercised from the TestBackend or from the pty harness."
  - test: "Ctrl-w interception by terminal or multiplexer (assumption A7)"
    expected: "In Normal mode, Ctrl-w then h, j or l moves focus to sidebar, input or messages and the thick border follows. If the terminal or tmux/zellij eats Ctrl-w (for example closing a tab or window), record it and decide on an alternative binding."
    why_human: "Depends on the user's terminal emulator and multiplexer. The keymap and the reducer path are tested, but delivery of the key to the process is not."
  - test: "1 s key-timeout feel (D-06)"
    expected: "Press g alone: the pending g shows in the status line and clears after about 1 s. Press Ctrl-w alone: same. Typing 5 then j moves 5 rows. A pending count or sequence is dropped after the timeout without any visible glitch."
    why_human: "Timing and perceived responsiveness are subjective. The select! deadline and KeyTimeout path are unit-tested only through injected Actions, not wall-clock timing."
  - test: "Alt-Enter newline delivery and bracketed paste (assumptions A6, A25)"
    expected: "In Insert mode, Alt-Enter and Ctrl-j insert a newline and Enter submits. Pasting a multi-line block from the system clipboard lands in the draft as one insert and does not submit any message. Terminals that send Alt-Enter differently should be noted."
    why_human: "Depends on the terminal emulator's encoding of Alt-Enter and on its bracketed-paste support. The pty harness cannot reproduce a real clipboard paste."
  - test: "D-18 input-bar choices"
    expected: "With focus on the input bar in Normal mode, h/l move the cursor within the line, j/k move between lines of a multi-line draft, gg/G jump to the first and last line, and the hardware cursor is visible. The input bar grows from 1 to 5 rows and then scrolls. The author confirms these choices feel right."
    why_human: "The choices were a taste decision added during planning. Whether they are the right ergonomics is a human judgment."
  - test: "Moving around #general"
    expected: "Launch the app, press Enter on #general, then use j/k, gg/G, Ctrl-d/Ctrl-u, 5j and 3<C-d>. The cursor highlight is clear and the long Japanese and 20-row messages wrap with a hanging indent. NOTE WR-01: the last 3 rows of the 20-row long message cannot be scrolled into view; confirm whether that is acceptable for Phase 1."
    why_human: "Visual quality and the feel of scrolling in a real terminal."
---

# Phase 1: Modal TUI Shell Verification Report

**Phase Goal:** A runnable ratatui app with the sidebar + messages + input layout, fake rooms, and Normal/Insert/Command modes
**Verified:** 2026-11-15T16:30:00Z
**Status:** human_needed
**Re-verification:** No, initial verification

## Notes on how this was verified

- ROADMAP.md marks Phase 1 as `mode: mvp`, but its goal is not in "As a ..., I want to ..., so that ..." form (`user-story.validate` returned `valid: false`). The MVP guard says to refuse. The caller explicitly asked for verification, so standard goal-backward verification was run against the four ROADMAP success criteria. The roadmap criteria are already written as user flows, so this narrows nothing. If MVP-mode verification is wanted, run `/gsd-mvp-phase 1` first to give the phase a user-story goal.
- Everything below was run or read in the codebase, not taken from the SUMMARY files.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User launches the app and sees a sidebar of fake rooms, a message pane and an input bar | VERIFIED | `target/debug/rockettui` builds. The pty smoke `quit` and `insert` scenarios pass, including the readiness check that `general` appears before `NORMAL` in the first frame. `layout_80x24_normal.snap` shows Rooms (Unread, Folders, Recent sections, 8 fake rooms), Messages and Input panes, the status bar and the command row. `run()` in `src/lib.rs` drains the seed Actions before the first `terminal.draw`. |
| 2 | User moves with `hjkl`, `gg`/`G` and `Ctrl-d`/`Ctrl-u` in Normal mode | VERIFIED | `default_bindings()` in `src/input/keymap.rs` binds all of them. One router `apply_motion` in `src/app/update.rs` dispatches by focused pane to `sidebar::apply_motion`, `messages::apply_motion` or `input_motion`. Passing tests: `motion_j_k_sidebar`, `motion_count_5j_equals_five_j`, `motion_huge_count_clamps`, `sidebar::tests::{edges,folds,half_page,cursor_never_on_header}`, `messages::tests::{motion_edges,motion_counts,motion_gg_g_capital,motion_half_page,motion_h_l_noop}`, `update::tests::input_normal_{h_l_within_line,j_k_lines,gg_G,half_page}`. h/l are no-ops on room rows and in the messages pane by author decision D-19, which narrows the NAV-02 wording. |
| 3 | Current mode is visible and user can switch between Normal, Insert and Command | VERIFIED | `ui/status.rs` renders the mode badge. Snapshots show NORMAL, INSERT and COMMAND. Passing tests: `mode_colon_enters_command`, `mode_i_a_o_enter_insert_and_esc_restores_focus`, `mode_esc_and_ctrl_c_cancel_command`, `mode_ctrl_c_in_insert_returns_normal_without_quitting`, `mode_edges_and_repeats`, `status_shows_mode_room_and_pending`. The pty `insert` scenario switches modes against the real binary. |
| 4 | User quits with `:q` | VERIFIED | `COMMANDS` in `src/app/command.rs` has `q` and `quit`. `colon_q_quits` and `colon_quit_quits` pass. The pty `quit` scenario sends `:q` and Enter to the real binary and requires exit status 0 plus alternate-screen and bracketed-paste restore ordering. It passed here. |

**Score:** 4/4 roadmap truths verified; 0 present-but-behavior-unverified.

### Plan must-haves (merged, not reducing the roadmap contract)

| Plan | Truths | Status | Evidence |
|------|--------|--------|----------|
| 01-01 | Seed through Actions only (D-13), tracer test, sidebar j/k edges, command mode lifecycle, unknown-command error, keymap-as-data, `parse_keys` errors, redacting Debug, D-13 dataset counts, no clock read in `update()` | VERIFIED | `tracer_seed_keys_quit`, `seed_matches_d13`, `parse_keys_errors`, the `debug_redacts*` tests and `unknown_command_errors` all pass. `grep KeyCode::Char src/app/update.rs` has no hit. The only clock reads in `src/` are `unix_now()` (`SystemTime`) and `Instant::now()` for the key deadline, both in `src/lib.rs`. |
| 01-02 | Real binary layout, seed-before-first-draw, pty smokes, panic restore, sanitizer, tiny sizes, no stdout writes | VERIFIED | `smoke_pty.py` scenarios `quit`, `panic` and `insert` all printed `SMOKE OK`. `tiny_sizes`, `hostile_text_is_sanitized_in_render` and the `sanitize::tests::*` suite pass. `grep println/eprintln/dbg` in `src/` has no hit. The only stdout use is the terminal escape writes in `tui.rs`. |
| 01-03 | Counts, trie matcher, 1 s timeout, saturation, Ctrl-w focus, status bar pending display | VERIFIED | `count_saturates`, `multi_key_prefix`, `exact_and_prefix_timeout_fires`, `prefix_only_timeout_cancels`, `focus_ctrl_w_moves_focus`, `pending_timeout_clears`, `default_keymap_parses` and `format_round_trip` pass. `KeyTimeout` reaches the reducer from a `select!` deadline in `lib.rs`. |
| 01-04 | Sidebar sections, folds, one home per room, per-pane hjkl, Enter opens room, glyphs and markers | VERIFIED | `sections_order_and_membership`, `each_room_once`, `folds`, `child_za_collapses_parent`, `enter_opens_room`, `empty_sidebar`, `glyphs_and_markers` and the two sidebar snapshots pass. |
| 01-05 | Messages cursor, scrolling, wrap with hanging indent, two layouts, empty states, resize clamping | VERIFIED | `open_room_starts_at_newest`, `motion_*`, `resize_clamps_view`, `resize_shrink_scrolls_into_view`, `window_matches_view`, `messages_empty_room`, `messages_no_room` and the two message snapshots pass. See warning WR-01 (a tall message tail is unreachable). |
| 01-06 | Insert mode, multiline input, D-18 motions, cursor rules, local submit, safe paste, snapshots | VERIFIED | `input_buf::tests::*` (including `insert_char_never_breaks_line`, `cap_enforced`), `submit_appends_local_message`, `submit_blank_is_noop`, `submit_without_room_errors_and_keeps_draft`, `paste_is_one_insert_never_submits`, `hardware_cursor_rules`, `input_grows_to_five_rows` and the three full-screen snapshots pass. The pty `insert` scenario passes. |

### Prohibitions (must_haves.prohibitions, test-tier)

| Prohibition | Status | Evidence |
|-------------|--------|----------|
| MUST NOT exit on any key except `:q` / `:quit` | VERIFIED | `should_quit` is set only in `command.rs:17` (the `:q` handler) and `update.rs:147` (`Action::Quit`). `Action::Quit` is produced only in `event.rs:26`, when the terminal input stream ends. No keymap row maps to `Quit`. `mode_ctrl_c_and_esc_in_normal_do_not_quit` and `mode_esc_and_ctrl_c_cancel_command` pass. |
| MUST NOT write message bodies, authors, typed keys or pasted text to logs, stdout/stderr, files or Debug | VERIFIED | No `println!`, `eprintln!`, `dbg!`, `File::` or `fs::` in `src/`. Redacting-Debug tests exist for `Action`, `Chord`, `Matcher`, `Message` and `InputBuffer`. |
| MUST NOT open network connections or depend on HTTP or WebSocket crates | VERIFIED | `Cargo.toml` lists only crossterm, futures, ratatui, textwrap, tokio (rt, macros, sync, time), unicode-segmentation, unicode-width and insta (dev). `Cargo.lock` has no reqwest, hyper or tungstenite. No standing test enforces this, so a later phase can add the crates without a failing check. This is an info note, not a gap. |
| MUST NOT turn a pasted block into message submissions | VERIFIED | `paste_is_one_insert_never_submits` passes. `Action::Paste` calls `insert_str` and never reaches `submit_input`. |

### Required Artifacts

`gsd_run query verify.artifacts` on all six plans: 8/8, 8/8, 6/6, 4/4, 5/5 and 6/6 artifacts passed (exist, substantive, expected patterns present). Spot reads of `src/app/update.rs`, `src/lib.rs`, `src/input/keymap.rs`, `src/app/command.rs`, `src/event.rs` and `src/tui.rs` show real implementations with no stubs.

### Key Link Verification

`gsd_run query verify.key-links`: 5/5, 6/6, 5/5, 3/3, 4/4 and 6/6 links verified. The critical chain is wired and exercised end to end by the pty smokes: `main` -> `tui::init` -> `run` -> seed Actions -> `update` -> `ui::view` -> `tui::restore`.

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|----------|------|--------|-----------|--------|
| `ui/sidebar.rs` | rooms and folders | `data::fake::seed()` -> `RoomsLoaded` / `FoldersLoaded` -> `App.rooms` | Yes (8 fake rooms, by design for Phase 1) | FLOWING |
| `ui/messages.rs` | room messages | `MessagesLoaded` and `SubmitInput` -> `App.messages` | Yes | FLOWING |
| `ui/input.rs` | draft | `InputBuffer` edited through the reducer | Yes | FLOWING |

The fake data is the intended source for this phase (D-13). It sits behind the Action seam that Phase 3 will replace.

### Behavioral Spot-Checks and Probes

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full test suite | `cargo test` | 149 passed, 0 failed | PASS |
| Lints | `cargo clippy --all-targets -- -D warnings` | clean | PASS |
| pty quit | `python3 -I scripts/smoke_pty.py target/debug/rockettui quit` | SMOKE OK | PASS |
| pty panic restore | `... panic` | SMOKE OK | PASS |
| pty insert and send | `... insert` | SMOKE OK | PASS |

Behavior-dependent truths (mode transitions, Esc/Ctrl-c never quitting, count saturation, key timeout, resize clamping) each have a named passing test or a pty scenario, so none is left as present-but-behavior-unverified. The key-timeout wall-clock feel is routed to human verification.

### Requirements Coverage

Requirement IDs in the plan frontmatter: 01-01 [NAV-01, NAV-02, NAV-03], 01-02 [NAV-01, NAV-03, NAV-04], 01-03 [NAV-01, NAV-02], 01-04 [NAV-02, NAV-04], 01-05 [NAV-02, NAV-04], 01-06 [NAV-01, NAV-02, NAV-04]. The union is NAV-01..NAV-04, which equals the set REQUIREMENTS.md maps to Phase 1. No orphaned requirements.

| Requirement | Source plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| NAV-01 | 01-01, 01-02, 01-03, 01-06 | Switch Normal/Insert/Command, mode shown | SATISFIED | Truth 3 above |
| NAV-02 | 01-01, 01-03, 01-04, 01-05, 01-06 | hjkl, gg/G, Ctrl-d/Ctrl-u move and scroll | SATISFIED (with D-19 narrowing: h/l are no-ops on room rows and in the messages pane) | Truth 2 above |
| NAV-03 | 01-01, 01-02 | Quit with `:q` | SATISFIED | Truth 4 above |
| NAV-04 | 01-02, 01-04, 01-05, 01-06 | Layout is sidebar + messages + input bar | SATISFIED | Truth 1 and the snapshots |

### Anti-Patterns Found

No `TBD`, `FIXME`, `XXX`, `TODO`, `HACK`, `todo!` or `unimplemented!` in `src/` or `scripts/`. No stub returns reach rendering.

### Warnings

These do not fail a must-have. They are listed so the author can decide.

| # | Item | Severity | Impact |
|---|------|----------|--------|
| W1 | `.planning/REQUIREMENTS.md` still shows NAV-03 as `[ ]` and `Pending`, and the ROADMAP Phase 1 checkbox is `[ ]`, although `:q` works and is tested. NAV-01, NAV-02 and NAV-04 are marked complete. | Warning | Tracking is stale. Update NAV-03 to complete when closing the phase. |
| W2 | 01-REVIEW.md WR-01: a message taller than the messages pane cannot be scrolled to its end. The 20-row fake message at 80x24 shows 17 rows, and the last 3 are unreachable with any key. | Warning | Edge of NAV-02 scrolling. It does not break any roadmap criterion on the fake data, but it is a real defect for stack traces and logs. Listed in the human check "Moving around #general". |
| W3 | 01-REVIEW.md WR-02: the sidebar scroll offset is rebuilt each frame, so the cursor sticks to the bottom row in long lists. | Warning | Invisible with 8 fake rooms. Matters in Phase 3 with real room lists. |
| W4 | 01-REVIEW.md WR-03: `MIN_WIDTH = 40` and `MIN_HEIGHT = 8` ignore the fixed 28-column sidebar and the 5-row input cap. | Warning | The "too small" notice satisfies the plan truth literally, but the UI is unusable between about 40 and 60 columns. Covered by the resize human check. |
| W5 | 01-REVIEW.md WR-04 (sanitizer misses some invisible characters, such as Hangul fillers and the Braille blank) and WR-05 (a long room name hides pending keys and errors in the status bar). | Warning | Hardening gaps. The control-sequence part of the sanitizer is sound. |
| W6 | The ROADMAP marks the phase `mode: mvp` while its goal is not a user story. | Warning | See the note at the top. |

The 8 info findings (IN-01..IN-08, for example `ratatui::init` panicking without a TTY, no SIGTERM restore, seed send loop deadlocking above 256 Actions, Ctrl-H not mapped to backspace) are advisory and are not repeated here.

### Human Verification Required

See the `human_verification` list in the frontmatter. Six items: resize, Ctrl-w delivery (A7), key-timeout feel, Alt-Enter and bracketed paste (A6, A25), the D-18 input-bar choices, and moving around #general. Automated checks all passed, so the phase is waiting on these.

### Gaps Summary

No blocking gaps. All four roadmap success criteria and all six plans' must-haves are verified against the running code, the passing test suite (149 tests), clean clippy and the three pty smokes. The phase status is `human_needed` only because the items above can be judged only in a real terminal, plus the six warnings above for the author to triage.

---

_Verified: 2026-11-15T16:30:00Z_
_Verifier: Claude (gsd-verifier)_
