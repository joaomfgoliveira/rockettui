# Tasks

## 1. Action and keymap

- [x] 1.1 Add `Action::ShowHelp` and the `show_help` name in `input/actions.rs` (both match arms and `ACTION_NAMES`)
- [x] 1.2 Bind `?` to `show_help` in Normal mode in `default_bindings`; tests for the default row and the `keymap.toml` rebind
- [x] 1.3 Check that `?` parses as a plain key and does not clash with existing bindings or the safe-override rules

## 2. Content model

- [x] 2.1 Add a static description table keyed by action name, and a test that every name in `ACTION_NAMES` has one
- [x] 2.2 Add a pure function that builds help lines (per-mode sections, then commands) from `&Keymap` and the command table
- [x] 2.3 Unit tests: rebound key listed, `"none"` removed key absent, `vim-extras` keys listed, `cmd:` shown as a command, commands with aliases listed

## 3. App state and reducer

- [x] 3.1 Add the optional help state (scroll offset) to `App`
- [x] 3.2 Open from `show_help` and from `:help` (command table row); `:help` returns to Normal mode
- [x] 3.3 While open, route keys through the Normal keymap and allow only motions (scroll, clamped, counts work), `cancel`, `show_help` and a fixed `q`; drop everything else, including `dd` and `i`
- [x] 3.4 Reducer tests: opens without changing mode, closes with each key, other actions ignored, live messages still applied underneath

## 4. UI

- [x] 4.1 Add `ui/help.rs`: bordered titled overlay over the messages pane, cleared first, sanitized text, aligned key column, scroll clamped
- [x] 4.2 Add the `help.*` theme elements with defaults and `theme.toml` parsing
- [x] 4.3 Snapshot tests: normal size, smallest supported size, scrolled to the end

## 5. Docs and wrap-up

- [ ] 5.1 Mention `?` and `:help` in the README keymap section and the example theme
- [x] 5.2 Run `cargo test`, `cargo clippy` and `cargo fmt --check`
- [ ] 5.3 Try it against the company server, then `openspec validate add-help-overlay --strict`
