---
phase: 02-configuration-system
plan: 05
subsystem: config
tags: [toml, theme, ratatui, author-palette, fnv1a, messages-pane]

requires:
  - phase: 02-configuration-system (plan 02)
    provides: App::row_ctx and the RowCtx row model shared by renderer and reducer
  - phase: 02-configuration-system (plan 04)
    provides: Theme, ELEMENT_PATHS, style_slot_mut, parse_color (D-08), StyleSpec.resolve
provides:
  - theme.toml top-level author_palette (1-64 colors, any D-08 form) driving the FNV-1a author color
  - DEFAULT_AUTHOR_PALETTE (the six Phase 1 colors) and author_color(sanitized, palette) that cannot panic
  - six messages.* elements (cursor, cursor_unfocused, time, author, body, notice), so ELEMENT_PATHS is complete at 22
  - RowCtx.theme, set by App::row_ctx, so the row model and the renderer see the same theme
affects: [06 any new styled element must add a theme path, verify-work CONF-06 UAT]

actuals:
  tokens: 14000
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "A theme value that is a list (author_palette) is read by its own parse_* fn, validated whole, and any problem keeps the default list"
    - "Row styles come from RowCtx.theme; padding and indent spans stay raw so span-index assertions hold"
    - "An explicit fg on messages.author beats the hashed palette color for every author (A31)"

key-files:
  created: []
  modified:
    - src/ui/palette.rs
    - src/config/theme.rs
    - src/ui/wrap.rs
    - src/ui/messages.rs
    - src/app/mod.rs

key-decisions:
  - "author_palette is a trusted user list: only validated as colors (1-64 entries, entries named by index in errors); the clash-exclusion rationale applies to DEFAULT_AUTHOR_PALETTE only (D-10)"
  - "author_color returns Color::Reset for an empty slice instead of a modulo by zero (T-02-17, Pitfall 11), a second line of defense behind load-time validation"
  - "fg on messages.author replaces the palette color for every author; fg on messages.cursor overrides author colors on the cursor row (A31, A32 documented, not prevented)"

patterns-established:
  - "Every styled UI element has a ELEMENT_PATHS entry and a style_slot_mut arm; every_element_is_themeable guards the pair"

requirements-completed: [CONF-06]

coverage:
  - id: D1
    description: "author_palette in theme.toml recolors every author name in the running messages pane; the hash still picks the index"
    requirement: CONF-06
    verification:
      - kind: integration
        ref: "cargo test --lib -- config::theme::tests::tracer_author_palette_from_dir"
        status: pass
      - kind: unit
        ref: "cargo test --lib -- ui::palette::tests::custom_palette_mapping"
        status: pass
    human_judgment: false
  - id: D2
    description: "author_palette validation: 1-64 entries, per-entry errors named by index, type error, empty and oversized arrays rejected, default kept"
    requirement: CONF-06
    verification:
      - kind: unit
        ref: "cargo test --lib -- config::theme::tests::palette_validation"
        status: pass
      - kind: unit
        ref: "cargo test --lib -- ui::palette::tests::empty_palette_is_reset"
        status: pass
    human_judgment: false
  - id: D3
    description: "Default palette and golden author colors are unchanged; every existing snapshot is unchanged"
    requirement: CONF-06
    verification:
      - kind: unit
        ref: "cargo test --lib -- ui::palette::tests::golden_author_colors"
        status: pass
      - kind: unit
        ref: "cargo test --lib -- ui::messages::tests::messages_general_newest_52x20"
        status: pass
    human_judgment: false
  - id: D4
    description: "messages.cursor, cursor_unfocused, time, author, body and notice styles take effect; all 22 element paths are themeable with no warning"
    requirement: CONF-06
    verification:
      - kind: unit
        ref: "cargo test --lib -- config::theme::tests::every_element_is_themeable"
        status: pass
      - kind: unit
        ref: "cargo test --lib -- ui::messages::tests::themed_cursor_highlight"
        status: pass
      - kind: unit
        ref: "cargo test --lib -- ui::wrap::tests::themed_time_author_body"
        status: pass
    human_judgment: false
  - id: D5
    description: "Colors and the cursor bar look right in a real terminal theme"
    requirement: CONF-06
    verification: []
    human_judgment: true
    rationale: "Real terminal color rendering cannot be asserted by buffer tests (02-VALIDATION.md Manual-Only Verifications); the human-check in Task 2 is still open"

duration: 4min
completed: 2026-11-16
status: complete
plan_head_before: f6d0af6180fdf31d4998025359d4936f8fdef5ba
plan_head_after: 67b34ce7be4f2248c78e690af051d9c4810ad6f7
commits: 2
---

# Phase 2 Plan 05: Author palette and messages pane theming Summary

**theme.toml now themes the author palette (FNV-1a still picks the index, validated 1-64 colors, panic-free `author_color`) and the six messages pane elements, completing all 22 element paths with every default snapshot unchanged.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-11-16T15:51:10Z
- **Completed:** 2026-11-16T15:53:05Z (code); SUMMARY and state follow
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Tracer: `author_palette = ["red"]` in theme.toml reaches the rendered author cells through `load_from_dir` -> `Theme` -> `App::row_ctx` -> `render_rows`; the hash still selects the index, so a user keeps one color across rooms and runs.
- `author_palette` validation: 1 to 64 entries in any D-08 form, a bad entry is an error named `author_palette[i]` (value not echoed), empty, oversized and non-array values are errors, and the default palette is kept on any error. Duplicates are allowed.
- `DEFAULT_AUTHOR_PALETTE` keeps the six Phase 1 colors in order (golden colors and the clash test still pass); `author_color(name, palette)` returns `Color::Reset` for an empty slice (T-02-17).
- Messages pane elements: `messages.cursor` (REVERSED), `cursor_unfocused` (bg DarkGray), `time` (DIM), `author` (BOLD), `body` (plain), `notice` (DIM). `ELEMENT_PATHS` is 22 paths and `every_element_is_themeable` proves each one resolves a distinct fg with no warning.
- With no theme.toml the pane renders exactly as before: no `.snap` file changed, no `.snap.new` written.

## Task Commits

1. **Task 1 (tracer): author_palette end to end** - `ce3c254` (feat)
2. **Task 2: messages pane elements and the every-element check** - `67b34ce` (feat)

**Plan metadata:** the docs commit that carries this SUMMARY, STATE.md, ROADMAP.md and REQUIREMENTS.md.

_Note: TDD commit limitation. Both tasks put their tests in the same files as the implementation (Rust `#[cfg(test)]` modules) and the tests reference symbols that do not exist before the change (`DEFAULT_AUTHOR_PALETTE`, `Theme.messages_*`, `RowCtx.theme`), so a standalone RED commit cannot compile. Each task is one commit with the tests written alongside the implementation, as the orchestrator allowed._

## Files Created/Modified

- `src/ui/palette.rs` - `DEFAULT_AUTHOR_PALETTE`, `author_color(sanitized, palette)`, tests `custom_palette_mapping`, `empty_palette_is_reset`
- `src/config/theme.rs` - `Theme.author_palette`, `MAX_PALETTE`, `parse_palette`, six `messages_*` styles, `ELEMENT_PATHS` of 22; tests `tracer_author_palette_from_dir`, `palette_validation`, `default_palette_is_phase1`, `every_element_is_themeable` (replaces `chrome_elements_parse`), `messages_defaults_match_phase1`, `palette_order_defines_mapping`
- `src/ui/wrap.rs` - `RowCtx.theme`; time, author and body spans take the theme; test `themed_time_author_body`
- `src/ui/messages.rs` - cursor highlight and notices read the theme; tests `themed_cursor_highlight`, `themed_notices`
- `src/app/mod.rs` - `App::row_ctx` passes `&self.theme`

## Decisions Made

- A user `author_palette` is trusted and only validated as colors; the exclusion rationale (no Red, Black, ...) is documented as applying to the default only (D-10).
- `author_color` guards the empty slice itself even though load-time validation forbids it (defense in depth for T-02-17).
- An explicit fg on `messages.author` wins over the palette for every author (A31); an fg on `messages.cursor` overrides author colors on the cursor row, documented in the comment in `ui/messages.rs` (A32, Pitfall 13).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] chrome_elements_parse asserted exactly 16 element paths**
- **Found during:** Task 2
- **Issue:** growing `ELEMENT_PATHS` to 22 would break the existing `chrome_elements_parse` assertion (`len() == 16`)
- **Fix:** replaced it with `every_element_is_themeable` (same loop, 22 paths, plus the retained default-modifier assertions), as the plan's behavior list intends
- **Files modified:** src/config/theme.rs
- **Committed in:** 67b34ce (part of Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking). **Impact on plan:** none; the replacement is what the plan names.

## Issues Encountered

None. The plan's expected-count lower bounds (54 and 53 passes) were met or exceeded (57 and 56); the full suite is 268 lib tests.

## Verification Results

- `cargo test`: 268 passed, 0 failed
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- Six pty scenarios: SMOKE OK for quit, insert, config_layout, config_error, config_keymap, panic
- `git diff --exit-code -- src/ui/snapshots/` exits 0; `find src -name '*.snap.new'` prints nothing

## Known Stubs

None.

## Threat Flags

None. No new network, auth, file or trust-boundary surface; palette entries are parsed into the typed `Color` enum at load (T-02-18), no crates were added (T-02-SC5), and the empty-palette DoS is mitigated at load time and in `author_color` (T-02-17).

## User Setup Required

None - no external service configuration required.

## Open Human Check

The Task 2 `<human-check>` (real-terminal colors with `XDG_CONFIG_HOME=/tmp/rt-uat cargo run`: orange/magenta authors, grey cursor bar, light-blue times, and the empty-palette refusal naming `author_palette`) cannot be run by an agent in a pty and is left for `/gsd-verify-work` (see coverage D5). The automated gate for it (buffer assertions and the config_error pty scenario) is green.

## Next Phase Readiness

- Phase 2 has all five plans executed; every styled element has a theme.toml key (CONF-06). Ready for verification and Phase 3.
- Convention for later phases: a new styled element adds one `ELEMENT_PATHS` line and one `style_slot_mut` arm; `every_element_is_themeable` will fail until both exist.

---
*Phase: 02-configuration-system*
*Completed: 2026-11-16*

## Self-Check: PASSED

- FOUND: src/ui/palette.rs, src/config/theme.rs, src/ui/wrap.rs, src/ui/messages.rs, src/app/mod.rs
- FOUND commits: ce3c254, 67b34ce
