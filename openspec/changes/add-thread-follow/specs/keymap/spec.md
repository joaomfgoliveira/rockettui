# Spec Delta

## ADDED Requirements

### Requirement: Follow thread action
The bindable action names SHALL include `toggle_follow_thread`. In Normal mode `gf` SHALL be bound to it by default, in every preset. It SHALL be rebindable and removable like any other action.

#### Scenario: Default key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gf` toggles following of the thread under the cursor

#### Scenario: Rebind
- **WHEN** the user binds `"F" = "toggle_follow_thread"` in `[normal]`
- **THEN** `F` toggles following

#### Scenario: Unbind
- **WHEN** the user sets `"gf" = "none"` in `[normal]`
- **THEN** `gf` does nothing

#### Scenario: Unknown spelling
- **WHEN** `keymap.toml` binds a key to `follow_thread`
- **THEN** the config error screen reports the unknown action name
