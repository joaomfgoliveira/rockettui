# Phase 2: Configuration System - Research

**Researched:** 2026-11-16
**Domain:** Rust TUI configuration: TOML loading, multi-error validation with source positions, keymap overrides on a key-sequence trie, ratatui theming, strftime timestamp formatting
**Confidence:** HIGH (the stack and the crate APIs were verified by compiling probes; the design choices that touch user-visible behavior are listed in the Assumptions Log)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

### Config errors (CONF-03)
- **D-01:** Any invalid config makes the app **refuse to start**. Missing files are fine (defaults, CONF-02); present-but-invalid files are fatal. Applies to TOML syntax errors, wrong types, bad color values, bad key notation, unknown action names, out-of-range values and keymap prefix clashes.
- **D-02:** Errors are shown on an **in-TUI error screen**: the app enters the TUI, lists every problem found across all three files (file, line/column when available, offending key, clear message), and **any key quits with a non-zero exit code**. No reload key, no "continue with defaults" escape hatch. Collect all errors in one pass (not first-error-only).
- **D-03:** **Unknown keys/sections are not errors.** They are ignored in normal startup, but listed as warnings on the error screen when a real error is also present. Do not use `deny_unknown_fields`.

### Keymap overrides (CONF-04, CONF-05)
- **D-04:** `keymap.toml` has one table per mode (`[normal]`, `[insert]`, `[command]`) in `"<keys>" = "action_name"` form, Vim notation per Phase 1 D-08. It layers on top of the active preset's rows via the existing `Keymap::from_bindings` (later row wins for identical mode+keys).
- **D-05:** Unbind with the special action `"none"`: `"<C-d>" = "none"` removes the preset binding and leaves nothing.
- **D-06:** If an override makes one binding a prefix of another (e.g. user binds `g` while `gg` is bound), that is a **config error** naming both bindings. The user must `"none"` the old one first.
- **D-07:** Bindable actions are a **curated snake_case name table** (e.g. `move_down`, `goto_top`, `focus_left`, `enter_insert`, `submit_input`), excluding internal actions (typed characters, ticks, etc.). Additionally, a `"cmd:<command>"` form (e.g. `"cmd:quit"`) binds a key to a command-line command. Unknown action name = error.

### Theme (CONF-06)
- **D-08:** Colors accept **named ANSI** (`"cyan"`, `"light-blue"`), **256-color index** (`208`) and **`#rrggbb`**. Named colors remain the built-in default so the UI keeps adapting to terminal themes.
- **D-09:** `theme.toml` is **per-element tables** grouped by pane (e.g. `[sidebar.cursor]`), each with optional `fg`, `bg`, `modifiers = ["bold", ...]`. Unset fields inherit the built-in default style. Every styled UI element (sidebar, messages, status bar, mode indicator, input bar, borders, error line, etc.) gets a key.
- **D-10:** The per-author color palette is **themeable**: `author_palette = ["cyan", "#ff8800", ...]` replaces the list in `src/ui/palette.rs`; the FNV-1a hash still picks the index. Default stays the current 6 colors; their exclusion rationale applies to defaults only (user-supplied palette is trusted but validated as colors). Changing the palette remaps users, which is expected.
- **D-11:** Theme also controls **border style** (e.g. `plain|rounded|double`) and **sidebar/glyph markers** (fold/unread/etc. markers), not just colors. Marker strings must stay sanitized and width-checked since they are drawn and measured.

### config.toml settings & presets (CONF-04, CONF-07)
- **D-12:** `config.toml` defines: `keymap_preset` (`vim` | `vim-extras`), `sidebar_width`, `history_size`, `timestamp_format`, `message_layout` (`compact` | `two_line`, from Phase 1 D-10), and the key-sequence timeout in ms (Phase 1 D-06). **No** `server_url`/credentials yet (Phase 3).
- **D-13:** Validation: `sidebar_width` 10-60, `history_size` 1-1000 (default 50), `timestamp_format` strftime-style, default `"%H:%M"`. Out-of-range or invalid format is a config error (D-01). `history_size` is stored now but only consumed in Phase 3.
- **D-14:** `vim-extras` = `vim` + `Tab`/`Shift-Tab` pane focus cycling only. `Ctrl-k` room switch is deferred (needs a picker UI).

### Claude's Discretion
- TOML crate usage (`serde` + `toml` per constraints) and spans for line/column errors; date crate choice for strftime formatting (`chrono` vs `time`) or a minimal formatter.
- Default sequence timeout value (currently ~1s) and exact TOML key names/section layout beyond what is above.
- Module layout under `src/config/` (config, keymap, theme loading), how the loaded `Config` is threaded into `App`/`ui` (the TEA constraint: one `App`, one `Action`, one reducer).
- How defaults are expressed (Rust structs vs embedded TOML) and the XDG path resolution.
- Exact error-screen layout.
- Whether to warn about config file permissions now (AUTH-02 is Phase 3 and concerns credentials; no credentials exist in Phase 2 files).

### Deferred Ideas (OUT OF SCOPE)
- `Ctrl-k` room switch (picker UI) — future phase.
- Config hot reload / reload key on the error screen — MISC-03 (v2).
- Per-user author color overrides (`[authors] alice = "red"`) — not requested; backlog.
- Bundled theme gallery — MISC-04 (v2).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CONF-01 | App loads `config.toml`, `keymap.toml` and `theme.toml` from `~/.config/rockettui/` (honoring `$XDG_CONFIG_HOME`) | Pure `config_dir(xdg, home)` resolution (Code Examples); hand-rolled, NOT the `dirs` crate (macOS differs); IO shell reads three files, pure core parses strings |
| CONF-02 | App runs with built-in defaults when any or all config files are absent | `ErrorKind::NotFound` on a file = "absent" = defaults; any other IO error is a config error (D-01); `Config::default()` must reproduce today's hard-coded look and keys byte-for-byte (keeps all 169 existing tests and snapshots green) |
| CONF-03 | Invalid config entries show a clear error instead of crashing | `DeTable::parse_recoverable` (multiple syntax errors) + manual typed walk over the spanned tree (multiple semantic errors, line:col, unknown-key warnings); in-TUI error screen; exit code 1; stderr dump |
| CONF-04 | User can pick the `vim` or `vim-extras` keymap preset in `config.toml` | `keymap_preset` enum validated; `vim` = `default_bindings()`, `vim-extras` = vim + `<Tab>`/`<S-Tab>` rows mapped to two NEW actions `FocusNext`/`FocusPrev` |
| CONF-05 | User can rebind keys per mode in `keymap.toml`, overriding the preset | Curated name table, `none` removal via new `Trie::remove`, two-pass apply (removals first), final-trie prefix-clash scan, duplicate-chord detection, lockout guard |
| CONF-06 | User can set colors and styles for every UI element in `theme.toml` | `Theme` struct whose `Default` equals today's hard-coded styles; `StyleSpec{fg,bg,modifiers}` patched over defaults; palette, border type and markers themeable; `ui/*` refactored to read `app.theme` |
| CONF-07 | User can set server URL, history size, timestamp format and sidebar width in `config.toml` | Server URL is EXCLUDED by locked D-12 (Phase 3). History size, timestamp format and sidebar width: ranges in D-13; `chrono` strftime validated with `StrftimeItems`; `layout::areas` gains a sidebar-width parameter. See Open Question 1 about the requirement text |
</phase_requirements>

## Summary

Phase 2 is almost entirely plumbing and validation, not new UI. Phase 1 already built the seams: `Keymap::from_bindings` takes `(Mode, notation, Action)` rows, `parse_keys` parses Vim notation, `Trie::insert` replaces on re-insert, and `App` already holds `keymap` and `message_layout`. What is missing is (a) a TOML-to-rows source with multi-error reporting, (b) removal and prefix-clash detection in the trie, (c) a `Theme` and a `Settings` value threaded into `App` and the renderers, (d) a pre-App error screen, and (e) a strftime formatter. Everything must default to exactly today's behavior so the 169 existing tests stay green.

The one architectural choice that matters: **do not use serde derive for the config files.** D-02 demands every problem across all three files in one pass, with file/line/column/key, and D-03 demands unknown keys be listed as warnings. A derived `Deserialize` yields the first error only, and has no way to list unknown keys without `deny_unknown_fields` (forbidden by D-03). The `toml` 1.x crate exposes a spanned lossless tree (`toml::de::DeTable::parse_recoverable`) that returns multiple syntax errors and gives a byte span for every key and value. Walk that tree with a small `Reader` helper that accumulates `Problem`s. This was verified by compiling and running probes (see Code Examples). Use `chrono` 0.4 for strftime: `StrftimeItems` + `Item::Error` validates a user format up front, which is required because formatting an unvalidated format can panic via `to_string()`.

The riskiest parts for planning are not the TOML parsing; they are the cross-cutting refactors (`layout::areas` signature touches 9 call sites, `wrap::render_rows` touches about 14, hard-coded styles in 6 UI files) and the keymap semantics (TOML tables iterate in sorted order, not file order, so "later row wins" is meaningless inside one file; removals must be applied before binds; clash detection must run on the final trie; and a user can lock themselves out by unbinding Esc/Ctrl-c or `:`).

**Primary recommendation:** Build a pure `config` core (`&str` sources in, `Result<Config, Report>` out, no filesystem, no env, no terminal), parse with `toml::de::DeTable` + a spanned `Reader` that accumulates sorted `Problem`s, use `chrono` (default-features off, `clock`+`std`) with the timezone injected so tests stay deterministic, and load config in `main` BEFORE terminal init so the error path is a separate tiny screen outside the TEA `App`.

## Project Constraints (from CLAUDE.md)

Source: `/home/joliveira/personal-projects/rockettui/.claude/CLAUDE.md` (read this session).

- **Tech stack:** Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `tokio-tungstenite`, `serde` + `toml`, chosen by the author. Phase 2 adds `toml` and (discretion) `chrono`; no networking crate may be added in this phase.
- **Architecture:** Elm/TEA: one `App` state, one `Action` enum, one channel, one reducer. Config is loaded before `App` creation and held in state; the reducer never reads files.
- **Security:** act only as a normal client; credentials never committed or logged; `chmod 600` warnings (AUTH-02 is Phase 3); never log message contents.
- **Config:** XDG convention at `~/.config/rockettui/`; TOML; "invalid entries yield clear errors, not crashes".
- **GSD enforcement:** repo edits go through a GSD workflow (this research file is the phase artifact).
- No project skills found (`.claude/skills/` etc. not present per CLAUDE.md); no other directives.

## Architectural Responsibility Map

This is a single-process terminal app, so "tiers" are the app's layers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Locate config dir (`$XDG_CONFIG_HOME` / `$HOME`) | Startup shell (`main.rs` / `config::load`) | — | Reads env and filesystem; must not be in the reducer or pure core |
| Parse + validate TOML into `Config` | Pure config core (`src/config/`) | — | Pure `&str -> Result`; unit-testable with no IO, no `set_var` (unsafe in edition 2024) |
| Build `Keymap` from preset + overrides | Input layer (`src/input/keymap.rs`, `trie.rs`) | Config core (supplies rows) | Trie owns removal and clash detection; config supplies parsed rows with spans |
| Hold settings, theme, keymap at runtime | `App` state (TEA model) | — | "held in state, not read from the reducer"; `ui::view` stays a pure function of `&App` |
| Key timeout, sidebar width, history size | `App` state read by run loop / layout | Run loop (`lib.rs`) | `KEY_TIMEOUT` const becomes `app.key_timeout`; history_size stored only |
| Styles, border type, markers, author palette | View (`src/ui/*`) | Config core (supplies `Theme`) | Renderers look up `app.theme.*` instead of hard-coding `Style`/`Color` |
| Timestamp formatting | View (`ui/messages.rs` / `wrap.rs`) | App state (holds format) | The same formatter must feed the reducer's row-height model and the renderer |
| Error screen + exit code | Startup shell (separate mini-loop, pre-App) | View (small render fn) | Config errors occur before an `App` exists; keeps TEA intact |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `toml` | 1.2.1 (spec 1.1.0) | TOML parse to a spanned tree (`DeTable`), multi-error `parse_recoverable` | Chosen by the author; the 1.x spanned API is what makes multi-error + line:col possible [VERIFIED: cargo search toml = "1.2.1"; probe compiled and ran this session; gsd-tools package-legitimacy: OK] |
| `chrono` | 0.4.45 | strftime formatting + `Local` timezone conversion | Named in 02-CONTEXT.md as an option; `StrftimeItems`/`Item::Error` validates formats; stable API [VERIFIED: cargo search chrono = "0.4.45"; probe compiled and ran; gsd-tools package-legitimacy: OK] |
| `ratatui` | 0.30.2 (already in Cargo.lock) | `Color: FromStr`, `Modifier`, `BorderType` | Already a dependency [VERIFIED: Cargo.lock `name = "ratatui"` `version = "0.30.2"`] |

Recommended manifest lines (both verified to compile together with `ratatui`, and `DeTable::parse` / `parse_recoverable` / `Spanned` work without the `serde` and `display` features):

```toml
chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }
toml = { version = "1.2", default-features = false, features = ["parse", "std"] }
```

Resulting new transitive crates: `iana-time-zone`, `num-traits`, `serde_spanned`, `toml_datetime`, `toml_parser`, `winnow` (from `cargo tree` in the probe). No networking crates [VERIFIED: probe `cargo tree`]. Phase 1's validation gate "no networking crate, resolved tokio has no net feature" (01-VALIDATION.md, T-01-SC1) stays satisfiable; re-run `cargo tree` after adding.

MSRV: project `rust-version = "1.88"`; `toml` 1.2.1 declares 1.85, `chrono` 0.4.45 declares 1.62.0 [VERIFIED: their Cargo.toml `rust-version` lines in the cargo registry]. Toolchain here is rustc 1.99.1.

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `insta` | 1 (already a dev-dependency) | Snapshot the error screen and a themed frame | Same pattern as Phase 1 |
| `serde` | 1.0.229 (transitive via ratatui) | Not needed for Phase 2 deserialization | Add directly in Phase 3 for REST JSON |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `chrono` | `jiff` 1.0.0 | Works (probe: strftime ok), but jiff 1.0 renamed core types (`Timestamp::to_zoned_datetime`, not `to_zoned`), so most online examples and training data are stale. Not recommended for a learning project |
| `chrono` | `time` | Needs the `formatting` + `local-offset` features and a different format-description syntax (NOT strftime), so it cannot satisfy D-13 "strftime-style" without a translation layer |
| `toml::de::DeTable` walk | `#[derive(Deserialize)]` | Derive gives first-error-only and cannot list unknown keys (see Summary). Use serde only if the planner accepts "one error per file" and overrides D-02 |
| hand-rolled XDG | `dirs` / `directories` crate | `dirs::config_dir()` returns `~/Library/Application Support` on macOS; CONF-01 requires `~/.config/rockettui/` |
| `tempfile` (dev-dep) | `std::env::temp_dir()` + pid-named dir | Not adopted: the pure-string core needs no filesystem in most tests, and one tiny helper covers the single IO test. Avoids an extra dependency. (`tempfile` 3.27.0 passes the legitimacy check if the planner wants it anyway, but the name came from training knowledge so tag it `[ASSUMED]` and gate it) |

**Installation:**
```bash
cargo add chrono --no-default-features --features clock,std
cargo add toml --no-default-features --features parse,std
```

**Version verification:** `cargo search` this session returned `toml = "1.2.1+spec-1.1.0"`, `chrono = "0.4.45"`, `jiff = "1.0.0"`, `tempfile = "3.27.0"` (2026-11-16).

## Package Legitimacy Audit

Run via `gsd-tools query package-legitimacy check --ecosystem crates` this session.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| toml | crates.io | since 2014-11 (~12 yrs) | ~24.9M/wk | github.com/toml-rs/toml | OK | Approved (named in project constraints) |
| chrono | crates.io | since 2014-11 (~12 yrs) | ~18.9M/wk | github.com/chronotope/chrono | OK | Approved (named by the user in 02-CONTEXT.md) |
| serde | crates.io | since 2014-12 | ~33.9M/wk | github.com/serde-rs/serde | OK | Not added in Phase 2 |
| jiff | crates.io | since 2024-02 | ~9.4M/wk | github.com/BurntSushi/jiff | OK | Alternative only, not recommended |
| tempfile | crates.io | since 2015-04 | ~20.1M/wk | github.com/Stebalien/tempfile | OK | Not adopted; if used, `[ASSUMED]` name, planner adds `checkpoint:human-verify` |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
Crates have no postinstall scripts (build scripts only); `autocfg` is a build-dependency of `num-traits` (normal).

## Architecture Patterns

### System Architecture Diagram

```
                       main()
                         |
        resolve config dir (pure fn: XDG_CONFIG_HOME abs? else HOME/.config) + "/rockettui"
                         |
   read 3 files  --NotFound--> "absent" -> defaults for that file
        |  \--other IO error / not a regular file / >1 MiB / not UTF-8 --> Problem (error)
        v
   config::load_from_sources(config_src, keymap_src, theme_src)   <-- PURE, no IO
        |
        +--> DeTable::parse_recoverable(src) per file
        |        syntax errors (several) --> Problems (skip semantic pass for that file)
        |        ok tree --> Reader walks spanned keys/values
        |              unknown keys  --> Problems (warning)
        |              type/range/color/notation/action errors --> Problems (error)
        |
        +--> keymap: preset rows -> Keymap; overrides: removals first, then binds
        |        duplicate chords, unknown action, bad cmd:, parse_keys error --> Problems
        |        final trie: Trie::clashes() --> Problems naming BOTH bindings
        |        lockout guard (cancel / quit reachable) --> Problems
        |
        v
   errors empty?  --yes--> Config { settings, keymap, theme }  (warnings discarded)
        |
        no --> Report { errors, warnings } sorted by (file, line, col)
        |
   +----+-----------------------------------------------+
   v                                                    v
 Ok(Config)                                        Err(Report)
   |                                                    |
 tui::init()                                        tui::init()
 run(terminal, config):                             error_screen(terminal, &report):
   App::with_config(size, &config)                    draw problems (sanitized), warnings
   seed -> channel -> reducer -> ui::view(&App)       wait for ANY key PRESS (ignore Release/Resize->redraw)
   uses app.theme / app.settings / app.keymap         tui::restore()
                                                      eprintln!(report)  (sanitized, full list)
                                                      exit code 1
```

### Recommended Project Structure
```
src/
├── config/
│   ├── mod.rs        # Config, Settings, load(), load_from_sources(), Report; re-exports
│   ├── paths.rs      # config_dir(xdg, home) pure fn; read_source(path) -> Source (IO shell)
│   ├── problem.rs    # Problem { file, line, col, key, message, severity }, Report, line_col()
│   ├── reader.rs     # Reader: typed accessors over DeTable that accumulate Problems
│   ├── settings.rs   # config.toml -> Settings (+ validation, TimeFormat)
│   ├── keymap.rs     # keymap.toml + preset -> Keymap (two-pass apply, clash, lockout)
│   └── theme.rs      # theme.toml -> Theme (StyleSpec, markers, borders, palette)
├── input/
│   ├── actions.rs    # NEW: curated name table  name <-> Action  (D-07)
│   ├── keymap.rs     # + presets (vim, vim_extras), Keymap::build(...)
│   └── trie.rs       # + remove(), clashes()
├── ui/
│   ├── theme.rs      # NEW (or config::theme): Theme struct + Default == today's hard-coded styles
│   └── error_screen.rs # NEW: render_config_errors(frame, &Report)
└── app/mod.rs        # App::with_config(size, &Config); App::new(size) = with_config(default)
```

### Pattern 1: Pure core, thin IO shell
**What:** `config::load_from_sources(&Sources) -> Result<Config, Report>` takes `Option<String>` per file (None = absent). A separate `config::load() -> Result<Config, Report>` resolves the dir, reads files and calls it.
**When to use:** always; makes 95% of tests filesystem-free. In edition 2024 `std::env::set_var` is `unsafe`, so env-dependent tests are painful; pass `xdg`/`home` as arguments to `config_dir`.

### Pattern 2: Defaults as Rust structs equal to today's hard-coded values
**What:** `Settings::default()`, `Theme::default()` and `vim` preset reproduce the current constants exactly. Do NOT embed default TOML and parse it at startup (a typo in the embedded default becomes a startup failure; structs are checked by the compiler).
**Why:** existing tests assert styles (`Modifier::REVERSED`, `Color::DarkGray`, author colors) and the Display snapshots; `App::new(size)` must keep meaning "defaults".

### Pattern 3: Patch-style theme resolution
**What:** `StyleSpec { fg: Option<Color>, bg: Option<Color>, modifiers: Option<Modifier> }`; resolved style = default with each `Some` field replacing the default's. `modifiers = []` (explicit empty array) clears the default modifiers; absent leaves them. Build the final `Style` from a plain struct, not by `Style::patch` (patch only ever adds modifiers).

### Pattern 4: Two-pass keymap application
**What:** Parse all rows (collecting errors) -> apply every `none` removal -> apply every bind -> scan the final trie for prefix clashes -> lockout guard. Never apply in TOML table order.

### Anti-Patterns to Avoid
- **`deny_unknown_fields` / derive-only deserialization:** forbidden by D-03 and first-error-only.
- **Using `toml::de::Error`'s `Display`:** it prints the offending source line (verified: output contained `1 | sidebar_width = `). Phase 3 puts credentials in `config.toml`; echoing source lines could leak a token to screen/stderr. Use `message()` + `span()` and compute your own `line:col`.
- **Reading config from the reducer or `ui::view`:** violates TEA; load once in `main`.
- **Calling `.to_string()` / `format!("{}")` on `dt.format(user_fmt)` without validation:** chrono panics on a `Display` error. Validate with `StrftimeItems` first.
- **Using `Local` in tests:** this machine's local zone is UTC+01:00 (probe printed `01:00` for epoch 0), so local-time snapshots would differ per developer machine. Inject the timezone.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TOML parsing with positions | Custom parser / regex | `toml::de::DeTable::parse_recoverable` | Spans for keys and values, multiple syntax errors, spec 1.1 |
| Color names / `#rrggbb` / `0-255` | Own color table | `ratatui::style::Color::from_str` plus a TOML-integer branch | Already supports names (case/`-`/`_`/space-insensitive, `bright`->`light`, `grey`), index strings, `#rrggbb` |
| Modifier flags | Own bitfield | `ratatui::style::Modifier` consts + a small name table | Names: BOLD, DIM, ITALIC, UNDERLINED, SLOW_BLINK, RAPID_BLINK, REVERSED, HIDDEN, CROSSED_OUT |
| strftime | Own formatter | `chrono::format::StrftimeItems` + `DateTime::format_with_items` | Validation (`Item::Error`), timezone, DST |
| Vim key notation | New parser | Existing `input::key::parse_keys` / `format_keys` | Already round-trip tested; its `ParseError` has `Display` messages |
| Width measuring | `str::len()` | `unicode_width::UnicodeWidthStr` (already a dep) | Marker width checks must match what is drawn |
| Escape stripping | Ad hoc filter | Existing `ui::sanitize::sanitize_line` | Project rule T-01-01: everything drawn or measured passes through it |
| XDG on Linux | `dirs` crate | 6-line pure fn | See Alternatives; macOS mismatch |

**Key insight:** the hard part is not parsing; it is accumulating many precise problems and keeping semantics identical to today's behavior. Spend effort on the `Problem` model and tests, not on parsing machinery.

## Runtime State Inventory

Not applicable: this is a greenfield feature (new `src/config/`), not a rename/refactor/migration. The only existing runtime state touched is the pty smoke harness environment (see Pitfall 14).

## Common Pitfalls

### Pitfall 1: Derive gives one error and no unknown-key list
**What goes wrong:** D-02/D-03 are unmeetable with `#[derive(Deserialize)]`.
**How to avoid:** `DeTable::parse_recoverable` + `Reader` walk. Syntax errors may be several per file (probe: 3 errors from one file); semantic errors only run when the file parsed cleanly (a recovered partial tree is unreliable, e.g. probe parsed a value-less `a =` as the string `"a"`).
**Warning signs:** a test with two bad keys in one file shows only one error.

### Pitfall 2: TOML table order is sorted, not file order
**What goes wrong:** `toml` maps iterate by key order (sorted) unless `preserve_order` is enabled, so "later row wins" cannot be relied on within `keymap.toml`, and problems arrive in arbitrary order.
**How to avoid:** sort `Problem`s by `(file, line, col)`; apply `none` removals before binds; never depend on iteration order for semantics. Duplicate keys are a TOML syntax error already (probe: `duplicate key` with a span), but two DIFFERENT strings can parse to the SAME chords (`"<C-d>"` vs `"<C-D>"`, `"<CR>"` vs `"<Enter>"`, `"G"` vs `"<S-g>"`) because `parse_angle` canonicalizes. Detect by keying a `HashMap<(Mode, Vec<Chord>), span>` and report "duplicates binding at line N".

### Pitfall 3: Prefix clashes are silently tolerated by the matcher
**What goes wrong:** `Lookup::ExactAndPrefix` exists and the matcher fires it on timeout, so `g` + `gg` "works" with a 1 s delay. D-06 makes it a config error, so it must be detected explicitly.
**How to avoid:** scan the FINAL trie (`Trie::clashes()` in Code Examples), after removals. Report both bindings via `format_keys`. Because the default preset has no clash (checked against `default_bindings()`: Normal `<C-w>h/j/l` with no bare `<C-w>`; Insert `<C-w>` with no `<C-w>x`), any clash is caused by user rows; point the span at the user row that is involved. `vim-extras` adds only `<Tab>`/`<S-Tab>`, no clash.

### Pitfall 4: Self-lockout
**What goes wrong:** The matcher's Esc/Ctrl-c reset only clears pending state, then fires whatever the keymap binds. A user who `none`s `<Esc>` and `<C-c>` in Insert (or Command) can never leave that mode; unbinding `:` (`enter_command`) removes the only way to quit (`:q`), and raw mode means SIGINT is not delivered to the app.
**How to avoid (recommended, needs user confirmation, see A3):** validation errors when (a) Insert has no `cancel` binding, (b) Command has no `cancel` or no `execute_command` binding, (c) Normal has neither an `enter_command` binding nor any binding to `quit` / `cmd:quit` / `cmd:q`.

### Pitfall 5: Insert/Command sequences and plain keys eat typed text
**What goes wrong:** Phase 1 keeps Insert rows single-key "so typing never waits for the sequence timeout" (keymap.rs comment). A user row like `"jk" = "cancel"` in `[insert]` makes `j` a prefix: the next non-`k` key breaks the sequence and, per the matcher, "A key that breaks a sequence is dropped", losing typed characters.
**How to avoid:** reject (error) multi-chord sequences whose first chord is an unmodified printable char in `[insert]`/`[command]`; allow modified/special-key sequences. See A4.

### Pitfall 6: Some chords can never fire
**What goes wrong:** In Normal mode a leading digit (1-9, and 0 once a count started) is consumed as a count before the trie (matcher.rs digit branch), so `"1" = ...` is dead. `<C-i>` and `<C-m>` arrive as Tab/Enter on legacy terminals (keymap.rs doc comment).
**How to avoid:** emit these as **warnings** (not errors; D-01 lists the error classes), shown only when a real error is also present, or simply document.

### Pitfall 7: TOML integer colors
**What goes wrong:** D-08 says `208` (an integer); `Color::from_str("208")` only takes strings. A naive `String` field rejects the integer.
**How to avoid:** accept `DeValue::String` -> `Color::from_str`, `DeValue::Integer` -> `u8::try_from` -> `Color::Indexed`; reject floats/bools/out-of-range with a clear message (probe: `fg = 300` -> error at its column). Note `from_str` quirks: it normalizes `bright`->`light`, `grey`->`gray`, `silver`->`gray`, `lightblack`->`darkgray`, `lightwhite`->`white`, **`lightgray`->`white`**, and the error type carries no detail, so write the message yourself.

### Pitfall 8: strftime validation and unsafe output
**What goes wrong:** (a) invalid formats (`%Q`, trailing `%`) must be rejected at load: `StrftimeItems::new(f).any(|i| matches!(i, Item::Error))` (probe: `%Q` and `%` -> true, `%H:%M` -> false). (b) `%n` and `%t` emit `\n`/`\t` (probe: `"00\n\t00"`), which would corrupt a one-line `Span` and width math. (c) empty format -> blank timestamp column. (d) out-of-range timestamps make `timestamp_opt(...).single()` return `None` (probe: `i64::MAX` -> `None`), so never `unwrap`.
**How to avoid:** validate at load, reject empty, pass the formatted string through `sanitize_line`, and fall back to a fixed placeholder on `None`. Keep the sanitized string as the single value both measured and drawn. Consider a max rendered width (A7) because Compact uses `time.width()` in `prefix_width`.

### Pitfall 9: Timestamp width must be identical in the reducer's row model and the renderer
**What goes wrong:** Compact layout computes `prefix_width` from `time.width()`; the reducer sizes scroll math through `wrap::message_rows`. If only the renderer sees the user's format, scrolling and rendering disagree.
**How to avoid:** carry the formatter inside one value passed to every `render_rows`/`message_rows` caller. Call sites today: `ui/messages.rs:46`, `ui/messages.rs:280` (test), `app/messages.rs:37`, `app/messages.rs:397` (test), `wrap.rs` internal tests (~10). Introduce a small context struct (layout, time format, theme styles, palette) so the signature changes once.

### Pitfall 10: `layout::areas` signature fans out
**What goes wrong:** sidebar width is a constant used inside `areas(area, input_rows)`. Callers: `ui/mod.rs:30`, `ui/mod.rs:219`, `ui/mod.rs:225` (tests), `app/mod.rs:123`, `app/mod.rs:136`, `app/sidebar.rs:184`, `app/messages.rs:24`, `ui/messages.rs:98` (test), `ui/input.rs:70` (test), plus `layout.rs` own tests. Missing one makes the reducer's geometry disagree with the renderer.
**How to avoid:** change the signature once (`areas(area, sidebar_width, input_rows)`) and let the compiler find every site; add `App::sidebar_width()`/`App::areas()` helper so callers cannot pass the wrong value. Also `ui/mod.rs:116` hard-codes `row.chars().nth(28)` in a test; fine at the default.

### Pitfall 11: Empty author palette divides by zero
**What goes wrong:** `author_color` computes `fnv1a_64(..) % len` (palette.rs:50-51); `len == 0` panics.
**How to avoid:** `author_palette = []` is a config error; cap length (e.g. <= 64). The existing `palette_avoids_clashes` and `golden_author_colors` tests keep targeting the default list; refactor `author_color(sanitized, &[Color])`.

### Pitfall 12: Markers are drawn AND measured
**What goes wrong:** sidebar markers (`"\u{25be} "`, `"\u{25b8} "`, `"* "`, `"  "` placeholder, `"> "` highlight symbol, `" [e2e]"`, kind glyphs `#`,`&`,`~`,`+`,`@`) feed width math (List highlight symbol, row alignment). A themed unread marker of width 3 with a hard-coded 2-space placeholder misaligns every room.
**How to avoid:** derive the "not unread" placeholder as `" ".repeat(width(unread_marker))`; validate each marker: `sanitize_line(m) == m` (reject if changed), no `\n`, width within a small cap (A7: 1..=4, 0 allowed only where it makes sense), and keep folder open/closed independent but warn if widths differ.

### Pitfall 13: Theme foreground on cursor rows hides author colors
**What goes wrong:** messages.rs comment: "Neither highlight sets a foreground, so author colors survive on the cursor rows". If a user sets `fg` on `[messages.cursor]`, it overrides author colors on that row.
**How to avoid:** acceptable and expected; document it; apply highlight with `set_style` as today.

### Pitfall 14: The pty smoke harness and `cargo run` read the developer's real config
**What goes wrong:** `scripts/smoke_pty.py` `spawn()` execs the binary with the inherited environment; a real `~/.config/rockettui/` (or a broken one) will change or fail the Phase 1 scenarios.
**How to avoid:** `spawn()` should set `XDG_CONFIG_HOME` to a fresh empty temp dir by default and let new scenarios pass a populated one. `spawn` already supports `extra_env`.

### Pitfall 15: Error screen details
**What goes wrong:** (a) "any key" must mean a key **press**: the input task already drops `KeyEventKind::Release`; the error screen must too, and `Resize` should redraw, not quit. (b) Long lists overflow a small terminal and D-02 forbids scrolling. (c) Problem text includes user-controlled strings (keys, values) and must go through `sanitize_line` both on screen and on the stderr dump. (d) `main` currently returns `io::Result<()>`; a non-zero exit needs `ExitCode`.
**How to avoid:** render as many problems as fit plus "... and N more (full list printed on exit)"; after `tui::restore()` print the full sanitized report to stderr; return `ExitCode::from(1)`. Use `crossterm::event::EventStream` + `futures::StreamExt` (already dependencies) or `spawn_blocking(event::read)`; no `App` involved.

### Pitfall 16: New `Action` variants must pick a Debug form
**What goes wrong:** `impl Debug for Action` is an exhaustive match "on purpose"; adding `FocusNext`, `FocusPrev`, `RunCommand(String)` fails to compile until handled; `RunCommand` carries a string, so print `RunCommand(..)` (consistent with T-01-03 redaction style).
**How to avoid:** update `action.rs` Debug, the reducer match, the name table, and `debug_redacts` test together.

### Pitfall 17: Reading config files defensively
**What goes wrong:** a FIFO at `config.toml` blocks forever; a huge file is read whole; invalid UTF-8 returns `InvalidData`.
**How to avoid:** `fs::metadata` (follows symlinks): require `is_file()` and `len() <= 1 MiB` (A9), then `read_to_string`; every non-`NotFound` failure becomes a `Problem` (D-01).

### Pitfall 18: `server_url` and other future keys
**What goes wrong:** users may already write `server_url` (Phase 3) into `config.toml`; with D-03 it is an ignored unknown key (warning only when another error exists), so forward-compat is free. Do not add it now (D-12).

## Code Examples

All three snippets below were compiled and run in a scratch crate this session (toml 1.2.1, chrono 0.4.45, ratatui 0.30.2) unless marked "sketch (unrun)".

### Spanned reader with accumulated problems (verified)
```rust
// Source: probe compiled/run this session; API per toml 1.2.1 src/de/parser/{detable,devalue}.rs
use toml::Spanned;
use toml::de::{DeTable, DeValue};
use ratatui::style::Color;
use std::str::FromStr;

struct Reader<'a> { file: &'static str, src: &'a str, out: Vec<Problem> }

impl<'a> Reader<'a> {
    fn line_col(&self, off: usize) -> (usize, usize) {
        let before = &self.src[..off.min(self.src.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        (line, col)
    }
    fn int_in(&mut self, v: &Spanned<DeValue<'_>>, key: &str, lo: i64, hi: i64) -> Option<i64> {
        match v.get_ref() {
            DeValue::Integer(i) => match i64::from_str_radix(i.as_str(), i.radix()) {
                Ok(n) if (lo..=hi).contains(&n) => Some(n),
                Ok(n) => { self.err(v.span().start, key, format!("must be between {lo} and {hi}, got {n}")); None }
                Err(_) => { self.err(v.span().start, key, "integer out of range"); None }
            },
            other => { self.err(v.span().start, key, format!("expected an integer, found {}", other.type_str())); None }
        }
    }
    fn color(&mut self, v: &Spanned<DeValue<'_>>, key: &str) -> Option<Color> {
        let parsed = match v.get_ref() {
            DeValue::String(s) => Color::from_str(s.as_ref()).ok(),
            DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix()).ok()
                .and_then(|n| u8::try_from(n).ok()).map(Color::Indexed),
            _ => None,
        };
        if parsed.is_none() { self.err(v.span().start, key, "invalid color (use a name like \"light-blue\", an index 0-255, or \"#rrggbb\")"); }
        parsed
    }
}
// Syntax errors (several per file): let (tree, errs) = DeTable::parse_recoverable(src);
// each err: e.message() (&str) and e.span() (Option<Range<usize>>). Do NOT use e's Display (echoes the source line).
// Table lookup: table.get("sidebar_width") -> Option<&Spanned<DeValue>>; iterate (k, v): k.get_ref(), k.span(), v.get_ref(), v.span().
```
Probe output for a file with a bad width, a string history_size, an unknown key and two bad colors: `config.toml:1:17 error sidebar_width must be between 10 and 60, got 99`, `config.toml:3:1 warn bogus unknown key ignored`, `config.toml:5:6 error c.fg invalid color ...`.

### Pure XDG resolution (verified)
```rust
// Source: probe; CONF-01 requires ~/.config/rockettui honoring $XDG_CONFIG_HOME
use std::{ffi::OsString, path::PathBuf};
fn config_dir(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = match xdg.map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,                       // XDG spec: relative values are ignored
        _ => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(base.join("rockettui"))
}
// config_dir(Some("rel"), Some("/home/u")) -> /home/u/.config/rockettui ; config_dir(None, None) -> None (use defaults)
```

### strftime validation (verified) and safe formatting
```rust
// Source: probe + chrono 0.4.45 src/format
use chrono::format::{Item, StrftimeItems};
fn valid_strftime(f: &str) -> bool {
    !f.is_empty() && !StrftimeItems::new(f).any(|i| matches!(i, Item::Error))
}
// valid_strftime("%H:%M") == true, ("%Q") == false, ("") == false.
// Format with an injected zone so tests are deterministic:
//   let dt = chrono::Utc.timestamp_opt(ts, 0).single()?;            // None for out-of-range ts
//   write!(&mut s, "{}", dt.with_timezone(&tz).format_with_items(StrftimeItems::new(fmt)))  // ignore Err
//   then sanitize_line(&s)
// Production tz = chrono::Local (system); tests use chrono::Utc or FixedOffset.
```

### Trie removal and clash scan (sketch, compiled against a u8 stand-in for `Chord`)
```rust
// Add to src/input/trie.rs. Verified: removing "gg" prunes empty branches; clashes() -> [([g],[g,g])].
pub fn remove(&mut self, seq: &[Chord]) -> bool {
    fn go<A>(node: &mut Node<A>, seq: &[Chord]) -> bool {
        let Some((first, rest)) = seq.split_first() else { return node.value.take().is_some() };
        let Some(child) = node.children.get_mut(first) else { return false };
        let removed = go(child, rest);
        if child.value.is_none() && child.children.is_empty() { node.children.remove(first); }
        removed
    }
    !seq.is_empty() && go(&mut self.root, seq)
}
// clashes(): DFS; at every node with a value, collect bound descendants; return (shorter, longer) pairs.
```
Pruning matters: without it, removing `gg` leaves a valueless `g` node and `lookup("g")` still returns `Prefix`.

### Existing seams (read this session; values quoted verbatim)
- `src/input/keymap.rs:20`: `pub fn from_bindings(rows: &[(Mode, &str, Action)]) -> Result<Keymap, ParseError> {`
- `src/ui/layout.rs:7`: `pub const SIDEBAR_WIDTH: u16 = 28;`
- `src/lib.rs:20`: `pub const KEY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);`
- `src/ui/palette.rs:28-35`: `const AUTHOR_PALETTE: [Color; 6] = [ Color::Cyan, Color::Magenta, Color::LightMagenta, Color::Green, Color::Yellow, Color::LightBlue, ];`
- `src/ui/messages.rs:14-17`: `pub fn hhmm(ts: i64) -> String { let secs = ts.rem_euclid(86_400); format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60) }` (UTC, replaced by the configured formatter; keep a default-format test)
- `src/ui/sidebar.rs:42`: `let marker = if *open { "\u{25be} " } else { "\u{25b8} " };` and `:55`: `let unread = if room.unread { "* " } else { "  " };` and `:83`: `.highlight_symbol("> ")`
- `src/ui/mod.rs:42-46`: focused pane `BorderType::Thick`, otherwise `BorderType::Plain`
- `src/app/mod.rs:69-70,98`: `pub message_layout: crate::ui::wrap::MessageLayout,` initialized `crate::ui::wrap::MessageLayout::default(),`; `MessageLayout` variants `Compact` (default) and `TwoLine` (`ui/wrap.rs:16-25`)
- `src/action.rs:13-59` bindable unit variants: `MoveDown MoveUp MoveLeft MoveRight GotoTop GotoBottom HalfPageDown HalfPageUp Select ToggleFold FocusLeft FocusDown FocusRight EnterCommand Cancel EnterInsert DeleteBack InsertNewline DeleteWordBack DeleteToLineStart CursorLeft CursorRight CursorUp CursorDown CursorHome CursorEnd SubmitInput ExecuteCommand Quit`; NOT bindable: `InsertChar(char)`, `Key(KeyEvent)`, `Resize(u16, u16)`, `Clock(i64)`, `Paste(String)`, `RoomsLoaded`, `FoldersLoaded`, `MessagesLoaded`, `KeyTimeout`, `Counted(u32, Box<Action>)`
- `src/app/command.rs:10-13`: `COMMANDS: &[CommandDef]` with `names: &["q", "quit"]` (validate `cmd:<name>` against this table at load time; it grows in later phases)
- `ratatui-widgets 0.3.2 borders.rs:36-49`: `pub enum BorderType { Plain (default), Rounded, Double, ... }` and `ui/mod.rs:43` proves `Thick` exists; restrict theme to `plain|rounded|double|thick`.

### Suggested name table (D-07 curated, snake_case)
`move_down, move_up, move_left, move_right, goto_top, goto_bottom, half_page_down, half_page_up, select, toggle_fold, focus_left, focus_down, focus_right, focus_next, focus_prev, enter_command, enter_insert, cancel, delete_back, insert_newline, delete_word_back, delete_to_line_start, cursor_left, cursor_right, cursor_up, cursor_down, cursor_home, cursor_end, submit_input, execute_command, quit, none` plus `cmd:<command>`. Implement as a function table plus a test that every bindable unit variant has a name (mirrors the existing exhaustive-Debug discipline). A `const` slice of `Action` values is avoided because `Action` has `String`/`Vec` variants.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `toml::Value` / derive only | `toml::de::DeTable` spanned tree, `parse_recoverable`, `Spanned<T>` | toml 0.9/1.x (current 1.2.1, spec 1.1.0) | Multi-error + positions without a second crate |
| jiff 0.2 names (`to_zoned`) | jiff 1.0 renamed (`to_zoned_datetime`, `ZonedDateTime`) | jiff 1.0.0 | Why chrono is the safer choice here |
| `std::env::set_var` safe | `unsafe` in edition 2024 | Rust 2024 edition (project is edition 2024) | Pass env as function args |

**Deprecated/outdated:** `dirs::config_dir()` for a `~/.config`-everywhere requirement (platform-specific path).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Key-timeout setting is named `key_timeout_ms`, default 1000, accepted range 100-5000 (D-12/D-13 give no name or range for it) | Standard Stack / Settings | Cosmetic; user may want other bounds. CONFIRMED by the author 2026-11-16 (name and range as stated) |
| A2 | Production timestamps should render in the system **local** timezone (Phase 1 shows UTC; message `ts` is Unix seconds) via `chrono::Local`, with tests injecting UTC | Pitfall 8, Code Examples | If the user wants UTC, drop `Local`; or add a `timezone` setting (not in D-12). CONFIRMED by the author 2026-11-16: system local, no timezone setting (Open Question 2) |
| A3 | Add a lockout guard (Insert needs `cancel`; Command needs `cancel` + `execute_command`; Normal needs `enter_command` or a quit binding) as config errors | Pitfall 4 | Adds an error class beyond D-01's list; skipping it risks an unrecoverable session |
| A4 | Reject multi-chord sequences starting with an unmodified printable char in `[insert]`/`[command]` | Pitfall 5 | Adds an error class; without it typed characters can be silently eaten |
| A5 | Validate `cmd:<name>` against `COMMANDS` at load time (error on unknown command) | Code Examples | Otherwise the error appears only at keypress time as "Unknown command" |
| A6 | Theme border types limited to `plain\|rounded\|double\|thick` | Don't Hand-Roll | Users may want more ratatui types |
| A7 | Marker strings: `sanitize_line(m) == m`, display width cap 1..=4 (placeholder derived from unread marker width); formatted timestamp width cap (e.g. <= 40) | Pitfalls 8, 12 | Cap values are my choice; D-11 only says "sanitized and width-checked" |
| A8 | `author_palette` must be non-empty, length <= 64 | Pitfall 11 | Cap value is my choice |
| A9 | Config file reads capped at 1 MiB and require a regular file | Pitfall 17 | Hardening only; cap is arbitrary |
| A10 | Do not add `serde` as a direct dependency in Phase 2 (author's stack lists it; Phase 3 needs it for JSON) | Standard Stack | The author may prefer it present now; trivial to add |
| A11 | `chrono` is preferred over `time`/jiff; `time` cannot do strftime syntax natively (training knowledge, not verified this session) | Alternatives | If wrong, `time` could also work; decision is already within discretion |
| A12 | Unbinding a key that is not bound (`"<C-x>" = "none"`) is a silent no-op | Keymap | Could be a warning instead |

## Open Questions (RESOLVED)

1. **CONF-07 says "server URL" but D-12 excludes it.**
   - What we know: REQUIREMENTS.md CONF-07 text lists "server URL"; 02-CONTEXT.md D-12 locks "No `server_url`/credentials yet (Phase 3)"; ROADMAP success criterion 4 lists only sidebar width, timestamp format, history size.
   - What's unclear: whether the traceability table should mark CONF-07 complete after Phase 2.
   - Recommendation: follow the locked decision; plan notes that the server-URL part lands in Phase 3 (AUTH/ROOM work), and the verifier should check CONF-07 against the roadmap criterion, not the requirement text.
   - **RESOLVED (author decision, 2026-11-16):** follow D-12. CONF-07 is split in REQUIREMENTS.md: in Phase 2 it covers history size, timestamp format and sidebar width, and the server-URL part is recorded on the CONF-07 line as deferred to Phase 3. Plans 02-01 and 02-02 verify CONF-07 against that text, which matches ROADMAP success criterion 4. `server_url` in config.toml stays an unknown key (warning only) in Phase 2.

2. **Timezone behavior (A2).**
   - What we know: Phase 1 displays UTC `HH:MM`; the code comment calls it a placeholder "until a time crate arrives in Phase 2".
   - What's unclear: local vs UTC, and whether a `timezone` setting is wanted.
   - Recommendation: local time via `chrono::Local`; ask the author once during plan review.
   - **RESOLVED (author decision, 2026-11-16):** system local time. The binary uses `chrono::Local` (set once in `run()`, plan 02-02 task 3); `App::new`, `App::with_config` and every test keep an injected UTC zone. No `timezone` setting is added.

3. **Lockout guard and insert-sequence rejection (A3, A4).**
   - Recommendation: include both; they are cheap and make "clear error instead of a stuck terminal" true.
   - **RESOLVED by planner adoption (2026-11-16; the author chose this route):** plan 02-03 task 3 includes both as config errors. The lockout guard requires Insert to bind `cancel`, Command to bind `cancel` and `execute_command`, and Normal to bind `enter_command` or a quit binding. In `[insert]` and `[command]`, a multi-key sequence starting with an unmodified printable key is an error. These are planner choices that the author can change at plan review.

4. **Plain-vs-warning treatment of dead chords (Pitfall 6).**
   - Recommendation: warnings, not errors.
   - **RESOLVED by planner adoption (2026-11-16; the author chose this route):** plan 02-03 task 3 reports dead chords (a leading count digit in Normal, `<C-i>`, `<C-m>`) as warnings and still applies the rows (02-03 A27). The planner also adopted the other values this research leaves to discretion: load-time `cmd:` validation (A5), border types (A6), marker and timestamp width caps (A7), palette length (A8), the 1 MiB regular-file read cap (A9), no direct serde dependency (A10) and the silent no-op for unbinding an unbound key (A12). Each one is recorded in its plan's Flagged Assumptions table as a planner choice that the author can change at plan review.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc | build, test | yes | 1.99.1 (MSRV 1.88) | — |
| clippy, rustfmt | Phase gate | yes | 0.1.99 / 1.10.0 | — |
| crates.io access | adding `toml`, `chrono` | yes (probe resolved and built them) | — | vendored cache already holds both |
| python3 | `scripts/smoke_pty.py` scenarios | yes | 3.14.10 | — |
| Real `~/.config/rockettui/` | none (absent here) | no | — | Tests use string sources / temp dirs |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none required.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | built-in `cargo test` (libtest) + `insta` 1 snapshots; pty e2e via `scripts/smoke_pty.py` |
| Config file | none (Cargo.toml dev-dependency `insta = "1"`) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| Baseline | 169 tests pass today (`cargo test` this session) |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CONF-01 | `config_dir` honors absolute XDG, ignores relative, falls back to `$HOME/.config`, `None` when neither | unit | `cargo test --lib config::paths::tests` | Wave 0 |
| CONF-01 | three files read from the dir; non-regular / oversize / non-UTF-8 becomes a Problem | unit (temp dir) | `cargo test --lib config::paths::tests::read_` | Wave 0 |
| CONF-02 | all sources `None` => `Config::default()`; default Config renders identical to `App::new` today; existing 169 tests unchanged | unit + regression | `cargo test --lib config::tests::absent_files_default && cargo test` | Wave 0 |
| CONF-03 | multiple syntax errors + several semantic errors across 3 files all reported, sorted, with line:col and key; unknown key = warning only; clean file with unknown keys = Ok | unit | `cargo test --lib config::problem config::reader` | Wave 0 |
| CONF-03 | error screen snapshot (errors + warnings + truncation); any key press quits; Release/Resize do not | snapshot + unit | `cargo test --lib ui::error_screen::tests` | Wave 0 |
| CONF-03 | binary: invalid config => error screen visible, any key exits non-zero, stderr has full report, terminal restored | e2e pty | `cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui config_error` | Wave 0 (new scenario) |
| CONF-04 | `keymap_preset` vim / vim-extras / unknown (error); extras adds `<Tab>`,`<S-Tab>` -> `focus_next`/`focus_prev`; `vim` equals `default_bindings()` | unit | `cargo test --lib config::keymap::tests::preset_` | Wave 0 |
| CONF-04 | `focus_next`/`focus_prev` cycle Sidebar->Messages->Input->Sidebar in reducer | unit (reducer) | `cargo test --lib app::update::tests::focus_cycle` | Wave 0 |
| CONF-05 | override replaces; `none` removes (incl. prune: `gg` removed leaves no `g` prefix); unknown action; bad notation; duplicate chords via different spellings; prefix clash names both bindings; `cmd:quit` fires; lockout guard; insert sequence rejection | unit | `cargo test --lib config::keymap input::trie input::actions` | Wave 0 |
| CONF-05 | binary: rebinding a key changes behavior | e2e pty | `python3 -I scripts/smoke_pty.py target/debug/rockettui config_keymap` | Wave 0 (new scenario) |
| CONF-06 | theme default == today's styles; per-field inheritance; `modifiers = []` clears; integer/named/hex colors; bad modifier name; border type; palette empty/invalid; markers sanitized + width; renderers use theme (cell fg/bg/modifier asserts) | unit + render | `cargo test --lib config::theme ui::` | Wave 0 |
| CONF-07 | `sidebar_width` 10-60 drives `layout::areas`; `history_size` 1-1000 stored; `timestamp_format` valid/invalid/empty/`%n`; `message_layout` selector; key timeout read by run loop | unit | `cargo test --lib config::settings ui::layout::tests app::messages::tests ui::messages::tests` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Phase gate:** full suite green, plus the smoke scenarios `quit`, `panic`, `insert` (existing) and the new `config_error`, `config_keymap`, plus manual UAT in a real terminal (edit each of the three files, observe the change; break each file, observe the screen)

### Wave 0 Gaps
- [ ] `src/config/*` modules with their `#[cfg(test)]` tests (none exist)
- [ ] `src/input/actions.rs` name table + exhaustive coverage test
- [ ] `Trie::remove` / `Trie::clashes` tests in `src/input/trie.rs`
- [ ] `src/ui/error_screen.rs` + insta snapshot
- [ ] `scripts/smoke_pty.py`: default isolated `XDG_CONFIG_HOME`, scenarios `config_error`, `config_keymap`
- [ ] Snapshots for a non-default config (e.g. `sidebar_width = 40`, `message_layout = "two_line"`)
- [ ] Framework install: none needed

## Security Domain

`security_enforcement` is enabled (absent/true in `.planning/config.json`, ASVS level 1, block on high).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (no credentials until Phase 3) | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Strict typed validation of every config value (ranges, enums, colors, strftime, notation, action names); reject-don't-sanitize for markers; `sanitize_line` on everything drawn or printed |
| V6 Cryptography | no | — |
| V7 Error Handling / Logging | yes (light) | Error messages built from `message()` + position, never source-line snippets; no logging added; never print file contents beyond the offending key/short value |
| V12 Files | yes (light) | Regular-file check, size cap, treat IO errors as config errors, no path built from file contents |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Terminal escape injection via config text (error messages, markers, timestamp output, `cmd:` text) | Tampering | `sanitize_line` before drawing/measuring/printing; reject markers that change under sanitize; sanitize stderr dump |
| Credential echo in a parse-error snippet (Phase 3 puts secrets in `config.toml`) | Information disclosure | Do not use `toml::de::Error` `Display`; build messages from `message()` + own line:col; never echo values of secret-named keys later |
| Panic from user input (empty palette modulo, invalid strftime `to_string()`, out-of-range timestamp, `unwrap`) | Denial of service | Validate at load; never `unwrap` on user-derived values; no `expect` outside built-in defaults |
| Blocking/huge read (FIFO, `/dev/zero`-like, giant file) | Denial of service | `metadata().is_file()` + 1 MiB cap |
| Relative `$XDG_CONFIG_HOME` pointing into an attacker-controlled cwd | Tampering | Ignore non-absolute XDG value per spec |
| Keymap self-lockout | Denial of service (self) | Lockout guard (A3) |
| Config permissions | Information disclosure | Out of scope now (no credentials; AUTH-02 is Phase 3); design `Report` warnings channel so Phase 3 can add the `chmod 600` warning |

## Sources

### Primary (HIGH confidence)
- Compiled and executed probes in a scratch crate this session: toml 1.2.1 `DeTable::parse`/`parse_recoverable`/`Spanned`, chrono 0.4.45 `StrftimeItems`/`Item::Error`/`Local`, ratatui 0.30.2 `Color::from_str`, trie remove/clash sketch.
- Registry source read this session: `toml-1.2.1/src/de/{error.rs,parser/*.rs}`, `serde_spanned-1.1.3`, `chrono-0.4.45/Cargo.toml` and `src/offset/local/unix.rs`, `jiff-1.0.0/Cargo.toml` + `src/fmt/strtime`, `ratatui-core-0.1.2/src/style/color.rs` + `style.rs`, `ratatui-widgets-0.3.2/src/borders.rs`.
- Repository files read this session: `src/{action,lib,main,event,tui,test_util}.rs`, `src/app/{mod,update,command}.rs`, `src/input/{keymap,key,trie,matcher}.rs`, `src/ui/{mod,layout,palette,messages,sidebar,status,input,wrap}.rs`, `Cargo.toml`, `Cargo.lock`, `scripts/smoke_pty.py`, `.planning/{REQUIREMENTS,STATE,config.json}`, `02-CONTEXT.md`, `01-VALIDATION.md`, `.claude/CLAUDE.md`.
- `gsd-tools query package-legitimacy check --ecosystem crates` for toml, serde, chrono, jiff, tempfile (all OK).

### Secondary (MEDIUM confidence)
- `cargo search` results for current crate versions (2026-11-16).

### Tertiary (LOW confidence)
- None used as authority. Items tagged in the Assumptions Log are design choices or unverified training knowledge (A11).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, crates compiled and exercised together with ratatui 0.30.2.
- Architecture: HIGH for the config core and call-site inventory (read from source); MEDIUM for exact `Theme` element naming (planner discretion).
- Pitfalls: HIGH for those reproduced by probes (multi-error parsing, `%n`, Local zone, duplicate keys, trie pruning); MEDIUM for matcher interactions (read from source, not executed).

**Research date:** 2026-11-16
**Valid until:** 2026-12-16 (toml/chrono APIs are stable; re-check versions if planning slips)
