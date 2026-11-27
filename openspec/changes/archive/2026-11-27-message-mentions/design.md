# Design

## Context

See proposal.md for motivation. Current state that shapes the approach:

- Message text goes `sanitize` -> `emoji::draw_shortcodes` -> `wrap_clean` -> per row `highlight_row` + `emoji::expand` (`ui/wrap.rs`). `draw_shortcodes` writes private-use marker cells into the text and `expand` turns them into styles and images after wrapping, carrying an `inside` flag across rows. `sanitize` strips those cells from server text first, so a server cannot forge one.
- `Message` (`data/mod.rs`) has no mentions data. The wire message carries `mentions` and `channels` arrays that `api/wire.rs` ignores.
- `App.own_username` is already known (from `GET /api/v1/me`).
- Opening a room is `App::open_room(id)`. A list-and-choose popup already exists in the room switcher and the reaction picker.
- Elm/TEA: one `Action` enum, one reducer. Keymap action names and theme element names are user-facing and stable once shipped.

## Goals / Non-Goals

**Goals:**
- Tokens are styled correctly across line wraps, with no change to stored or sent text.
- One styling path serves the message flow, thread pane, search results and the draft.
- `open_channel` follows the same Action/effect shape as `open_thread`.

**Non-Goals:**
- Autocomplete, DM-from-`@user`, a next-mention motion, joining rooms (see proposal.md).
- Interpreting full Markdown. Only code spans are recognised, to avoid false highlights.

## Decisions

**1. Reuse the marker-cell technique, not a post-wrap regex.**
Tokens are located in the sanitized body before wrapping and wrapped in zero-information marker cells, like emoji names; `expand` (or a sibling pass run next to it) resolves them to styles after wrapping, with an `inside` flag per kind carried across rows. This keeps a token that wraps across two rows styled on both, and keeps the row model measuring what is drawn.
Alternative: regex on each wrapped row. Rejected: a token cut by wrapping loses its second half, and it cannot use the server's lists.

**2. The server's lists decide, with a shape fallback.**
`mentions` (usernames) and `channels` (room names) are read into `Message`. A token is highlighted when it matches an entry. `@here` and `@all` are matched literally, since the server does not reliably list them. When a message carries no lists at all, tokens are matched by shape (`@` or `#` followed by name characters) and only when not directly preceded by a letter or digit, so `bob@example.com` stays plain.
Alternative: shape only. Rejected: highlights non-mentions, cannot tell a real room from `#1`.

**3. Code spans are excluded.**
The tokenizer skips text between backticks and inside fenced blocks, so a mention in code stays plain. Nothing else of Markdown is parsed.

**4. Four theme styles, own mention most prominent.**
New `Theme` fields `messages_mention`, `messages_mention_self`, `messages_mention_group`, `messages_channel`, set from `[messages.mention]`, `[messages.mention_self]`, `[messages.mention_group]`, `[messages.channel]`. The own-name check compares case-insensitively with `own_username`. On the cursor row the highlight layers over `messages_cursor` the way custom emoji do, so a mention stays visible.

**5. The draft is styled by the same tokenizer, without server lists.**
The input bar has no server lists, so it uses the shape fallback. It only affects drawing; the sent text is untouched.

**6. `open_channel` is a Normal-mode action bound to `gc`.**
`g` is already the open/go prefix (`gt`, `gT`, `gf`, `gR`, `gr`). New `Action::OpenChannel`, snake_case name `open_channel`, in all presets.
- One channel on the cursor message: look it up among the user's subscribed rooms by name and call `open_room`.
- Several: open a small picker (the reaction-picker/switcher pattern) listing them; Enter opens the choice, Esc cancels.
- Not subscribed: a status notice, no state change, no network call. The client never joins a room.
- No channel on the message: no-op.
Channel names are resolved against already-loaded subscriptions, so no new request is needed.

**7. Privacy.**
The new `Message` fields get redacting `Debug` like the rest of the type: counts only. No log line includes a username or channel name.

## Risks / Trade-offs

- [Tokenizer drifts from the server's own rules] -> Trust the server's lists first; the shape fallback is conservative and only used when the lists are absent.
- [Marker cells for a second feature complicate `expand`] -> Keep mention kinds in a separate marker range and a separate function next to `expand`, with the same test shape as the emoji tests.
- [Two channels with the same name, differing only in case or a federation suffix] -> Match case-insensitively on the room name; the first subscribed match wins.
- [`gc` collides with a user's own binding] -> The keymap's existing duplicate and prefix checks report it as a config error.
- [Wrap width differences] -> The markers are zero-width in measurement, like emoji cells, and covered by a wrapped-token test.
