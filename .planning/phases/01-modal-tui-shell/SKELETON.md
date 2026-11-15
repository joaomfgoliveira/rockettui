# Walking Skeleton — rockettui

**Phase:** 1
**Generated:** 2026-11-14

## Capability Proven End-to-End

> A user runs `cargo run`, sees the sidebar / messages / input layout listing fake rooms delivered through the data-source seam, presses `j`/`k` to move the sidebar cursor, types `:q` + Enter, and the app exits with the terminal fully restored (also after a panic).

Implemented by two tracer tasks that run back to back:

1. Plan `01-01` task 1, the **headless tracer** (stage A crate, stage B model, keys and data seam): `data::fake::seed()` Actions -> `app::update::update` -> keymap rows in Vim notation -> `j`/`k` move the sidebar selection -> `:q<CR>` runs the table-driven command -> `should_quit`, proven by `app::update::tests::tracer_seed_keys_quit` through the real reducer and keymap, with no terminal involved.
2. Plan `01-02` task 1, the **binary tracer** (stage C view, stage D terminal, input task, run loop, binary, pty harness): proven by `python3 -I scripts/smoke_pty.py target/debug/rockettui quit`, which drives the real binary through a pseudo-terminal, waits until the first frame shows both the seeded room `general` and `NORMAL` (with `general` first in the byte stream, which holds only because the run loop applies the seed batch before its first draw), sends `j`, `k`, `:q` and Enter, and requires exit 0 plus the alternate screen and bracketed paste being entered and left.

Plan `01-01` tasks 2-3 harden the headless layers (full Vim notation, redacting Debug, full D-13 dataset, mode/command edges) and plan `01-02` tasks 2-3 harden the terminal layers (sanitize choke point, tiny sizes, panic restore) without moving a seam.

### Why the skeleton spans two plans (and is still proven early)

By author decision at plan-check iteration 2 (B3) the original single skeleton plan (26 files, one five-part hardening task) was split so that every plan stays below the 15-file blocker threshold and every task has a bounded, independently verifiable scope. The cut follows the architecture's own seam: everything below `update()` is pure and testable without a terminal (plan 01-01, 14 committed files including the cargo-generated Cargo.lock; its test helpers `press` and `app_80x24` sit in an inline cfg(test) module at the end of `src/lib.rs`), everything that touches the terminal sits above it (plan 01-02, 13 files, including `src/test_util.rs`, which it creates by moving those helpers out of `src/lib.rs` under the same crate path). Each tracer task owns more than 10 files (14 and 12); both plans justify that inside the task: splitting a tracer would turn its first half into a foundation-only task with no end-to-end check, so the size is bounded by stage checks instead. The tracer-first guarantee holds for the pair:

- Each plan leads with its own `type="tracer"` task that wires every layer the plan adds end to end, with a real runnable check.
- Plan 01-02 depends only on plan 01-01, so the `cargo run` -> layout -> `j`/`k` -> `:q` -> restored-terminal path is proven by the first task after the headless core lands (the fourth task of the phase), and no expansion plan (01-03 onward) can start before it is green.
- An architectural fault in the core surfaces in the very first commit (headless tracer); an architectural fault in the terminal or run loop surfaces in plan 01-02's first commit, before the matcher, sidebar, messages or Insert-mode plans build on either.

## Template Adaptation (why there is no database or deployment here)

The generic skeleton template assumes a web stack: "one real DB read/write" and "dev deployment". rockettui is a single-process Rust TUI whose Phase 1 runs on in-memory fake data with no networking (networking starts in Phase 3, and company-server policy is still unconfirmed). The substitutions are:

| Template item | rockettui substitution | Why |
|---|---|---|
| Framework init, routing, build, lint | `cargo init --name rockettui` (edition 2024, lib + thin bin), module tree `app/ input/ ui/ data/`, `cargo clippy -D warnings`, `cargo fmt --check` | Rust/cargo is the build system; "routing" is the TEA `Action` dispatch |
| One real DB read AND one real write | The fake data-source seam (D-13): `data::fake::seed()` produces `Action::RoomsLoaded` / `FoldersLoaded` / `MessagesLoaded` that travel through the one Action channel into the reducer (the "write" into `App` state), and the view reads them back to render (the "read") | Phase 3 replaces the producer (REST tasks sending the same Actions) without touching the reducer or view |
| One real UI interaction wired to the API | One real key interaction through the full key pipeline: crossterm event -> input task -> channel -> `normalize` -> keymap data (D-07, Vim notation D-08) -> reducer: `j`/`k` moves the sidebar cursor; `:q<CR>` runs the table-driven `:q` command and quits | Exercises the same seam every later keybinding uses |
| Deployment to a dev environment | `cargo build` and `cargo run` working from a clean checkout, plus the pty smoke script as the documented end-to-end run | A local binary is the deliverable; there is no server to deploy to in Phase 1 |

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language / toolchain | Rust, edition 2024, `rust-version = "1.88"` (installed rustc 1.99.1) | Author's learning goal; highest MSRV in the tree is ratatui/textwrap 1.88 |
| TUI framework | `ratatui` 0.30 + `crossterm` 0.29 (feature `event-stream`), one crossterm in the tree | Fixed by author; `ratatui::init()/restore()` own raw mode, alt screen and the panic hook |
| Async runtime | `tokio` 1 (`rt-multi-thread`, `macros`, `sync`, `time`) + `futures` 0.3 for `EventStream` | Fixed by author; no `net` feature in Phase 1 |
| Architecture | Elm/TEA: one `App` (src/app/mod.rs), one `Action` enum (src/action.rs), one bounded `mpsc` channel (capacity 256), one reducer `app::update::update` | Project constraint; reducer is pure and unit-testable without a terminal |
| Event loop | Spawned input task maps crossterm events to `Action`s; the main loop drains the channel into a batch, applies it (with `Action::Clock(unix_now())` first) through the single `update` call site, then draws once, then waits; the seed Actions queued before the loop form the first batch, so the first frame already shows the fake rooms (plan 01-02); key-sequence timeout is a `tokio::select!` deadline in the wait step that emits `Action::KeyTimeout` (added in plan 01-03); no tick | Zero CPU while idle; deterministic first frame (plan-check B1); every clock read lives in `src/lib.rs`, none under `src/app` or `src/input` (grep gate) |
| Keymap | Data shaped `mode -> key sequence -> Action`, written in Vim notation (`<C-d>`, `gg`, `<C-w>h`), parsed by `input::key::parse_keys`. The one deliberate exception: the matcher of plan 01-03 names `<Esc>` and `<C-c>` itself as reset chords that clear pending keys and counts before the trie lookup (D-17); what those chords then fire still comes from keymap rows | D-07/D-08: the same rows Phase 2 loads from `keymap.toml`; Phase 2 may move the reset chords into keymap data too (for example a cancel-keys list) |
| Data seam | `data::fake::seed() -> Vec<Action>` sent on the Action channel at startup | D-13: Phase 3 swaps the producer, not the consumer |
| Untrusted text | Every rendered string from data or user input passes `ui::sanitize::{sanitize, sanitize_line}` before width math (added in plan 01-02 task 2; typed and pasted input too, plan 01-06) | Single choke point for terminal-escape and bidi spoofing defense before real server data arrives |
| Logging / Debug | No logging in Phase 1; no print macros in `src/`; `Message`, `Action`, `Chord` (plan 01-01), `Outcome`, `Matcher` (plan 01-03) and `InputBuffer` (plan 01-06) have hand-written redacting `Debug`; `StatusMsg` and `App` have none | Project security rule: never log message contents or typed text |
| Terminal lifecycle | `tui::init()` = `ratatui::init()` + bracketed paste + panic-hook layer; `tui::restore()` always runs after `run()` returns | Terminal is never left raw or in the alt screen |
| Persistence / auth / deployment | None in Phase 1 | Config arrives in Phase 2, auth and networking in Phase 3 |
| Directory layout | `src/{main,lib,tui,event,action}.rs`, `src/app/` (state + reducer), `src/input/` (key notation, keymap, matcher), `src/ui/` (pure view, layout, panes, sanitize, wrap), `src/data/` (domain types + fake seed), `scripts/smoke_pty.py` | Matches `docs/objective.md` planned modules; lib + thin bin keeps tests fast |
| Test strategy | `cargo test --lib` unit tests, `insta` snapshots of `TestBackend`, pty smoke script for the real binary | No terminal needed for reducer/matcher/view tests |

## Stack Touched in Phase 1

- [x] Project scaffold (cargo, build, clippy, rustfmt, cargo test + insta; every audited crate installed once) — plan 01-01 task 1
- [x] "Routing" — the single Action enum dispatched by the one reducer — plan 01-01 task 1
- [x] "Database" read + write — fake data seam (Actions written into `App` in plan 01-01 tasks 1 and 3, read back by the view in plan 01-02 task 1)
- [x] UI — at least one interactive element wired end-to-end (`j`/`k` sidebar cursor, `:q`) — headless in plan 01-01 task 1, on the real binary in plan 01-02 task 1
- [x] "Deployment" — `cargo build` / `cargo run` from a clean checkout + `scripts/smoke_pty.py` (`quit`, then `panic`) — plan 01-02 tasks 1 and 3

## Out of Scope (Deferred to Later Slices)

- Config files `config.toml` / `keymap.toml` / `theme.toml`, keymap presets, message-layout selector (Phase 2)
- Tab pane switch and Ctrl-k room switch (`vim-extras` preset, Phase 2)
- Authentication, REST, real rooms and history, "encrypted, not supported" notice (Phase 3)
- Sending to a server (Phase 4), DDP realtime and reconnect (Phase 5), company server (Phase 6)
- Real unread tracking, folder management and persistence (backlog)
- Logging to a file, SIGTERM/SIGHUP graceful restore, keyboard-enhancement (kitty) protocol

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions:

- Phase 1 expansion (plans 01-03..01-06): Vim key matcher with counts, timeout and the per-pane motion router (01-03), sidebar sections/folders and Enter to open a room (01-04, parallel with 01-05), wrapped message pane with cursor scrolling (01-05), multiline Insert mode plus the Normal-mode input-bar motions of D-18 and the full-screen proof (01-06)
- Phase 2: config / keymap / theme files loaded into the same keymap rows and App settings
- Phase 3: PAT login; REST tasks send `RoomsLoaded` / `MessagesLoaded` on the same channel instead of `data::fake::seed()`
- Phase 4: `SubmitInput` sends through the REST API instead of appending a local message
- Phase 5: DDP task pushes live messages as Actions on the same channel; reconnect
- Phase 6: point at the company server
