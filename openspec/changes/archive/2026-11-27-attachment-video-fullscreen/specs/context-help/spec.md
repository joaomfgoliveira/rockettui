# Spec Delta

## ADDED Requirements

### Requirement: Fullscreen video action on messages
A message that has a video attachment the client may fetch SHALL list `play_video_fullscreen` in the context popup, with the keys currently bound to it, worded as watching the video fullscreen. A message with no such video, and a video whose poster failed, SHALL NOT list it, and an action with no key bound SHALL be left out.

#### Scenario: Listed
- **WHEN** the cursor is on a video and the user presses `?`
- **THEN** the popup lists watching it fullscreen with `gV`

#### Scenario: No video
- **WHEN** the cursor is on a message with no video and the user presses `?`
- **THEN** the popup has no fullscreen row

#### Scenario: Unbound
- **WHEN** the user sets `"gV" = "none"` and opens the popup on a video
- **THEN** no fullscreen row is shown
