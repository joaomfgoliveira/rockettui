# Roadmap: rockettui

## Overview

rockettui grows in vertical slices. First a navigable modal TUI with fake data, then the config system that makes it customizable, then real Rocket.Chat reading, sending, and live messages against a local Docker server. The last phase points it at the company server so it is usable day to day.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Modal TUI Shell** - Navigable sidebar + messages + input layout with fake rooms and vim modes (completed 2026-11-15)
- [ ] **Phase 2: Configuration System** - Config, keymap and theme files with presets and defaults
- [ ] **Phase 3: Connect & Read** - Token login, real room list and message history from a local server
- [ ] **Phase 4: Send Messages** - Compose and send messages from Insert mode
- [ ] **Phase 5: Realtime** - Live incoming messages over DDP with reconnect
- [ ] **Phase 6: Company Server** - Run day to day against the company Rocket.Chat

## Phase Details

### Phase 1: Modal TUI Shell
**Goal**: A runnable ratatui app with the sidebar + messages + input layout, fake rooms, and Normal/Insert/Command modes
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: NAV-01, NAV-02, NAV-03, NAV-04
**Success Criteria** (what must be TRUE):
  1. User launches the app and sees a sidebar of fake rooms, a message pane and an input bar
  2. User moves with `hjkl`, `gg`/`G` and `Ctrl-d`/`Ctrl-u` in Normal mode
  3. Current mode is visible and user can switch between Normal, Insert and Command
  4. User quits with `:q`
**UI hint**: yes
**Plans:** 6/6 plans complete

Plans:
**Wave 1**
- [x] 01-01-PLAN.md — Walking skeleton, headless core: cargo crate (all audited crates), Action/App/reducer, Vim-notation keymap data, fake data seam with the D-13 dataset, j/k sidebar, `:q` command (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 01-02-PLAN.md — Walking skeleton, binary: view and layout, terminal lifecycle, input task, run loop (seed applied before the first draw), pty smoke, sanitizer, tiny sizes and panic restore (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 01-03-PLAN.md — Vim key matcher: trie, counts, 1 s timeout, pending keys and room in status, Ctrl-w focus, per-pane motion router (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*
- [x] 01-04-PLAN.md — Sidebar sections (Unread / Folders / Recent), folds (h/l on folder rows only, D-19), Enter opens a room, glyphs and markers (wave 4, parallel with 01-05)
- [x] 01-05-PLAN.md — Messages pane: wrap with hanging indent, Compact/TwoLine layouts, message cursor and scrolling, h/l no-ops (D-19) (wave 4, parallel with 01-04)

**Wave 5** *(blocked on Wave 4 completion)*
- [x] 01-06-PLAN.md — Insert mode and input bar: multiline input, Normal-mode motions in the input bar (D-18), Enter appends a local message, safe paste, full-screen snapshots (wave 5)

### Phase 2: Configuration System
**Goal**: The TUI is customizable through three TOML files in `~/.config/rockettui/`, with built-in defaults
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: CONF-01, CONF-02, CONF-03, CONF-04, CONF-05, CONF-06, CONF-07
**Success Criteria** (what must be TRUE):
  1. App starts normally with no config files present
  2. User changes the keymap preset, and rebinds a key in `keymap.toml`, and the behavior changes
  3. User edits `theme.toml` colors and the UI look changes
  4. User sets sidebar width, timestamp format and history size in `config.toml`
  5. A malformed config entry shows a clear error instead of a crash
**UI hint**: yes
**Plans:** 1/5 plans executed

Plans:
**Wave 1**
- [x] 02-01-PLAN.md — Config pipeline tracer: XDG loading, config.toml validation (D-12/D-13), one-pass multi-file problem report, refuse-to-start error screen (exit 1), message_layout end to end, isolated smoke harness (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*
- [ ] 02-02-PLAN.md — config.toml consumers: sidebar_width in layout and reducer geometry, chrono timestamp_format with shared RowCtx, key_timeout_ms in the run loop, local time in the binary (wave 2, parallel with 02-03)
- [ ] 02-03-PLAN.md — keymap.toml: curated action names, vim / vim-extras presets, none unbinding, cmd: bindings, duplicate / prefix-clash / lockout checks (wave 2, parallel with 02-02)

**Wave 3** *(blocked on Wave 2 completion)*
- [ ] 02-04-PLAN.md — theme.toml core and pane chrome: D-08 colors, D-09 element tables for sidebar / status / cmdline / input, border styles and types, validated markers (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*
- [ ] 02-05-PLAN.md — Messages pane theme and themeable author palette (D-10); every element path themeable (wave 4)

### Phase 3: Connect & Read
**Goal**: User authenticates with a PAT and reads real rooms and history from a local Docker Rocket.Chat
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: AUTH-01, AUTH-02, AUTH-03, ROOM-01, ROOM-02, ROOM-03, MSG-01, DLVR-01
**Success Criteria** (what must be TRUE):
  1. User supplies user id + token (env var or config) and the app authenticates
  2. Sidebar lists real channels, private groups and DMs, and user can open one
  3. User scrolls the message history of the opened room
  4. Encrypted rooms show "encrypted, not supported"
  5. App warns about a config file with credentials that is not `chmod 600`, and logs contain no token or message content
**UI hint**: yes
**Plans**: TBD

### Phase 4: Send Messages
**Goal**: User composes and sends messages to the open room
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: MSG-02
**Success Criteria** (what must be TRUE):
  1. User enters Insert mode, types a message and sends it
  2. The sent message appears in the room (verified in the official client)
**UI hint**: yes
**Plans**: TBD

### Phase 5: Realtime
**Goal**: Incoming messages appear live over a DDP WebSocket connection
**Mode:** mvp
**Depends on**: Phase 4
**Requirements**: MSG-03, MSG-04
**Success Criteria** (what must be TRUE):
  1. A message sent from another client appears in the open room without refreshing
  2. After a dropped connection, the app reconnects and resumes live updates
**UI hint**: yes
**Plans**: TBD

### Phase 6: Company Server
**Goal**: rockettui is usable day to day against the company Rocket.Chat
**Mode:** mvp
**Depends on**: Phase 5
**Requirements**: DLVR-02
**Success Criteria** (what must be TRUE):
  1. Company policy and PAT availability are confirmed
  2. User connects to the company server, reads, sends and receives live messages in normal daily use
**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Modal TUI Shell | 6/6 | Complete    | 2026-11-15 |
| 2. Configuration System | 1/5 | In Progress | - |
| 3. Connect & Read | 0/TBD | Not started | - |
| 4. Send Messages | 0/TBD | Not started | - |
| 5. Realtime | 0/TBD | Not started | - |
| 6. Company Server | 0/TBD | Not started | - |
