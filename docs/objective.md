# rockettui — Objective

> Status: DRAFT. Written together, edit freely.

## Vision

A terminal client for Rocket.Chat, built with Rust and [ratatui](https://ratatui.rs), navigated with Neovim-style modal keybindings and fully customizable through plain config files. It is a fun side project and a way to learn Rust.

## Goals

- Read and send messages in channels and DMs from the terminal.
- Navigate entirely from the keyboard with vim-like modes and motions.
- Receive new messages in real time.
- Work against a company self-hosted Rocket.Chat server.
- Be completely customizable: behavior, keymap, and look are all user config.
- Learn Rust, async, and TUI architecture along the way.

## Non-goals (for now)

- End-to-end encrypted (E2EE) rooms. They will show as "encrypted, not supported".
- Voice/video calls, file uploads, admin features.
- Mouse-first UX.
- Replacing the official client.

## Scope: v0.1 (small)

Done means: usable day to day on the company server for basic conversations.

1. Authenticate with a Personal Access Token (user id + token).
2. List rooms (channels, private groups, DMs).
3. View message history of a room.
4. Send a message.
5. Live incoming messages via the realtime API (DDP over WebSocket).
6. Vim-style navigation: Normal / Insert / Command modes, `hjkl`, `gg`/`G`, `Ctrl-d`/`Ctrl-u`, `:q`.
7. Layout: sidebar (rooms) + messages + input bar.
8. Configuration from `~/.config/rockettui/` split across three files (see below).
9. Two selectable keymap presets: `vim` (strict) and `vim-extras` (vim-inspired plus app keys such as `Tab` to switch panes, `Ctrl-k` room switch). Chosen in `config.toml`; the keymap file overrides on top.

## Configuration

Follows the XDG convention: `$XDG_CONFIG_HOME/rockettui/`, defaulting to `~/.config/rockettui/`.

| File | Purpose |
| --- | --- |
| `config.toml` | General settings: server URL, credentials source (env var or token), keymap preset, history size, timestamp format, layout (sidebar width, etc.) |
| `keymap.toml` | Per-mode key bindings; overrides the selected preset |
| `theme.toml` | Colors and styles for every UI element (sidebar, messages, status bar, mode indicator, etc.) |

Principles:

- Sensible built-in defaults: the app runs with no config files present.
- Each file is optional and independently overridable; invalid entries produce a clear error, not a crash.
- Config files holding credentials must be `chmod 600`; the app warns otherwise.
- Format: TOML (`serde` + `toml`). Hot reload is a later idea.

## Later (ideas, not committed)

- Username + password + OTP login.
- Threads, reactions, edit/delete, mentions, unread badges.
- Light markdown rendering.
- Search (`/`), desktop notifications, bundled theme gallery.
- Token storage in the system keyring.
- Config hot reload.

## Technical direction

| Concern | Choice |
| --- | --- |
| UI | `ratatui` + `crossterm` |
| Async runtime | `tokio` |
| REST | `reqwest` + `serde` |
| Realtime | `tokio-tungstenite` (custom minimal DDP client) |
| Config | `serde` + `toml`, XDG paths (`~/.config/rockettui/`) |
| Architecture | Elm/TEA style: one `App` state, one `Action` enum, one channel, one reducer |

Planned modules: `api/` (REST), `realtime/` (DDP), `app/` (state + reducer), `ui/` (widgets), `input/` (modes + keymap), `config/` (loading config, keymap, theme).

## Security principles

- Act only as a normal client with the user's own account.
- Use a revocable Personal Access Token; never commit it or log it.
- Load credentials from env var or a `chmod 600` config file.
- Do not log message contents.
- Develop against a local Docker Rocket.Chat first, then the company server.
- Confirm company policy allows third-party clients and PATs (not checked yet; the app is already used against the company server).

## Milestones

1. Hello-world ratatui app with fake room list and `j/k` navigation.
2. TEA structure, modes, and keymap matcher.
3. Config system: `config.toml`, `keymap.toml`, `theme.toml`, presets, defaults.
4. REST: login with token, list rooms, load history (local Docker server).
5. Send messages.
6. DDP realtime.
7. Point at the company server. Done: in use day to day, no company-specific differences found so far.

## Open questions

- Which Rocket.Chat version does the company run? Is the policy on third-party clients and PATs formally confirmed?
