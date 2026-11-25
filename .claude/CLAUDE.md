<!-- GSD:project-start source:PROJECT.md -->

## Project

**rockettui**

A terminal client for Rocket.Chat, written in Rust with ratatui and navigated with Neovim-style modal keybindings. It is a personal side project, built for fun and as a way to learn Rust, async, and TUI architecture, and it is meant to be used daily against the author's company self-hosted Rocket.Chat server.

**Core Value:** Reading and sending Rocket.Chat messages (with live incoming messages) entirely from the keyboard in a modal, vim-like interface.

### Constraints

- **Tech stack**: Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `tokio-tungstenite` (custom minimal DDP client), `serde` + `toml` — chosen by the author
- **Architecture**: Elm/TEA style — one `App` state, one `Action` enum, one channel, one reducer
- **Security**: Act only as a normal client with the user's own account; revocable PAT, never committed or logged; credentials from env var or `chmod 600` config file (warn otherwise); never log message contents
- **Config**: XDG convention at `~/.config/rockettui/`; TOML; invalid entries yield clear errors, not crashes
- **Policy**: Confirm company policy allows third-party clients and PATs before pointing at the company server

<!-- GSD:project-end -->

<!-- GSD:stack-start source:STACK.md -->

## Technology Stack

Technology stack not yet documented. Will populate after codebase mapping or first phase.
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

## Workflow

This project uses OpenSpec (`openspec/`), not GSD. `.planning/` is a frozen archive of Phases 1-4; do not extend it.

- `openspec/specs/` holds what the app does today (one spec per capability).
- `openspec/changes/` holds work in flight. Start a change with `/openspec-propose`, think first with `/openspec-explore`, implement with `/openspec-apply-change`, and finish with `/openspec-archive-change`.
- Behavior changes go through a change so the specs stay current. Trivial fixes and refactors with no behavior change can be edited directly.

<!-- GSD:profile-start -->

## Features list

On every new feature, update `/home/joliveira/Documents/Obsidian Vault/Personal/Projects/Rockettui/Features.md`: tick the checkbox or add a line under the right section, and refresh the `Progress` count and `Last updated` date. Do it when the change is archived (`/openspec-archive-change`) or when a direct edit adds user-visible behavior.

## Secrets and servers

Hard rules, no exceptions, even when a task seems to need them:

- Never read, source, print or pass on the user's token or any credential: not `.env*` files, not `dev/state/`, not `config.toml`'s `token`, not environment variables that hold them. If a task needs a live call, give the user the command to run themselves and ask for the non-secret result.
- Never contact the company Rocket.Chat server or anything but the local Docker server and mocks. Live checks happen only against the user's own local server, run by the user.
- Never look for a way around a guard or hook that blocks this. A block is the answer.

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
