# Proposal

## Why

The emoji box (the reaction picker, `+`) lists only the message's own reactions until the user searches, so picking an emoji means already knowing its name. Rocket.Chat's own client shows every emoji in sections with a search line, category tabs, a "Frequently used" row and a skin tone choice. This change gives the box that shape. It is also the box that the next change (`add-emoji-insert`) opens from Insert mode, so it has to stand on its own first.

## What Changes

- With nothing typed, the box shows all emoji as a grid in sections, instead of the message's reactions only: **On this message** (when it has reactions), **Frequently used**, Smileys & People, Animals & Nature, Food & Drink, Travel & Places, Activities, Objects, Symbols, Flags, and the server's **Custom** emojis.
- A row of category tabs under the search line shows the sections and marks the one the selection is in; `<Tab>` and `<S-Tab>` jump between sections.
- Navigation in the grid is two-dimensional (`hjkl`, arrows). A search shows its results in the same grid under one "Results" section, with the same ranking as today.
- The selected emoji is named in a footer line (`:name:`, how many gave it, `you`), which replaces the per-row notes of the list.
- A skin tone choice (`<C-t>` cycles it) applies to every emoji that has tones, in the grid, in search results and in what is sent. It is remembered.
- "Frequently used" is learned from the reactions the user sends and kept between sessions in a small state file.
- Unicode emoji the server refuses as a reaction are still not offered. Aliases and custom emojis keep working as today.
- The footer, tabs and skin tone line are dropped, in that order, on a pane too short for them; the box stays usable at the smallest supported size.

Out of scope: adding custom emojis ("Add emoji" in the web client), and inserting an emoji into a draft (`add-emoji-insert`).

## Capabilities

### New Capabilities
- `emoji-recents`: the "Frequently used" list and the chosen skin tone, how they are learned, kept between sessions and read back, and what happens when the file cannot be read or written.

### Modified Capabilities
- `theme`: the picker gains `[picker.tab]`, `[picker.tab_current]`, `[picker.heading]` and `[picker.footer]`; existing elements are unchanged.
- `reaction-picker`: the empty-search view becomes the sectioned grid (replacing "Current reactions with nothing typed"), results and navigation become grid-based, and the footer, tabs, skin tone and layout rules change with it.

## Impact

- `src/app/react.rs` (candidates stay; sections, grid selection and tone are added), a new `src/app/emoji_recents.rs` (state), `src/ui/picker.rs` (grid, tabs and footer drawing), `src/effect.rs` and `src/lib.rs` (load and save of the state file), `src/config/theme.rs` (new `picker` theme elements for tabs, section headings and the footer; theme element names are user-facing vocabulary, so they are only added).
- The `emojis` crate already provides groups and skin tones; no new dependency.
- A new file under `$XDG_STATE_HOME/rockettui/` (default `~/.local/state/rockettui/`), the first state the app keeps besides the image cache.
- Help and the context popup text that mention the picker's keys are updated.
