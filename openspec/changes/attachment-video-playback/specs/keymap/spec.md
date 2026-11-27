# Spec Delta

## ADDED Requirements

### Requirement: Play-video action
`play_video` SHALL be a bindable action, bound to `gv` in Normal mode in every preset. In the messages pane on a message with a video it SHALL step that video's box (load, play, stop). Elsewhere it SHALL do nothing. The other `g` bindings SHALL be unchanged.

#### Scenario: Default key
- **WHEN** no `keymap.toml` overrides it
- **THEN** `gv` runs `play_video` in Normal mode in every preset, and `gg`, `gp` and `go` are unchanged

#### Scenario: Rebind
- **WHEN** the user binds `"V" = "play_video"` in `[normal]`
- **THEN** `V` steps the video of the cursor message

#### Scenario: Unbind
- **WHEN** the user sets `"gv" = "none"` in `[normal]`
- **THEN** `gv` does nothing

### Requirement: Video player keys
`keymap.toml` SHALL accept a `[video]` table, applying while a video plays or is paused, for the actions `video_pause`, `video_seek_back`, `video_seek_forward`, `video_seek_back_far`, `video_seek_forward_far` and `video_stop`, with the same notation, `none` removal and clash rules as the other tables. The defaults SHALL be `Space`, `h`, `l`, `H`, `L`, and `Esc`, `q` and `Ctrl-c` for `video_stop`. The six actions SHALL be accepted only in `[video]`, and any other action name there SHALL be a config error. Removing every way to stop (`video_stop` with no key) SHALL be a config error.

#### Scenario: Defaults
- **WHEN** no `keymap.toml` overrides them
- **THEN** `Space` pauses, `h` and `l` seek 5 seconds, `H` and `L` seek 30 seconds, and `Esc`, `q` and `Ctrl-c` stop

#### Scenario: Rebind seek
- **WHEN** the user binds `"," = "video_seek_back"` and `"." = "video_seek_forward"` in `[video]`
- **THEN** `,` and `.` seek while a video plays

#### Scenario: Wrong table
- **WHEN** the user binds `"x" = "video_pause"` in `[normal]`
- **THEN** a config error says the action belongs in `[video]`

#### Scenario: No way to stop
- **WHEN** the user unbinds `Esc`, `q` and `Ctrl-c` in `[video]` without binding another key to `video_stop`
- **THEN** a config error says a key to stop the video is required

## MODIFIED Requirements

### Requirement: Per-mode overrides
`keymap.toml` SHALL have `[normal]`, `[insert]`, `[command]` and `[video]` tables mapping a key sequence to an action name, layered on top of the active preset. Binding `"none"` SHALL remove a preset binding.

#### Scenario: Rebind
- **WHEN** the user binds `"J" = "move_down"` in `[normal]`
- **THEN** `J` moves down in Normal mode

#### Scenario: Unbind
- **WHEN** the user sets `"<C-d>" = "none"`
- **THEN** `Ctrl-d` does nothing

#### Scenario: Player table
- **WHEN** the user binds `"s" = "video_pause"` in `[video]`
- **THEN** `s` pauses a playing video
