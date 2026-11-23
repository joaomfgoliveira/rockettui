# Spec Delta

## Purpose

Lets users discover the key bindings and commands of the running app from inside it, always matching their active preset and `keymap.toml` overrides.

## ADDED Requirements

### Requirement: Opening and closing help
The app SHALL open a help overlay over the messages pane with `?` in Normal mode or with `:help`. `q`, `Esc`, `Ctrl-c` and `?` SHALL close it. Opening it SHALL NOT change the current mode.

#### Scenario: Open from Normal mode
- **WHEN** the user presses `?` in Normal mode
- **THEN** the help overlay is shown and the mode badge still reads Normal

#### Scenario: Open from the command line
- **WHEN** the user runs `:help`
- **THEN** the help overlay is shown

#### Scenario: Close
- **WHEN** the help overlay is open and the user presses `q`
- **THEN** the overlay closes and the focused pane and room are as before

### Requirement: Help is generated from the live keymap
The overlay SHALL list the bindings of the active keymap, meaning the selected preset with `keymap.toml` overrides applied, grouped by mode (Normal, Insert, Command), each with the keys in Vim notation and a short description of the action. A binding removed with `"none"` SHALL NOT be listed, and a `cmd:<command>` binding SHALL be shown as that command.

#### Scenario: Rebound key
- **WHEN** the user binds `"J" = "move_down"` in `[normal]` and opens help
- **THEN** the Normal section lists `J` for moving down

#### Scenario: Unbound key
- **WHEN** the user sets `"<C-d>" = "none"` and opens help
- **THEN** `<C-d>` is not listed

#### Scenario: Preset keys
- **WHEN** `keymap_preset = "vim-extras"` and the user opens help
- **THEN** the Normal section lists `<Tab>` and `<S-Tab>`

### Requirement: Commands listed
The overlay SHALL also list the available `:` commands with their aliases and a short description.

#### Scenario: Commands section
- **WHEN** the user opens help
- **THEN** `:quit` (with `:q`) and `:reconnect` (with `:retry`) are listed

### Requirement: Scrolling
While the overlay is open, the motion keys of the active Normal keymap (`j`, `k`, `gg`, `G`, `Ctrl-d`, `Ctrl-u` by default) SHALL scroll it, with numeric prefixes working as in Normal mode. No other key SHALL act on the room, the input or the keymap while it is open, and scrolling past either end SHALL stop at that end.

#### Scenario: Scroll
- **WHEN** the overlay is longer than the pane and the user presses `j`
- **THEN** the overlay scrolls one line and the room cursor does not move

#### Scenario: Other keys ignored
- **WHEN** the overlay is open and the user presses `dd` or `i`
- **THEN** nothing happens to any message and the app does not enter Insert mode

### Requirement: Help in small terminals and with live updates
The overlay SHALL fit the messages pane at any size and be usable at the smallest supported size by scrolling. Incoming messages and connection changes SHALL keep updating the room underneath while it is open.

#### Scenario: Message arrives while open
- **WHEN** the overlay is open and a live message arrives
- **THEN** the message is added to its room and the overlay stays open and unchanged

### Requirement: Help styling
Every element of the overlay (border, title, section heading, key column, description) SHALL be themeable through `theme.toml` like other UI elements, with built-in defaults, and SHALL show only text that is built into the app, never message content or credentials.

#### Scenario: Themed heading
- **WHEN** `theme.toml` sets `fg` on the help section heading
- **THEN** section headings are drawn in that color
