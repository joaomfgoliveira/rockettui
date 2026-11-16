---
phase: quick-261116-ed1
plan: 01
subsystem: ui
tags: [ratatui, wrap, layout, unicode]
requires: []
provides:
  - wrap::author_column room-wide author column
  - Compact layout bodies aligned at one x column per room
affects: [src/ui/wrap.rs, src/app/messages.rs, src/ui/messages.rs]
key-files:
  modified:
    - src/ui/wrap.rs
    - src/app/messages.rs
    - src/ui/messages.rs
    - src/ui/snapshots/rockettui__ui__messages__tests__messages_general_newest_52x20.snap
    - src/ui/snapshots/rockettui__ui__tests__layout_80x24_insert_multiline.snap
decisions:
  - Cap lives inside render_rows so renderer and reducer row model share one value
  - TwoLine ignores author_col and keeps the full author header
status: complete
requirements: [NAV-04]
actuals:
  tasks: 2
  commits: 2
plan_head_before: 021fd92f155ecc1ed7386b3944f9ff1f21e54fc2
plan_head_after: ad02c9d
---

# Quick 261116-ed1: Align message bodies at a shared x column Summary

Compact-layout message bodies now start at one x column per room, set by the widest sanitized author, capped so the body keeps MIN_BODY columns, with grapheme-safe ellipsis truncation of over-wide authors.

## Tasks

1. Shared author column end-to-end (commit efc93fd): `author_column(msgs)`, `render_rows`/`message_rows` take `author_col`, `row_starts` and `render` compute it once per room. Two snapshots regenerated; `messages_long_message_52x20` unchanged.
2. Narrow-pane cap and truncation (commit ad02c9d): `col = author_col.min(width - (5+1+2+MIN_BODY))` (saturating), private `fit_author` cuts on grapheme boundaries and appends U+2026. Added cap, truncation, tiny-width, row-model consistency (`rows_match_render_with_long_author`) and render alignment (`long_author_truncated_and_aligned`) tests.

## Verification

- `cargo test`: 159 passed, 0 failed
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `cargo build`: ok; pty smoke `quit` and `insert`: SMOKE OK
- Cargo.toml and Cargo.lock unchanged; no `.snap.new` files

## Deviations from Plan

None - plan executed as written. Tasks were committed as one commit each (tests and implementation together rather than separate RED/GREEN commits, per the one-commit-per-task constraint).

## Known Stubs

None.

## Threat Flags

None.

## Self-Check: PASSED
