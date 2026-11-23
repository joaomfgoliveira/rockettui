# Proposal

## Why

rockettui is entirely keyboard-driven and its keymap keeps growing (presets, `keymap.toml` overrides, `e`, `dd`, `r`, folds, pane focus). There is nowhere in the app to see what keys exist, so the only way to find out is to read the source or the config. A help screen is also the groundwork for later features (which-key hints, room switching): it makes new bindings discoverable the moment they ship.

## What Changes

- Add a help overlay opened with `?` in Normal mode or `:help`, drawn over the messages pane.
- The overlay lists the key bindings of the live keymap (active preset plus `keymap.toml` overrides), grouped by mode, each with a short description of its action, plus the available `:` commands. It is generated, so it can never go stale.
- The overlay scrolls with the usual motions and closes with `q`, `Esc` or `?`.
- Add a bindable action `show_help`, bound to `?` in Normal mode by default. This only adds to the action vocabulary and renames nothing.
- The overlay is not a new mode. Like the confirmation prompt, it is a state of the app, so the badge still says NORMAL and `keymap.toml` gets no new table.

Out of scope: which-key style hints while typing a prefix (a later change that reuses the same binding data), searching inside the help, and per-room or per-message context help.

## Capabilities

### New Capabilities
- `help`: the in-app help overlay: how it opens and closes, what it lists, how it scrolls.

### Modified Capabilities
- `keymap`: new bindable action `show_help`, bound to `?` by default in Normal mode.
- `modal-navigation`: command mode gains `:help`, and the overlay's input handling is stated next to the other transient UI (confirmation prompt).

## Impact

- `input/`: new `Action::ShowHelp`, its name in `actions.rs`, the default `?` row in `keymap.rs`.
- `app/`: overlay state on `App`, its handling in the reducer, a `:help` row in `command.rs`.
- `ui/`: a new `help.rs` widget, themed through `theme.toml`.
- `config/`: new theme element names for the overlay (additive).
- No new dependencies, no network, no change to credentials or message handling.
