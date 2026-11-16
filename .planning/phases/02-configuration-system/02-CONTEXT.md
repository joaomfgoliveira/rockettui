# Phase 2: Configuration System - Context

**Gathered:** 2026-11-16
**Status:** Ready for planning

<domain>
## Phase Boundary

Make the TUI customizable through three TOML files in `~/.config/rockettui/` (honoring `$XDG_CONFIG_HOME`): `config.toml`, `keymap.toml`, `theme.toml`, with built-in defaults when files are absent. Covers CONF-01..CONF-07. Still fake data only; no server URL, credentials or networking (Phase 3+).

</domain>

<decisions>
## Implementation Decisions

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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project definition
- `.planning/PROJECT.md` — constraints (XDG, TOML, invalid entries yield clear errors, TEA architecture), key decisions
- `.planning/REQUIREMENTS.md` — CONF-01..CONF-07
- `.planning/ROADMAP.md` — Phase 2 goal and success criteria
- `docs/objective.md` — Configuration section: file split, presets, defaults, planned `config/` module

### Prior phase decisions
- `.planning/phases/01-modal-tui-shell/01-CONTEXT.md` — D-06 (seq timeout), D-07/D-08 (keymap data shape + Vim notation), D-10 (message layouts), D-13 (sidebar width fixed), deferred items handed to Phase 2

### Code seams
- `src/input/keymap.rs` — `Keymap::from_bindings`, `default_bindings()` (rows of mode, notation, action)
- `src/input/key.rs` — `parse_keys` Vim notation parser
- `src/ui/palette.rs` — `AUTHOR_PALETTE`, `author_color`
- `src/action.rs`, `src/app/mod.rs`, `src/app/update.rs` — Action enum, App state, reducer

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Keymap::from_bindings` + `parse_keys`: already take `(Mode, notation, Action)` rows with later-wins; Phase 2 only adds the TOML row source, presets, `none` unbinding and clash detection.
- `ui/palette.rs`: deterministic author colors; the const palette becomes a theme value.
- `ui/layout.rs` and `ui/sidebar.rs`: fixed ~28-col sidebar to be driven by `sidebar_width`.
- Both message layouts (Compact/TwoLine enum) already exist; only the selector is missing.

### Established Patterns
- TEA: one `App`, one `Action` enum, one channel, one reducer; config should be loaded before `App` creation and held in state, not read from the reducer.
- Untrusted/user text passes through `sanitize_line` before drawing and measuring (glyph/marker themes must follow).
- Snapshot tests with `insta` for full-screen rendering; theme/defaults should keep existing snapshots stable.

### Integration Points
- Startup in `src/main.rs`/`src/lib.rs`: load config, either build `App` or show the error screen.
- Matcher timeout (D-06) and sidebar width become config inputs.
- Hard-coded `Style`/`Color` uses across `src/ui/*` become theme lookups.

</code_context>

<specifics>
## Specific Ideas

- Error screen should list all problems at once so a single edit-and-relaunch fixes everything.
- Both message layouts should be user-selectable via `config.toml` (carried from Phase 1 specifics).

</specifics>

<deferred>
## Deferred Ideas

- `Ctrl-k` room switch (picker UI) — future phase.
- Config hot reload / reload key on the error screen — MISC-03 (v2).
- Per-user author color overrides (`[authors] alice = "red"`) — not requested; backlog.
- Bundled theme gallery — MISC-04 (v2).

</deferred>

---

*Phase: 2-Configuration System*
*Context gathered: 2026-11-16*
