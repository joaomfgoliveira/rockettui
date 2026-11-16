---
phase: 02-configuration-system
reviewed: 2026-11-16T00:00:00Z
depth: standard
files_reviewed: 26
files_reviewed_list:
  - Cargo.toml
  - scripts/smoke_pty.py
  - src/action.rs
  - src/app/messages.rs
  - src/app/mod.rs
  - src/app/update.rs
  - src/config/diag.rs
  - src/config/keymap.rs
  - src/config/mod.rs
  - src/config/paths.rs
  - src/config/settings.rs
  - src/config/theme.rs
  - src/input/actions.rs
  - src/input/keymap.rs
  - src/input/trie.rs
  - src/lib.rs
  - src/main.rs
  - src/test_util.rs
  - src/ui/error_screen.rs
  - src/ui/input.rs
  - src/ui/messages.rs
  - src/ui/mod.rs
  - src/ui/sidebar.rs
  - src/ui/status.rs
  - src/ui/timefmt.rs
  - src/ui/wrap.rs
findings:
  critical: 0
  warning: 4
  info: 5
  total: 9
  necessity: 1
status: issues_found
---

# Phase 2: Code Review Report

**Reviewed:** 2026-11-16
**Depth:** standard
**Files Reviewed:** 26
**Status:** issues_found

## Summary

Phase 2 (TOML configuration) is carefully built: the pure core / IO shell split is clean, file reads are
hardened (metadata check before open, 1 MiB cap, UTF-8 check, FIFO/device never opened), all echoed
user text is length-capped and run through `sanitize_line`, the toml crate's source-echoing `Display`
is never used, and nothing in the config path panics on the inputs I tried. The strftime pattern
validation (`Item::Error` scan plus `write!` instead of `to_string`) is correct, and `format_ts` did not
panic for timestamps near chrono's limits under three different `TZ` values (probed).

I confirmed four problems by running probes against the real crate (probe crate lives in the scratchpad,
outside the repo):

- A 1 MiB `config.toml` of unknown keys takes ~228 s to load (release build) because `line_col` is
  O(file size) per problem. This defeats the 1 MiB hardening (T-02-03).
- `sidebar_width = 60` (legal) removes the messages and input panes entirely on a 50-column terminal.
- The keymap lockout guard (A3) accepts `[command] q = "cancel"`, after which `:q` cannot be typed and
  the app cannot be quit from the keyboard.
- The dead-chord detector misses sequences that contain `<Esc>` or `<C-c>` after the first chord; the
  matcher resets on those keys, so such bindings can never fire, silently.

No structural (fallow) findings were supplied. The necessity rubric was read; one necessity finding is
recorded (IN-05). No secrets, injection, or path-traversal issues were found; the config path never
executes anything and `cmd:` values are validated against the in-app command table.

## Warnings

### WR-01: `line_col` is O(file size) per problem; a 1 MiB config hangs startup for minutes

**File:** `src/config/diag.rs:78-87` (called from `diag.rs:139`, `diag.rs:255`, `src/config/keymap.rs:226`)
**Issue:** `line_col` rescans `src[..offset]` for every recorded problem (`matches('\n').count()` plus
`rsplit`). Problem count is unbounded and scales with file size, so total work is quadratic. Measured on
a release build: a 1 MiB `config.toml` of 262,000 lines `a=1` (every key an "unknown key" warning, plus
one real error so the report is returned) took 227.6 s inside `load_from_sources`, before the terminal
is even initialised. The 1 MiB cap in `paths.rs:35` was meant to bound work on hostile or accidental
input (T-02-03); this makes the bound meaningless. The same input also produces a 260k-line report
printed to stderr on exit. The error screen only shows what fits, so the volume is pure cost.
**Fix:** Compute line starts once per file and binary-search, and cap the number of problems recorded.
For example, build `Vec<usize>` of newline offsets in `parse_table`/`Reader::new` and have `line_col`
use `partition_point`:
```rust
pub struct LineIndex { starts: Vec<usize> }
impl LineIndex {
    pub fn new(src: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { starts }
    }
    pub fn line_col(&self, src: &str, offset: usize) -> (usize, usize) {
        let mut end = offset.min(src.len());
        while !src.is_char_boundary(end) { end -= 1; }
        let line = self.starts.partition_point(|&s| s <= end);
        let col = src[self.starts[line - 1]..end].chars().count() + 1;
        (line, col)
    }
}
```
and stop pushing problems (replace with one "... and N more" problem) past a limit such as 100 per file.

### WR-02: `sidebar_width` up to 60 can erase the messages and input panes on narrow terminals

**File:** `src/config/settings.rs:13` (range), `src/ui/layout.rs:31`, `src/ui/mod.rs:20-33`
**Issue:** The range `10..=60` is validated without reference to the terminal, while the UI only
switches to the "too small" notice below `MIN_WIDTH = 40`. `areas()` passes the width straight to
`Constraint::Length(sidebar_width)` ("not clamped", `layout.rs:21`). With `sidebar_width = 60` on a
terminal 40 to 60 columns wide, the sidebar takes everything and the messages pane and input pane get
width 0. Probe at 50x10: the render shows only the Rooms pane and status line, no messages, no input
bar, no message of any kind, and no indication why. A valid config therefore silently makes the app
unusable on a common terminal size (a half-screen split on a 100 to 120 column monitor is exactly this).
The reducer's geometry (`App::areas`, `messages::viewport`, `input_viewport`) inherits the same zero
widths.
**Fix:** Clamp at the single geometry choke point so view and reducer stay in agreement:
```rust
const MIN_MAIN: u16 = 20;
let sidebar_width = sidebar_width.min(body.width.saturating_sub(MIN_MAIN));
```
(or tighten the validated range relative to `MIN_WIDTH`, and document that the width is a maximum).

### WR-03: Lockout guard (A3) accepts a Command-mode plain-key binding that makes `:q` untypeable

**File:** `src/config/keymap.rs:243-263` (`reject_text_eaters`) and `src/config/keymap.rs:335-377` (`check_lockout`)
**Issue:** `check_lockout` exists so a keymap cannot trap the user in a raw-mode terminal (SIGINT is
swallowed, and `Cancel`/Esc/Ctrl-c never quit). It only asserts that certain *actions exist* in the
trie. It does not verify the user can still type a quit command. `reject_text_eaters` only rejects
*sequences* (`chords.len() >= 2`); a single plain character bound in Command mode is accepted. Verified
by probe: `[command]\nq = "cancel"\n` loads `Ok` with no warning. After that, Normal `:` still exists
(guard passes), Command `<CR>` and `<Esc>` still exist (guard passes), but pressing `q` in Command mode
fires `Cancel` instead of inserting `q`, so neither `:q` nor `:quit` can be typed; the only exits left are
an extra `quit`/`cmd:quit` binding or killing the process from another terminal. The same applies to
binding `u`, `i` or `t` in Command mode, and to rebinding `<CR>` away is already covered but this hole is
not.
**Fix:** In `check_lockout`, for Command mode require that at least one command name can be typed with
every character resolving to text (`Lookup::None` for each single-char chord), e.g.:
```rust
let typeable = COMMANDS.iter().flat_map(|d| d.names.iter()).any(|name| {
    name.chars().all(|c| matches!(
        keymap.lookup(Mode::Command, &[Chord { code: KeyCode::Char(c), mods: KeyModifiers::NONE }]),
        Lookup::None))
});
```
and fail with a message like "a command-mode key shadows the letters of :q" when neither the typed path
nor a Normal-mode `quit`/`cmd:quit` binding survives. Alternatively reject plain-character bindings in
Command mode outright, as is already done for sequences.

### WR-04: Dead-chord warning misses `<Esc>` / `<C-c>` inside key sequences

**File:** `src/config/keymap.rs:266-290` (`warn_dead_chords`); behaviour defined at `src/input/matcher.rs:77-86`
**Issue:** `Matcher::feed` handles Esc (no modifiers) and Ctrl-c first, unconditionally: it resets all
pending state and only fires something if that single chord is bound on its own. A binding whose
sequence contains either key at any position, for example `<C-w><Esc>` in Normal mode, can therefore
never fire: the second key resets the matcher. `warn_dead_chords` (Pitfall 6) exists for exactly this
class but only knows about leading digits and `<C-i>`/`<C-m>`. Verified by probe: `[normal]
"<C-w><Esc>" = "move_down"` loads `Ok` with no warning and the row is applied but unreachable. The same
holds if a user unbinds `<C-c>` (`"none"`) and then binds `<C-c>x`: the first chord is still a reset
chord, so the sequence is dead.
**Fix:** Add to `warn_dead_chords`:
```rust
let is_reset = |c: &Chord| (c.code == KeyCode::Esc && c.mods.is_empty())
    || (c.code == KeyCode::Char('c') && c.mods == KeyModifiers::CONTROL);
if row.chords.len() >= 2 && row.chords.iter().any(is_reset) {
    reader.warning(row.offset, &path, "can never fire: Esc and Ctrl-c reset a pending sequence");
}
```
(Ideally share the predicate with `matcher::is_reset_chord` so the two cannot drift.)

## Info

### IN-01: Unknown-key warnings are discarded when the config is otherwise valid

**File:** `src/config/mod.rs:46-66`
**Issue:** `load_from_sources` returns `Ok(config)` and drops all warnings when there are no errors
(D-03). A typo such as `sidebar_widht = 30`, `[stauts.mode]`, or `"<C-i>"` dead-chord warnings is
therefore silent in the normal case and only surfaces if some unrelated error exists. The warnings
machinery (and its tests) is mostly invisible to the user. This is a recorded decision, so it is only
noted; consider returning warnings alongside the `Config` and showing them as a one-line status message.
**Fix:** Return `(Config, Vec<Problem>)` and have `run` seed `app.status_msg` with a "N config warnings"
notice, or print them to stderr after terminal restore.

### IN-02: `TimeFormat::parse` width cap does not do what its comment says, and zero-width output is accepted

**File:** `src/config/settings.rs:18-20`, `src/config/settings.rs:55-73`
**Issue:** The comment claims the sample instant (2026-01-31 23:59:59) uses a "wide month, day and hour,
so a format that overflows with real data overflows here too". January (7 columns) and Saturday (8) are
not the widest names: September (9) and Wednesday (9) are. A pattern such as `%B %B %B %B %B` renders
39 columns for the sample (accepted) and 49 for September data, above `MAX_TIMESTAMP_WIDTH = 40`. Nothing
breaks at runtime because `time_column` measures real data, but the stated invariant is false. Separately,
a pattern that renders to nothing (e.g. `%.f` renders `""` for a whole-second timestamp, confirmed by
probe) is accepted and yields an empty time column.
**Fix:** Either test-render a set of samples (a September Wednesday plus the current sample) and take the
maximum width, or reword the comment to say the cap is approximate. Optionally reject
`rendered.width() == 0`.

### IN-03: `check_syntax` is dead code

**File:** `src/config/diag.rs:264-270`
**Issue:** `check_syntax` ("Syntax-check a file whose semantics are not parsed yet") has no callers; it
is a leftover from an earlier plan now that `keymap::build` and `theme::parse` call `parse_table`
themselves. Being `pub` in a library crate, the compiler does not flag it.
**Fix:** Delete it.

### IN-04: Two sources of truth for the message layout

**File:** `src/app/mod.rs:69-71`, `src/app/mod.rs:111-112`
**Issue:** `App::message_layout` is a copy of `settings.message_layout`, set once in `with_config` and
read by `row_ctx`. The two can diverge (tests or later `:set`-style commands mutating only one), and the
doc comment already acknowledges it exists "so the reducer and the renderer read the layout in one
place", which `app.settings.message_layout` would also satisfy.
**Fix:** Drop the field and read `self.settings.message_layout` in `row_ctx`.

### IN-05: Impossible-state guard: `u16::try_from(n).unwrap_or(default)` after range validation

**File:** `src/config/settings.rs:142`, `148`, `174`
**Category:** necessity
**Issue:** The guarded failure cannot occur. `reader.int_in` only returns `Some(n)` when `n` lies in the
inclusive ranges `SIDEBAR_WIDTH_RANGE = (10, 60)`, `HISTORY_SIZE_RANGE = (1, 1000)` and
`KEY_TIMEOUT_MS_RANGE = (100, 5000)` (`settings.rs:13-15`, `diag.rs:203`), all of which fit `u16`/`u64`. The
`unwrap_or(settings.<field>)` therefore never runs, and if the ranges were ever widened past `u16::MAX`
it would silently keep the default instead of failing loudly, hiding a validation bug. This is not a
trust-boundary guard: the value has already crossed the boundary and been range-checked by `int_in`.
**Fix:** Use the checked conversion's invariant explicitly (`u16::try_from(n).expect("range-checked")`) or
make `int_in` generic over the target type so the range and the type cannot disagree.

---

_Reviewed: 2026-11-16_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
