# keymap Specification

## Purpose

Defines the built-in key bindings, presets and the rules for rebinding keys per mode through `keymap.toml`.

## Requirements

### Requirement: Presets
The app SHALL ship a `vim` preset and a `vim-extras` preset, which is `vim` plus `Tab` and `Shift-Tab` pane focus cycling. The cycle SHALL visit only the sidebar and the messages pane, and SHALL NOT stop on the input bar. When the input bar is focused, `Tab` SHALL move focus to the sidebar and `Shift-Tab` to the messages pane.

#### Scenario: Extras preset
- **WHEN** `keymap_preset = "vim-extras"`
- **THEN** `Tab` cycles pane focus

#### Scenario: Cycle skips the input bar
- **WHEN** `keymap_preset = "vim-extras"` and the user presses `Tab` or `Shift-Tab` repeatedly starting from the sidebar
- **THEN** focus alternates between the sidebar and the messages pane and never lands on the input bar

#### Scenario: Cycling away from the input bar
- **WHEN** the input bar is focused in Normal mode and the user presses `Tab`
- **THEN** the sidebar is focused, and `Shift-Tab` from the input bar focuses the messages pane

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
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send`, `discard_send`, `edit_message`, `delete_message`, `show_reactions`, `show_help` and `show_context`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error. By default `e` is bound to `edit_message`, `dd` to `delete_message`, `gr` to `show_reactions` and `?` to `show_context` in Normal mode; `show_help` has no default key.

#### Scenario: Command binding
- **WHEN** the user binds `"<C-q>" = "cmd:quit"`
- **THEN** `Ctrl-q` runs `:quit`

#### Scenario: Rebind edit
- **WHEN** the user binds `"E" = "edit_message"` in `[normal]`
- **THEN** `E` starts editing the user's own message under the cursor

#### Scenario: Rebind help
- **WHEN** the user binds `"H" = "show_help"` in `[normal]`
- **THEN** `H` opens the help overlay

#### Scenario: Restore the old help key
- **WHEN** the user binds `"?" = "show_help"` in `[normal]`
- **THEN** `?` opens the help overlay directly and the context popup has no default key

#### Scenario: Default reactions key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gr` opens the reactions popup in Normal mode, and `gg` still jumps to the oldest message

#### Scenario: Rebind reactions
- **WHEN** the user binds `"R" = "show_reactions"` in `[normal]`
- **THEN** `R` opens the reactions popup

#### Scenario: Unbind reactions
- **WHEN** the user sets `"gr" = "none"` in `[normal]`
- **THEN** `gr` does nothing

### Requirement: Safe overrides
The app SHALL reject overrides that bind the same keys twice in one mode table of `keymap.toml`, make one binding a prefix of another, remove all ways to leave a mode or quit (lockout), or bind a multi-key sequence starting with a plain character (no Ctrl or Alt) in Insert or Command mode. A user binding replaces a preset binding of the same keys.

#### Scenario: Prefix clash
- **WHEN** the user binds `g` while `gg` is bound
- **THEN** a config error names both bindings

### Requirement: Search actions
The bindable action names SHALL include `search_start`, `search_next` and `search_prev`. In Normal mode `/` SHALL be bound to `search_start`, `n` to `search_next` and `N` to `search_prev` by default, in every preset. They SHALL be rebindable and removable like any other action.

#### Scenario: Default search keys
- **WHEN** no `keymap.toml` overrides them
- **THEN** `/` opens the search prompt and `n` and `N` jump to the next and previous match in Normal mode

#### Scenario: Rebind search
- **WHEN** the user binds `"<C-f>" = "search_start"` in `[normal]`
- **THEN** `Ctrl-f` opens the search prompt

#### Scenario: Unbind search jumps
- **WHEN** the user sets `"n" = "none"` in `[normal]`
- **THEN** `n` does nothing in Normal mode

### Requirement: React action
The bindable action names SHALL include `react_message`. In Normal mode `+` SHALL be bound to `react_message` by default, in every preset. It SHALL be rebindable and removable like any other action.

#### Scenario: Default react key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `+` opens the reaction picker in Normal mode

#### Scenario: Rebind react
- **WHEN** the user binds `"R" = "react_message"` in `[normal]`
- **THEN** `R` opens the reaction picker

#### Scenario: Unbind react
- **WHEN** the user sets `"+" = "none"` in `[normal]`
- **THEN** `+` does nothing

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `react_msg`
- **THEN** the config error screen reports the unknown action name

### Requirement: Switch room action
The bindable action names SHALL include `switch_room`. In Normal mode `<C-k>` SHALL be bound to `switch_room` by default, in every preset. It SHALL be rebindable and removable like any other action.

#### Scenario: Default switch key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `Ctrl-k` opens the room switcher in Normal mode

#### Scenario: Rebind switch
- **WHEN** the user binds `"<C-p>" = "switch_room"` in `[normal]`
- **THEN** `Ctrl-p` opens the room switcher

#### Scenario: Unbind switch
- **WHEN** the user sets `"<C-k>" = "none"` in `[normal]`
- **THEN** `Ctrl-k` does nothing

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `switch_rooms`
- **THEN** the config error screen reports the unknown action name

### Requirement: Thread view actions
The bindable action names SHALL include `open_thread` and `close_thread`. In Normal mode `gt` SHALL be bound to `open_thread` by default, in every preset, and `gg` and `gr` SHALL keep working. `close_thread` SHALL have no default key, because `Esc` closes the pane already. Both SHALL be rebindable and removable like any other action.

#### Scenario: Default open key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gt` opens the thread of the message under the cursor

#### Scenario: Existing g keys
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gg` still jumps to the oldest message and `gr` still opens the reactions popup

#### Scenario: Rebind open
- **WHEN** the user binds `"T" = "open_thread"` in `[normal]`
- **THEN** `T` opens the thread

#### Scenario: Bind close
- **WHEN** the user binds `"q" = "close_thread"` in `[normal]` and a thread is open
- **THEN** `q` closes the pane

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `open_threads`
- **THEN** the config error screen reports the unknown action name

### Requirement: Thread reply actions
The bindable action names SHALL include `reply_in_thread` and `toggle_reply_to_channel`. In Normal mode `gR` SHALL be bound to `reply_in_thread` by default and in Insert mode `<C-b>` SHALL be bound to `toggle_reply_to_channel`, in every preset. Both SHALL be rebindable and removable like any other action.

#### Scenario: Default keys
- **WHEN** no `keymap.toml` overrides them
- **THEN** `gR` starts a thread reply in Normal mode and `<C-b>` toggles also-send-to-channel in Insert mode

#### Scenario: Rebind
- **WHEN** the user binds `"<C-r>" = "reply_in_thread"` in `[normal]`
- **THEN** `Ctrl-r` starts a thread reply

#### Scenario: Unbind
- **WHEN** the user sets `"<C-b>" = "none"` in `[insert]`
- **THEN** `Ctrl-b` does nothing in Insert mode

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `reply_thread`
- **THEN** the config error screen reports the unknown action name
