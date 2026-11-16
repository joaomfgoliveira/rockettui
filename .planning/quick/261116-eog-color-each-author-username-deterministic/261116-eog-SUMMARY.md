---
phase: quick-261116-eog
plan: 01
subsystem: ui
tags: [ratatui, messages-pane, colors, fnv1a]
requires: [quick-261116-ed1]
provides: [ui::palette::author_color, colored author names in Compact and TwoLine layouts]
affects: [src/ui/palette.rs, src/ui/mod.rs, src/ui/wrap.rs, src/ui/messages.rs]
tech-stack:
  added: []
  patterns: [hand-written FNV-1a 64 hash selecting a fixed named-ANSI palette entry]
key-files:
  created: [src/ui/palette.rs]
  modified: [src/ui/mod.rs, src/ui/wrap.rs, src/ui/messages.rs]
decisions:
  - "FNV-1a 64 over sanitized name bytes, index = hash % 6: unseeded and specified, so colors are stable across runs and platforms"
  - "Color keyed on the full sanitized name before truncation, so truncated names keep their color"
  - "6 named ANSI colors (Cyan, Magenta, LightMagenta, Green, Yellow, LightBlue), excluding colors that clash with defaults, cursor-row backgrounds and status errors"
status: complete
metrics:
  tasks: 2
  completed: 2026-11-16
estimate_actuals_note: no estimate comparison recorded
commits: 2
plan_head_before: d8b2557db7c7d607a5fb25f25c72f9f5d0040cce
plan_head_after: 9b10c73b39d4db3ee02541fe65c89f674c23892a
---

# Phase quick-261116-eog Plan 01: Deterministic author colors Summary

Each author name in the messages pane is now bold and colored from a 6-entry named-ANSI palette chosen by a hand-written FNV-1a 64 hash of the sanitized name, in both Compact and TwoLine layouts.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 | Deterministic author color end-to-end in Compact (palette, wrap, pane) | b070fd4 |
| 2 | TwoLine header color, truncation/hostile keying, palette guard, highlight coexistence | 9b10c73 |

## What changed

- New `src/ui/palette.rs`: `AUTHOR_PALETTE`, private `fnv1a_64`, `pub fn author_color(&str) -> Color`. Golden mapping: alice LightBlue, bob Cyan, carol Yellow, dave Green.
- `src/ui/wrap.rs`: `render_rows` builds `name = bold.fg(author_color(&author))` from the sanitized name before Compact truncation, and uses it for the author span in both layouts. Time stays dim, padding/separator/body stay raw.
- `src/ui/messages.rs`: comment only in production code (neither highlight sets a foreground).
- Tests added: FNV-1a published vectors, golden mapping, palette clash guard (1000 names), compact/two-line span styles, truncated and hostile name keying, buffer-level colors in the 52x20 pane, and color under focused (REVERSED) and unfocused (DarkGray bg) cursor highlights.

## Verification

- `cargo test`: 169 passed, 0 failed. All 7 insta snapshots byte-identical, no `.snap.new` files.
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo build` ok.
- pty smoke: `SMOKE OK: quit`, `SMOKE OK: insert`.
- `Cargo.toml`, `Cargo.lock`, `src/app/messages.rs`, `src/ui/snapshots` unchanged.

## Deviations from Plan

None - plan executed exactly as written.

## Pending for the user

- Human readability check: run `cargo run` on a dark theme (and a light one if handy), open #general, and confirm alice/bob/carol/dave show 4 distinct readable colors, padding and bodies stay the default color, the focused (reversed) and unfocused (dark gray) cursor rows stay readable, and bodies line up at one column. Not attempted by the executor.

## Known Stubs

None.

## Threat Flags

None.

## Self-Check: PASSED

- src/ui/palette.rs exists; commits b070fd4 and 9b10c73 are ancestors of HEAD.
