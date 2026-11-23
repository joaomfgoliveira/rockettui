# keymap Specification

## Purpose

Defines the built-in key bindings, presets and the rules for rebinding keys per mode through `keymap.toml`.

## Requirements

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
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send`, `discard_send`, `edit_message`, `delete_message` and `show_help`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error. By default `e` is bound to `edit_message`, `dd` to `delete_message` and `?` to `show_help` in Normal mode.

#### Scenario: Command binding
- **WHEN** the user binds `"<C-q>" = "cmd:quit"`
- **THEN** `Ctrl-q` runs `:quit`

#### Scenario: Rebind edit
- **WHEN** the user binds `"E" = "edit_message"` in `[normal]`
- **THEN** `E` starts editing the user's own message under the cursor

#### Scenario: Rebind help
- **WHEN** the user binds `"H" = "show_help"` in `[normal]`
- **THEN** `H` opens the help overlay

### Requirement: Safe overrides
The app SHALL reject overrides that bind the same keys twice in one mode table of `keymap.toml`, make one binding a prefix of another, remove all ways to leave a mode or quit (lockout), or bind a multi-key sequence starting with a plain character (no Ctrl or Alt) in Insert or Command mode. A user binding replaces a preset binding of the same keys.

#### Scenario: Prefix clash
- **WHEN** the user binds `g` while `gg` is bound
- **THEN** a config error names both bindings
