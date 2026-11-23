# Tasks

Docs-only change: no design.md (nothing to design) and no code edits. Specs were drafted from the GSD requirements and CONTEXT decisions, then spot-checked against `src/`. These tasks finish the check.

## 1. Check specs against the code

- [x] 1.1 Verify modal-navigation and keymap against `src/input/` (default bindings, presets, validation rules)
- [x] 1.2 Verify configuration and theme against `src/config/` and `src/ui/` (ranges, defaults, marker widths, author palette)
- [x] 1.3 Verify authentication, rooms and message-history against `src/api/` and `src/app/`
- [x] 1.4 Verify message-sending against `src/app/` (states, retry/discard, quit warning, own username)
- [x] 1.5 Fix any spec line the code contradicts

## 2. Land the specs

- [x] 2.1 Run `openspec validate baseline-specs --strict`
- [x] 2.2 Archive the change so the specs sync into `openspec/specs/`
