# Spec Delta

## ADDED Requirements

### Requirement: Inline video settings
`config.toml` SHALL support `inline_video` (boolean, default `true`) and `ffmpeg` and `ffprobe` (strings, default `ffmpeg` and `ffprobe`): the program names or paths used to decode video. With `inline_video = false` no video box is drawn or loaded and a video keeps only its caption. The programs SHALL be run directly, never through a shell, and only when a video is loaded or played. A value of the wrong type, an empty program name or one with a NUL character SHALL be a config error.

#### Scenario: Turned off
- **WHEN** the user sets `inline_video = false`
- **THEN** a room with videos shows only their captions and runs no program

#### Scenario: Another ffmpeg
- **WHEN** the user sets `ffmpeg = "/opt/ffmpeg/bin/ffmpeg"`
- **THEN** that program decodes videos

#### Scenario: Invalid value
- **WHEN** the user sets `ffmpeg = ""` or `inline_video = "yes"`
- **THEN** the error screen reports the key and what is expected
