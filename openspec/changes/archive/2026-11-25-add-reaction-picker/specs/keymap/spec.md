# Spec Delta

## ADDED Requirements

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
