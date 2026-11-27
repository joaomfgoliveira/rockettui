# Spec Delta

## ADDED Requirements

### Requirement: Inline image settings
`config.toml` SHALL support `inline_images` (boolean, default `true`) and `image_max_rows` (3-40, default 12). With `inline_images = false` no attachment image is fetched or drawn inline and `[attachment: name]` is shown as without the feature. `image_max_rows` is the tallest an inline image may be, in rows. Invalid or out-of-range values SHALL be config errors.

#### Scenario: Turned off
- **WHEN** the user sets `inline_images = false`
- **THEN** a room with image attachments shows only their captions and requests no image

#### Scenario: Taller boxes
- **WHEN** the user sets `image_max_rows = 20`
- **THEN** an image that needs more than 12 rows may take up to 20

#### Scenario: Invalid rows
- **WHEN** the user sets `image_max_rows = 100`
- **THEN** the error screen reports the value and the allowed range

### Requirement: Animated image setting
`config.toml` SHALL support `animate_images` (boolean, default `true`). With `animate_images = false` an animated image attachment is shown as its first frame, inline and in the full-size view. A value that is not a boolean SHALL be a config error.

#### Scenario: Turned off
- **WHEN** the user sets `animate_images = false`
- **THEN** a gif shows its first frame and no redraw timer runs for it

#### Scenario: Invalid value
- **WHEN** the user sets `animate_images = "no"`
- **THEN** the error screen reports the key and that true or false is expected
