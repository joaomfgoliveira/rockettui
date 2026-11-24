# rockettui backlog

Ideas for later. Nothing here is committed or scheduled. Each item becomes its own
OpenSpec change (`/openspec-propose`) when picked up. Source: the end of the
`company-server` change (2026-11-23), after the app started daily use on the company server.

## Working method

Use the app for about a week against the company server and note every gap here as it
shows up. Real annoyances beat guesses. Optionally run `/openspec-explore` to compare the
current specs with what the Rocket.Chat web app does daily, to get a ranked gap list.

## Candidates (unchecked against the specs, to be confirmed)

- Search: `/`, `n`, `N` in the open room is the `find-in-room` change (`n` goes toward older messages, `Esc` clears); searching the whole history on the server is the separate `search-room` change
- Threads
- Reactions
- Room switching and filtering
- Unread jump (go to next room with unread)
- Missing keybindings
- Room-list quirks (for example enterprise sidebar categories)
- Unsupported message types

## Open questions

- Which Rocket.Chat version does the company run?
- Is the third-party client / PAT policy formally confirmed?
- Is the company CA trusted via the system store (appears so, since it works), or would a `ca_cert` option be needed?

## Already in `objective.md` "Later"

Username/password/OTP login, light markdown, desktop notifications, theme gallery,
keyring token storage, config hot reload.
