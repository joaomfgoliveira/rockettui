# Phase 2: Configuration System - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-16
**Phase:** 2-Configuration System
**Areas discussed:** Config errors, Keymap overrides, Theme scope, Settings & presets

---

## Config errors

| Question | Options | Selected |
|----------|---------|----------|
| Startup behavior on bad entry | Skip bad entry / Fall back per file / Refuse to start | Refuse to start |
| Error presentation | All errors to stderr / First error only / In-TUI error screen | In-TUI error screen |
| Unknown keys | Error / Ignore silently / Ignore but list on error screen only | Ignore but list on error screen only |
| Error screen actions | Any key quits / `r` reload / Continue with defaults | Any key quits |

## Keymap overrides

| Question | Options | Selected |
|----------|---------|----------|
| keymap.toml shape | key = action per mode / action = [keys] / array of rows | key = action per mode |
| Unbind | `"none"` action / unbind list / none | `"none"` action |
| Prefix clashes | Config error / Last wins / Rely on timeout | Config error |
| Action names | Curated list + `cmd:` / Curated only / All variants | Curated list + `cmd:` |

## Theme scope

| Question | Options | Selected |
|----------|---------|----------|
| Color formats | Named+256+hex / Named+hex / Named only | Named+256+hex |
| Theme shape | Per-element fg/bg/modifiers / Flat / Palette refs | Per-element fg/bg/modifiers |
| Author colors | Themeable palette / Fixed / Palette + per-user | Themeable palette |
| Glyphs | Styles only / Include border + markers | Include border + markers (non-recommended choice) |

## Settings & presets

| Question | Options | Selected |
|----------|---------|----------|
| Settings | sidebar_width; history_size + timestamp_format; message_layout + key timeout; server_url | first three (no server_url) |
| vim-extras | Tab only / Tab + Ctrl-k focus sidebar / Room picker | Tab only |
| Validation | Clamped ranges + strftime / Minimal / You decide | Clamped ranges + strftime |

## Claude's Discretion

TOML/date crates, default timeout, module layout, how defaults are expressed, error screen layout.

## Deferred Ideas

Ctrl-k room switch, hot reload / reload key, per-user author colors, theme gallery.
