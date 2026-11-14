# Phase 1: Modal TUI Shell - Research

**Researched:** 2026-11-14
**Domain:** Rust TUI (ratatui + crossterm + tokio), TEA architecture, modal key matcher, wrapped-text rendering
**Confidence:** HIGH (stack and terminal behavior verified by compiling a spike, reading crate sources, and driving a real pty this session)

## User Constraints (from CONTEXT.md)

<user_constraints>

### Locked Decisions

**Focus & pane model**
- **D-01:** Focus moves between panes in Normal mode with `Ctrl-w` + `h`/`j`/`l` (vim window-style). Tab / Ctrl-k belong to the `vim-extras` preset (Phase 2), not Phase 1.
- **D-02:** In the sidebar, `j`/`k` only move the cursor; `Enter` opens the room and moves focus to the messages pane.
- **D-03:** `i` (and `a`/`o`) enters Insert mode on the input bar from any pane; `Esc` returns to Normal mode and to the previously focused pane.
- **D-04:** Messages pane has a line-based cursor on a message: `j`/`k` move it and scroll at edges, `gg`/`G` jump to oldest/newest, `Ctrl-d`/`Ctrl-u` half-page. A room opens positioned at the newest message.

**Key matcher**
- **D-05:** Matcher supports count prefixes (`5j`, `3<C-d>`) from the start; counts are generic over actions.
- **D-06:** Multi-key sequences use a trie/prefix matcher with a timeout (~1s, to become configurable later). Pending keys are shown in the status line. Ambiguous prefix waits for the timeout, then fires or cancels.
- **D-07:** The default keymap is built-in data shaped `mode -> key sequence -> Action`, the same structure Phase 2 will load from `keymap.toml`. No hardcoded per-mode `match` blocks.
- **D-08:** Key syntax is Vim notation: `<C-d>`, `<Esc>`, `<CR>`, `gg`. This becomes the `keymap.toml` syntax.

**Layout & chrome**
- **D-09:** Bottom status bar shows mode, current room and pending keys; a vim-style `:` command-line row sits below it.
- **D-10:** Two message render layouts are implemented as an enum: compact (`HH:MM author  text`, word wrap with hanging indent) and two-line (author/timestamp header, indented body). Phase 1 uses one built-in default (compact); Phase 2 adds the `config.toml` selector.
- **D-11:** Sidebar has three sections: Unread, Folders, and everything else ordered by most recent activity. Phase 1 is layout only: render the sections from fake data, `j`/`k` across them, fold/unfold of folders, fake unread flags. Real unread tracking and folder management/persistence are deferred.
- **D-12:** Room model carries a kind enum covering channel, private group, discussion, team and DM; all kinds can be placed in folders (membership by room id). Include one encrypted room in fake data for ROOM-03 later.
- **D-13:** Fake data: ~8 rooms, 50+ messages in some rooms, one very long wrapped message, one empty room. It sits behind a replaceable data-source boundary so Phase 3 swaps it for real data. Sidebar width fixed (~28 cols) until config exists.

**Insert & Command behavior**
- **D-14:** Input bar is multiline from the start (grows with content, with a sensible max height). Editing: typing, Backspace, arrows/Home/End, `Ctrl-w`, `Ctrl-u`.
- **D-15:** `Enter` in Insert mode appends a local fake message to the current room, clears input and stays in Insert mode. Phase 4 swaps this for an API send.
- **D-16:** Command mode supports only `:q` / `:quit`; unknown commands show an error in the status bar. Dispatch is table-driven so later phases can add commands.
- **D-17:** `:` from Normal opens Command mode; `Esc` cancels any mode back to Normal; `Ctrl-c` acts as `Esc` and does not quit. Only `:q` quits.

### Claude's Discretion
- Key used to insert a newline in the multiline input (Shift-Enter is unreliable in terminals; likely `Alt-Enter` or `Ctrl-j`).
- Max height of the input bar, exact fold keys for sidebar folders, fake data content, border/glyph styling, event-loop/tick design, terminal setup/teardown and panic-restore handling, module skeleton (following TEA: one `App`, one `Action` enum, one channel, one reducer).

### Deferred Ideas (OUT OF SCOPE)
- Real unread tracking (overlaps deferred MSG-06) and in-app folder management with persistence — future phase/backlog.
- `config.toml` setting to choose message layout — Phase 2.
- Tab pane switch and Ctrl-k room switch — `vim-extras` preset, Phase 2.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| NAV-01 | User can switch between Normal, Insert and Command modes, with the current mode shown | `Mode` enum in `App`; mode -> keymap tries (Pattern 2); status bar widget with mode badge (Pattern 6); `Esc`/`Ctrl-c` normalization (Pitfall 3) |
| NAV-02 | In Normal mode, `hjkl`, `gg`/`G` and `Ctrl-d`/`Ctrl-u` move and scroll | Trie matcher with `ExactAndPrefix` timeout for `g`/`gg` (Pattern 2); key normalization so `G` arrives as `Char('G')`+SHIFT is handled (Pitfall 1); generic motion Actions interpreted by focus; row-based scroll model (Pattern 5) |
| NAV-03 | User can quit with `:q` in Command mode | Table-driven command dispatch (Pattern 7); `ratatui::restore()` teardown path (Pattern 1) |
| NAV-04 | Layout is sidebar (rooms) + messages + input bar | Pure `ui::layout::areas()` shared by view and reducer (Pattern 4); `TestBackend` + `insta` snapshots (Validation Architecture) |
</phase_requirements>

## Summary

The stack is fixed (ratatui + crossterm + tokio, TEA). Current versions work together cleanly: **ratatui 0.30.2** (MSRV 1.88), **crossterm 0.29.0**, **tokio 1.54.1** (MSRV 1.85), all well under the installed rustc 1.99.1, so edition 2024 is usable. A spike project in the scratchpad compiled and ran with these versions, was driven through a real pty (quit restores the alternate screen; a deliberate panic restores the terminal *before* the panic message prints), and confirmed there is exactly one `crossterm` in the dependency tree. Phase 1 needs no networking crates.

The three genuinely tricky parts are (1) the key matcher: crossterm delivers `G` as `Char('G')` **with a SHIFT modifier**, `Ctrl-j` as `Char('j')`+CONTROL, `Alt-Enter` as `Enter`+ALT, and `Ctrl-c` as an ordinary key in raw mode (no SIGINT), so a normalization layer is mandatory before the trie; (2) a wrapped-text message pane with a message-granular cursor but row-granular scrolling, which needs a pure shared layout/wrap function so the reducer (for `Ctrl-d` half pages and scroll-into-view) and the renderer agree; and (3) untrusted-text hygiene: ratatui's `Buffer` already drops grapheme clusters containing control characters (verified), but this also silently eats tabs and leaves bidi-override characters through, so a sanitizer must be built in now as the single choke point for all remote text.

Everything else is conventional: use `ratatui::init()/restore()` (it installs the panic hook), merge crossterm's `EventStream` into one bounded `mpsc` Action channel via a spawned input task, and handle the ~1s key-sequence timeout with a `tokio::select!` deadline in the main loop that emits `Action::KeyTimeout`. This keeps the reducer free of clocks and trivially unit-testable.

**Primary recommendation:** Build as a `lib + thin bin` crate with `input/` (hand-rolled Vim-notation parser + trie matcher, ~250 lines, tested table-driven), `app/` (state + pure reducer), `ui/` (pure `view` + shared `layout`/`wrap`/`sanitize`), `data/` (domain types + fake seed delivered as Actions); use `textwrap` for body wrapping, hand-roll the multiline input buffer, and verify with `cargo test` + `insta` snapshots on `TestBackend`.

## Project Constraints (from CLAUDE.md)

Read from `/home/joliveira/personal-projects/rockettui/.claude/CLAUDE.md`:
- **Tech stack (fixed by author):** Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `tokio-tungstenite`, `serde` + `toml`. Phase 1 uses only the first three.
- **Architecture:** Elm/TEA — one `App` state, one `Action` enum, one channel, one reducer.
- **Security:** act only as a normal client; never log tokens or message contents. (Relevant now: do not derive `Debug` on types that carry message text without redaction; do not print to stdout/stderr while in the alternate screen.)
- **Config:** XDG, TOML — not in this phase, but the keymap data shape (D-07/D-08) must be TOML-ready.
- **GSD workflow:** file edits go through a GSD command (execute-phase), not ad hoc.
- No project skills found (`.claude/skills/`, `.agents/skills/` do not exist) [VERIFIED: `ls` this session].

## Architectural Responsibility Map

This is a single-process TUI, not a web app; "tiers" are the layers of the TEA loop.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Terminal raw mode / alt screen / panic restore | Terminal shell (`tui.rs`) | — | One place owns setup and teardown so no other code can leave the terminal dirty |
| Reading crossterm events, filtering Press/Release | Input task (`event.rs`) | — | Only producer of `Action::Key/Resize/Paste`; keeps blocking/async IO out of the reducer |
| Key normalization, notation parsing, trie matching, counts, pending state | Input layer (`input/`) | Reducer (owns the matcher state in `App`) | Pure data + pure functions; testable without a terminal |
| Key timeout (~1s) | Event loop (`select!` deadline) | Reducer (`Action::KeyTimeout`) | Clock stays outside the reducer; reducer only resolves "ambiguous prefix now" |
| Mode, focus, cursors, folds, input buffer, status message | Reducer / `App` state | — | TEA: all state transitions in one pure `update` |
| Which Action a key maps to | Keymap data (`input/keymap.rs`) | — | D-07: data, not per-mode `match` |
| Interpreting `MoveDown` per focused pane | Reducer | — | Keymap stays `mode -> seq -> Action`; focus decides what the motion means |
| Pane geometry (sidebar 28 cols, status, cmdline, input height) | `ui::layout` (pure fn of `Rect` + input rows) | Reducer reads it for page size | Shared so scroll math and rendering never disagree |
| Word wrap, width, sanitize | `ui::wrap` / `ui::sanitize` | — | Single choke point for every remote string |
| Fake rooms/messages | `data/fake.rs` delivered as Actions | — | D-13 boundary: Phase 3 sends the same Actions from REST tasks |
| Rendering | `ui::view` (pure `&App -> Frame`) | ratatui `Terminal` diff | No side effects in view |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ratatui | 0.30.2 (MSRV 1.88, published 2026-06-19) | Widgets, layout, `init()/restore()`, `TestBackend` | Fixed by author; 0.30 split into `ratatui-core/-widgets/-crossterm` crates, default features include `crossterm` (-> `crossterm_0_29`) [VERIFIED: crates.io API + `docs.rs/crate/ratatui/0.30.2/features`] |
| crossterm | 0.29.0 with feature `event-stream` | Terminal events (`EventStream`), key types | ratatui 0.30 re-exports `ratatui::crossterm`, but `event-stream` is only enabled if you add a **direct** dependency with the feature; cargo unifies it to the same single crate (`cargo tree -i crossterm` shows one `crossterm v0.29.0`) [VERIFIED: spike build + `ratatui-0.30.2/src/lib.rs:483`] |
| tokio | 1.54.1 (MSRV 1.85) features `rt-multi-thread, macros, sync, time` | Runtime, `mpsc` Action channel, `select!`, `sleep_until` | Fixed by author [VERIFIED: crates.io API] |
| futures | 0.3.34 | `StreamExt::next()` for crossterm's `EventStream` | `EventStream` implements `futures_core::Stream`; `tokio-stream` is NOT needed [VERIFIED: spike compiles with only `futures`] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| textwrap | 0.16.4 (MSRV 1.88), default features | Word-wrap message bodies (unicode-width aware, breaks over-long words, honors `\n`) | Wrapping the body of each message to `pane_width - prefix_width` |
| unicode-width | 0.2.3 | Display width of strings/chars for cursor column and prefix width | Input cursor math, prefix width |
| unicode-segmentation | 1.13.3 | Grapheme-cluster-aware cursor movement/backspace in the input buffer | `InputBuffer` (emoji ZWJ sequences, combining marks) |
| insta | 1.49.0 (MSRV 1.66), dev-dependency | Snapshot tests of `TestBackend` output | Render snapshots at fixed sizes |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled multiline input buffer | `ratatui-textarea` 0.9.4 (ratatui-org fork of `tui-textarea`, published 2026-10-31, MSRV 1.86, depends on ratatui-core 0.1) | Crate is maintained and has soft wrap + undo, but its `TextArea::input(key)` consumes raw key events with Emacs bindings, bypassing the Action channel and keymap (conflicts with TEA/D-07), and it hides the cursor/wrap math that is a core Rust-learning exercise. The original `tui-textarea` 0.7.0 was last released 2024-10-22 and targets older ratatui — do not use it. Choose the crate only if the author decides input editing is not worth learning. |
| `textwrap` for body wrap | Hand-written wrap loop with `unicode-width` | Hand-rolling is a legitimate learning exercise but the edge cases (wide chars at boundary, long tokens, ZWJ emoji, `\n`) are exactly what `textwrap` already handles (verified outputs below). Recommend `textwrap`; keep the wrap behind one function so it can be swapped. |
| ratatui `Paragraph` + `Wrap` | — | `Paragraph::wrap` has no hanging indent; pre-wrap into `Vec<Line>` instead. |
| `color-eyre` | `anyhow`/plain `io::Result` | Phase 1 has no fallible logic beyond terminal IO; `std::io::Result` is enough. Skip `color-eyre` (installs its own panic hook that interacts with ratatui's hook ordering). Add `anyhow`/`thiserror` in Phase 2 with config errors. |
| Time crate (`chrono`/`time`) | `i64` unix-seconds in `Message` + tiny `HH:MM` formatter | Defer a time crate to Phase 2 (CONF-07 timestamp format). |

**Installation:**
```bash
cargo init --name rockettui   # edition 2024 default with cargo 1.99.1; also writes .gitignore (/target)
cargo add ratatui@0.30
cargo add crossterm@0.29 --features event-stream
cargo add tokio@1 --features rt-multi-thread,macros,sync,time
cargo add futures@0.3 unicode-width@0.2 unicode-segmentation@1 textwrap@0.16
cargo add --dev insta@1
```
`rust-version = "1.88"` may be set in `Cargo.toml` (highest MSRV in the tree is ratatui/textwrap 1.88) [VERIFIED: crates.io API]. Installed toolchain is rustc/cargo 1.99.1 [VERIFIED: `rustc --version`].

**Version verification:** versions above were read from crates.io API this session: ratatui 0.30.2 (2026-06-19), crossterm 0.29.0 (2025-04-05, no newer stable), tokio 1.54.1 (2026-10-21), futures 0.3.34 (2026-11-02), unicode-width 0.2.3 (2026-10-28), unicode-segmentation 1.13.3 (2026-06-01), textwrap 0.16.4 (2026-09-13), insta 1.49.0 (2026-10-03). Spike `Cargo.lock` resolved exactly these.

## Package Legitimacy Audit

Run via `gsd_run query package-legitimacy check --ecosystem crates ...` (verdicts from the seam; all discovered via the author's fixed stack or official ratatui docs).

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| ratatui | crates.io | 3.7 yrs | 2.1M/wk | github.com/ratatui/ratatui | OK | Approved |
| crossterm | crates.io | 8.8 yrs | 4.6M/wk | github.com/crossterm-rs/crossterm | OK | Approved |
| tokio | crates.io | 10 yrs | 24.2M/wk | github.com/tokio-rs/tokio | OK | Approved |
| futures | crates.io | 10 yrs | 17.2M/wk | github.com/rust-lang/futures-rs | OK | Approved |
| unicode-width | crates.io | 11 yrs | 17.9M/wk | github.com/unicode-rs/unicode-width | OK | Approved |
| unicode-segmentation | crates.io | 11 yrs | 14.2M/wk | github.com/unicode-rs/unicode-segmentation | OK | Approved |
| textwrap | crates.io | 9.9 yrs | 5.4M/wk | github.com/mgeisler/textwrap | OK | Approved |
| insta | crates.io | 7.8 yrs | 3.1M/wk | github.com/mitsuhiko/insta | OK | Approved |
| ratatui-textarea | crates.io | 3.1 yrs | 61k/wk | github.com/ratatui/ratatui-textarea | OK | Not recommended (alternative only, not installed) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
Crates have no install-time `postinstall`; build scripts of transitive deps are the normal Rust risk and are out of scope here.

## Architecture Patterns

### System Architecture Diagram

```
                         ┌──────────────────────────── tokio runtime ───────────────────────────┐
 terminal (stdin) ──►  [input task: crossterm EventStream]                                       │
   key bytes            │  Press/Repeat -> Action::Key(KeyEvent)   (Release dropped)             │
   resize               │  Resize(w,h)  -> Action::Resize                                         │
   bracketed paste      │  Paste(s)     -> Action::Paste(String)                                  │
                        ▼                                                                         │
 data::fake::seed() ─► ┌───────────────┐   bounded mpsc (256)    ┌──────────────────────────────┐ │
 (Phase 3: REST tasks) │ Action channel│ ──────────────────────► │ main loop                    │ │
                       └───────────────┘                         │  select!{ rx.recv(),         │ │
                                                                 │          key_deadline→KeyTimeout}│
                                                                 │  drain try_recv() batch      │ │
                                                                 │        │                     │ │
                                                                 │        ▼                     │ │
                                                                 │  update(&mut App, Action)    │ │
                                                                 │   Key ─► normalize ─► Matcher│ │
                                                                 │         (mode trie, count,   │ │
                                                                 │          pending) ─► Action* │ │
                                                                 │   Counted(n, a) ─► repeat a  │ │
                                                                 │   motion ─► by focus pane    │ │
                                                                 │   Submit ─► push fake msg    │ │
                                                                 │   Command :q ─► should_quit  │ │
                                                                 │        │                     │ │
                                                                 │        ▼                     │ │
                                                                 │  rearm/clear key_deadline    │ │
                                                                 │  terminal.draw(ui::view(&app))│ │
                                                                 └──────────────┬───────────────┘ │
                        └────────────────────────────────────────────────────────┼─────────────────┘
                                                                 should_quit ───► ratatui::restore() ─► shell
 panic anywhere ─► panic hook (restore, then print message)
```

### Recommended Project Structure
```
Cargo.toml                 # edition 2024, [lib]+[[bin]] implicit
src/
├── main.rs                # thin: tui::init, #[tokio::main], run loop, tui::restore
├── lib.rs                 # pub mod ...; pub async fn run(...)
├── tui.rs                 # init/restore wrappers + bracketed paste + extra panic-hook layer
├── event.rs               # spawn_input_task(tx): EventStream -> Action
├── action.rs              # the single Action enum (unit variants = keymap-bindable)
├── app/
│   ├── mod.rs             # App state, Mode, Focus
│   ├── update.rs          # pure reducer: update(&mut App, Action)
│   ├── sidebar.rs         # sidebar_rows(&App) -> Vec<SidebarRow>, fold state, cursor
│   ├── messages.rs        # message cursor + row-based scroll math
│   ├── input_buf.rs       # InputBuffer (lines, grapheme cursor)
│   └── command.rs         # table-driven `:` commands
├── input/
│   ├── key.rs             # Chord, normalize(KeyEvent), parse_keys(&str), format_keys
│   ├── trie.rs            # Trie<A>, Lookup
│   ├── matcher.rs         # Matcher: per-mode pending + count state, feed()/timeout()
│   └── keymap.rs          # default_keymap() built from ("normal","<C-w>h", Action::FocusLeft) rows
├── ui/
│   ├── mod.rs             # view(frame, &App)
│   ├── layout.rs          # areas(Rect, input_rows) -> Areas  (pure)
│   ├── sidebar.rs  messages.rs  input.rs  status.rs
│   ├── wrap.rs            # wrap_body(text, width), layout enum Compact|TwoLine
│   └── sanitize.rs        # sanitize(&str) -> Cow<str>
└── data/
    ├── mod.rs             # Room, RoomKind, Message, Folder (domain types)
    └── fake.rs            # seed() -> Vec<Action> (RoomsLoaded, MessagesLoaded, ...)
tests/ (or inline #[cfg(test)])   # snapshots in src/**/snapshots/
```
`app/`, `ui/`, `input/` match `docs/objective.md` planned modules [VERIFIED: `docs/objective.md:76`: "Planned modules: `api/` (REST), `realtime/` (DDP), `app/` (state + reducer), `ui/` (widgets), `input/` (modes + keymap), `config/`"]. Library + thin binary lets integration tests and snapshot tests import everything and keeps `cargo test --lib` fast.

### Pattern 1: Terminal lifecycle (init, bracketed paste, panic-safe restore)
**What:** Use `ratatui::init()` (raw mode, alt screen, installs a panic hook that restores before the previous hook runs) and `ratatui::restore()`. Add bracketed paste on top, with its own disable in teardown and in the hook, because `ratatui::restore()` only disables raw mode and leaves the alternate screen.
**Why bracketed paste in Phase 1:** D-14/D-15 make bare `Enter` submit; without bracketed paste a pasted multi-line text arrives as N Enter presses (N sends). With it, the terminal wraps the paste and crossterm yields a single `Event::Paste(String)` (verified in the pty test: `Paste("hello\nworld")`). `bracketed-paste` is a default crossterm feature [VERIFIED: `crossterm-0.29.0/Cargo.toml` features block: `default = ["bracketed-paste", "events", "windows", "derive-more"]`].
**Verified facts** (read from `ratatui-0.30.2/src/init.rs`, this session):
- `pub fn init() -> DefaultTerminal` (line 365), `pub fn restore()` (line 524), `pub fn run<F, R>(f: F) -> R` where `F: FnOnce(&mut DefaultTerminal) -> R` (lines 318-325, calls `init()`, `f`, then `restore()`).
- `try_restore` doc: "disabling raw mode first is important as it has more side effects than leaving the alternate screen buffer" then `execute!(stdout(), LeaveAlternateScreen)`.
- Panic hook: `std::panic::take_hook(); std::panic::set_hook(Box::new(move |info| { restore(); hook(info); }))`. Docs say to call `init()` **after** installing any other panic hooks.
**Example:**
```rust
// Source: spike verified under a pty this session; APIs from ratatui 0.30.2 init.rs + crossterm 0.29
use std::io::stdout;
use ratatui::{DefaultTerminal, crossterm::{execute, event::{EnableBracketedPaste, DisableBracketedPaste}}};

pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();                    // raw mode + alt screen + panic hook
    let _ = execute!(stdout(), EnableBracketedPaste);
    let prev = std::panic::take_hook();                // ratatui's hook: restore() then original hook
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableBracketedPaste);
        prev(info);
    }));
    terminal
}

pub fn restore() {
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
}
```
Early returns: keep `restore()` in `main` after the run result is captured (`let res = run(...).await; tui::restore(); res`) rather than relying on `?` inside, or use a drop guard. `ratatui::run(|t| ...)` is fine only if the closure does not need the tokio runtime; here use `init()/restore()` around an async `run`.

### Pattern 2: Key pipeline — normalize -> parse -> trie -> matcher (hand-rolled, data-driven)
**What:** Four small pure pieces; verified compiling and passing 7 unit tests in the spike (`shift_g_normalizes`, `ctrl_d`, `ctrl_w_h_seq`, `backtab`, `errors`, `lt`, `trie`).
1. `Chord { code: KeyCode, mods: KeyModifiers }` — `Hash + Eq` (crossterm's `KeyCode` and `KeyModifiers` derive both [VERIFIED: `crossterm-0.29.0/src/event.rs:839` `#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)] pub struct KeyModifiers`; spike compiled `HashMap<Chord,_>`]).
2. `normalize(KeyEvent) -> Chord`: keep only CONTROL/ALT/SHIFT; **drop SHIFT when `code` is a `Char`** (case is in the char); map `BackTab` to `Tab`+SHIFT; lowercase the char under CONTROL.
3. `parse_keys(&str) -> Result<Vec<Chord>, ParseError>` for Vim notation (`gg`, `<C-d>`, `<C-w>h`, `<Esc>`, `<CR>`, `<A-CR>`, `<S-Tab>`, `<Space>`, `<BS>`, `<lt>` for a literal `<`). Return a real error type (Phase 2 shows these as config errors, CONF-03). Also write `format_keys(&[Chord]) -> String` (inverse) for the status-line pending display; add a round-trip test over every default-keymap entry.
4. `Trie<A>` with `Lookup::{None, Prefix, Exact(&A), ExactAndPrefix(&A)}`; `ExactAndPrefix` is the `g` (when `gg` is also bound) case that triggers the timeout.

**Matcher semantics** (per `Mode`, state lives in `App` so the reducer stays pure):
- State: `pending: Vec<Chord>`, `count: Option<u32>`.
- Count (D-05): Normal mode only; digits `1-9` start a count, `0-9` extend it **only if a count has started** (so `0` stays bindable). Use `saturating_mul/saturating_add` and clamp to a max (e.g. 9999) — an overflow panics in debug builds and a huge count would otherwise loop the reducer.
- On `feed(chord)`: append to `pending`; `lookup`:
  - `Exact(a)` -> emit `a` (wrapped as `Counted(n, a)` if a count is set), clear state.
  - `Prefix` / `ExactAndPrefix` -> stay pending (status line shows `count + format_keys(pending)`); loop re-arms the deadline.
  - `None` -> clear state; in Insert/Command mode, an unmatched plain `Char(c)` (no CONTROL/ALT) becomes `Action::InsertChar(c)` (text entry fallback — keep Insert/Command keymaps single-key in Phase 1 so typing is never delayed).
- `Esc` and `Ctrl-c` always cancel pending/count first (D-17), then fall through to the bound `Cancel` action.
- On `Action::KeyTimeout`: if `pending` resolves to `ExactAndPrefix(a)` fire `a`; otherwise discard. Clear state.

**One-enum Action & keymap data (D-07/D-08):**
```rust
// Unit variants are keymap-bindable (Phase 2 can serde-derive them with snake_case names and
// #[serde(skip)] the data-carrying event variants). Prefer EnterInsert/EnterCommand over SetMode(Mode).
#[derive(Clone, PartialEq)]            // custom Debug: do NOT print Paste/message text
pub enum Action {
    // motions (meaning depends on focused pane)
    MoveDown, MoveUp, MoveLeft, MoveRight, GotoTop, GotoBottom, HalfPageDown, HalfPageUp, Select,
    FocusLeft, FocusDown, FocusRight,                 // <C-w>h/j/l (D-01)
    EnterInsert, EnterNormal, EnterCommand, Cancel,
    ToggleFold,                                       // sidebar folder fold keys
    // insert/command editing
    InsertChar(char), InsertNewline, DeleteBack, DeleteWordBack, DeleteToLineStart,
    CursorLeft, CursorRight, CursorUp, CursorDown, CursorHome, CursorEnd,
    SubmitInput, ExecuteCommand,
    // events / data (not bindable)
    Key(KeyEvent), KeyTimeout, Resize(u16, u16), Paste(String),
    RoomsLoaded(Vec<Room>), MessagesLoaded(RoomId, Vec<Message>),
    Counted(u32, Box<Action>),                        // generic count wrapper (D-05)
    Quit,
}
```
Reducer handles `Counted(n, a)` generically (`for _ in 0..n { update(app, (*a).clone()) }`), with `GotoTop/GotoBottom` special-cased to treat the count as a line number (vim `5gg`) or ignoring it. The default keymap is rows `(Mode::Normal, "<C-w>h", Action::FocusLeft)` parsed at startup with `parse_keys` — the exact string syntax Phase 2 will read from `keymap.toml` [CONTEXT.md D-07, D-08].

**Prior art:** Helix's `helix-term/src/keymap.rs` uses the same shape: `KeyTrieNode { map: IndexMap<KeyEvent, KeyTrie> }` and `KeymapResult::{Pending, Matched, MatchedSequence, NotFound, Cancelled}`; `Esc` with pending state returns `Cancelled`, and its `get()` has **no timeout** (pending only changes on the next key) [CITED: raw.githubusercontent.com/helix-editor/helix/master/helix-term/src/keymap.rs]. The `Lookup` enum above adds the `ExactAndPrefix` + timeout behavior D-06 requires, which Helix does not implement.

**Default Normal keymap (recommended):**

| Keys | Action | Notes |
|------|--------|-------|
| `j` `k` `h` `l` | MoveDown/Up/Left/Right | sidebar: `j/k` move; `h`/`l` collapse/expand folder under cursor (NERDTree convention); messages: `j/k` message cursor; `h/l` no-op in Phase 1 |
| `gg` / `G` | GotoTop / GotoBottom | `g` is `ExactAndPrefix`-free unless `g` alone is bound — do not bind bare `g` |
| `<C-d>` / `<C-u>` | HalfPageDown / HalfPageUp | count-aware |
| `<C-w>h` `<C-w>j` `<C-w>l` | FocusLeft / FocusDown / FocusRight | D-01; `<C-w>` alone is a pure `Prefix` |
| `<CR>` | Select | sidebar room: open + focus messages (D-02); folder row: toggle fold |
| `za` | ToggleFold | discretionary fold key; `h`/`l` are the ergonomic ones |
| `i` `a` `o` | EnterInsert | D-03 (all three identical in Phase 1) |
| `:` | EnterCommand | |
| `<Esc>` `<C-c>` | Cancel | D-17 |

Insert: `<Esc>`/`<C-c>` -> EnterNormal, `<CR>` -> SubmitInput, `<A-CR>` and `<C-j>` -> InsertNewline, `<BS>`, `<C-w>` -> DeleteWordBack, `<C-u>` -> DeleteToLineStart, arrows/`<Home>`/`<End>` -> cursor motions. Command: `<CR>` -> ExecuteCommand, `<Esc>`/`<C-c>` -> Cancel, `<BS>` (on empty buffer, cancel like vim).

### Pattern 3: One channel, one reducer, deadline-in-loop for the key timeout
**What:** A spawned input task is the only producer in Phase 1; Phase 3+ network tasks are added as more producers on the same `Sender<Action>`. The loop merges the channel with an optional deadline using `tokio::select!`; no polling, zero CPU while idle (no tick needed; nothing animates).
**Verified:** crossterm `EventStream` works under tokio with only the `futures` crate (spike compiled, ran in a pty, quit cleanly).
```rust
// Source: spike this session (compiled + ran under pty); ratatui 0.30.2 / crossterm 0.29 / tokio 1.54
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::{sync::mpsc, time::{Instant, sleep_until}};

pub fn spawn_input_task(tx: mpsc::Sender<Action>) {
    tokio::spawn(async move {
        let mut events = EventStream::new();
        while let Some(Ok(ev)) = events.next().await {
            let action = match ev {
                Event::Key(k) if k.kind != KeyEventKind::Release => Action::Key(k), // Press + Repeat
                Event::Resize(w, h) => Action::Resize(w, h),
                Event::Paste(s) => Action::Paste(s),
                _ => continue,                                    // focus/mouse ignored
            };
            if tx.send(action).await.is_err() { break; }
        }
        let _ = tx.send(Action::Quit).await;                      // stream ended/errored: exit cleanly
    });
}

async fn sleep_until_opt(d: Option<Instant>) {
    match d { Some(d) => sleep_until(d).await, None => std::future::pending().await }
}

pub async fn run(mut terminal: DefaultTerminal, mut app: App) -> std::io::Result<()> {
    let (tx, mut rx) = mpsc::channel::<Action>(256);
    spawn_input_task(tx.clone());
    for a in data::fake::seed() { let _ = tx.send(a).await; }     // D-13 seam: same Actions Phase 3 will send
    app.set_size(terminal.size()?);
    let mut key_deadline: Option<Instant> = None;
    loop {
        terminal.draw(|f| ui::view(f, &app))?;
        let first = tokio::select! {
            a = rx.recv() => a,
            _ = sleep_until_opt(key_deadline) => Some(Action::KeyTimeout),
        };
        let Some(first) = first else { break };
        let mut saw_key = false;
        let mut batch = vec![first];
        while let Ok(a) = rx.try_recv() { batch.push(a); }        // coalesce bursts: one draw per batch
        for a in batch {
            saw_key |= matches!(a, Action::Key(_));
            let timed_out = matches!(a, Action::KeyTimeout);
            update(&mut app, a);
            if timed_out { key_deadline = None; }
        }
        if saw_key {                                              // re-arm only after a key changed pending state
            key_deadline = app.keys_pending().then(|| Instant::now() + KEY_TIMEOUT);
        }
        if app.should_quit { break; }
    }
    Ok(())
}
```
The reducer never reads a clock; unit tests call `update(&mut app, Action::KeyTimeout)` directly. (Phase 3+ alternative, if the reducer ever needs to start async work: have `update` return `Vec<Effect>` that the loop executes — not needed now.) Use `tokio::time::sleep_until` with `tokio::time::Instant`.

### Pattern 4: Pure layout shared by view and reducer
**What:** `ui::layout::areas(area: Rect, input_rows: u16) -> Areas { sidebar, messages, input, status, cmdline }`.
```rust
// Source: ratatui 0.30.2 Layout API (compiled in spike: Layout::vertical([...]).areas(f.area()))
let [body, status, cmdline] = Layout::vertical([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)]).areas(area);
let [sidebar, main] = Layout::horizontal([Constraint::Length(28), Constraint::Min(1)]).areas(body);
let [messages, input] = Layout::vertical([Constraint::Min(3), Constraint::Length(input_rows + 2)]).areas(main); // +2 borders
```
`input_rows` = wrapped line count of the input buffer clamped to `1..=MAX_INPUT_ROWS` (recommend 5; discretionary). The reducer stores `App.size: Size` (from `terminal.size()` at start and `Action::Resize`) and calls `areas()` to get the messages pane height/width for half-page and scroll-into-view math, so tests can set any size and assert exact offsets. `Terminal::draw` auto-resizes its buffers (`autoresize()` is called inside draw, `ratatui-core-0.1.2/src/terminal/render.rs:196`), so `Resize` only needs to update `App.size` and trigger the normal redraw.
Use `saturating_sub` everywhere on `u16` geometry (tiny terminals: 0x0, 10x3) — snapshot/smoke-test at tiny sizes.

### Pattern 5: Messages pane — message cursor, row-based scroll, shared wrap
**What:** `App.messages_view: { cursor: usize (message index), top_row: usize (scroll offset in rendered rows) }` per room (store in a `HashMap<RoomId, ViewState>` so reopening a room keeps position; "a room opens positioned at the newest message" per D-04 means set cursor = last on first open).
- Render model: for each message produce `Vec<Line>` = first line `[HH:MM][ ][author][2 spaces]` styled prefix spans + first body row; continuation rows = `" " * prefix_width` + body row (hanging indent). Body rows = `textwrap::wrap(sanitized_body, max(width - prefix_width, MIN_BODY))`.
- Row count per message is a pure function `message_rows(&Message, width, MessageLayout) -> usize`; the reducer uses prefix sums to scroll: after any cursor move, `top_row` is adjusted so the cursor message's rows are visible (if the message is taller than the viewport, align its top). `HalfPageDown/Up` = move cursor to the message at `top_row + viewport/2 ± ` and scroll the same number of rows.
- Render only the window `[top_row, top_row + height)` (slice the `Vec<Line>`; do not render thousands of rows); highlight the cursor message's rows with a style (reversed or subtle bg) via `Line::style`.
- `MessageLayout::{Compact, TwoLine}` enum (D-10): TwoLine = header line `author  HH:MM` then body rows indented by 2. Both implement the same `render_rows` signature so the cursor/scroll math is layout-agnostic.
- Cache of wrapped rows keyed by `(message id, width, layout)` is an optimization; with 50-ish fake messages re-wrapping per frame is fine — do not build the cache in Phase 1.
- Width handling: wrap *then* sanitize? No — **sanitize first**, then wrap (so width math and rendered text agree).
- Verified `textwrap 0.16.4` behavior (spike, width 20): ordinary words wrap at spaces; a 48-char token splits into 20/20/8; CJK wide chars counted as width 2 (`[日本語のテキストはと]` = 20); `"line one\nline two\n\nline four"` -> 4 rows incl. an empty row; ZWJ family emoji kept as one 2-wide cluster; `wrap("", w)` returns one empty row; `width 0` and `1` do not panic (they return 1 char/row). With `Options::new(10).subsequent_indent("      ")` it also does hanging indent, but styled prefix spans make "wrap body only, compose prefix yourself" cleaner.

### Pattern 6: Sidebar as a derived row list + `List`/`ListState`
- `sidebar_rows(&App) -> Vec<SidebarRow>` where `enum SidebarRow { Header(Section), Folder { id, open }, Room { id, indent } }`; pure, unit-tested; recomputed after any fold toggle. Cursor = index into rows, **skipping `Header` rows** on `j/k`, clamped after recompute.
- Render with ratatui `List` + `ListState` (handles scrolling to keep the selection visible); section headers dim/bold; unread rooms bold with a marker; `RoomKind` shown as a prefix glyph (`#` channel, lock private, `@` DM, etc. — styling discretionary).
- Placement rule (**ASSUMED, flag to author**): each room appears exactly once with precedence Unread > Folder > Rest (rest sorted by `last_activity` descending). Duplicating a room across Unread and a Folder makes cursor identity and `Select` ambiguous. Folder rows may show an unread count. Fold/unfold on `h`/`l`/`za`/`<CR>`-on-folder.
- Room model: `Room { id: RoomId, name, kind: RoomKind, encrypted: bool, unread: bool, last_activity: i64 }`; `enum RoomKind { Channel, PrivateGroup, Discussion, Team, DirectMessage }`; `Folder { id, name, room_ids: Vec<RoomId>, open: bool }` (membership by room id, D-12). One fake room has `encrypted: true`.

### Pattern 7: Multiline input buffer (hand-rolled) and table-driven commands
- `InputBuffer { lines: Vec<String>, row: usize, col: usize /* byte index at a grapheme boundary */ }` with methods `insert_char`, `insert_str` (paste: sanitize, normalize `\r\n`->`\n`, cap size e.g. 64 KiB), `newline`, `backspace`, `delete_word_back` (Ctrl-w: skip trailing whitespace then a run of non-whitespace; at column 0 join with previous line), `delete_to_line_start` (Ctrl-u), `left/right` (by grapheme via `unicode-segmentation`), `home/end`, `up/down` (**by logical line**, preserve display column via `unicode-width`; visual-row movement through soft-wrapped lines is deliberately out of scope), `take() -> String`.
- Rendering: wrap each logical line with the same `wrap` helper (width = pane inner width), draw as `Paragraph`; compute the hardware cursor `(x, y)` from `display_width(line[..col])` and wrapped row index; `frame.set_cursor_position((x, y))` only in Insert/Command mode — otherwise omit, because `Terminal::draw` hides the cursor when no position is set (`render.rs:298`: `None => self.hide_cursor()?`).
- Commands: `const COMMANDS: &[CommandDef] = &[CommandDef { names: &["q", "quit"], run: cmd_quit }]` where `run: fn(&mut App, &[&str]) -> Result<(), String>`; unknown name -> `app.status_msg = Some(StatusMsg::Error(format!("E492: Not an editor command: {name}")))`, cleared on next key. Trim and ignore an empty command.

### Anti-Patterns to Avoid
- **Per-mode `match key.code { ... }` blocks:** violates D-07; every binding must come from the keymap data.
- **Matching `KeyEvent` directly** (e.g. `KeyCode::Char('G')` with `modifiers == NONE`): fails because `G` arrives with SHIFT. Always go through `normalize`.
- **Clock inside the reducer / `std::thread::sleep` / `event::poll` loop in an async fn:** blocks the runtime; use `EventStream` + `select!`.
- **`println!`/`eprintln!`/`dbg!`/`tracing` to stderr while in the alternate screen:** corrupts the display; any later logging goes to a file.
- **`#[derive(Debug)]` on `Message`/`Action` carrying text, then logging it:** violates the "never log message contents" principle; write a redacting `Debug` impl for `Message` (show id/len only) and for `Action::Paste`.
- **Rendering from the reducer or mutating `App` in `view`:** keep `view(&App)` pure; geometry comes from the shared `layout::areas`.
- **Drawing after every single event in a burst:** drain `try_recv()` then draw once.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Terminal raw mode, alt screen, panic restore | Own `enable_raw_mode` + hook | `ratatui::init()/restore()` | Already installs the panic hook and orders disable-raw-before-leave-alt correctly [VERIFIED: init.rs] |
| Word wrap incl. wide chars and long tokens | Own wrap loop | `textwrap::wrap` | CJK/emoji/long-word/`\n` edge cases verified correct |
| Display width / grapheme boundaries | `s.len()`/`chars().count()` | `unicode-width`, `unicode-segmentation` | Byte or char counts are wrong for CJK/emoji; cursor column drifts |
| Sidebar scrolling | Manual offset math | `ratatui::widgets::List` + `ListState` | Keeps selection visible automatically |
| Layout splitting | Manual Rect arithmetic | `ratatui::layout::Layout` + `Constraint` | Handles tiny areas without underflow |
| Screen snapshot tests | Manual buffer-to-string + file compare | `insta` + `TestBackend` Display | `assert_snapshot!(terminal.backend())` works out of the box (verified) |
| Escape/control stripping safety net | — | ratatui `Buffer` already drops control-containing graphemes (`ratatui-core-0.1.2/src/buffer/buffer.rs:351`) | But it is not enough alone (see Security); keep your own sanitizer as the primary control |

Deliberately hand-rolled (learning value, small, well-bounded): Vim-notation parser + trie matcher + count handling; multiline `InputBuffer`; reducer; message scroll math.

**Key insight:** the deceptive complexity is Unicode (width, graphemes, wrapping) and terminal quirks (key encodings); lean on crates for those and spend the hand-rolling budget on the architecture (matcher, reducer, state model) where the learning goal lives.

## Common Pitfalls

### Pitfall 1: `G`, Ctrl-keys and Enter variants arrive in surprising shapes
**What goes wrong:** `G` bound as `Char('G')`/no modifiers never fires; `Ctrl-j` fires as "j with CONTROL"; `Enter` vs `Ctrl-m` indistinguishable.
**Verified wire behavior** (real pty run of crossterm 0.29 `EventStream`, this session):
- `G` -> `code: Char('G'), modifiers: SHIFT`; `g` -> `Char('g')`, none.
- `Ctrl-j` (byte 0x0A) -> `Char('j')` + CONTROL (raw mode). `Enter` (0x0D) -> `Enter`, no modifiers. `Alt-Enter` (ESC CR) -> `Enter` + ALT. `Alt-x` -> `Char('x')` + ALT.
- `Ctrl-c` (0x03) -> `Char('c')` + CONTROL as a normal key (raw mode disables ISIG; no SIGINT). `Ctrl-w` -> `Char('w')` + CONTROL, `Ctrl-d` -> `Char('d')` + CONTROL, `Ctrl-h` (0x08) -> `Char('h')` + CONTROL, while real Backspace (0x7F) -> `Backspace`.
- `Ctrl-[` (0x1B) -> `Esc` (identical to Esc); `Tab` -> `Tab`; `Ctrl-Space` -> `Char(' ')` + CONTROL; `Home` (ESC [ H) -> `Home`.
- Bracketed paste -> a single `Event::Paste("hello\nworld")`.
**How to avoid:** single `normalize()` used by both the parser and the live path; table-test every default binding against its raw bytes' decoded event. Newline-in-input: bind **both** `<A-CR>` and `<C-j>` (Shift-Enter is not distinguishable without the kitty protocol); document it. `Ctrl-j`/`Ctrl-m`/`Ctrl-i`/`Ctrl-[` collisions are inherent to legacy terminals — never bind `<C-i>`/`<C-m>` as distinct from Tab/Enter.
**Warning signs:** `G` does nothing; `Esc` sequences mis-detected as `Alt-<key>` when typing fast over slow SSH (crossterm treats `ESC x` as `Alt-x`).

### Pitfall 2: Key kinds (Press/Repeat/Release)
**What goes wrong:** On Windows (and on unix only if you opt into the kitty keyboard protocol) crossterm also reports Release events; processing them double-fires every key. **Fix:** drop `KeyEventKind::Release`, accept `Press` and `Repeat` so holding `j` scrolls. The enum is `pub enum KeyEventKind { Press, Repeat, Release }` [VERIFIED: `crossterm-0.29.0/src/event.rs:905-909`]. Do **not** push keyboard-enhancement flags in Phase 1 (they are terminal-dependent and require matching `PopKeyboardEnhancementFlags` on teardown/panic). On the author's Linux terminal every observed key arrived as `kind: Press`.

### Pitfall 3: `Esc`/`Ctrl-c`/`Ctrl-w` collisions
- `Ctrl-c` does not quit in raw mode (D-17 satisfied by default) — it must be *bound* to `Cancel`. External `SIGTERM`/`SIGHUP` still kill the process without restoring; optional hardening: `tokio::signal::unix` (needs tokio feature `signal`) mapping SIGTERM to `Action::Quit` [ASSUMED nice-to-have].
- `Ctrl-w` is both the Normal-mode window prefix (D-01) and Insert-mode delete-word (D-14). Different mode tries make this safe. Some terminal multiplexers/browsers' embedded terminals intercept `Ctrl-w`/`Ctrl-d` [ASSUMED]; not a problem for a native terminal.
- Lone `Esc` has latency only inside crossterm's parser for ambiguous sequences; the matcher's own ~1s timeout is separate and only applies to multi-key bindings.

### Pitfall 4: Terminal left dirty
**What goes wrong:** panic inside a *spawned* task fires the global panic hook (which restores the terminal) while the main loop keeps running, leaving a live app on a cooked terminal. **Fix:** the input task has no panicking code paths by design; if tasks are added later, await/abort their `JoinHandle`s and treat a task panic as fatal (`Action::Quit`). Early `?` returns: always call `tui::restore()` after the run future completes regardless of `Result`. Verified: normal quit emitted `\x1b[?1049l` (leave alt screen) and `\x1b[?25h` (show cursor); a forced panic emitted `\x1b[?1049l` **before** the panic text.

### Pitfall 5: Resize and tiny terminals
`Event::Resize` -> update `App.size`, redraw; `Terminal::draw` auto-resizes buffers. `u16` underflow (`area.height - 2`) panics in debug builds on tiny terminals; use `saturating_sub` and make panes degrade (hide sidebar/messages below a minimum rather than panic). Include snapshot/smoke tests at 0x0, 1x1, 10x3, 80x24, 200x60.

### Pitfall 6: Flicker and cursor
ratatui diffs the buffer so there is no flicker if you call `draw` once per loop iteration and never write to stdout directly. Batch-drain the channel before drawing. Hardware cursor appears only when `Frame::set_cursor_position` is called in that frame; omit it in Normal mode (cursor hidden automatically).

### Pitfall 7: Wide characters and the cursor
Cursor x = `UnicodeWidthStr::width(&line[..col])`, not byte offset or char count. Backspace/arrow must move by grapheme cluster (`unicode-segmentation`), otherwise ZWJ emoji and combining marks corrupt the buffer. A pane narrower than a wide char (width 1 vs a 2-wide glyph) must not loop forever in custom wrap code — `textwrap` returns rows (verified width 0/1 no panic).

### Pitfall 8: Tab characters and control chars silently disappear
Verified: `Paragraph` rendering `"tab\there"` produced `tabhere`, because `Buffer::set_stringn` drops any grapheme containing a control char (`buffer.rs:351`: `.filter(|symbol| !symbol.contains(char::is_control))`). Sanitizer must expand `\t` to spaces before wrapping and normalize `\r\n`/`\r` to `\n`.

### Pitfall 9: Count overflow / unbounded repetition
`999999999999j` overflows `u32` (debug panic) and a large count would loop the reducer. Clamp with saturating math; cap (e.g. 9999) and apply motions as clamped jumps, not N redraws.

## Code Examples

### Normalize + parse (verified in spike, 7 tests passing)
```rust
// Source: spike src/keys.rs this session (crossterm 0.29)
pub fn normalize(ev: KeyEvent) -> Chord {
    let mut mods = ev.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
    let code = match ev.code {
        KeyCode::BackTab => { mods |= KeyModifiers::SHIFT; KeyCode::Tab }
        KeyCode::Char(c) => {
            mods.remove(KeyModifiers::SHIFT);          // 'G' already encodes shift
            if mods.contains(KeyModifiers::CONTROL) { KeyCode::Char(c.to_ascii_lowercase()) } else { KeyCode::Char(c) }
        }
        other => other,
    };
    Chord { code, mods }
}
// parse_keys("gg") -> [g,g]; parse_keys("<C-w>h") -> [C-w, h]; parse_keys("<C-d") -> Err(UnclosedAngle(0))
// Trie::lookup(&[g]) with both g and gg bound -> Lookup::ExactAndPrefix(&single_g)
```
(`<C-X>` handling in `parse_angle`: split on `-`, last part is the key, earlier parts are `C|A|M|S`, names `Esc/CR/Enter/Tab/BS/Del/Space/lt/Up/Down/Left/Right/Home/End`; unknown -> `ParseError::UnknownKey`.)

### Snapshot test of the view
```rust
// Source: spike this session (ratatui 0.30.2 TestBackend + insta 1.49.0)
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn normal_mode_80x24() {
    let app = App::with_fake_data(Size::new(80, 24));
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| ui::view(f, &app)).unwrap();
    insta::assert_snapshot!(t.backend());        // symbols only; quoted per-row lines
}
// Style assertions (mode badge color, focus border): inspect t.backend().buffer().cell((x, y)).unwrap().style()
```
A missing snapshot **fails** the test by default (`stored new snapshot ... .snap.new`, `test ... FAILED`, "To update snapshots run `cargo insta review` or set `INSTA_UPDATE=always`") [VERIFIED: spike]. `cargo-insta` is not installed, so first-time snapshot creation is `INSTA_UPDATE=always cargo test`, then review the `.snap` diffs by eye and commit them.

### Reducer unit test (no terminal)
```rust
#[test]
fn gg_goes_to_top_via_timeout_free_exact_match() {
    let mut app = App::with_fake_data(Size::new(80, 24));
    app.open_room(RoomId::from("general"));
    update(&mut app, key('g')); assert!(app.keys_pending());
    update(&mut app, key('g'));
    assert_eq!(app.messages.cursor, 0);
}
#[test]
fn bare_g_then_timeout_cancels_when_g_alone_unbound() {
    let mut app = App::with_fake_data(Size::new(80, 24));
    update(&mut app, key('g')); update(&mut app, Action::KeyTimeout);
    assert!(!app.keys_pending());
}
#[test]
fn ctrl_c_cancels_not_quits() { /* update(Key Ctrl-c) in Insert -> mode == Normal && !should_quit */ }
#[test]
fn colon_q_quits() { /* ':' 'q' <CR> -> should_quit */ }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `crossterm` is a direct ratatui dependency; manual `enable_raw_mode` + `EnterAlternateScreen` + hook | `ratatui::init()/restore()/run()` (hook included); ratatui re-exports `ratatui::crossterm` | ratatui 0.28-0.30 | Use `init/restore`; no manual setup |
| ratatui is one crate | 0.30 split into `ratatui-core`, `ratatui-widgets`, `ratatui-crossterm` (+ termion/termwiz/termina backends); `crossterm_0_28`/`crossterm_0_29` features select the version | ratatui 0.30 | Only matters for crate selection: `tui-textarea 0.7.0` targets older ratatui; `ratatui-textarea 0.9.4` targets `ratatui-core 0.1` |
| `tui-textarea` (rhysd) | `ratatui-textarea` (ratatui org fork "maintained independently") | 2026 | Relevant only if the author chooses a textarea crate |
| `Frame::size()` / `set_cursor(x, y)` | `Frame::area()` / `set_cursor_position(...)` | ratatui 0.28-0.29 | Use new names (spike compiled with `f.area()`) |

**Deprecated/outdated:** `tui-textarea` 0.7.0 (2024-10-22) for new work; `color-eyre` hook-ordering complexity not needed.

## Runtime State Inventory

Omitted — greenfield phase (no rename/refactor/migration; no `Cargo.toml`/`src/` exists).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Placement rule: each room appears once, precedence Unread > Folder > Rest | Pattern 6 | Author may expect an unread room in a folder to show in both; changes `sidebar_rows` and cursor identity (low cost now, needs confirmation) |
| A2 | `h`/`l` collapse/expand folders in the sidebar, `za` toggles, `<CR>` on a folder row toggles (fold keys are Claude's discretion) | Pattern 2/6 | Author may prefer other keys; data-driven keymap makes it a one-line change |
| A3 | Max input height 5 content rows (+2 border) | Pattern 4 | Cosmetic |
| A4 | Bounded channel size 256 | Pattern 3 | None in Phase 1; revisit when DDP floods arrive |
| A5 | SIGTERM/SIGHUP -> clean quit is a nice-to-have, not required | Pitfall 3 | Terminal left raw if the process is killed by SIGTERM; low impact |
| A6 | `Alt-Enter` reaches the app as `Enter`+ALT in the author's terminal/multiplexer (verified only at the byte level on a pty: ESC CR -> Enter+ALT) | Pitfall 1 | If the terminal/tmux swallows it, `Ctrl-j` is the bound fallback; confirm in the manual UAT |
| A7 | Terminal multiplexers/some terminals may intercept `Ctrl-w`/`Ctrl-d` | Pitfall 3 | Would surface in manual UAT; remap via keymap in Phase 2 |
| A8 | Up/Down in the input move by logical line, not visual wrapped row | Pattern 7 | Slightly un-vim-like when a long line wraps; acceptable for Phase 1 |
| A9 | `MIN_BODY` wrap width (e.g. 10) when the pane is very narrow | Pattern 5 | Cosmetic |

## Open Questions

1. **Does an unread room that is also in a folder appear in Unread, the Folder, or both?**
   - What we know: D-11 lists three sections; D-12 allows any kind in folders.
   - What's unclear: precedence/duplication.
   - Recommendation: show once, Unread wins (A1); folder row shows an unread count. Confirm with the author before the sidebar plan locks.
2. **Hand-roll the multiline input or adopt `ratatui-textarea`?**
   - What we know: crate is maintained (0.9.4, 2026-10-31) but owns key handling, conflicting with D-07/TEA; the project's stated goal is learning Rust/TUI architecture.
   - Recommendation: hand-roll (Pattern 7). Revisit only if the author wants undo/redo, selections, etc.
3. **Should Phase 1 enable bracketed paste?**
   - Recommendation: yes (small, prevents N accidental sends once Phase 4 sends for real).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc / cargo | build | yes | 1.99.1 (Fedora 1.99.1-2.fc43) | — |
| cargo clippy | lint gate | yes | `/usr/bin/cargo-clippy` | — |
| rustfmt | format gate | yes | `/usr/bin/rustfmt` | — |
| crates.io network | `cargo add`/build | yes | spike built all deps | — |
| cargo-insta | snapshot review | no | — | `INSTA_UPDATE=always cargo test`, review `.snap` diff in git |
| cargo-nextest | faster tests | no | — | plain `cargo test` (suite is tiny) |
| python3 | optional pty smoke script | yes | 3.14 | manual UAT in a real terminal |
| real terminal / pty | manual UAT of raw mode, resize, Ctrl-c | yes (pty via python verified) | — | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `cargo-insta` (use env var), `cargo-nextest` (not needed).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | built-in `cargo test` (libtest) + `insta` 1.49.0 for render snapshots |
| Config file | none — see Wave 0 (`Cargo.toml` dev-dependency; snapshots stored in `src/**/snapshots/`) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NAV-01 | `i`/`a`/`o` -> Insert, `:` -> Command, `Esc`/`Ctrl-c` -> Normal, never quits | unit (reducer) | `cargo test --lib app::update::tests::mode_` | Wave 0 |
| NAV-01 | Mode name shown in status bar | snapshot | `cargo test --lib ui::status::tests` | Wave 0 |
| NAV-02 | `hjkl`, `gg`, `G`, `<C-d>`/`<C-u>`, counts (`5j`, `3<C-d>`) move cursor/scroll | unit (reducer + matcher) | `cargo test --lib app::update::tests::motion_` | Wave 0 |
| NAV-02 | Notation parse/format round trip, normalize table (`G`+SHIFT, Ctrl letters, BackTab), trie `Exact/Prefix/ExactAndPrefix/None`, timeout fire vs cancel | unit | `cargo test --lib input::` | Wave 0 |
| NAV-02 | Wrap with hanging indent, wide chars, one very long message, row-count/scroll-into-view math | unit | `cargo test --lib ui::wrap::tests app::messages::tests` | Wave 0 |
| NAV-03 | `:q`/`:quit` set `should_quit`; `:foo` sets error status, does not quit | unit | `cargo test --lib app::command::tests` | Wave 0 |
| NAV-04 | Sidebar (sections, fold, unread flag), messages pane, input bar, status + cmdline rows at 80x24 | snapshot | `cargo test --lib ui::tests::layout_80x24` | Wave 0 |
| NAV-04 | No panic at 0x0, 1x1, 10x3, 200x60 | unit (render smoke) | `cargo test --lib ui::tests::tiny_sizes` | Wave 0 |
| (security) | sanitizer strips ESC/CSI/OSC/C1/bidi/NUL, expands tab, normalizes CRLF | unit | `cargo test --lib ui::sanitize::tests` | Wave 0 |
| (security) | `Debug` of `Message`/`Action::Paste` does not contain body text | unit | `cargo test --lib data::tests::debug_redacts` | Wave 0 |
| NAV-01..04 | End-to-end smoke: launches, shows layout, `:q` exits 0, terminal restored; panic restores | optional pty script (python3 -I) or manual UAT | `python3 -I scripts/smoke_pty.py target/debug/rockettui` | Wave 0 (optional) |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** full suite command above
- **Phase gate:** full suite green, plus manual UAT (real terminal: Ctrl-c in each mode, resize while running, paste multi-line, panic restore) before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `Cargo.toml` + `src/lib.rs` skeleton with the module tree (nothing exists yet)
- [ ] dev-dependency `insta`; first snapshot generation via `INSTA_UPDATE=always cargo test`, commit `.snap` files (a missing snapshot fails by default)
- [ ] test helpers: `key('g')`, `ctrl('d')` constructors producing `Action::Key`; `App::with_fake_data(size)`
- [ ] `.gitignore` containing `/target` (written by `cargo init`)
- [ ] optional `scripts/smoke_pty.py` modeled on the pty harness used for this research (spawns binary under `pty.fork`, sends keys, asserts `\x1b[?1049l` and exit code 0)

## Security Domain

`security_enforcement` is enabled (ASVS level 1, block on high) [VERIFIED: `.planning/config.json` workflow.security_enforcement true, security_asvs_level 1]. No network, auth or credential handling exists in this phase; the relevant surface is what the renderer and input layer do with text that will become untrusted in Phase 3+.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Phase 3 |
| V3 Session Management | no | Phase 3 |
| V4 Access Control | no | single local user |
| V5 Input Validation / Output Encoding | yes | `ui::sanitize` applied to every remote string before wrap/render (message bodies, authors, room names, folder names); paste sanitization + size cap; clamp counts |
| V6 Cryptography | no | none in this phase |
| V7 Error Handling & Logging | yes | no logging of message text; redacting `Debug`; no stdout/stderr writes in alt screen |
| V14 Config | no | Phase 2 |

### Known Threat Patterns for TUI + untrusted chat text

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Terminal escape injection (ANSI CSI/OSC, title set, OSC 52 clipboard, cursor moves, 8-bit C1 `U+009B`) via message/author/room names | Tampering / Spoofing | Strip all `char::is_control()` except `\n` (and expand `\t`) in `sanitize()` **before** wrap/width math; ratatui's `Buffer` additionally drops control-containing graphemes (verified: `ESC[31m` rendered as literal `[31m`, `ESC]0;pwn BEL` dropped, `U+009B` dropped) — defense in depth, not the primary control |
| Bidi / invisible-character spoofing (`U+202A-202E`, `U+2066-2069`, `U+200B-200F`, `U+FEFF`) in author/room names | Spoofing | Strip bidi overrides/isolates and zero-width format chars in `sanitize()`; they are **not** `is_control()` so ratatui does not drop them (not verified per-codepoint; implement from the Unicode categories) |
| Oversized message / paste / repeat count causing CPU or memory blowup | Denial of service | Clamp counts (saturating, cap ~9999); cap pasted/input size (e.g. 64 KiB); render only the visible window; `textwrap` handles width 0/1 without panic |
| Message content leaking into logs/panic output | Information disclosure | Redacting `Debug` for `Message` and `Action::Paste`; no tracing in Phase 1; never print `App`/`Action` with `{:?}` |
| Terminal left in raw mode / alt screen after crash | Denial of service (usability) | `ratatui::init` panic hook + `tui::restore()` after the run result; verified under a pty |
| Accidental multi-send from pasted newlines | Tampering (integrity of user intent) | Bracketed paste -> single `Action::Paste` (verified) |

Plan threat model should cover: escape/control injection via rendered text (primary), bidi spoofing, input/paste/count DoS, log redaction, terminal-state restoration. The sanitizer must have unit tests with real payloads (`"\x1b[2J\x1b]52;c;ZXZpbA==\x07"`, `"\u{202E}evil"`, `"\u{9b}31m"`, `"a\tb"`, `"\r\n"`) and a render test asserting none of those bytes appear in `TestBackend` cells.

## Sources

### Primary (HIGH confidence)
- Local source reads this session: `ratatui-0.30.2/src/init.rs` (init/restore/run/panic hook), `ratatui-0.30.2/src/lib.rs:483` (`pub use ratatui_crossterm::crossterm;`), `ratatui-core-0.1.2/src/buffer/buffer.rs:351` (control-char filter), `ratatui-core-0.1.2/src/terminal/render.rs:196,298` (autoresize, hide cursor), `crossterm-0.29.0/src/event/sys/unix/parse.rs` (legacy key decoding), `crossterm-0.29.0/src/event.rs` (`KeyEventKind`, `KeyModifiers`), `crossterm-0.29.0/Cargo.toml` (features)
- crates.io API (versions, MSRV, features, dependencies) for ratatui, crossterm, tokio, futures, tokio-stream, insta, unicode-width, unicode-segmentation, textwrap, tui-textarea, ratatui-textarea, tui-input, anyhow, color-eyre
- Spike project (scratchpad): compiled ratatui 0.30.2 + crossterm 0.29 (event-stream) + tokio 1.54.1 + futures + insta + textwrap; pty-driven runs for key decoding, quit and panic restore; 7 matcher unit tests passing; `insta` snapshot behavior
- `gsd_run query package-legitimacy check` verdicts (all OK)

### Secondary (MEDIUM confidence)
- docs.rs ratatui 0.30.2 `init` and features pages (fetched via WebFetch) [CITED: docs.rs/ratatui/0.30.2/ratatui/fn.init.html, docs.rs/crate/ratatui/0.30.2/features]
- ratatui.rs TEA page: `update(model, msg) -> Option<Message>`, follow-up messages [CITED: ratatui.rs/concepts/application-patterns/the-elm-architecture/]
- Helix keymap trie prior art [CITED: raw.githubusercontent.com/helix-editor/helix/master/helix-term/src/keymap.rs]
- ratatui-textarea README (fork of tui-textarea, soft wrap, undo) [CITED: github.com/ratatui/ratatui-textarea]

### Tertiary (LOW confidence)
- Items tagged `[ASSUMED]` in the Assumptions Log (terminal/multiplexer key interception, SIGTERM handling, layout sizing choices)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - versions and features read from crates.io and confirmed by an actual build; MSRV vs rustc 1.99.1 checked
- Architecture: HIGH - event loop, restore and panic behavior exercised in a pty; matcher skeleton compiled and unit-tested; scroll/sidebar models are design recommendations (MEDIUM until implemented)
- Pitfalls: HIGH for key decoding and sanitization (empirically verified); MEDIUM for emulator-specific behavior (Alt-Enter, multiplexers) which was verified only at the byte level

**Research date:** 2026-11-14
**Valid until:** 2026-12-14 (ratatui 0.30.x is stable; recheck if ratatui 0.31 or a crossterm 0.30 appears)
