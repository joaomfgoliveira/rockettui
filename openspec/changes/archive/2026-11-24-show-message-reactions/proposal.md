# Proposal

## Why

Reactions are part of how people answer each other on Rocket.Chat ("+1", "done", "on it"), and the client hides them completely: the wire field is dropped, and the realtime path deliberately ignores a message that only gained a reaction. A reader of a busy channel misses acknowledgements and cannot tell who reacted. This is the first of the missing-feature changes and stays read-only.

## What Changes

- Messages that carry reactions get an extra row under the body showing each reaction and its count, for example `👍 3  🎉 1`. Messages without reactions are drawn as before.
- Reactions are shown as emoji glyphs. Rocket.Chat sends shortcodes (`:+1:`, `:thumbsup_tone2:`); they are resolved through a unicode shortcode table, including the `_toneN` skin-tone suffix. A shortcode that does not resolve is shown as its raw `:name:`, which is also how server custom emoji appear (they are images and cannot be drawn in a terminal).
- A new `[emoji_aliases]` table in `config.toml` maps a shortcode name to a replacement string, so a custom emoji can be shown as a lookalike (`partyparrot = "🦜"`). Lookup order: alias, unicode table, raw `:name:`.
- A new bindable action `show_reactions`, bound to `gr` in Normal mode, opens a popup for the message under the cursor listing each reaction with the users who gave it. The user's own username is shown as `you`, and their reactions are highlighted. Any key closes the popup. On a message with no reactions the action does nothing.
- Realtime: a reaction added or removed on a held message now updates that message in place (without marking it edited, without moving the cursor). This **inverts** the current "Reaction on a held message: the row does not change" scenario.
- History: reactions load with fetched pages, and a refetched page after a reconnect refreshes the reactions of the held messages it covers.
- The context popup (`?`) lists `show_reactions` on a message that has reactions.
- Theme: new elements for the reaction row, the user's own reactions and the popup.
- Out of scope: adding or removing reactions (`chat.react`), drawing custom emoji images (a possible follow-up using the kitty graphics protocol), reaction notifications.

## Capabilities

### New Capabilities
- `message-reactions`: showing a message's reactions as a row of emoji with counts, resolving shortcodes to glyphs with aliases and a raw fallback, and the popup that lists who reacted.

### Modified Capabilities
- `realtime-messages`: a live update that changes only the reactions of a held message now replaces its reactions in place instead of being ignored.
- `message-history`: fetched pages carry reactions, and the reconnect reconcile refreshes them.
- `context-help`: the popup lists the reactions action on messages that have reactions.
- `keymap`: the action vocabulary gains `show_reactions`, bound to `gr` by default.
- `theme`: new themeable elements for reactions.
- `configuration`: `config.toml` gains the `emoji_aliases` table.

## Impact

- Code: `src/api/wire.rs` (`reactions` field), `src/api/map.rs`, `src/data/mod.rs` (`Message` gains reactions; the redacting `Debug` must not print usernames or names), `src/ui/wrap.rs` and `src/ui/messages.rs` (extra row, which `row_starts` and the anchoring already absorb), `src/app/messages.rs` and `src/app/realtime.rs` (`is_edit` and the equality checks), a new popup in `src/app` and `src/ui`, `src/input/actions.rs` and `src/input/keymap.rs`, `src/config/{settings,theme}.rs`, `src/app/context.rs` and `src/app/help.rs`.
- Dependency: one unicode emoji shortcode crate, chosen in the design.
- Privacy: reaction usernames are shown on screen only. They are never logged and never printed by `Debug`.
- Docs: help text and the keymap and theme references.
