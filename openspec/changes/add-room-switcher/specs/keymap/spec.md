# Spec Delta

## ADDED Requirements

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
