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

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-fast` for a trivial task inline, with no subagents and no PLAN.md
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
