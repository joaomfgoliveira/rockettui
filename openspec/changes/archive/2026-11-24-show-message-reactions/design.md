# Design

## Context

See `proposal.md` for the motivation. Today:

- `WireMsg` has no `reactions` field, so serde skips it; `Message` has no reactions; `map::message` has nothing to map.
- `app::messages::is_edit` treats a redelivered message with equal body and edited flag as a no-op. That is where reactions are dropped, and a test (`redelivery_without_a_change_is_ignored`) and a spec scenario pin it.
- The row model is shared: `row_starts_of` and `render_rows` both go through `wrap::block_rows` and `RowCtx`, built only by `App::row_ctx`, so the reducer and the renderer cannot disagree on a message's height. Replacing a message in place already goes through `anchored()`, which keeps the cursor on the same message and screen row.
- The `?` popup is the model for an overlay: a pure content builder in `app/context.rs`, a thin drawer in `ui/context.rs`, "any key closes and is swallowed" handled in `app/update.rs`.
- Rocket.Chat sends `reactions` as an object keyed by shortcode: `{ ":+1:": { "usernames": ["bob", "ana"] } }`. Some versions add a parallel `names` array of display names.

## Goals / Non-Goals

**Goals:**
- Reactions on every message, live and from history, drawn as glyphs with counts, with a popup for who reacted.
- No new failure modes: an unknown or malformed reaction degrades to its raw name or to nothing, never an error.
- The row model stays the single source of message height.

**Non-Goals:**
- Sending or removing reactions, custom emoji images, reaction notifications (see proposal).
- Using display names (`names`): `usernames` is what the rest of the app shows.

## Decisions

### Data model: an ordered `Vec`, parsed with a map visitor
`Message` gains `reactions: Vec<Reaction>` with `Reaction { name: String, users: Vec<String> }`, `name` stored without colons. The count is `users.len()`. The wire type deserializes the `reactions` object with a small custom `Deserialize` (a map visitor collecting into a `Vec`), not into `serde_json::Map`.

*Why:* `serde_json::Map` is a `BTreeMap` unless the `preserve_order` feature is on, which would sort reactions alphabetically and break "in the order the server lists them" (spec: Reaction row). The visitor keeps server order for a typed parse straight from text (history). The field is lenient like the rest of `wire.rs`: absent, `null` or a malformed entry yields no reactions for that message, never a failed page.

*Correction found in task 4.3:* the visitor alone is not enough. The realtime stream decodes a whole DDP frame into a `serde_json::Value` first, and a `Value` map is sorted too, so live reactions came out alphabetical while history (and a reconnect refetch) gave server order. The captured-stream test caught it. The fix is to enable serde_json's `preserve_order` feature: `indexmap` is already in the dependency tree through `h2`, so it adds no crate, and it makes both paths keep server order. The earlier rejection of `preserve_order` ("heavier than a visitor") assumed only the history path mattered. The visitor stays for the leniency. Guards: `reactions_keep_server_order_through_a_json_value` in `wire.rs` and the stream-versus-history convergence test in `app::realtime`.

### One change check for edits and reactions
`is_edit` becomes a check on "text, edited state or reactions differ" and the in-place replace is the same path as today (`anchored` + `apply_edit`), so a reaction update cannot move the cursor and is not marked edited (the edited flag comes from the server's `editedAt`, which a reaction does not set). `Message` equality already compares all fields, so history merges and the reconnect reconcile need only to copy `reactions` along with text and edited state.

*Alternative:* a separate `Action::RtReaction`. Rejected: the stream delivers a whole message, and splitting it would need a second diff in the realtime layer for no gain.

A reaction update for a message not held follows the existing edit-of-unheld-message behavior (appended like any live message when the room is loaded). That is odd for an old message, but it is existing, tested behavior and not worth a second rule.

### Rendering: reaction rows inside the block
`wrap::block_rows` adds the reaction rows to a message's height and `render_rows` emits them, so `row_starts` and the renderer stay in agreement. The resolved text depends on config, so `RowCtx` gains a reference to the alias table. Resolution (alias, unicode, raw) is done at render time, not stored in `Message`, so a config change needs no reload.

Layout: items are `{glyph} {count}` joined by two spaces, wrapped by whole items to the body width and never split mid-item; an item wider than the row is cut with the existing author-fitting ellipsis helper. In `two_line` and `compact` the rows start at the body column. Width is measured with `unicode-width` on the sanitized final string, as for message bodies.

The own-reaction highlight needs the user's own username, which the app already holds for edit/delete ownership; `RowCtx` carries it too.

*Cost:* `block_rows` runs for every message on each `row_starts`. Shortcode lookup is a hash lookup, so this is acceptable; if profiling ever shows it, cache the resolved string per reaction.

### Shortcode resolution
Order: alias, unicode table, raw `:name:`. A trailing `_tone1`..`_tone5` (Rocket.Chat's older emoji set) is stripped, the base is resolved, and the matching skin tone applied (1 light, 2 medium-light, 3 medium, 4 medium-dark, 5 dark); if the base has no skin-tone variants the plain glyph is used. An alias is looked up under the full name first, so an alias can also cover a `_toneN` name.

*Crate (chosen in task 1.1):* `emojis` 0.9.1: `(MIT OR Apache-2.0) AND Unicode-3.0`, MSRV 1.66 (project is 1.88), no default dependencies, lookup by gemoji shortcode and `with_skin_tone`. Checked in a scratch project against the names captured from the 8.9.1 server and 55 other common Rocket.Chat reactions: 50 resolve, tones 1-5 resolve for the tone-capable ones, and the misses are Rocket.Chat's older emojione-only names plus custom emoji. Because `slight_smile` (Rocket.Chat's default smiley) is among the misses, the resolver also carries a small built-in map of legacy names to gemoji names (`slight_smile` and `simple_smile` to `slightly_smiling_face`, `slight_frown` to `slightly_frowning_face`, `upside_down` to `upside_down_face`, `white_frowning_face` to `frowning_face`), tried after the unicode table misses and before the raw fallback. It is not user config and is not in the spec: it only makes more shortcodes resolve to their obvious glyph. The resolver sits behind one function (`resolve(name, aliases) -> String`), so swapping the crate touches one module.

*Captured server facts (8.9.1, `src/api/fixtures/docker/{history,stream}_reactions.json`, written by `dev/capture_reactions.py`, never edited):* `reactions` is `{":+1:": {"usernames": [...]}}` with no `names` array; the server keeps insertion order; a live reaction event is the whole message with `reactions`; taking back the last reaction delivers the message with the `reactions` key **absent** (not `{}`, not `null`), so a missing field must mean "no reactions". The server rejects an unknown shortcode with 400, but accepts skin-tone and custom names. Only 8.9.1 was running locally; 9.0.0 was not captured. *Alternative:* a hand-written table of the top ~100 reactions; rejected as it never stops growing, and unknown names are already safe through the raw fallback.

### Popup: state names the message, content is derived
`App` holds the open popup as the room id and message id, not a copy of the reactions. Content is rebuilt from the held message on every frame, so a live update changes it for free (spec: Popup follows live changes), and if the message or its last reaction disappears the popup closes. Structure mirrors the context popup: pure builder in `app/`, drawer in `ui/`, key swallowing in `update.rs`.

This popup shows usernames, which the `?` popup's styling requirement forbids ("never message content, author names or room names"). That rule is about the context popup only; the reactions popup has its own spec and its own sanitizing requirement, and its elements are `reactions.*`, not `context.*`.

Content: one entry per reaction, `glyph :shortcode:  user, user, you`, the user's own username replaced by `you` and placed first; the user's own entries use `reactions.own`. Height is capped; overflow is cut with a `... N more` line.

### Keymap
New action `show_reactions`, bound to `gr` in Normal mode in every preset. `g` is already a prefix of `gg`, so `gr` adds a second continuation; the existing prefix-clash validation applies unchanged (a user binding of plain `g` clashes with both). The action only acts when the messages pane is focused on a message with reactions; otherwise it does nothing. Theme and action names are user-facing vocabulary, so the names in the specs are final.

### Config
`[emoji_aliases]` is parsed in `settings.rs` into a map from name to string. Validation reuses the marker rules (sanitizing must not change the value, at most 8 columns) and rejects an empty name or one containing a colon, whitespace or control character, naming the entry in the error like every other config error.

### Privacy
`Reaction` and the wire types do not derive a printing `Debug`; the redacting `Debug` of `Message` prints only the number of reactions. Names and usernames are sanitized before drawing and never reach a log or status message.

## Risks / Trade-offs

- [Terminals disagree on the width of ZWJ and flag sequences, which can misalign the rest of a row] -> Kitty draws them correctly; the reaction row has no padded columns after the glyphs, so the damage stays within that row. Not worked around beyond measuring width on the final string.
- [Rocket.Chat's shortcode set differs from gemoji's] -> Unknown names fall back to the raw `:name:`; the aliases table lets the user fix any that matter; coverage is checked against captured server data.
- [Reactions make messages taller, shifting rows] -> Handled by the existing row model and `anchored()`; covered by a test like the existing "edit above the cursor" one.
- [An old message that gets a reaction while not held is appended at the end] -> Same as edits today; revisit for both together if it bothers in practice.
- [Server version differences in the `reactions` shape] -> Lenient parsing; verified on 8.9.1 and 9.0.0 with captured fixtures that are never edited to pass.

## Open Questions

- Which unicode shortcode crate, settled by the first task; it does not change specs or task breakdown beyond that task.
