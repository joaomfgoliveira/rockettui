# Spec Delta

## ADDED Requirements

### Requirement: Play-video action on messages
A message that has a video attachment the client may fetch SHALL list `play_video` in the context popup, with the keys currently bound to it, saying what the next press does: load the video when the box is a placeholder, play it when it shows its poster, and stop it when it plays or is paused. A message with no such video SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Placeholder
- **WHEN** the cursor is on a video that shows the placeholder and the user presses `?`
- **THEN** the popup lists loading the video with `gv`

#### Scenario: Poster
- **WHEN** the video shows its poster
- **THEN** the popup lists playing it with `gv`

#### Scenario: No video
- **WHEN** the cursor is on a message with no video and the user presses `?`
- **THEN** the popup has no row for videos

#### Scenario: Unbound
- **WHEN** the user sets `"gv" = "none"` and opens the popup on a message with a video
- **THEN** no row for videos is shown
