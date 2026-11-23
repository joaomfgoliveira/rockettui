# Tasks

## 1. Action and keymap

- [x] 1.1 Add `Action::ShowContext` and the `show_context` name in `input/actions.rs` (both match arms, `ACTION_NAMES`, the redacting `Debug`)
- [x] 1.2 Move the default `?` row from `show_help` to `show_context` in `default_bindings`; update the keymap tests and add one for restoring `"?" = "show_help"`
- [x] 1.3 Add a description for `show_context` to the help descriptions table and fix the `?` expectations in the existing help tests and snapshots

## 2. Content model

- [x] 2.1 Add `app/context.rs` with the object kinds and `classify(&App)` using `sidebar::selected_row`, `edit::own_under_cursor`, `send::unsent_under_cursor` and `App.focus`
- [x] 2.2 Add the static kind-to-action-names table and `lines(&App)` resolving keys from the live Normal keymap (unbound actions dropped, `cmd:` rows ignored, ends with the full-help line)
- [x] 2.3 Unit tests: one per kind (room, folder, header, own message, unsent message, other message, input, nothing), rebound key, `"none"` removed action, table names valid

## 3. App state and reducer

- [x] 3.1 Add `App.context` state; open from `show_context`
- [x] 3.2 While open, close on any key and consume it; a key resolving to `show_context` or `show_help` opens the full overlay instead; `KeyTimeout` and counts do not act
- [x] 3.3 Let `show_context` also close the full overlay (`app/update.rs` `help_dispatch`)
- [x] 3.4 Reducer tests: opens without changing mode, focus or selection; any key closes without acting (`j`, `dd`, `i`); `?` `?` opens the overlay and a third `?` closes it; live messages still applied underneath

## 4. UI

- [x] 4.1 Add `ui/context.rs`: bordered popup above the status row, cleared first, sanitized, height capped and clipped at small sizes
- [x] 4.2 Add the `context.*` theme elements with defaults and `theme.toml` parsing; update the element-count test
- [x] 4.3 Snapshot tests: room, own message, folder and the smallest supported size

## 5. Wrap-up

- [x] 5.1 Run `cargo test`, `cargo clippy` and `cargo fmt --check`
- [x] 5.2 Try it against the local server on each kind of object, then `openspec validate add-context-help --strict`
