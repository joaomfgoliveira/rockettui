# Phase 2: Configuration System - Pattern Map

**Mapped:** 2026-11-16
**Files analyzed:** 22 new/modified
**Analogs found:** 20 / 22

All analog paths below are git-tracked (`git ls-files src scripts` verified). Line numbers are from the current tree.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `src/config/mod.rs` (Config, load, load_from_sources) | service | transform (pure) | `src/input/keymap.rs` (`from_bindings`/`default_keymap`) | role-match |
| `src/config/paths.rs` (config_dir, read_source) | utility | file-I/O | `src/lib.rs` `unix_now` (isolated env/IO read, pure core elsewhere) | partial |
| `src/config/problem.rs` (Problem, Report) | model | transform | `src/input/key.rs` `ParseError` + Display (L52-74) | role-match |
| `src/config/reader.rs` (spanned Reader) | utility | transform | RESEARCH.md "Spanned reader" (no codebase analog) | none |
| `src/config/settings.rs` | model/config | transform | `src/app/mod.rs` App fields + `MessageLayout` in `src/ui/wrap.rs` | role-match |
| `src/config/keymap.rs` (overrides, clash, lockout) | service | transform | `src/input/keymap.rs` L17-31, L58-100 | exact |
| `src/config/theme.rs` / `src/ui/theme.rs` (Theme) | model | transform | `src/ui/palette.rs` + hard-coded Styles in `src/ui/sidebar.rs` | role-match |
| `src/input/actions.rs` (name table) | utility | transform | `src/action.rs` exhaustive Debug (L82+) | role-match |
| `src/input/trie.rs` (+remove, +clashes) | model | CRUD | itself (`insert` L50-59, `lookup` L62+) | exact |
| `src/input/keymap.rs` (+presets, +build) | model | transform | itself (`default_bindings` L58-100) | exact |
| `src/action.rs` (+FocusNext/FocusPrev/RunCommand) | model | event-driven | itself (Debug match L82+) | exact |
| `src/app/update.rs` (reducer arms) | controller/reducer | event-driven | itself (existing FocusLeft/Right arms) | exact |
| `src/app/mod.rs` (`App::with_config`) | model/store | request-response | `App::new` L86-108, `with_fake_data` L111 | exact |
| `src/ui/layout.rs` (sidebar width param) | utility | transform | itself L7-30 | exact |
| `src/ui/palette.rs` (`author_color(&[Color])`) | utility | transform | itself L28-53 | exact |
| `src/ui/sidebar.rs` (themed markers/styles) | component | request-response | itself L30-90 | exact |
| `src/ui/mod.rs` (`pane_block` themed borders) | component | request-response | itself L40-50 | exact |
| `src/ui/messages.rs`, `status.rs`, `input.rs`, `wrap.rs` (theme + timestamp fmt) | component | request-response | `src/ui/sidebar.rs` | role-match |
| `src/ui/error_screen.rs` | component | request-response | `src/ui/status.rs` / `ui::view` L20-35 | role-match |
| `src/lib.rs` (`run(terminal, config)`, error-screen loop, `key_timeout`) | service | event-driven | itself L50-105 + `src/event.rs` | exact |
| `src/main.rs` (load config, ExitCode) | config/entry | request-response | itself (9 lines) | exact |
| `scripts/smoke_pty.py` (isolated XDG, new scenarios) | test | file-I/O | itself (`spawn` + `extra_env`) | exact |
| `Cargo.toml` (+toml, +chrono) | config | n/a | existing dependency lines | exact |

## Pattern Assignments

### `src/input/trie.rs` (+ `remove`, `clashes`) (model, CRUD)

**Analog:** same file. Node layout (L6-18) and `insert` (L50-59):
```rust
struct Node<A> { value: Option<A>, children: HashMap<Chord, Node<A>> }
pub fn insert(&mut self, seq: &[Chord], value: A) {
    if seq.is_empty() { return; }
    let mut node = &mut self.root;
    for chord in seq { node = node.children.entry(*chord).or_insert_with(Node::new); }
    node.value = Some(value);
}
```
Add `remove` (prune empty branches) and `clashes()` per RESEARCH.md "Trie removal and clash scan". Keep the `Default`/`new` style (L37-46); tests go in the file's `#[cfg(test)]` module.

### `src/input/keymap.rs` (+ presets, build from rows) (model, transform)

**Analog:** same file.
- Row shape and later-wins (L17-31): `from_bindings(rows: &[(Mode, &str, Action)]) -> Result<Keymap, ParseError>`; `parse_keys(notation)?` then `trie_mut(*mode).insert(...)`.
- `default_bindings()` (L58-100) is the `vim` preset. `vim-extras` = `default_bindings()` + `(Mode::Normal, "<Tab>", Action::FocusNext)`, `(Mode::Normal, "<S-Tab>", Action::FocusPrev)`.
- `default_keymap()` (L103-105) uses `.expect("built-in keymap parses")`: the only acceptable `expect` (built-ins).
- `trie_mut` (L33-39) is private; the config layer needs a `remove`/bind entry point on `Keymap` (add `pub fn unbind(mode, &[Chord])` and `pub fn bind(...)`, mirroring `trie_mut`).
- Test style (L108+): iterate rows, assert `matches!(keymap.lookup(*mode, &chords), Lookup::Exact(bound) if bound == action)`.

### `src/config/keymap.rs` (service, transform)

**Analog:** `src/input/keymap.rs` above. Error type: wrap `ParseError` (Display at `src/input/key.rs` L63-74 gives user-ready messages such as `unknown key name 'x'`). Two-pass apply (removals, then binds, then `Trie::clashes()`, then lockout guard) per RESEARCH.md Pattern 4. Use `format_keys` from `src/input/key.rs` to name both bindings in clash errors.

### `src/input/actions.rs` (utility, transform)

**Analog:** `src/action.rs` exhaustive Debug match (L82-~125):
```rust
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::MoveDown => write!(f, "MoveDown"),
            ...
```
Copy the "exhaustive on purpose" discipline: `fn action_from_name(&str) -> Option<Action>` plus `fn name_of(&Action) -> Option<&'static str>`, with a test that every bindable unit variant has a name. `RunCommand(..)` must print redacted in Debug (like `Paste`/`InsertChar` do there). Update the `debug_redacts` test in `src/action.rs` with new variants.

### `src/app/mod.rs` (`App::with_config`) (store)

**Analog:** `App::new` (L86-108). Fields to replace with config-driven values: `message_layout: ...::default()` (L98), `keymap: default_keymap()` (L104). Add `theme`, `settings` (sidebar_width, history_size, time format, key_timeout). Keep `App::new(size)` = `with_config(size, &Config::default())` so the 169 existing tests and `with_fake_data` (L111) are untouched. Add `App::areas()` / `sidebar_width()` helper (Pitfall 10).

### `src/ui/layout.rs` (utility, transform)

**Analog:** same file. `pub const SIDEBAR_WIDTH: u16 = 28;` (L7) stays as the default; change L22 signature to `areas(area, sidebar_width, input_rows)`; L30-31 `Constraint::Length(SIDEBAR_WIDTH)` becomes the parameter. Fix every call site the compiler flags (see RESEARCH.md Pitfall 10 list).

### `src/ui/palette.rs` (utility, transform)

**Analog:** same file. L28-35 const `AUTHOR_PALETTE` becomes the `Theme` default; L49-53:
```rust
pub fn author_color(sanitized: &str) -> Color {
    let len = AUTHOR_PALETTE.len() as u64;
    let index = usize::try_from(fnv1a_64(sanitized.as_bytes()) % len).unwrap_or(0);
    AUTHOR_PALETTE[index]
}
```
Change to `author_color(sanitized, palette: &[Color])`; keep the doc-comment exclusion rationale on the default only; keep `fnv1a_published_vectors`/`golden_author_colors` tests pointed at the default list. Empty palette must be rejected at load (division by zero).

### `src/ui/sidebar.rs` and `src/ui/mod.rs` (component, request-response)

**Analog:** same files; these hold the hard-coded values to move into `Theme`:
- `sidebar.rs` L35-37 header `Modifier::BOLD | Modifier::DIM`; L48 `"\u{25be} "`/`"\u{25b8} "`; L55 `"* "`/`"  "` (derive placeholder from marker width); L58 `" [e2e]"`; L59-61 unread bold; L72-76 highlight REVERSED (focused) / BOLD; L83 `.highlight_symbol("> ")`.
- `ui/mod.rs` L40-48 `pane_block`: `BorderType::Thick` focused / `Plain` otherwise; take border types from theme.
- Pattern to keep: untrusted text via `sanitize_line(&room.name)` before drawing/measuring (T-01-01).
Other `ui/*` files follow the same refactor: replace `Style::new()...` literals by `app.theme.<element>` lookups; `ui::view` stays a pure function of `&App`.

### `src/ui/messages.rs` / `src/ui/wrap.rs` (timestamp formatting)

**Analog:** `ui/messages.rs` L14-17 `hhmm(ts)` (UTC placeholder) replaced by a formatter validated by `chrono::format::StrftimeItems`. The same formatter must feed `wrap::render_rows` (call sites `ui/messages.rs:46`, `app/messages.rs:37`, plus tests) so the reducer row model and renderer agree: introduce one context struct passed to both. Output goes through `sanitize_line`.

### `src/ui/error_screen.rs` (component, request-response)

**Analog:** `ui::view` (`src/ui/mod.rs` L20-35) for the "pure fn draws onto Frame" shape and the small-terminal fallback `Paragraph::new(notice)`; snapshot test style from `src/ui/mod.rs` tests (`render_to_string` in `src/test_util.rs`, `insta` snapshots in `src/ui/snapshots/`). Sanitize every problem string with `sanitize_line`.

### `src/lib.rs` + `src/main.rs` (startup, event-driven)

**Analog:** `lib.rs` run loop L50-105 and `event.rs` input task (L11-28: drops `KeyEventKind::Release`, forwards `Resize`).
- `KEY_TIMEOUT` const (L29) becomes `app.key_timeout`, used at L84 (`Instant::now() + KEY_TIMEOUT`).
- `run(terminal)` gains the `Config`; `App::new(terminal.size()?)` (L61) becomes `App::with_config`.
- Error-screen loop: reuse `crossterm::event::EventStream` + `futures::StreamExt` as in `event.rs`; any key Press quits, `Resize` redraws.
- `main.rs` (9 lines) restore pattern to keep:
```rust
let terminal = rockettui::tui::init();
let result = rockettui::run(terminal).await;
rockettui::tui::restore();
result
```
Load config before `tui::init()`; on `Err(report)` init TUI, show error screen, `tui::restore()`, print sanitized report to stderr, return `ExitCode::from(1)` (main currently returns `io::Result<()>`).

### `src/config/problem.rs`, `settings.rs`, `reader.rs`, `paths.rs`

- `problem.rs`: model on `ParseError` (`src/input/key.rs` L52-74): plain enum/struct + `impl fmt::Display`, no source-line echoes. Never use `toml::de::Error` Display.
- `settings.rs`: enum selectors mirror `MessageLayout` (`src/ui/wrap.rs` L16-25, variants `Compact` default, `TwoLine`); `Default` must equal current hard-coded values.
- `reader.rs`, `paths.rs`: no codebase analog; copy RESEARCH.md "Spanned reader" and "Pure XDG resolution" snippets verbatim (compiled and verified there). `paths.rs` takes env as args (edition 2024 `set_var` is unsafe).

### `scripts/smoke_pty.py` (test)

**Analog:** same file; `spawn()` already accepts `extra_env`. Default it to an empty temp `XDG_CONFIG_HOME`; add scenarios `config_error`, `config_keymap`.

## Shared Patterns

### Sanitize everything drawn, measured or printed
**Source:** `src/ui/sanitize.rs` `sanitize_line`; used in `src/ui/sidebar.rs` L40-44, L56.
**Apply to:** error screen, stderr dump, markers (reject if `sanitize_line(m) != m`), formatted timestamps.

### Defaults reproduce today's behavior
**Source:** `App::new` (`src/app/mod.rs` L86-108), `default_keymap()` (`src/input/keymap.rs` L103-105).
**Apply to:** `Config::default()`, `Theme::default()`, `Settings::default()`. Defaults are Rust structs, not embedded TOML. Existing snapshots in `src/ui/snapshots/` must not change.

### Exhaustive matches as compile-time checklist
**Source:** `src/action.rs` Debug impl (L82+, "exhaustive on purpose").
**Apply to:** new Action variants, name table, reducer arms.

### Pure core, thin IO shell, TEA preserved
**Source:** `lib.rs` `unix_now` ("the only wall-clock read"; reducer gets `Action::Clock`).
**Apply to:** `config::load_from_sources(&str...)` pure; env/file reads only in `config::load`/`main`; reducer never reads files.

### Test conventions
**Source:** `src/test_util.rs` (`press`, `app_80x24`, `render_to_string`), `insta` snapshots, `#[cfg(test)] mod tests` per file.
**Apply to:** all new modules; inject timezone (`Utc`) for timestamp tests.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `src/config/reader.rs` | utility | transform | No TOML/spanned parsing exists; use RESEARCH.md verified snippet |
| `src/config/paths.rs` file-reading part | utility | file-I/O | No filesystem reads in codebase; use RESEARCH.md (metadata `is_file`, 1 MiB cap, NotFound = absent) |

## Metadata

**Analog search scope:** `src/**`, `scripts/`, `Cargo.toml`
**Files scanned:** about 30 tracked source files (targeted reads of keymap, trie, palette, sidebar, ui/mod, layout, lib, main, tui, event, action, app/mod, test_util)
**Pattern extraction date:** 2026-11-16
