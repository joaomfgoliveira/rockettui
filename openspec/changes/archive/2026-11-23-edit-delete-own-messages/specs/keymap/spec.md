# Spec Delta

## MODIFIED Requirements

### Requirement: Action vocabulary
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send`, `discard_send`, `edit_message` and `delete_message`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error. By default `e` is bound to `edit_message` and `dd` to `delete_message` in Normal mode.

#### Scenario: Command binding
- **WHEN** the user binds `"<C-q>" = "cmd:quit"`
- **THEN** `Ctrl-q` runs `:quit`

#### Scenario: Rebind edit
- **WHEN** the user binds `"E" = "edit_message"` in `[normal]`
- **THEN** `E` starts editing the user's own message under the cursor
