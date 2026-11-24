# Spec Delta

## ADDED Requirements

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
