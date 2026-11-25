# Spec Delta

## ADDED Requirements

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
