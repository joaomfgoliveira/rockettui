# Phase 1: Modal TUI Shell - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-14
**Phase:** 1-Modal TUI Shell
**Areas discussed:** Focus & pane model, Key matcher design, Layout & chrome, Insert & Command behavior

---

## Focus & pane model

| Question | Options | Chosen |
|---|---|---|
| Pane focus | h/l; Ctrl-w + h/j/l; Tab | Ctrl-w + h/j/l |
| Open room | Enter+focus msgs; Enter stay; auto-open | Enter, focus moves to messages |
| Input focus | i from anywhere; third pane | i from anywhere |
| Msg cursor | line cursor; viewport scroll | line cursor |

## Key matcher design

| Question | Options | Chosen |
|---|---|---|
| Counts | yes; no | yes |
| Sequences | trie+timeout; no timeout; hardcode gg | trie + timeout |
| Keymap repr | data table; hardcoded match | data table |
| Key syntax | vim notation; descriptive | vim notation |

## Layout & chrome

| Question | Chosen |
|---|---|
| Status line | Bottom status bar + `:` cmdline row |
| Messages | User: create both layouts, selectable in config |
| Sidebar | User: Unread, Folders, rest by most recent; folders hold channels/discussions/teams/DMs. Scoped to layout only with fake data |
| Fake data | ~8 rooms, 50+ msgs, long wrapped, one empty |

## Insert & Command behavior

| Question | Chosen |
|---|---|
| Input editing | Multiline from the start (user chose over recommended single-line) |
| Enter | Append local fake message |
| Commands | :q / :quit only |
| Mode keys | `:` opens Command; Esc cancels; Ctrl-c does not quit |

## Claude's Discretion

Newline key in multiline input, input max height, fold keys, event loop, terminal restore, module skeleton.

## Deferred Ideas

Real unread tracking, folder management/persistence, config selector for message layout.
