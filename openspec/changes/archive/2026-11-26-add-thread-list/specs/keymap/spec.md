# Spec Delta

## ADDED Requirements

### Requirement: List threads action
The bindable action names SHALL include `list_threads`. In Normal mode `gT` SHALL be bound to it by default, in every preset. It SHALL be rebindable and removable like any other action.

#### Scenario: Default key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gT` opens the thread list

#### Scenario: Rebind
- **WHEN** the user binds `"<C-t>" = "list_threads"` in `[normal]`
- **THEN** `Ctrl-t` opens the thread list

#### Scenario: Unbind
- **WHEN** the user sets `"gT" = "none"` in `[normal]`
- **THEN** `gT` does nothing

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `thread_list`
- **THEN** the config error screen reports the unknown action name
