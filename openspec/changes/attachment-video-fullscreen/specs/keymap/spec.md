# Spec Delta

## ADDED Requirements

### Requirement: Fullscreen video action
`play_video_fullscreen` SHALL be a bindable action, bound to `gV` in Normal mode in every preset. In the messages pane on a message with a video it SHALL open that video in the fullscreen overlay. Elsewhere it SHALL do nothing. The other `g` bindings SHALL be unchanged.

#### Scenario: Default key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gV` runs `play_video_fullscreen` in Normal mode in every preset, and `gv`, `gp` and `go` are unchanged

#### Scenario: Rebind
- **WHEN** the user binds `"F" = "play_video_fullscreen"` in `[normal]`
- **THEN** `F` opens the fullscreen player on the video of the cursor message

#### Scenario: Unbind
- **WHEN** the user sets `"gV" = "none"` in `[normal]`
- **THEN** `gV` does nothing
