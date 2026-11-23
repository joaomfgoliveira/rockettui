# Spec Delta

## Purpose

Defines the built-in key bindings, presets and the rules for rebinding keys per mode through `keymap.toml`.

## ADDED Requirements

### Requirement: Presets
The app SHALL ship a `vim` preset and a `vim-extras` preset, which is `vim` plus `Tab` and `Shift-Tab` pane focus cycling.

#### Scenario: Extras preset
- **WHEN** `keymap_preset = "vim-extras"`
- **THEN** `Tab` cycles pane focus

### Requirement: Key notation
Keys SHALL be written in Vim notation such as `<C-d>`, `<Esc>`, `<CR>` and `gg`.

#### Scenario: Bad notation
- **WHEN** `keymap.toml` contains an unparseable key
- **THEN** the config error screen reports it, with the key path and parser message truncated

### Requirement: Per-mode overrides
`keymap.toml` SHALL have `[normal]`, `[insert]` and `[command]` tables mapping a key sequence to an action name, layered on top of the active preset. Binding `"none"` SHALL remove a preset binding.

#### Scenario: Rebind
- **WHEN** the user binds `"J" = "move_down"` in `[normal]`
- **THEN** `J` moves down in Normal mode

#### Scenario: Unbind
- **WHEN** the user sets `"<C-d>" = "none"`
- **THEN** `Ctrl-d` does nothing

### Requirement: Action vocabulary
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send` and `discard_send`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error.

#### Scenario: Command binding
- **WHEN** the user binds `"<C-q>" = "cmd:quit"`
- **THEN** `Ctrl-q` runs `:quit`

### Requirement: Safe overrides
The app SHALL reject overrides that bind the same keys twice in one mode table of `keymap.toml`, make one binding a prefix of another, remove all ways to leave a mode or quit (lockout), or bind a multi-key sequence starting with a plain character (no Ctrl or Alt) in Insert or Command mode. A user binding replaces a preset binding of the same keys.

#### Scenario: Prefix clash
- **WHEN** the user binds `g` while `gg` is bound
- **THEN** a config error names both bindings
