# Proposal

## Why

Phases 1-4 were built with GSD, whose planning artifacts (about 1.7 MB) are expensive to re-read. The behavior that was shipped needs to live in OpenSpec specs so future changes build on a compact, checked contract instead of the GSD history.

## What Changes

- Backfill eight baseline capability specs describing what rockettui does today (Phases 1-4 as built).
- Documentation only: no code changes, no behavior changes.
- `.planning/` stays as a frozen archive.

## Capabilities

### New Capabilities
- `modal-navigation`: Normal/Insert/Command modes, motions, pane focus, quit, and the sidebar + messages + input layout.
- `configuration`: loading `config.toml` from the XDG directory, defaults, validation errors, and the user settings it holds.
- `keymap`: keymap presets, per-mode rebinding in `keymap.toml`, and its validation rules.
- `theme`: colors, styles, markers and the author palette in `theme.toml`.
- `authentication`: PAT credentials from env var or config, permission warnings, and secret redaction.
- `rooms`: the sidebar room list, opening a room, and encrypted rooms.
- `message-history`: reading and scrolling the history of the open room.
- `message-sending`: composing in Insert mode, sending, delivery states, retry/discard, drafts, and the quit warning.

### Modified Capabilities

## Impact

Adds `openspec/specs/` content only. No source, dependency or API impact.
