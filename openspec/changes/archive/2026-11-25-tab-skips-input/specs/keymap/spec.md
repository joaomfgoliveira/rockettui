# Spec Delta

## MODIFIED Requirements

### Requirement: Presets
The app SHALL ship a `vim` preset and a `vim-extras` preset, which is `vim` plus `Tab` and `Shift-Tab` pane focus cycling. The cycle SHALL visit only the sidebar and the messages pane, and SHALL NOT stop on the input bar. When the input bar is focused, `Tab` SHALL move focus to the sidebar and `Shift-Tab` to the messages pane.

#### Scenario: Extras preset
- **WHEN** `keymap_preset = "vim-extras"`
- **THEN** `Tab` cycles pane focus

#### Scenario: Cycle skips the input bar
- **WHEN** `keymap_preset = "vim-extras"` and the user presses `Tab` or `Shift-Tab` repeatedly starting from the sidebar
- **THEN** focus alternates between the sidebar and the messages pane and never lands on the input bar

#### Scenario: Cycling away from the input bar
- **WHEN** the input bar is focused in Normal mode and the user presses `Tab`
- **THEN** the sidebar is focused, and `Shift-Tab` from the input bar focuses the messages pane
