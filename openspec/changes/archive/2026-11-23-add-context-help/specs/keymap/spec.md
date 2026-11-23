# Spec Delta

## MODIFIED Requirements

### Requirement: Action vocabulary
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send`, `discard_send`, `edit_message`, `delete_message`, `show_help` and `show_context`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error. By default `e` is bound to `edit_message`, `dd` to `delete_message` and `?` to `show_context` in Normal mode; `show_help` has no default key.

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
