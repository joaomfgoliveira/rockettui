# Design

## Context

See `proposal.md` for the motivation and `specs/reaction-picker/spec.md` for the behavior. Today:

- Reactions are read-only. `Message.reactions` holds them in server order, `ui::emoji::resolve` turns a name into the text drawn, `app::reactions` is the `gr` popup, and a live reaction already redraws the row from the message the server streams back.
- Writes follow one shape: an `Effect` carries the request, `effect::run` calls the `api::Client` and returns the `Action`s that report the outcome, and a failure becomes one fixed status line built by `edit::failure_text`. Edit and delete refuse in a detached room through `jump::refuse_if_detached`.
- Overlays are state on `App`, not modes: `search_prompt`, `context`, `reactions_popup`. Each takes keys before the keymap in `update::reduce`, has a pure content builder in `app/` and a thin drawer in `ui/`, and is rebuilt from the held data on every draw.
- Facts measured on the Docker server (8.9.1), recorded in the archived `show-message-reactions` change: `chat.react` takes `messageId`, `emoji` (with colons) and `shouldReact`, and both values answer 200; the server rejects a name it does not know with 400; it accepts standard, skin-tone and custom names; `emoji-custom.list` answers `{"emojis": {"update": [{"_id", "name", "aliases", "extension"}], "remove": []}}`.
- The server's standard emoji set is the older emojione one. The `emojis` crate used for display is GitHub's (gemoji). Going from a server name to a glyph is safe; going from a glyph or crate name to a server name is the open risk.

## Goals / Non-Goals

**Goals:**
- React, and take a reaction back, from the keyboard in a few keys, with a search that finds the server's custom emojis.
- Offer only names the server accepts, or fail with one clear status line, never silently.
- Reuse the existing popup, effect and status patterns so the picker adds no new kind of machinery.

**Non-Goals:**
- Reacting from the `gr` popup, glyph input, recent or frequent ranking, skin-tone variants in the search, custom emoji images, and a reaction-sent confirmation (the streamed message is the confirmation).

## Decisions

### State: the picker names its message and holds only the query
`App.react_picker: Option<ReactPicker>` holds the room id, the message id, the typed query and the selected index. Candidates, counts, own marks and the visible window are derived on every draw from the held message, the settings, the custom-emoji cache and the query, exactly like `reactions::content`. The module is `app/react.rs`, the drawer `ui/picker.rs`, the action `Action::ReactMessage` with the keymap name `react_message`.

*Why:* a copy of the reactions would go stale when a live reaction arrives, which the spec requires to show without a key. *Alternative:* a mode (`Mode::React`). Rejected: no other overlay is a mode, and the mode badge and keymap tables would have to learn a fourth mode.

### Key routing: the picker takes every key, ahead of the keymap
In `update::reduce` the picker check sits right after the search prompt, before help, context and the keymap, and swallows all keys. Pastes route to it the way they route to the search prompt, and `KeyTimeout` is ignored while it is open. `<C-c>` and `Esc` always close it, so it can never lock the user in. The post-action step that already closes the reactions popup when it has nothing to show also closes the picker when its message is gone or deleted.

### Two states, as in vim: browsing and searching
`ReactPicker` has a `typing` flag. It opens with it off (**browsing**): `j`, `k`, the arrows, `<C-n>` and `<C-p>` move, `Enter` sends, `q` and `Esc` close, `/` clears the query and starts searching, `i` and `a` start searching with the query kept, and every other key, a paste included, does nothing. With it on (**searching**) letters are text, `<C-n>`, `<C-p>` and the arrows move, `Enter` sends, `Esc` and `Backspace` on an empty query turn it off, keeping the query and its results so `j` and `k` can walk them. `<C-c>` closes from both. The picker always opens browsing, even on a message without reactions, so the first key after `+` always means the same thing.

*Why:* the first version made every letter a search, which left `j` and `k` unusable and made movement `<C-n>`/`<C-p>`/arrows, as in fzf. In a vim-style app that felt wrong, and the user chose this two-state design over adding `<C-j>`/`<C-k>`. The cost, accepted by the user, is that typing no longer searches the moment it starts: it takes `/` or `i` first.

*Alternative:* keep one state and add `<C-j>`/`<C-k>` to the movement keys. Smaller, but it keeps the picker unlike everything else in the app.

### Search: a linear scan, ranked by exact, prefix, substring
No index. Each keystroke lowercases the query (leading and trailing colons stripped) and scans: the message's own reaction names (only for an empty query), the aliases, the custom emojis (name and aliases), then every unicode emoji with each of its shortcodes (`emojis::iter()`, a few thousand names). A candidate matches when the query occurs anywhere in a name. Rank is exact, then prefix, then substring; ties break on shorter name, then alphabetical, so results are deterministic. A candidate appears once, under the name that will be sent. The list is capped at a few hundred rows.

*Why a plain scan:* a few thousand short strings per key press is far below a frame; an index would be extra state to keep correct. *Alternative:* fuzzy matching (subsequence). Rejected for now: it ranks poorly on short queries and substring is what the spec promises.

### The name that is sent
- An existing reaction, an alias or a custom emoji is sent under the name the user sees.
- A unicode emoji is sent under the first of its shortcodes the server accepts. A probe on the Docker server (below) records, for every emoji the crate knows, which shortcode is accepted. Only the exceptions are stored: a small generated table of "send as this other shortcode" entries and a list of emoji with no accepted shortcode, which the picker does not offer, because it could not send them.
- The `emoji` argument is the name wrapped in colons, as the capture showed.

*Why a probe and a table, not a guess:* the crate's names and the server's differ in places that matter (`slight_smile` against `slightly_smiling_face` is one we already found). A rejected reaction in the middle of a search is the worst outcome, so the cost is paid once at development time.

### Add or take back: decided from the held message at Enter
At `Enter` the reducer looks the chosen name up among the message's reactions: if the user's own username is among its users the request has `shouldReact` false, otherwise true. An explicit value is sent every time, never a toggle, so the outcome does not depend on server toggle semantics. The own username is required, which is why opening the picker is refused while it is unknown.

*Alternative:* omit `shouldReact` and let the server toggle. Rejected: unverified, and it would make a stale view act in the wrong direction.

### Custom emojis: fetched once, retried only after a failure
`App.custom_emojis` is `NotLoaded`, `Loading` or `Loaded(list)`. The first open moves `NotLoaded` to `Loading` and pushes `Effect::LoadCustomEmojis`; `Action::CustomEmojisLoaded` stores the list (the open picker simply starts matching it) and `Action::CustomEmojisFailed` returns the state to `NotLoaded`, so the next open retries. No error is shown: the picker degrades to unicode and aliases. The endpoint is read with lenient wire types (absent fields and entries without a name are skipped) and its `remove` list is ignored since the whole list is fetched.

### Sending: an effect with no success action
`Effect::React { id, emoji, add }` (no room: `chat.react` takes only the message id, and the emoji is a redacting `EmojiName` so the derived `Debug` never prints a reaction name) calls `Client::react` and returns nothing on success, because the streamed message is what changes the row. On failure it returns `Action::ReactFailed { error }`, shown through `failure_text` with "reaction not sent" and "reaction may not have been sent". The client-build-failure arm of `effect::run` maps both new effects too. The server's text never reaches the error, as for edit and delete.

*Risk this accepts:* if the stream is down when the user reacts, the row does not change until the next refetch, and nothing says the reaction went through. A success status would fix it but adds a line on every reaction; revisit if it bites.

### Refusals live in `open`, not at `Enter`
`open` checks, in order: messages pane focused, the message is reactable (a server message that is neither a tombstone nor a system message), the room is not detached (the existing refusal and its status text), the own username is known (a new fixed status). Nothing is refused after the picker is open except by the server.

### Drawing
`ui/picker.rs` follows `ui/reactions.rs`: a bordered panel over the bottom of the messages and input column, one search row (`> ` and the query) then result rows, at one steady height (the most the screen allows, up to a cap) so the search line does not jump as the list grows while the user types, and usable at the smallest size. The search line shows its state: while searching it is drawn as live input with the hardware cursor after the text; while browsing it is dim, shows the text of the last search (or says to press `/` to search), and sets no cursor. Each row shows the glyph (or the raw name), the `:shortcode:`, and a short right-hand note: the count and `you` for the message's own reactions, `alias` or `custom` for those sources. The window scrolls so the selection is visible; the offset is computed from the selection each draw, not stored. A row wider than the space ends in an ellipsis, using the same width helper as the other popups. The hardware cursor sits after the typed query, only while searching. The key hint goes in the bottom border only when it fits, it names the keys of the current state, and it names what `Enter` will do with the selected result (`add`, or `remove` when the user's own name is already on it), because the same key does both and nothing else on screen says which. The `?` popup and the help text word the action as adding or removing the user's reaction, not as "reacting", for the same reason. New theme elements are `picker.border`, `.title`, `.input`, `.item`, `.selected`, `.own` and `.hint`; the selected row defaults to reversed video and own entries to bold, so neither depends on colour.

### Probe and generated table
`dev/probe_react_names.py` (standard library, loopback only, same output rules as the other probes) posts a throwaway message as the app user, reacts and takes back each shortcode the crate knows through `chat.react`, and writes which are accepted. The server's REST rate limiter would throttle thousands of calls, so the probe turns it off as admin and restores it in a `finally`, like `probe_edit.py` does for its settings. Its output generates `src/ui/emoji_names/data.rs`; a unit test checks that every entry names an emoji the crate knows. Only 8.9.1 is available locally, so the table is a measurement of that version.

*Found while running it:* trying only the crate's shortcodes left 410 of 1870 emoji (flags, gendered and profession variants, newer emoji) with no accepted name, though the server has them under others. The server bundles `emojibase-data`, whose shortcode sets list those names, so `dev/emoji_alternates.py` turns them into extra candidates (copied from the throwaway dev container) and the probe tries them only after the crate's own are rejected. The result is every emoji sendable: 1451 as the crate names them, 419 renamed, none dropped. The accepted names are not always the obvious ones (flags are `flag_af`, not `flag_afghanistan`), which is the reason to measure. `UNSENDABLE` stays in the format for a future server version that lacks some emoji, and is empty today.

## Risks / Trade-offs

- [The server accepts a different shortcode set than the crate offers] -> The probe-generated table renames or drops the differences, and a remaining rejection is one fixed status line. A different server version may differ; only 8.9.1 is measured.
- [The probe changes a server setting] -> Loopback only, the setting is read first and restored in a `finally`, and the last output line says whether it is back, as `probe_edit.py` does.
- [A reaction sent while the stream is down shows no change] -> Accepted for now (see Sending); a refetch after reconnect shows it.
- [The custom emoji list could be large] -> Fetched once per session, one request, matched by a linear scan; an empty or failed fetch costs only the custom results.
- [The picker captures every key] -> `Esc` and `<C-c>` always close it, and keys are never forwarded, so no keymap can lock the user in.
- [Reacting changes shared state on the company server] -> Only the user's own reactions, through the documented endpoint, as the user; developed and probed against the local Docker server first, per the project constraints.
- [`+` needs Shift on many layouts] -> It is rebindable and removable like every action; the key notation already handles shifted characters such as `?`, `G` and `N`.

## Open Questions

- None that change the specs or the task breakdown. The probe's result decides how long the generated table is, not how the picker behaves.
