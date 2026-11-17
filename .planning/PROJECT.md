# rockettui

## What This Is

A terminal client for Rocket.Chat, written in Rust with ratatui and navigated with Neovim-style modal keybindings. It is a personal side project, built for fun and as a way to learn Rust, async, and TUI architecture, and it is meant to be used daily against the author's company self-hosted Rocket.Chat server.

## Core Value

Reading and sending Rocket.Chat messages (with live incoming messages) entirely from the keyboard in a modal, vim-like interface.

## Requirements

### Validated

- ✓ Authenticate with a Personal Access Token (user id + token) — Phase 3
- ✓ List rooms (channels, private groups, DMs) — Phase 3
- ✓ View message history of a room — Phase 3
- ✓ Vim-style navigation: Normal / Insert / Command modes, `hjkl`, `gg`/`G`, `Ctrl-d`/`Ctrl-u`, `:q` — Phase 1
- ✓ Layout: sidebar (rooms) + messages + input bar — Phase 1
- ✓ Config under `~/.config/rockettui/` (XDG, honoring `$XDG_CONFIG_HOME`) split into `config.toml`, `keymap.toml`, `theme.toml`; app runs with built-in defaults if files are absent — Phase 2
- ✓ Two selectable keymap presets in `config.toml`: `vim` (strict) and `vim-extras` (vim-inspired plus app keys such as `Tab` pane switch, `Ctrl-k` room switch); `keymap.toml` overrides the active preset — Phase 2 (`Ctrl-k` room switch still deferred)
- ✓ Fully customizable look via `theme.toml` (colors/styles for every UI element) — Phase 2

### Active

- [ ] Send a message
- [ ] Receive live incoming messages via the realtime API (DDP over WebSocket)
- [ ] Usable day to day on the company server for basic conversations (v0.1 "done")

### Out of Scope

- End-to-end encrypted (E2EE) rooms — shown as "encrypted, not supported"
- Voice/video calls, file uploads, admin features — not needed for basic chat
- Mouse-first UX — keyboard-driven by design
- Replacing the official client — personal tool, not a product
- Username/password/OTP login, threads, reactions, edit/delete, mentions, unread badges, markdown rendering, search, desktop notifications, keyring storage, config hot reload — deferred past v0.1

## Context

- Rust learning project: architecture choices favor clarity and learning value.
- Target server is a company self-hosted Rocket.Chat; its version and whether it allows Personal Access Tokens / third-party clients is not yet confirmed.
- Develop against a local Docker Rocket.Chat first, then point at the company server.
- Existing design notes live in `docs/objective.md`.
- Planned modules: `api/` (REST), `realtime/` (DDP), `app/` (state + reducer), `ui/` (widgets), `input/` (modes + keymap), `config/` (config, keymap, theme loading).
- Planned build order: hello-world ratatui app with fake rooms; TEA structure + modes + keymap matcher; config system; REST (token login, rooms, history); send messages; DDP realtime; company server.

## Constraints

- **Tech stack**: Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `tokio-tungstenite` (custom minimal DDP client), `serde` + `toml` — chosen by the author
- **Architecture**: Elm/TEA style — one `App` state, one `Action` enum, one channel, one reducer
- **Security**: Act only as a normal client with the user's own account; revocable PAT, never committed or logged; credentials from env var or `chmod 600` config file (warn otherwise); never log message contents
- **Config**: XDG convention at `~/.config/rockettui/`; TOML; invalid entries yield clear errors, not crashes
- **Policy**: Confirm company policy allows third-party clients and PATs before pointing at the company server

## Key Decisions

| Decision | Rationale | Outcome |
| --- | --- | --- |
| Project/crate name `rockettui` (renamed from `rocket-tui`) | Author preference | — Pending |
| Sidebar + messages + input layout | Familiar chat layout, matches original sketch | — Pending |
| Three config files (config/keymap/theme) in `~/.config/rockettui/` | Make the TUI completely customizable; XDG convention | — Pending |
| Two keymap presets (`vim` strict, `vim-extras`) with user overrides | Serve both strict-vim and pragmatic users | — Pending |
| PAT auth first | Simplest for v0.1; revocable | — Pending |
| Custom minimal DDP client over `tokio-tungstenite` | Learning value; Rocket.Chat realtime is DDP | — Pending |
| Elm/TEA architecture | Clean state/reducer model, good for learning | — Pending |
| Sidebar folders = Favorites then custom categories (A6) | Judged usable on real servers during Phase 3 UAT | ✓ Good |
| REST history is a one-shot snapshot; no live updates until DDP | Phase 3 scope; unread state refreshes only on `:reconnect` until Phase 5 | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-11-17 after Phase 3*
