# Spec Delta

## ADDED Requirements

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
