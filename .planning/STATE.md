---
gsd_state_version: "1.0"
current_phase: 04
current_phase_name: Send Messages
status: executing
stopped_at: Completed 04-01-PLAN.md
last_updated: "2026-11-19T19:52:46.160Z"
last_activity: 2026-11-19
last_activity_desc: Phase 04 execution started
state_head: fdbdaff4e37627c7b67ce733ae1f562950aba716
progress:
  total_phases: 6
  completed_phases: 3
  total_plans: 26
  completed_plans: 24
  percent: 50
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-11-17)

**Core value:** Reading and sending Rocket.Chat messages (with live incoming messages) entirely from the keyboard in a modal, vim-like interface.
**Current focus:** Phase 04 — Send Messages

## Current Position

Phase: 04 (Send Messages) — EXECUTING
Plan: 7 of 8
Status: Ready to execute
Last activity: 2026-11-19 — Phase 04 execution started

Progress: [█████░░░░░] 50%

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
- [Phase 02]: Tracer-first: message_layout wired end to end before other keys, so later plans extend the pipeline without moving seams
- [Phase 02]: Unknown key names echoed at most 32 characters on screen and stderr
- [Phase 02]: TimeFormat validated via StrftimeItems plus write! sample render; user formats can never panic via to_string
- [Phase 02]: Pre-App error screen reads crossterm directly (accepted TEA exception, documented in show_config_errors)
- [Phase 02]: sidebar_width is not clamped to the terminal width; a wide sidebar leaves the main column narrow or empty and never panics (A26)
- [Phase 02]: The binary shows system local time (chrono::Local set once in run); App::new, with_config and tests stay UTC (A2)
- [Phase 02]: Reducer geometry and row model go through App::areas and App::row_ctx, the same helpers the renderer uses (Pitfalls 9 and 10)
- [Phase 02]: 02-03: keymap action names are the 31 curated snake_case names (D-07) plus focus_next/focus_prev, none and cmd:<command>; the vocabulary is user-facing
- [Phase 02]: 02-03: keymap rows are ordered by file offset, removals apply before binds; duplicates, prefix clashes, lockout and text-eating insert/command sequences are config errors
- [Phase 02]: theme.toml element names (sidebar.cursor, status.error, input.hint, border.focused, ...) and the [markers] keys are the user-facing vocabulary; renaming one silently drops a user's setting
- [Phase 02]: Config error screen keeps fixed built-in styles and never reads the user theme (A30)
- [Phase 02]: Marker strings are rejected, not sanitized, when sanitize_line would change them; width caps 4 columns (encrypted 12)
- [Phase 02]: author_palette is a trusted user list (1-64 colors, validated as colors only); explicit fg on messages.author beats the palette, fg on messages.cursor overrides author colors on the cursor row
- [Phase 02]: author_color returns Color::Reset for an empty palette (defense in depth behind load-time validation, T-02-17)
- [Phase 03]: 03-01: Folders = Favorites (favorited rooms without a category) then custom categories (A6, awaiting author confirmation); literal D-10 reading is one condition in map::folders
- [Phase 03]: 03-01: Effect outbox on App drained only by run(); the token lives only in api::Client headers, never in App; reqwest redirects disabled, reqwest::Error never formatted
- [Phase 03]: 03-02: dev secrets live in dev/state/<project>/ (0700 dir, 0600 files, git-ignored); fixtures are scrubbed of lastMessage and md and guarded by assert_no_secrets
- [Phase 03]: 03-02: local server facts for mapping: discussion subscription name is the room id (title in fname), channels count only mentions as unread, history carries discussion-created system messages, categories work license-free via setPreferences plus MongoDB
- [Phase 03]: 03-03: first history page merges before local messages and shifts a stored cursor; a failed first page leaves the room Failed and reopening retries; endpoint chosen by Room.private (server type p), never by name
- [Phase 03]: 03-03: fake-source MessagesLoaded marks rooms Loaded+exhausted so Phase 1/2 snapshots stay unchanged; placeholders use messages.notice with no new theme element
- [Phase 03]: 03-04: an older page that adds no message (empty or all ids held) ends the history; a shorter page never does; paging sends the raw server timestamp of the oldest held message
- [Phase 03]: 03-04: MessageKind { Normal, System }; E2E leftovers keep kind Normal with the fixed text [encrypted message] and their msg and content are never mapped; attachment names are listed once across file, files and attachment titles
- [Phase 03]: 03-05: FileConn records keys with a non-string value so a wrong type is reported once; a value given anywhere makes a partial setting an error for the other keys; a bad file URL errors only when the file URL is in use
- [Phase 03]: 03-05: warnings (chmod 600 when the token comes from the file, cleartext http to non-loopback) travel Config.warnings -> App.warnings, drawn every frame after the connection span and repeated on stderr after restore; an env token over a file token gives no permission warning (A-D04)
- [Phase 03]: 03-06: :reconnect and :retry are one command row; retry_failed forgets Failed history entries and reopens only the current room, Loaded and Loading rooms are never refetched; the reducer never retries on its own
- [Phase 03]: 03-06: src/redaction.rs is the single D-07 sentinel sweep; every new type holding a secret or message text must be added to it; Client::with_timeouts varies deadlines only, never TLS or redirect policy
- [Phase 03]: 03-07: docker_env refuses a non-loopback server URL; docker_read asserts the wrapped halves of the attachment tag; the RESEARCH-derived mapping matched real 8.9.1 output and 9.0.0 passed with no client change
- [Phase 04]: 04-01: chat.sendMessage keeps the client _id on 8.9.1 (probe P1); a repeated id is a 400 (A1), so reconcile via chat.getMessage stays in plan 04-03
- [Phase 04]: 04-01: a send result is applied by (room, id) never by the current room; failures show one fixed 'message not sent: <reason>' per ApiError kind, never server text; all 4xx currently map to BadResponse
- [Phase 04]: 04-01: client ids are App.id_prefix + local_seq; run() draws a per-process rt+32hex prefix once, tests keep local-

### Pending Todos

None yet.

### Blockers/Concerns

- Company server PAT / third-party client policy not yet confirmed (needed by Phase 6).

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 261116-ed1 | Align message bodies at a shared x column (widest author in room) | 2026-11-16 | b8a2f25 | [261116-ed1-align-message-bodies-at-a-shared-x-colum](./quick/261116-ed1-align-message-bodies-at-a-shared-x-colum/) |
| 261116-eog | Color each author username deterministically per name | 2026-11-16 | d0f4f4b | [261116-eog-color-each-author-username-deterministic](./quick/261116-eog-color-each-author-username-deterministic/) |

## Session

**Last session:** 2026-11-19T19:17:07.569Z
**Stopped at:** Completed 04-01-PLAN.md
**Resume file:** None

## Performance Metrics

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 02 P01 | 25 min | 3 tasks | 14 files |
| Phase 02 P02 | 15 min | 3 tasks | 13 files |
| Phase 02 P03 | 5 min | 3 tasks | 9 files |
| Phase 02 P04 | 6 min | 3 tasks | 10 files |
| Phase 02 P05 | 4 min | 2 tasks | 5 files |
| Phase 03 P01 | 8 min | 2 tasks | 19 files |
| Phase 03 P02 | 7 min | 2 tasks | 9 files |
| Phase 03 P03 | 5 min | 2 tasks | 18 files |
| Phase 03 P04 | 6 min | 2 tasks | 15 files |
| Phase 03 P05 | 6 min | 2 tasks | 8 files |
| Phase 03 P06 | 5 min | 2 tasks | 6 files |
| Phase 03 P07 | 6 min | 2 tasks | 3 files |
| Phase 04 P01 | 5 min | 3 tasks | 13 files |
