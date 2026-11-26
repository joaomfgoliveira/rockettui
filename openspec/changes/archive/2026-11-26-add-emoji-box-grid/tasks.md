# Tasks

## 1. Probe and data

- [ ] 1.1 Extend `dev/probe_react_names.py` to ask the server which toned names (`<name>_tone1..5`) it accepts, and hand the user the command to run against their local Docker server; verify by the user pasting back the non-secret summary of refused names
- [ ] 1.2 Add the refused toned names to the data in `src/ui/emoji_names`, with a test that an emoji whose toned name is refused is offered without a tone

## 2. Sections and the grid model (pure)

- [x] 2.1 Add `sections()` and `flatten()` in `src/app/react.rs`: "On this message", "Frequently used", the eight groups (Smileys and People merged), "Custom", skipping empty ones and the refused emoji; verify with unit tests for the order, for no-reactions and no-custom cases and for every unicode emoji the server accepts appearing exactly once
- [x] 2.2 Make a non-empty query produce one "Results" section from the existing ranking; verify the existing ranking tests still pass unchanged and a test for the empty-query switch back
- [x] 2.3 Add the `Grid` (rows per section from the column count) and movement: left/right in reading order, up/down by column, `<Tab>`/`<S-Tab>` by section, edges; verify with tests for each scenario in the spec, including the short last row
- [x] 2.4 Apply the tone to emoji with tones in the sections and results and to the name sent; verify with tests for default, tone 2 and an emoji without tones
- [x] 2.5 Set the first selection on open and on a search change; verify with tests for both scenarios

## 3. Keys

- [x] 3.1 Update `browse_key` and `search_key` for `hjkl`, arrows, `<C-n>/<C-p>`, `H/L`, `<Tab>/<S-Tab>` and `<C-t>`; verify with key-driven tests for every scenario of "Keys while browsing" and "Keys while searching", including that letters are text while searching
- [x] 3.2 Update the help and context-popup text that names the picker's keys; verify with the existing help snapshot tests updated and read through

## 4. State file

- [x] 4.1 Add `app::emoji_recents` (counts, tie by recency, top 16, tone, parsing that skips bad lines, formatting) and `state_dir` mirroring `config_dir`; verify with unit tests for ordering, ties, the cap, XDG and relative-XDG paths, malformed lines and the 64 KiB cap
- [x] 4.2 Add `Effect::LoadRecents`, `Effect::SaveRecents` and `Action::RecentsLoaded` (atomic write, 0600, failures ignored, `Debug` prints lengths only); verify with `effect.rs` tests against a temp dir, an unwritable dir and a missing file
- [x] 4.3 Count a pick when a reaction is added (not when taken back) and save after a pick or a tone change; verify with reducer tests that the right effects are pushed

## 5. Drawing

- [x] 5.1 Rework `src/ui/picker.rs`: search line, tab row, grid with headings and 3-column cells, footer with the selection and tone, steady height and the degrade rule; verify with insta snapshots at 52 columns, a short screen and 40x8
- [x] 5.2 Draw custom emojis in cells (image, stand-in) and make `request_images` read the visible grid rows only; verify with a snapshot of the "Custom" section and a test that only visible custom emojis are requested
- [x] 5.3 Add the `picker.tab`, `picker.tab_current`, `picker.heading` and `picker.footer` theme elements with defaults and document them in the theme spec's element list; verify with the theme tests and a snapshot with a custom theme
- [x] 5.4 Keep the sanitizing rule: custom names in the footer and the cells are sanitized and never logged; verify with the existing hostile-name tests extended to the footer

## 6. Integration

- [x] 6.1 Run `cargo test`, `cargo clippy` and `cargo fmt --check`; verify all pass
- [x] 6.2 Ask the user to check the grid in their terminal (kitty) against the local Docker server for glyph alignment, the custom section with images and the skin tone; verify by the user's confirmation

## Workflow follow-up

- Update `/home/joliveira/Documents/Obsidian Vault/Personal/Projects/Rockettui/Features.md` when the change is archived.
- Archive with `/openspec-archive-change` after the three emoji changes' dependencies are satisfied.
