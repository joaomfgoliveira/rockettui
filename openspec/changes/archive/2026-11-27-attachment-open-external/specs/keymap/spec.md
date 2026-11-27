# Spec Delta

## MODIFIED Requirements

### Requirement: Action vocabulary
Bindable actions SHALL be a fixed set of snake_case names (including `retry_send`, `discard_send`, `edit_message`, `delete_message`, `show_reactions`, `open_channel`, `show_image`, `open_attachment`, `show_help` and `show_context`), plus `none` and `cmd:<command>` to run a command-line command. An unknown action name SHALL be a config error. By default `e` is bound to `edit_message`, `dd` to `delete_message`, `gr` to `show_reactions`, `gc` to `open_channel`, `gp` to `show_image`, `go` to `open_attachment` and `?` to `show_context` in Normal mode; `show_help` has no default key.

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

#### Scenario: Default reactions key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gr` opens the reactions popup in Normal mode, and `gg` still jumps to the oldest message

#### Scenario: Rebind reactions
- **WHEN** the user binds `"R" = "show_reactions"` in `[normal]`
- **THEN** `R` opens the reactions popup

#### Scenario: Unbind reactions
- **WHEN** the user sets `"gr" = "none"` in `[normal]`
- **THEN** `gr` does nothing

#### Scenario: Default open-channel key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gc` runs `open_channel` in Normal mode in every preset, and the other `g` bindings are unchanged

#### Scenario: Rebind open channel
- **WHEN** the user binds `"C" = "open_channel"` in `[normal]`
- **THEN** `C` opens the channel the cursor message references

#### Scenario: Unbind open channel
- **WHEN** the user sets `"gc" = "none"` in `[normal]`
- **THEN** `gc` does nothing

#### Scenario: Default show-image key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gp` runs `show_image` in Normal mode in every preset, and the other `g` bindings are unchanged

#### Scenario: Rebind show image
- **WHEN** the user binds `"P" = "show_image"` in `[normal]`
- **THEN** `P` opens the image of the cursor message

#### Scenario: Unbind show image
- **WHEN** the user sets `"gp" = "none"` in `[normal]`
- **THEN** `gp` does nothing

#### Scenario: Default open-attachment key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `go` runs `open_attachment` in Normal mode in every preset, and the other `g` bindings are unchanged

#### Scenario: Rebind open attachment
- **WHEN** the user binds `"O" = "open_attachment"` in `[normal]`
- **THEN** `O` opens the attachment of the cursor message

#### Scenario: Unbind open attachment
- **WHEN** the user sets `"go" = "none"` in `[normal]`
- **THEN** `go` does nothing
