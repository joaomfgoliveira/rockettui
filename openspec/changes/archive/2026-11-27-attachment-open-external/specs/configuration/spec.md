# Spec Delta

## ADDED Requirements

### Requirement: Attachment opener settings
`config.toml` SHALL support an `[open]` table with the optional keys `image`, `video`, `audio` and `default`, each a non-empty array of strings: the program and its leading arguments. The program for an attachment SHALL be the key for its kind, else `default`, else `xdg-open` (`open` on macOS). An empty array, an empty program name, a non-string entry or an entry with a NUL character SHALL be config errors. The values SHALL NOT be run through a shell and SHALL be run only when the user opens an attachment.

#### Scenario: Video player
- **WHEN** the user sets `[open]` with `video = ["mpv", "--force-window"]`
- **THEN** opening a video runs `mpv --force-window <file>`

#### Scenario: Fallback
- **WHEN** only `default = ["xdg-open"]` is set and the user opens an audio file
- **THEN** `xdg-open <file>` runs

#### Scenario: Nothing set
- **WHEN** the table is absent
- **THEN** the platform's opener (`xdg-open` on Linux) is used

#### Scenario: Empty list
- **WHEN** the user sets `video = []`
- **THEN** the error screen reports the key and that a program is required

#### Scenario: Not an array
- **WHEN** the user sets `video = "mpv"`
- **THEN** the error screen reports the key and that an array of strings is expected
