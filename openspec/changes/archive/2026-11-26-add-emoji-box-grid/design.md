# Design

## Context

The picker is state on `App` (`ReactPicker { room, id, query, selected, typing }`), not a mode. `react::candidates(app, message, query)` returns a ranked flat `Vec<Candidate>` and `react::view` derives everything drawn from the app on each draw, so live changes need no plumbing. `ui/picker.rs` draws it as a list in a panel of at most 12 inner rows. `ui::emoji_names` already knows which unicode emoji the server refuses as reactions (`UNSENDABLE`) and the name it wants for the rest (`send_name`). The reducer is pure and I/O goes through `Effect`s run in `lib.rs`; the only file the app writes today is the custom emoji image cache. `config::paths` only reads. The terminal size is on `App` (`app.size`). See proposal.md for the motivation.

## Goals / Non-Goals

**Goals:**
- One pure function from (app, message, query, tone) to a list of sections, used by both the reducer (selection, movement) and the renderer, so they cannot disagree.
- Keep the picker's existing rules: selection stored as an index, the view derived on each draw, the ranking of a search, sanitized names, never logging names or search text.
- Keep the box usable at the smallest supported size (40x8).

**Non-Goals:**
- Inserting into a draft (next change), adding custom emojis, drawing custom emoji images differently from today, recently used synced with the server.

## Decisions

### Sections are a derived list, the selection stays a flat index
`sections(app, message, query, tone) -> Vec<Section { kind, title, cells: Vec<Candidate> }>` builds the sections; `flatten` gives the one list the existing `selected: usize` indexes, so `ReactPicker` keeps its shape and `Enter` still reads `view.candidates[view.selected]`. A search yields one `Results` section built by the existing `candidates()` ranking.
*Alternative:* store `(section, index)` in the picker. Rejected: every mutation then has two coordinates to keep in range, and live changes (a reaction arriving, the custom list loading) would shift sections under a stale pair.

### Movement needs the column count, taken from the same function the renderer uses
Up/down by row requires knowing cells per row. `ui::picker` exposes `columns(size) -> usize` (inner width divided by the 3-column cell), and the reducer calls it with `app.size`. Rows are laid out per section (a section starts a new row), so `app::react` computes a `Grid` (rows of flat indexes plus heading rows) from the sections and the column count; movement and drawing both use it.
*Alternative:* store the column count in `ReactPicker` on render. Rejected: the reducer would read state the drawer wrote, which breaks the Elm shape and the "pure draw" rule.

### Groups come from the `emojis` crate
`Group::emojis()` gives each group in the standard order; Smileys & Emotion and People & Body are one section, "Smileys & People", as in the web client. The `Component` group is skipped. Emoji the server refuses are filtered with `send_name` returning `None`, exactly as `candidates()` does today. `emoji.skin_tones()` and `with_skin_tone` give the toned variants; only the base emoji appear in the grid (the crate lists tone variants separately) and the chosen tone swaps the displayed one.

### Skin tone names need a probe, not a guess
The server's name for a toned emoji is `<name>_tone<N>`. Whether every toned emoji of the crate has an accepted name is a fact about the server, as with `SEND_AS`. The first task extends `dev/probe_react_names.py` (run by the user against the local Docker server) to report the toned names the server refuses; those go in `UNSENDABLE`-style data and such an emoji is shown without a tone.

### Recents and tone in one small state file, written through an effect
New `app::emoji_recents` holds `Recents { counts: Vec<(String, u32, u64)>, tone }` (name, count, last-pick sequence) and is pure. It is loaded once at start by a new `Effect::LoadRecents` that answers with `Action::RecentsLoaded`, and saved by `Effect::SaveRecents` after a pick or a tone change. The file is `$XDG_STATE_HOME/rockettui/emoji-recents`, plain text, one `name count` per line and a `tone N` line, written to a temp file and renamed, mode 0600, read capped at 64 KiB. It follows the image cache's helper pattern (`cache_dir`, `write_cached`) with a `state_dir` that mirrors `config_dir`. Debug output of the effects prints only lengths, like `EmojiName`.
*Alternative:* put it in `config.toml`. Rejected: config is read-only by design (CONF-01) and hand-edited; this is machine-written state.
*Alternative:* count on the server. Rejected: Rocket.Chat keeps it in browser storage; there is no API.

### Custom cells and their images
A custom emoji's cell is the two glyph columns of `image_glyph` (image or stand-in), as the picker's rows draw them today. `request_images`, which already reads the picker's visible rows, is changed to read the grid's visible rows (the window the drawer shows, derived from the same `Grid` and scroll) instead of the old list window, so a long "Custom" section does not request every image.

### Footer, not per-row notes
Each cell is just a glyph, so the per-row `alias`/`custom`/count/`you` notes of the list move to one footer line for the selected emoji. Own reactions are marked by the existing `picker_own` style on the cell. Alias candidates keep their `alias` note in the footer.

### Layout degrades from the bottom
Inner rows, top to bottom: search, tabs, grid, footer. The panel keeps its steady height (max 12 inner rows). When `inner_height` is below 5 the footer goes, below 4 the tabs go; the search line and one grid row always stay. At 40x8 the available inner height is about 4, which keeps search, tabs-or-grid and a row; the existing small-terminal rule (title and search line) is kept.

### Theme elements are only added
`picker.tab`, `picker.tab_current`, `picker.heading` and `picker.footer` are new theme elements with defaults derived from the existing picker styles, so existing themes keep working.

## Risks / Trade-offs

- [Emoji widths differ between terminals and fonts: a glyph the table calls 2 columns may draw as 1 or 3, shifting the rest of the row] → every glyph is drawn into a fixed 3-column cell and padded by measured width; a manual check in the user's terminal (kitty) is a task, and emoji of the newest Unicode versions the font lacks are accepted as a known limit, as in any client.
- [Selection index and live changes: a reaction arriving shifts the flat list] → `close_if_stale` already clamps; the selection stays an index, as today.
- [About 1,900 cells recomputed each draw] → sections are built from static tables and a filter; a search already scans the whole table per key today. Measure before caching.
- [A state file is new surface: names written to disk] → only emoji names and counts, owner-only, never logged, size-capped on read; a failure never blocks the user.

## Migration Plan

No data migration. The state file is created on the first pick. Removing the change leaves a harmless unused file.

## Open Questions

- Whether "Frequently used" should decay old picks (a plain count never forgets). Deferrable: the file format already holds a sequence per name.
