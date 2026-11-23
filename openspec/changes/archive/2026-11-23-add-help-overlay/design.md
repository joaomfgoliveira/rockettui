# Design

## Context

See `proposal.md` for motivation. The keymap is already a per-mode `Trie<Action>` built from a preset plus `keymap.toml` overrides. `Keymap::bindings(mode)` returns every `(chords, action)` pair and `actions::name_of` gives the user-facing name, so the data for a help screen exists. The `:` commands live in one table in `app/command.rs` (names and aliases). The confirmation prompt (`App.confirm`) is the existing precedent for transient UI that is not a mode.

## Goals / Non-Goals

**Goals:**
- One source of truth: help is derived from the live keymap and command table, with no hand-written key list.
- Follows user rebinding: scroll and close keys come from the active Normal keymap where they have an action.
- Reusable binding data, so a later which-key change does not need to recompute it.

**Non-Goals:**
- Which-key hints, in-help search, per-context help.
- A new `[help]` table in `keymap.toml` or a fourth mode.

## Decisions

**Overlay state, not a `Mode`.** `App` gets an optional help state (scroll offset). The mode badge keeps showing the real mode. Alternative: `Mode::Help`. Rejected because the specs promise three modes and three `keymap.toml` tables, and a mode would force a fourth table, lockout rules and preset rows. The confirmation prompt already sets this pattern.

**Key handling while open.** The reducer checks the overlay state first and resolves keys through the Normal keymap as usual, but only lets a small set of actions through: motions (`move_down`, `move_up`, `goto_top`, `goto_bottom`, `half_page_down`, `half_page_up`) scroll, and `cancel` and `show_help` close. Every other resolved action is dropped. `q` closes as a fixed extra key, since `q` is not bound in Normal mode by default. Counts and the pending-keys display keep working because the same matcher is used. Alternative: a separate hard-coded key table for the overlay. Rejected because it would ignore the user's rebinds, which is the point of the feature.

**Opened by an action, `show_help`.** A new unit `Action::ShowHelp` with the name `show_help`, bound to `?` in Normal mode, and `:help` calls the same reducer path. Adding a name is additive, not a breaking rename. `name_of` has no wildcard arm, so the compiler forces the new variant to be handled.

**Content model.** A pure function builds a list of lines (sections, rows) from `&Keymap` and the command table, with no I/O and no `Frame`. The UI only renders it and applies scroll. This keeps it unit-testable and is the piece which-key can later share. Descriptions come from one static table keyed by action name, and a test asserts every name in `ACTION_NAMES` has a description, so adding an action without one fails CI. `cmd:<command>` rows show as `:<command>`. Keys are rendered with the existing Vim-notation formatter.

**Rendering.** A `ui/help.rs` widget draws a bordered, titled block over the messages pane area, clearing it first. Text goes through the existing sanitizer even though it is built-in text, because `cmd:` bindings are user config. Scroll is clamped to content length minus viewport height. The key column width is computed from the widest keys.

**Theming.** New element names (`help.border`, `help.title`, `help.heading`, `help.key`, `help.desc`) are added to the theme with defaults. They are additive. The spec only requires that they are themeable, so exact names are an implementation detail documented in the README and example theme.

## Risks / Trade-offs

- [The overlay hides the messages pane, so a long read is cut off by the live view] → Messages still update underneath, and closing is one key.
- [A user unbinds all motions, so scroll does nothing] → The `?` and `q` close keys still work, and `q` is fixed, so there is no lockout.
- [A `cmd:` binding holds unusual text] → Sanitized like all user-provided strings.
- [Static descriptions drift from behavior] → One table plus the all-names-covered test. Descriptions are one short line each.

## Open Questions

- Whether to highlight the keys of the current pane's context first. Deferred; it does not change the specs.
